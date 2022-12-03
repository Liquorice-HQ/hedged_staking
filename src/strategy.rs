// Main strategy module
//
#[allow(unused_imports)]
use log::{debug, error, info, trace, warn };
use openlimits::model::Side;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal_macros::dec;
use strum::EnumMessage;
use tokio::time::{sleep,timeout,Duration};

use std::cmp::{min, max};

use crate::consts::*;
use crate::helpers::*;
use crate::monitoring;
use crate::types::*;
use crate::utils::*;


#[derive(Debug, Clone, Copy, PartialEq, Eq, strum_macros::EnumMessage)]
pub enum StrategyState {

    #[strum(message="Monitoring")]
    Monitoring,

    //
    // Processing ratio overflow -------------------------------------------------------
    //

    #[strum(message="Overflow: transfer from dYdX to Wallet")]
    TransferE1ToWallet(PrimaryAsset),

    #[strum(message="Overflow: transfer from Wallet to Binance")]
    TransferWalletToEE(PrimaryAsset),

    #[strum(message="Overflow: transfer from Binance to Kraken")]
    TransferEEToE2(SecondaryAsset),

    #[strum(message="Overflow: stake on Kraken")]
    Stake(SecondaryAsset),

    #[strum(message="Overflow: reduce short position on dYdX")]
    ReduceShort(SecondaryAsset),

    #[strum(message="Overflow: enlarge short position on dYdX")]
    EnlargeShort(SecondaryAsset),

    #[strum(message="Overflow: enlarge ATOM positions")]
    EnlargeSecondaryBoth(PrimaryAsset),
    //
    // Processing ratio underflow ------------------------------------------------------
    //

    #[strum(message="Underflow: unstake")]
    Unstake(SecondaryAsset),

    #[strum(message="Underflow: transfer Kraken to Binance")]
    TransferE2ToEE(SecondaryAsset),

    #[strum(message="Underflow: transfer Binance to Wallet")]
    TransferEEToWallet(PrimaryAsset),

    #[strum(message="Underflow: transfer Wallet to dYdX")]
    TransferWalletToE1(PrimaryAsset),

    #[strum(message="Underflow: reduce ATOM positions")]
    ReduceSecondaryBoth(PrimaryAsset),

}


fn pretty_state_msg(s: &StrategyState) -> String {
    match s.get_message() {
        None => "? no msg ?".to_string(),
        Some(msg) => msg.to_string(), // TODO make formatted output
    }
}


type NewStateAndDelay = (Option<StrategyState>, Option<Duration>);


async fn process_state(ctx: &mut Context, start_state: Option<StrategyState>, is_monitoring_only: bool) -> StrategyResult<NewStateAndDelay> {
    let state_pre = match start_state {
        None => detect_current_state(ctx, is_monitoring_only).await?,
        Some(st) => st,
        };
    let state = if is_monitoring_only { StrategyState::Monitoring } else { state_pre };
    let monitoring_msg = if is_monitoring_only { " (MONITORING) " } else { "" };
    info!("=== Current state: {} ({:?}) {}===", pretty_state_msg(&state), state, monitoring_msg);
    log_balances(ctx).await?;
    if ctx.consts.keypress_to_continue { press_enter_to_continue() } // debug: waiting for key press
    match state {
        StrategyState::Monitoring => {
            debug!("Waiting {} seconds...", ctx.consts.monitoring_timeout);
            return Ok((None, if ctx.consts.keypress_to_continue { None } else { Some(Duration::from_secs(ctx.consts.monitoring_timeout)) }));
        },
        StrategyState::TransferE1ToWallet(v) => do_e1_to_wallet(ctx, Some(v)).await?,
        StrategyState::ReduceSecondaryBoth(v) => do_reduce_secondary_soft(ctx, Some(v)).await?,
        StrategyState::EnlargeSecondaryBoth(v) => do_enlarge_secondary_soft(ctx, Some(v)).await?,
        StrategyState::TransferWalletToEE(v) => do_wallet_to_ee(ctx, Some(v)).await?,
        StrategyState::TransferEEToE2(v) => do_ee_to_e2(ctx, Some(v)).await?,
        StrategyState::Stake(v) => do_stake(ctx, Some(v)).await?,
        StrategyState::ReduceShort(v) => do_change_short(ctx, Some(-v)).await?,
        StrategyState::EnlargeShort(v) => do_change_short(ctx, Some(v)).await?, 
        //
        StrategyState::Unstake(v) => do_unstake(ctx, Some(v)).await?,
        StrategyState::TransferE2ToEE(v) => do_e2_to_ee(ctx, Some(v)).await?,
        StrategyState::TransferEEToWallet(v) => do_ee_to_wallet(ctx, Some(v)).await?,
        StrategyState::TransferWalletToE1(v) => do_wallet_to_e1(ctx, Some(v)).await?,
    }
    Ok((None, None))
}


pub async fn prepare(ctx: &mut Context) -> ActionResult {
    monitoring::E1_E2_RATIO_LOW.set(percent_to_decimal(ctx.consts.low_ratio_percent).to_f64().unwrap());
    monitoring::E1_E2_RATIO_HIGH.set(percent_to_decimal(ctx.consts.high_ratio_percent).to_f64().unwrap());
    info!(target: "NOTIFICATION", "STARTED\n{}", get_long_version_string()); 
    Ok(())
}


pub async fn strategy(ctx: &mut Context, is_monitoring_only: bool) -> ActionResult {
    prepare(ctx).await?;
    let mut state = None;
    loop {
        match timeout(Duration::from_secs(ctx.consts.operations_timeout), process_state(ctx, state, is_monitoring_only)).await.map_or_else(
                |err| Err (StrategyError::Misc { msg: format!("Timeout during processing state '{:?}': {:?}", state, err) }),
                |ok| ok) {
            Ok((new_state, delay)) => {
                match delay {
                    Some(delay) => sleep(delay).await,
                    None => (),
                };
                state = new_state;
            },
            Err(err) => return Err(err),
        }
    }
}


