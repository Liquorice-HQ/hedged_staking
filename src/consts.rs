use rust_decimal_macros::dec;

use crate::types::*;

pub static TRANSFERRING_COIN: TokenName = "USDC";

pub static INTERMEDIATE_COIN: TokenName = "ZUSD";

pub static OPERATING_COIN: TokenName = "ATOM";

pub static GAS_COIN: TokenName = "ETH";

/// Name of unstaked coin on second exchange.
/// For example, on Kraken staked coin have name with ".S" suffix
pub static E2_STAKED_COIN: TokenName = "ATOM.S"; // TODO: const_concat!(E2_UNSTAKED_COIN, ".S");

pub static E2_UNSTAKED_COIN: TokenName = OPERATING_COIN;

pub static E2_TRANSFERRING_COIN: TokenName = TRANSFERRING_COIN;

pub static E2_INTERMEDIATE_COIN: TokenName = INTERMEDIATE_COIN;

pub static E2_WITHDRAW_COMMISSION: Value = dec!(3.5); // TODO get from config file

pub static EE_TRANSFERRING_COIN: TokenName = "BUSD";


/// Get apropriate market name pair in some exchange
/// (for example for `ZUSD` on Kraken outputs `USD`).
///
pub fn get_part_of_market_pair(we: WhichExchange, name: &str) -> &str {
    match we {
        WhichExchange::First => {
            if name == TRANSFERRING_COIN || name == INTERMEDIATE_COIN { "USD" }
            else { name }
        },
        WhichExchange::Second => {
            if name == E2_INTERMEDIATE_COIN { "USD" }
            else { name }
        },
        WhichExchange::Exchange => {
            if name == TRANSFERRING_COIN || name == INTERMEDIATE_COIN { EE_TRANSFERRING_COIN }
            else { name }
        },
        WhichExchange::Wallet => {
            if name == E2_INTERMEDIATE_COIN { "USD" }
            else { name }
        },
    }
}


pub fn get_market_pair_name(we: WhichExchange, from: &str, to: &str) -> String {
    let xfrom = get_part_of_market_pair(we, from);
    let xto = get_part_of_market_pair(we, to);
    match we {
        WhichExchange::First  => format!("{}-{}", xfrom, xto),
        // Note: https://api.kraken.com/0/public/AssetPairs
        WhichExchange::Second => format!("{}{}", xfrom, xto),
        WhichExchange::Exchange => format!("{}{}", xfrom, xto),
        WhichExchange::Wallet => format!("{}{}", xfrom, xto),
    }
}


pub fn get_exchange_name(we: WhichExchange) -> &'static str {
    match we {
        WhichExchange::First  => E1_NAME,
        WhichExchange::Second => E2_NAME,
        WhichExchange::Exchange => EE_NAME,
        WhichExchange::Wallet => WALLET_NAME,
    }
}