/// NOTE TODO this is for reduce short position on E1. (and *just trying* on EE/E2...)
///
///
/// Reduce short position in process of Wallet/EE rebalancing.
///
/// So we can face with big ATOMs disbalances. Due this,
/// we do not try to support strong balance here, relying
/// on other parts of strategy.
///
///
pub async fn do_reduce_secondary_soft(ctx: &mut Context, opt_amount: Option<PrimaryAsset>) -> ActionResult {
    let bal = update_balances(ctx, None).await?;
    let e1_price = get_token_price(ctx, WhichExchange::Exchange).await?.buy;
    let ee_price = get_token_price(ctx, WhichExchange::Exchange).await?.sell;
    let amount: PrimaryAsset = match opt_amount {
        Some(amount) => amount,
        _ => min(-bal.e1.operational_coins, bal.ee.operational_coins).to_prim(ee_price)
    };
    assert!(amount > ZERO_P);
    assert!(ctx.consts.use_binance_for_exchange);
    let amount_sec = amount.to_sec(e1_price).max(1.into());
    let e1_total = -bal.e1.operational_coins;
    let ee_e2_total = bal.ee.operational_coins + bal.e2.staked_coins + bal.e2.unstaked_coins;
    let ee_amount_sec_fixed = if ee_e2_total - e1_total >= (-1).into() {
        // EE+E2 have as much or more then E1 have.
        if bal.ee.operational_coins >= 1.into() {
            // If we have on EE enough assets, then reduce on both E1 ~ E2/EE
            let amount2_sec : SecondaryAsset = min(bal.ee.operational_coins, amount_sec);
            // If we have only some pennies after exchange, then sell everything
            let rest : SecondaryAsset = bal.ee.operational_coins - amount2_sec;
            if rest < 1.into() { bal.ee.operational_coins } else { amount2_sec }
        }
        else {
            ZERO_S
        }
    } else {
        ZERO_S
    };
    //
    // If we have only some pennies after exchange, then sell everything
    let rest = -bal.e1.operational_coins - amount_sec;
    let e1_amount_sec_fixed = (if rest < 1.into() { -bal.e1.operational_coins } else { amount_sec }).max(ONE_S * dec!(1.1)); // + small commission to not to sell just 1 ATOM TODO make passing amount in ATOMS to eliminate this rounding
    info!("{}, {}: decrease short position {} on {}; sell {} on {}",
          E1_NAME, EE_NAME,
          e1_amount_sec_fixed, E1_NAME,
          ee_amount_sec_fixed, EE_NAME);
    // TODO make parallel chaning
    if ee_amount_sec_fixed > ZERO_S {
        change_tokens_ex(ctx, WhichExchange::Exchange, Side::Sell, Asset::Secondary(ee_amount_sec_fixed)).await?;
    }
    if e1_amount_sec_fixed >= ONE_S {
        // TODO ??? sometimes here `long` position. Why??
        dydx_close_short_position(ctx, e1_amount_sec_fixed).await?;
    }
    Ok(())
}


pub async fn do_enlarge_secondary_soft(ctx: &mut Context, opt_amount: Option<PrimaryAsset>) -> ActionResult {
    assert!(ctx.consts.use_binance_for_exchange);
    let bal = update_balances(ctx, None).await?;
    let e1_price = get_token_price(ctx, WhichExchange::Exchange).await?.sell;
    let ee_price = get_token_price(ctx, WhichExchange::Exchange).await?.buy;
    let min_lot = (ONE_S * dec!(1.005)).to_prim(ee_price);
    let ee_amount: PrimaryAsset = match opt_amount {
        Some(amount) => amount,
        _ => bal.ee.transferring_coins,
    }.max(min_lot);
    let e1_amount: PrimaryAsset = min(ee_amount, bal.e1.free * dec!(5)); // TODO use leverage, not `5`
    info!("{}, {}: buy {} on {} (and increase short position on {}) ",
          E1_NAME, EE_NAME,
          ee_amount.to_sec(ee_price), EE_NAME,
          E1_NAME);
    let e1_ex_delta = (-bal.e1.operational_coins - (bal.ee.operational_coins + bal.e2.staked_coins + bal.e2.unstaked_coins)).to_prim(e1_price);
    // TODO if dYdX have no money, exception will rise. Make it work without money on dYdX
    if e1_ex_delta <= e1_amount {
        dydx_make_short_position(ctx, max(e1_amount.to_sec(e1_price), ONE_S)).await?;
    }
    change_tokens_ex(ctx, WhichExchange::Exchange, Side::Buy, Asset::Primary(ee_amount)).await
}


pub async fn do_e1_to_wallet(ctx: &mut Context, opt_from_e1_amount: Option<PrimaryAsset>) -> ActionResult {
    let from_e1_amount = match opt_from_e1_amount {
        Some(amount) => amount,
        _ => {
            let bal = update_balances(ctx, Some(WhichExchange::First)).await?;
            bal.e1.free
            // TODO
            //warn!("Wrong ctx.margin_event = {:?}! Suppose withdraw all free {}s ({} {})",
            //      ctx.margin_event, TRANSFERRING_COIN, amount, TRANSFERRING_COIN);
        }
    };
    // TODO make it work with large `credit_amount`
    info!("{}: processing 'margin overflow': withdraw {} {} to {}", E1_NAME, from_e1_amount, TRANSFERRING_COIN, WALLET_NAME);
    internal_do_withdraw(ctx, from_e1_amount, WhichExchange::First).await
}


pub async fn do_wallet_to_e1(ctx: &mut Context, opt_from_wallet_amount: Option<PrimaryAsset>) -> ActionResult {
    let from_wallet_amount: PrimaryAsset = match opt_from_wallet_amount {
        Some(amount) => {
            let to_e1_amount_min = amount * dec!(0.9);
            let to_e1_amount_max = amount * dec!(1.1);
            let bal = update_balances(ctx, Some(WhichExchange::Wallet)).await?;
            if bal.wallet.transferring_coins < to_e1_amount_min {
                todo!("Too low transferring coins: {} {}, expected at least {} {}",
                      bal.wallet.transferring_coins, TRANSFERRING_COIN,
                      to_e1_amount_min, TRANSFERRING_COIN);
            }
            else if bal.wallet.transferring_coins > to_e1_amount_max {
                amount
            }
            else {
                bal.wallet.transferring_coins
           }
        },
        _ => {
            let bal = update_balances(ctx, Some(WhichExchange::Wallet)).await?;
            bal.wallet.transferring_coins
        }
    };
    info!("{}: transfer {} to {}", WALLET_NAME, from_wallet_amount, E1_NAME);
    internal_do_e1_deposit(ctx, from_wallet_amount).await
}


pub async fn do_wallet_to_ee(ctx: &mut Context, opt_from_wallet_amount: Option<PrimaryAsset>) -> ActionResult {
    let from_wallet_amount = match opt_from_wallet_amount {
        Some(amount) => amount,
        _ => update_balances(ctx, Some(WhichExchange::Wallet)).await?.wallet.transferring_coins,
    };
    info!("{}: transfer {} to {}", WALLET_NAME, from_wallet_amount, EE_NAME);
    internal_do_deposit(ctx, WhichExchange::Exchange, from_wallet_amount).await
}



pub async fn do_ee_to_wallet(ctx: &mut Context, opt_to_wallet_amount: Option<PrimaryAsset>) -> ActionResult {
    let to_wallet_amount = match opt_to_wallet_amount {
        Some(amount) => amount,
        _ => update_balances(ctx, Some(WhichExchange::Exchange)).await?.e2.transferring_coins,
    };
    info!("{}: transfer {} to {}", EE_NAME, to_wallet_amount, WALLET_NAME);
    internal_do_withdraw(ctx, to_wallet_amount, WhichExchange::Exchange).await
}


pub async fn do_ee_to_e2(ctx: &mut Context, opt_amount: Option<SecondaryAsset>) -> ActionResult {
    let amount = match opt_amount {
        Some(v) => v,
        _ => {
            let bal = update_balances(ctx, Some(WhichExchange::Exchange)).await?;
            bal.ee.operational_coins
        }
    };
    info!("{}: processing 'margin overflow': deposit {} to {}", EE_NAME, amount, E2_NAME);
    internal_do_ee_to_e2_deposit_operating(ctx, amount).await
}


pub async fn do_e2_to_ee(ctx: &mut Context, opt_amount: Option<SecondaryAsset>) -> ActionResult {
    let amount = match opt_amount {
        Some(v) => v,
        _ => update_balances(ctx, Some(WhichExchange::Exchange)).await?.e2.unstaked_coins,
    };
    info!("{}: transfer {} to {}", E1_NAME, amount, E2_NAME);
    internal_do_e2_to_ee_deposit_operating(ctx, amount).await
}


/// Stake some assets
///
pub async fn do_stake(ctx: &mut Context, opt_amount: Option<SecondaryAsset>) -> ActionResult {
    let amount = match opt_amount {
        Some(v) => v,
        _ => update_balances(ctx, Some(WhichExchange::Second)).await?.e2.unstaked_coins,
    };
    info!("{}: stake {}", E2_NAME, amount);
    stake_unstake_impl(ctx, true, Some(amount)).await
}


/// Unstake some assets
///
pub async fn do_unstake(ctx: &mut Context, opt_amount: Option<SecondaryAsset>) -> ActionResult {
    let amount = match opt_amount {
        Some(v) => v,
        _ => update_balances(ctx, Some(WhichExchange::Exchange)).await?.e2.staked_coins,
    };
    info!("{}: unstake {}", E2_NAME, amount);
    stake_unstake_impl(ctx, false, Some(amount)).await
}


pub async fn do_change_short(ctx: &mut Context, opt_amount: Option<SecondaryAsset>) -> ActionResult {
    let amount = match opt_amount {
        Some(v) => v,
        _ => {
            let bal = update_balances(ctx, None).await?;
            -bal.e1.operational_coins - bal.e2.staked_coins
        }
    };
    if amount > ZERO_S {
        info!("{}: I'm going to enlarge short position on {}", E1_NAME, amount);
        dydx_make_short_position(ctx, amount).await
    } else {
        info!("{}: I'm going to reduce short position on {}", E1_NAME, -amount);
        dydx_close_short_position(ctx, -amount).await
    }
}


// ---- Debug actions:
//

fn show_balances(bal: Balances) {
    println!("e1: total        = {:.8}", bal.e1.total);
    println!("    free         = {:.8}", bal.e1.free);
    println!("    operational  = {:.8}", bal.e1.operational_coins);
    println!("ee: transferring = {:.8}", bal.ee.transferring_coins);
    println!("    operational  = {:.8}", bal.ee.operational_coins);
    println!("e2: transferring = {:.8}", bal.e2.transferring_coins);
    println!("    intermediate = {:.8}", bal.e2.intermediate_coins);
    println!("    unstaked     = {:.8}", bal.e2.unstaked_coins);
    println!("    staked       = {:.8}", bal.e2.staked_coins);
    println!("wl: transferring = {:.8}", bal.wallet.transferring_coins);
    println!("    gas          = {:.8} ETH", bal.wallet.gas_coins);
}

/// Show balances
///
pub async fn do_debug_show_balances(ctx: &mut Context) -> ActionResult {
    show_balances(update_balances(ctx, None).await?);
    Ok(())
}


/// Show state
///
pub async fn do_debug_show_state(ctx: &mut Context) -> ActionResult {
    show_balances(update_balances(ctx, None).await?);
    println!("--------------------------------------------");
    let state = detect_current_state(ctx, false).await?;
    println!("=== Current state: {} ({:?}) ===", state.get_message().unwrap(), state);
    Ok(())
}


fn notify_state(state: StrategyState, msg: String) -> StrategyResult<StrategyState> {
    let whole_msg = if state == StrategyState::Monitoring { msg } else { format!("{}\n\n{}", msg, state.get_message().unwrap_or("?")) };
    info!(target: "NOTIFICATION", "{}", whole_msg);
    Ok(state)
}


/// Detects current exchange state to run or continue state machine working.
///
async fn detect_current_state(ctx: &mut Context, is_monitoring_only: bool) -> Result<StrategyState, StrategyError> {
    //return Ok(StrategyState::Monitoring);
    let bal = update_balances(ctx, None).await?;
    // TODO make getting pricess in parallel
    let e1_price = get_token_price(ctx, WhichExchange::First).await?;
    let e2_price = get_token_price(ctx, WhichExchange::Second).await?;
    //let ee_price = get_token_price(ctx, WhichExchange::Exchange).await?;
    let ee_price = get_token_price(ctx, WhichExchange::Exchange).await?;
    let e1_total = bal.e1.total;
    let e2_total = bal.e2.transferring_coins + bal.e2.intermediate_coins + (bal.e2.staked_coins + bal.e2.unstaked_coins).to_prim(e2_price.sell);
    let ee_total = bal.ee.transferring_coins + bal.ee.operational_coins.to_prim(e2_price.sell);
    let wallet_total = bal.wallet.transferring_coins;
    let ew_total = wallet_total + ee_total;
    let total = e1_total + e2_total + ee_total + wallet_total;
    let e1_e2_ratio = *e1_total / *e2_total;
    let e1_ex_ratio = *(e1_total + wallet_total) / *(e2_total + ee_total);
    let high_ratio = percent_to_decimal(ctx.consts.high_ratio_percent);
    let init_ratio = percent_to_decimal(ctx.consts.initial_ratio_percent);
    let low_ratio = percent_to_decimal(ctx.consts.low_ratio_percent);
    info!("Total: {:.2} ({}: {:.2}, {}: {:.2}, {}: {:.2}, {}: {:.2} (and gas: {:.6} {})), ratio: {:.4}",
          total,
          E1_NAME, e1_total,
          E2_NAME, e2_total,
          EE_NAME, ee_total,
          WALLET_NAME, wallet_total,
          bal.wallet.gas_coins, GAS_COIN,
          e1_e2_ratio);
    let aprice = (avg_price(&e1_price) + avg_price(&ee_price) + avg_price(&e2_price)) / dec!(3);
    info!("Avg. price: {:.4} {}/{}",
          aprice, OPERATING_COIN, TRANSFERRING_COIN);
    debug!("Ratio: {}", e1_e2_ratio);
    debug!("Ratio with {},{}: {}", WALLET_NAME, EE_NAME, e1_ex_ratio);
    // Notification:
    let notify_ratio =
        if e1_e2_ratio < low_ratio { format!("{:.4} ({0:.4} < {:.4}~{:.4})", e1_e2_ratio, low_ratio, high_ratio) }
        else if e1_e2_ratio > high_ratio { format!("{:.4} ({:.4}~{:.4} > {0:.4})", e1_e2_ratio, low_ratio, high_ratio) }
        else { format!("{:.4} < {:.4} < {:.4}", low_ratio, e1_e2_ratio, high_ratio) };
    let monitoring_msg = if is_monitoring_only { "(only monitoring)\n" } else { "" }; 
    let notify_message = format!(
        "Ratio: {}\n\
         \n\
         {}Total: {:.2}\n\
         {}: {:.2} (incl. {:.2}),\n\
         {}: {:.2} (incl. {:.2})\n\
         {}: {:.2} (incl. {:.2})\n\
         {}: {:.2} (gas: {:.6} {})\n\
         \n\
         Price: {:.4} {}/{}",
         notify_ratio,
         //
         monitoring_msg,
         total,
         E1_NAME, e1_total, -bal.e1.operational_coins,
         E2_NAME, e2_total, (bal.e2.staked_coins + bal.e2.unstaked_coins),
         EE_NAME, ee_total, bal.ee.operational_coins,
         WALLET_NAME, wallet_total, bal.wallet.gas_coins, GAS_COIN,
         aprice, OPERATING_COIN, TRANSFERRING_COIN);
    monitoring::WALLET_BALANCE_TOTAL.set(bal.wallet.transferring_coins.to_f64().unwrap());
    monitoring::WALLET_BALANCE_GAS.set(bal.wallet.gas_coins.to_f64().unwrap());
    monitoring::E1_BALANCE_TOTAL.set(e1_total.to_f64().unwrap());
    monitoring::E1_BALANCE_FREE.set(bal.e1.free.to_f64().unwrap());
    monitoring::E1_BALANCE_SHORTED.set(bal.e1.operational_coins.to_f64().unwrap());
    monitoring::E2_BALANCE_TOTAL.set(e2_total.to_f64().unwrap());
    monitoring::E2_BALANCE_TRANSFERRING.set(bal.e2.transferring_coins.to_f64().unwrap());
    monitoring::E2_BALANCE_INTERMEDIATE.set(bal.e2.intermediate_coins.to_f64().unwrap());
    monitoring::E2_BALANCE_UNSTAKED.set(bal.e2.unstaked_coins.to_f64().unwrap());
    monitoring::E2_BALANCE_STAKED.set(bal.e2.staked_coins.to_f64().unwrap());
    monitoring::EE_BALANCE_TOTAL.set(ee_total.to_f64().unwrap());
    monitoring::EE_BALANCE_TRANSFERRING.set(bal.ee.transferring_coins.to_f64().unwrap());
    monitoring::EE_BALANCE_OPERATIONAL.set(bal.ee.operational_coins.to_f64().unwrap());
    monitoring::E1_E2_RATIO.set(e1_e2_ratio.to_f64().unwrap());
    monitoring::PRICE.set(avg_price(&e1_price).to_f64().unwrap());
    //The limit of funds on the account, below which the strategy will consider the account as zero.
    //
    // When converting or depositing/withdrawing coins, small amounts may remain in the account;
    // the strategy will consider them as null if they do not exceed this threshold.
    const EE_MIN_PRIMARY_WITHDRAW: PrimaryAsset = PrimaryAsset(dec!(50)); // TODO 
    let transferring_coins_min_limit = std::cmp::max(EE_MIN_PRIMARY_WITHDRAW, SecondaryAsset(dec!(2)).to_prim(ee_price.sell));
    //let staking_coins_min_limit = std::cmp::max(ONE, dec!(20) / e2_price); // (total / e2_price) * dec!(0.03);
    //let operational_coins_min_limit = PrimaryAsset(dec!(1.01) * e2_price.sell); // 1.01 -- with small commission
    let use_binance_for_exchange = ctx.consts.use_binance_for_exchange;

    // TODO
    // const MIN_SECONDARY : SecondaryAsset = SecondaryAsset(dec!(1));

    //let expected_e1_total_high = (total * high_ratio) / (ONE + high_ratio);
    let expected_e1_total = (total * init_ratio) / (ONE + init_ratio);
    let expected_e1_total_low = (total * low_ratio) / (ONE + low_ratio);

    let expected_e2_total_low = total / (high_ratio + ONE);

    let e1_lack = expected_e1_total - e1_total;
    let e1_excess = -e1_lack;

    const TRANSFER_COMMISSION: Value = dec!(0.005); // expected commission on transfers to simplify calculations
    const OPERATING_COIN_MIN: SecondaryAsset = ONE_S; // minimal amount for buy/sell
    const EE_TO_WALLET_MIN: PrimaryAsset = PrimaryAsset(dec!(50)); // minimal transferrable amount
    const BUY_SELL_COMMISSION: Value = dec!(0.005);

    const TRANSFERS_TOLERANCE: Value = dec!(0.10);

    let wallet_transferable = bal.wallet.transferring_coins;
    let ee_prim_transferable = if bal.ee.transferring_coins >= EE_TO_WALLET_MIN { bal.ee.transferring_coins * (ONE - TRANSFER_COMMISSION) } else { ZERO_P };
    let ee_exchangable = if bal.ee.operational_coins >= OPERATING_COIN_MIN { bal.ee.operational_coins.to_prim(ee_price.sell) * (ONE -  BUY_SELL_COMMISSION) }  else { ZERO_P };
    let ee_total_transferable_pre = ee_prim_transferable + ee_exchangable * (ONE - TRANSFER_COMMISSION);
    let ee_total_transferable = if ee_total_transferable_pre >= EE_TO_WALLET_MIN { ee_total_transferable_pre } else { ZERO_P };
    let eew_total_transferable = wallet_transferable + ee_total_transferable;

    debug!("e1_total = {}", e1_total);
    debug!("expected_e1_total = {}", expected_e1_total);
    debug!("wallet_transferable = {}", wallet_transferable);
    debug!("ee_prim_transferable = {}", ee_prim_transferable);
    debug!("ee_exchangable = {}", ee_exchangable);
    debug!("ee_total_transferable_pre = {}", ee_total_transferable_pre);
    debug!("ee_total_transferable = {}", ee_total_transferable);
    debug!("eew_total_transferable = {}", eew_total_transferable);

    // Wallet ---> EE/E2
    //
    // Funds on Wallet in 'ATOM', which could be transferrable to EE (or `0`, if it's not profitable)
    let wallet_transferable_to_sec =
        if bal.wallet.transferring_coins >= transferring_coins_min_limit
            { bal.wallet.transferring_coins.to_sec(ee_price.buy) * (ONE - BUY_SELL_COMMISSION) }
        else
            { ZERO_S }; 
    // Funds on Wallet and EE in `ATOM`, which could be transferrable (or `0`, if it's not profitable)
    let ee_wallet_exchangable_to_sec = {
        let ee_prim_exchangable_pre = bal.ee.transferring_coins.to_sec(ee_price.buy) * (ONE - BUY_SELL_COMMISSION);
        let ee_wallet_exchangable_to_sec_pre = wallet_transferable_to_sec + ee_prim_exchangable_pre;
        if ee_wallet_exchangable_to_sec_pre >= ONE_S { ee_wallet_exchangable_to_sec_pre } else { ZERO_S }
    };
    let ee_total_transferable_to_sec = ee_wallet_exchangable_to_sec + bal.ee.operational_coins;

    debug!("wallet_transferable_to_sec = {}", wallet_transferable_to_sec);
    debug!("ee_wallet_exchangable_to_sec = {}", ee_wallet_exchangable_to_sec);
    debug!("ee_total_transferable_to_sec = {}", ee_total_transferable_to_sec);

    let e1_delta = min(ew_total, expected_e1_total - e1_total);
    let e1_delta_with_tol = e1_delta * (ONE + TRANSFERS_TOLERANCE);
    debug!("e1_delta = {}", e1_delta);
    debug!("e1_delta_with_tol = {}", e1_delta_with_tol);
    debug!("eew_total_transferable = {}", eew_total_transferable);

    assert!(use_binance_for_exchange);

    if eew_total_transferable > transferring_coins_min_limit {
        // Wallet and/or EE have funds to redistribution.
        // So it's these that are to be redistributed first.
        if e1_total + ew_total >= expected_e1_total_low && e2_total + ew_total >= expected_e2_total_low {
            // It is need to redistribute funds from Wallet & EE
            //
            debug!("It's seems {} ({}) and {} ({}) only need for rebalancing", WALLET_NAME, wallet_total, EE_NAME, ee_total);
            //
            // Determine the order in which the funds will be transferred.
            //
            // It may be that the entire amount needed will be in the wallet, on the EE in USDC and/or ATOM.
            //
            // In this case it is necessary to make less transfers, so as not to pay commission.
            // In addition, EE has restrictions on the amount you can transfer, you have to take them into account when transferring.
            //
            // And it may be so that there is a lot of money (from 1M), and then due to fluctuations
            // of price after operation completion there may be some amounts on accounts (or there may be
            // insufficient funds). Example:
            //
            // 1. We transfer from E1 to Wallet 10'000 USDC.
            // 2. The price is going up, so E1 needs more coverage, and E2 needs less coverage.
            //    transfer, for example, not 10'000 USDC, but only 9'900 USDC.
            // 3. As a result, after transferring to E2 on Wallet remains 100 USDC, which earlier versions of
            //    versions of the strategy threw back to E1, thereby wasting commission.
            //
            // Calculate the expected relationship before the transfer.
            //
            // If one of them turns out to be within, we can assume that the strategy
            // had previously transferred funds to a wallet in that direction.
            //
            // Either both will be in bounds, or both will be out of bounds, in which case the replenishment
            // wallet, in which case the direction is not important. Or the price jumped so much
            // that you can't figure out the previous state, in which case the direction doesn't matter either.
            // 
            let supposed_prev_ratio_if_to_e1 = *(e1_total + ew_total) / *e2_total;
            let supposed_prev_ratio_if_to_e2 = *e1_total / *(e2_total + ew_total);
            debug!("supposed_prev_ratio_if_to_e1 = {}", supposed_prev_ratio_if_to_e1);
            debug!("supposed_prev_ratio_if_to_e2 = {}", supposed_prev_ratio_if_to_e2);
            let ratio_range = low_ratio..high_ratio;
            let is_transfer_to_e1 =
                (ratio_range.contains(&supposed_prev_ratio_if_to_e1) ^ ratio_range.contains(&supposed_prev_ratio_if_to_e2))
                &&
                ratio_range.contains(&supposed_prev_ratio_if_to_e1)
                ;
            let is_transfer_to_e2 =
                (ratio_range.contains(&supposed_prev_ratio_if_to_e1) ^ ratio_range.contains(&supposed_prev_ratio_if_to_e2))
                &&
                ratio_range.contains(&supposed_prev_ratio_if_to_e2)
                ;
            debug!("is_transfer_to_e2 = {}", is_transfer_to_e2);
            debug!("is_transfer_to_e1 = {}", is_transfer_to_e1);

            if (! is_transfer_to_e2) && (e1_delta >= transferring_coins_min_limit && eew_total_transferable >= transferring_coins_min_limit) {
                // To E1 it is necessary to transfer a part of funds from the Wallet/EE
                if eew_total_transferable < e1_delta {
                    // There are funds in the wallet, but not enough to cover e1_delta, so transfer what we can.
                    if bal.ee.operational_coins >= OPERATING_COIN_MIN {
                        let to_exchange = bal.ee.operational_coins;
                        info!("{}: I'm going to reduce {} ({}) to deposit {}", EE_NAME, to_exchange.to_prim(ee_price.sell), to_exchange, E1_NAME);
                        return notify_state(StrategyState::ReduceSecondaryBoth(get_single_order_size(ctx, to_exchange.to_prim(ee_price.sell))), notify_message);
                    }
                    else if bal.ee.transferring_coins >= EE_TO_WALLET_MIN {
                        info!("{}: I'm going to transfer {} to {} to deposit {}", EE_NAME, bal.ee.transferring_coins, WALLET_NAME, E1_NAME);
                        return notify_state(StrategyState::TransferEEToWallet(bal.ee.transferring_coins), notify_message);
                    }
                    else if bal.wallet.transferring_coins >= transferring_coins_min_limit  {
                        info!("{}: I'm going to transfer {} to {}", WALLET_NAME, bal.wallet.transferring_coins, E1_NAME);
                        return notify_state(StrategyState::TransferWalletToE1(bal.wallet.transferring_coins), notify_message);
                    }
                    else {
                        // Nothing can be transferred.
                        // Continued below.
                    }
                }
                else {
                    // There's plenty of money in the wallet, so you only have to transfer part of it.
                    if wallet_transferable >= e1_delta {
                        // Only the Wallet will suffice.
                        let to_transfer = min(bal.wallet.transferring_coins, e1_delta_with_tol * (ONE + TRANSFERS_TOLERANCE)); // here is another increase, because the price may not have changed at the previous steps, as a result the same amount will come
                        info!("{}: I'm going to transfer {} to {}", WALLET_NAME, to_transfer, E1_NAME);
                        return notify_state(StrategyState::TransferWalletToE1(to_transfer), notify_message);
                    }
                    else {
                        let price = ee_price.sell;
                        let to_transfer =  EE_TO_WALLET_MIN.max(e1_delta_with_tol - bal.wallet.transferring_coins);
                        let to_exchange0 = to_transfer - bal.ee.transferring_coins;
                        let to_exchange =
                            if to_transfer * (ONE + TRANSFERS_TOLERANCE) >= bal.ee.operational_coins.to_prim(price) + bal.ee.transferring_coins {
                                bal.ee.operational_coins.to_prim(price)
                            }
                            else {
                                to_exchange0.max(ONE_S.to_prim(ee_price.sell))
                            };
                        debug!("{}: price = {}", EE_NAME, price);
                        debug!("{}: to_transfer = {}", EE_NAME, to_transfer);
                        debug!("{}: to_exchange0 = {}", EE_NAME, to_exchange0);
                        debug!("{}: bal.ee.operational_coins.to_prim(price) = {}", EE_NAME, bal.ee.operational_coins.to_prim(price));
                        debug!("{}: to_exchange = {}", EE_NAME, to_exchange);
                        if to_exchange > ONE_S.to_prim(price) { // after the sale may remain pennies of ATOMs -- don't transfer it
                            // Exchange ATOMs first
                            info!("{}: I'm going to reduce {} ({}) to deposit {}", EE_NAME, to_exchange.to_sec(ee_price.sell), to_exchange, E1_NAME);
                            return notify_state(StrategyState::ReduceSecondaryBoth(get_single_order_size(ctx, to_exchange)), notify_message);
                        }
                        else {
                            // Now to withdraw all that was exchange
                            let to_cover_e1_delta = e1_delta_with_tol - bal.wallet.transferring_coins;
                            assert!(to_cover_e1_delta > ZERO_P);
                            let to_transfer_pre = to_cover_e1_delta.max(EE_TO_WALLET_MIN).min(bal.ee.transferring_coins);
                            let to_transfer =
                                // If there are pennies left after the transfer, then transfer everything at all.
                                if bal.ee.transferring_coins - to_transfer_pre * (ONE + TRANSFERS_TOLERANCE) < ONE_S.to_prim(ee_price.buy)
                                { bal.ee.transferring_coins }
                                else
                                { to_transfer_pre };

                            debug!("{}: to_cover_e1_delta = {}", EE_NAME, to_cover_e1_delta);
                            debug!("{}: to_transfer_pre = {}", EE_NAME, to_transfer_pre);
                            debug!("{}: bal.ee.transferring_coins = {}", EE_NAME, bal.ee.transferring_coins);
                            debug!("{}: to_transfer = {}", EE_NAME, to_transfer);

                            info!("{}: I'm going to transfer {} to {} (for deposit {})", EE_NAME, to_transfer, WALLET_NAME, E1_NAME);
                            return notify_state(StrategyState::TransferEEToWallet(to_transfer), notify_message);
                        }
                    }
                }
            }
            else {
                // Either there is nothing to transfer to e1, or there is not enough money in wallets (not profitable to transfer).
                // In this case you can calculate another direction of transfer - for example, to top up Wallet -> EE -> E2
                if ee_total_transferable_to_sec >= ONE_S {
                    if bal.wallet.transferring_coins >= transferring_coins_min_limit {
                        info!("{}: I'm going to transfer {} to {} to deposit on {}", WALLET_NAME, bal.wallet.transferring_coins, EE_NAME, E2_NAME);
                        return notify_state(StrategyState::TransferWalletToEE(bal.wallet.transferring_coins), notify_message);
                    }
                    else if bal.ee.transferring_coins.to_sec(ee_price.buy) >= ONE_S {
                        info!("{}: I'm going to change {} to {} to deposit on {}", EE_NAME, bal.ee.transferring_coins, bal.ee.transferring_coins.to_sec(ee_price.buy), E2_NAME);
                        return notify_state(StrategyState::EnlargeSecondaryBoth(get_single_order_size(ctx, bal.ee.transferring_coins)), notify_message);
                    }
                    else if bal.ee.operational_coins >= ONE_S {
                        info!("{}: I'm going to transfer {} to deposit {}", EE_NAME, bal.ee.operational_coins, E2_NAME);
                        return notify_state(StrategyState::TransferEEToE2(bal.ee.operational_coins), notify_message);
                    }
                    else {
                        // Nothing can be transferred.
                        // Continued below.
                    }
                }
                else {
                    // Nothing can be transferred.
                    // Continued below.
                }
            }
        }
        else {
            // It is need to transfer some funds from one of account
            if e1_excess > transferring_coins_min_limit {
                // Too many funds on E1, so transfer to
                debug!("Too many ({} > {}, delta: {}) funds on {}", e1_total, expected_e1_total, e1_excess, E1_NAME);
                const MAX_LEVERAGE : Value = dec!(8); // TODO
                //let total_after_withdraw = bal.e1.total - e1_excess * dec!(1.05); // + commission
                //let leverage_after_withdraw = *sec_to_prim((-bal.e1.operational_coins).into(), e1_price) / total_after_withdraw;
                //debug!("{}: total_after_withdraw = {}, leverage_after_withdraw = {}", E1_NAME, total_after_withdraw, leverage_after_withdraw);
                let shorting_to_cancel : PrimaryAsset = (-bal.e1.operational_coins).to_prim(e1_price.buy) - (bal.e1.total - e1_excess) * MAX_LEVERAGE * (ONE + TRANSFERS_TOLERANCE);
                debug!("{}: shorting_to_cancel = {}", E1_NAME, shorting_to_cancel);
                if shorting_to_cancel <= ZERO.into() {
                    return notify_state(StrategyState::TransferE1ToWallet(e1_excess), notify_message);
                }
                else {
                    // There is no enough USDC, so sell some ATOMs
                    let shorting_to_cancel_fixed = max(shorting_to_cancel, (ONE_S * (ONE + TRANSFERS_TOLERANCE)).to_prim(e1_price.buy));
                    debug!("{}: I'm going to sell {} ({}) for make withdraw {} ({:.2} -> {:.2})",
                        E1_NAME, shorting_to_cancel_fixed.to_sec(e1_price.buy), shorting_to_cancel_fixed, e1_excess, bal.e1.total, expected_e1_total);
                    return notify_state(StrategyState::ReduceSecondaryBoth(get_single_order_size(ctx, shorting_to_cancel_fixed)), notify_message);
                }
            }
            else if e1_lack > transferring_coins_min_limit {
                // There is too little on E1, it is need to transfer from Wallet+EE+E2.
                // At the same time on EE may be too much, so it is need to transfer only part of it.
                // In this case, none of the accounts may not have the right amount of money.
                if wallet_total - e1_lack >= -transferring_coins_min_limit {
                    // There is enough money on one wallet.
                    // At that, the price can change during the transfer and it may turn out that there is
                    // *slightly* not enough money on the wallet. If this "slightly" is too little for a normal
                    // transfer, we transfer what we have, in which case the balance of E1/E2 will still be sufficient.
                    //
                    let amount = min(e1_lack, wallet_total) * (ONE + TRANSFERS_TOLERANCE);
                    return notify_state(StrategyState::TransferWalletToE1(amount), notify_message);
                }
                else {
                    // It is need to transfer some funds from EE+Wallet
                    if ee_total + wallet_total - e1_lack >= -transferring_coins_min_limit {
                        // At first, transfer from EE
                        let ee_delta = max((e1_lack - wallet_total)*(ONE + TRANSFER_COMMISSION), EE_TO_WALLET_MIN);
                        if (ee_delta <= bal.ee.transferring_coins) || (bal.ee.operational_coins < ONE_S)  {
                            // There is enought USDC
                            let amount = min(ee_delta, bal.ee.transferring_coins);
                            return notify_state(StrategyState::TransferEEToWallet(amount), notify_message);
                        }
                        else {
                            // There is no enough USDC, exchange ATOM to USDC
                            let ee_delta_to_conv_sec = min(bal.ee.operational_coins, (ee_delta - bal.ee.transferring_coins).to_sec(ee_price.sell));
                            debug!("{}: I'm going to sell (part of) {} for withdraw ({:.2} -> {:.2})",
                                EE_NAME, ee_delta_to_conv_sec, bal.e1.total, expected_e1_total);
                            return notify_state(StrategyState::ReduceSecondaryBoth(get_single_order_size(ctx, ee_delta_to_conv_sec.to_prim(ee_price.sell))), notify_message);
                        }
                    }
                    else {
                        // Nothing can be transferred.
                        // Continued below.
                    }
                }
            }
            else {
                // Nothing can be transferred.
                // Continued below.
            }
        }
    }

    // All options for processing the wallet worked previously, below are operations with E1 & E2.
    // Check the balance and, if necessary, transfer.
    // For correction `e1_ex_ratio` is used to take into account pennies on wallet and EE.

    if e1_ex_ratio < low_ratio {
        // E2 have too much funds, transfer some to E1 (with a small surplus to compensate for the change in price at the time of transfer).
        let delta_prim0 = expected_e1_total - e1_total;
        let delta_prim = delta_prim0 * (ONE + TRANSFER_COMMISSION)   // commission for transfer from E2 to EE
                                     * (ONE + BUY_SELL_COMMISSION)   // commission for exchange
                                     * (ONE + TRANSFER_COMMISSION)   // commission for transfer from EE to WALLET
                                     * (ONE + TRANSFERS_TOLERANCE);  // "insurance" for changing price during this transfer
        let delta = delta_prim.to_sec(ee_price.sell);
        let to_unstake = delta - (bal.e2.unstaked_coins + bal.ee.operational_coins);

        debug!("{}: staked = {}", E2_NAME, bal.e2.staked_coins);
        debug!("{}: unstaked = {}", E2_NAME, bal.e2.unstaked_coins);
        debug!("{}: delta_prim0 = {}", E2_NAME, delta_prim0);
        debug!("{}: delta_prim = {}", E2_NAME, delta_prim);
        debug!("{}: delta = {}", E2_NAME, delta);
        debug!("{}: to_unstake = {}", E2_NAME, to_unstake);


        if to_unstake < ONE_S / dec!(3)  { // no need to pay attention on small changes
            // If there are kopecks, too, transfer them, because then they can not be used in any way.
            let to_transfer_pre = min(delta, bal.e2.unstaked_coins);
            let to_transfer =
                if bal.e2.unstaked_coins - to_transfer_pre * (ONE + TRANSFERS_TOLERANCE) < ONE_S
                { bal.e2.unstaked_coins }
                else
                { to_transfer_pre };
            debug!("{}: to_transfer = {}", E2_NAME, to_transfer);
            info!("{}: I'm going to transfer {} from {} to {} to raise ratio from {:.4} to {:.4}",
                   E2_NAME, to_transfer, E2_NAME, E1_NAME, e1_ex_ratio, init_ratio);
            notify_state(StrategyState::TransferE2ToEE(to_transfer), notify_message)
        }
        else {
            // If some pennies remain, then unstake it.
            let to_unstake_fix0 = max(ONE_S, to_unstake);
            let to_unstake_fix1 = if bal.e2.staked_coins - to_unstake_fix0 < ONE_S { bal.e2.staked_coins } else { to_unstake_fix0 };
            let to_unstake_fix = max(ONE_S, to_unstake_fix1);
            debug!("{}: to_unstake_fix1 = {}", E2_NAME, to_unstake_fix1);
            debug!("{}: to_unstake_fix = {}", E2_NAME, to_unstake_fix);
            info!("{}: I'm going to unstake {} from {} to {} to raise ratio from {:.4} to {:.4}", E2_NAME, to_unstake_fix, E2_NAME, E1_NAME, e1_ex_ratio, init_ratio);
            notify_state(StrategyState::Unstake(to_unstake_fix), notify_message)
        }
    }
    else if e1_ex_ratio > high_ratio {
        // There is too much funds on E1, transfer some to E2.
        // It is enough just to make a withdraw, then the upper code will figure it out.
        let delta0 = max(transferring_coins_min_limit,
                         (e1_total - e2_total * init_ratio) / (ONE + init_ratio));
        let delta = delta0 * (ONE + TRANSFER_COMMISSION)   // commission for withdraw from dYdX
                           * (ONE + BUY_SELL_COMMISSION)   // commission for exchange ATOM on EE
                           * (ONE + TRANSFER_COMMISSION)   // commission for transfer to E2
                           * (ONE + TRANSFERS_TOLERANCE);  // "insurance" for changing price during this transfer
        debug!("{}: I'm going to transfer {} from {} to {} to reduce ratio from {:.4} to {:.4}",
               E1_NAME, delta, E1_NAME, E2_NAME, e1_ex_ratio, init_ratio);
        notify_state(StrategyState::TransferE1ToWallet(delta), notify_message)
    }
    else {
        // The ratio is within the given limits, check stake/short.
        // But there may be some trace amounts of ATOM on EE, unstaked, etc., so this balance must be taken into account when calculating
        assert!(bal.e2.transferring_coins < transferring_coins_min_limit);
        assert!(bal.e2.intermediate_coins < transferring_coins_min_limit);
        let short_stake_delta = -bal.e1.operational_coins - (bal.ee.operational_coins + bal.e2.staked_coins - bal.e2.unstaked_coins);
        if bal.e2.unstaked_coins >= ONE_S {
            notify_state(StrategyState::Stake(bal.e2.unstaked_coins), notify_message)
        }
        else if short_stake_delta > ONE_S {
            // It is need to reduce short position
            notify_state(StrategyState::ReduceShort(get_single_order_size_sec(ctx, short_stake_delta, ee_price.buy)), notify_message)
        }
        else if short_stake_delta < -ONE_S {
            // It is need to enlarge short position
            notify_state(StrategyState::EnlargeShort(get_single_order_size_sec(ctx, -short_stake_delta, ee_price.sell)), notify_message)
        }
        else {
            // Just monitoring...
            info!("Current {} / {} ratio: {:.4} < {:.4} < {:.4}, continue monitoring...", E1_NAME, E2_NAME,
                low_ratio, e1_e2_ratio, high_ratio);
            notify_state(StrategyState::Monitoring, notify_message)
        }
    }
}


// TODO add more operations to debug!

/// Run action by its name
///
pub async fn run_action_by_name(action_name: String, ctx: &mut Context, additional_value: Option<Value>) -> ActionResult {
    // NOTE this function is autogenerated by "update-action-names.hs" script
    let d = || debug!("Run \"{}\"", action_name);
    match action_name.as_str() {
        "do_change_short" => { d(); do_change_short(ctx, additional_value.map(|v| v.into())).await }
        "do_debug_show_balances" => { d(); do_debug_show_balances(ctx).await }
        "do_debug_show_state" => { d(); do_debug_show_state(ctx).await }
        "do_e1_to_wallet" => { d(); do_e1_to_wallet(ctx, additional_value.map(|v| v.into())).await }
        "do_e2_to_ee" => { d(); do_e2_to_ee(ctx, additional_value.map(|v| v.into())).await }
        "do_ee_to_e2" => { d(); do_ee_to_e2(ctx, additional_value.map(|v| v.into())).await }
        "do_ee_to_wallet" => { d(); do_ee_to_wallet(ctx, additional_value.map(|v| v.into())).await }
        "do_stake" => { d(); do_stake(ctx, additional_value.map(|v| v.into())).await }
        "do_unstake" => { d(); do_unstake(ctx, additional_value.map(|v| v.into())).await }
        "do_wallet_to_e1" => { d(); do_wallet_to_e1(ctx, additional_value.map(|v| v.into())).await }
        "do_wallet_to_ee" => { d(); do_wallet_to_ee(ctx, additional_value.map(|v| v.into())).await }
        "do_reduce_secondary_soft" => { d(); do_reduce_secondary_soft(ctx, additional_value.map(|v| v.into())).await }
        "do_enlarge_secondary_soft" => { d(); do_enlarge_secondary_soft(ctx, additional_value.map(|v| v.into())).await }
        _ => err(format!("No such action \"{}\"", action_name)),
    }
}
