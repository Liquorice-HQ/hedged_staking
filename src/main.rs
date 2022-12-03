//use flexi_logger::{Logger, FileSpec, Duplicate, AdaptiveFormat};
use flexi_logger::*;
#[allow(unused_imports)]
use log::{debug, error, trace, warn, log_enabled, info, Level};
use openlimits::binance::*;
use openlimits::dydx::{Dydx, DydxCredentials, decode_eth_key, DydxParameters, model::BlockchainNetwork, };
use openlimits::exchange::Exchange;
use openlimits::kraken::{Kraken, KrakenCredentials, KrakenParameters, };
use rust_decimal_macros::dec;
use tokio::time::{sleep,Duration};


mod cli;
mod config;
mod consts;
mod helpers;
mod monitoring;
mod notifications;
mod strategy;
mod types;
mod types_ex;
mod utils;


async fn init_exchange(cfg: &config::Config) -> types::Connections {
    let e1 = {
        let credentials = Some(DydxCredentials {
            blockchain_network: BlockchainNetwork::Mainnet,
            api_key: cfg.dydx.key.clone(),
            api_secret: cfg.dydx.secret.clone(),
            passphrase: cfg.dydx.passphrase.clone(),
            stark_private_key: decode_eth_key(&cfg.dydx.stark_private_key).unwrap(),
        });
        let parameters = DydxParameters { credentials };
        Dydx::new(parameters).await.unwrap()
    };

    let e2 = {
        let credentials = Some(KrakenCredentials { api_key: cfg.kraken.key.clone(), api_secret: cfg.kraken.secret.clone() });
        let parameters = KrakenParameters {
            credentials,
            validate_orders: false,
        };
        Kraken::new(parameters).await.unwrap()
    };

    let auto_cancel = BinanceAutoCancelSettings { interval_ms: 5000, redundancy_percent: 100, retry: 10 };
    let ee_trade = {
        let credentials = Some(BinanceCredentials { api_key: cfg.binance.trading_key.to_owned(), api_secret: cfg.binance.trading_secret.to_owned() });
        let parameters = BinanceParameters { sandbox: false, credentials, auto_cancel: auto_cancel.clone(), };
        Binance::new(parameters).await.unwrap()
    };

    let ee_funding = {
        let credentials = Some(BinanceCredentials { api_key: cfg.binance.funding_key.to_owned(), api_secret: cfg.binance.funding_secret.to_owned() });
        let parameters = BinanceParameters { sandbox: false, credentials, auto_cancel, };
        Binance::new(parameters).await.unwrap()
    };

    types::Connections { e1, e2, ee_trade, ee_funding }
}


async fn make_ctx(config: &config::Config) -> types::Context {
    let connections = init_exchange(config).await;
    types::Context {
            connections,
            transferring_min_limit: dec!(10.0).into(),
            staking_min_limit: dec!(1.0).into(),
            short_min_limit: dec!(1.05), // dYdX allow to make order with minimum 1.0 ATOM price, but due of rounding make slightly bigger
            e1_eth_withdraw_address: config.wallet.key.to_owned(),
            e2_withdrawal_key: config.kraken.withdrawal_key.to_owned(),
            e2_withdrawal_key_operational: config.kraken.atom_withdrawal_key.to_owned(),
            e2_operational_token_deposit_address: config.kraken.atom_account.to_owned(),
            wallet_withdraw_address: config.wallet.key.to_owned(),
            balances: None,
            consts: config.strategy.clone(),
        }
}


#[tokio::main]
async fn main() {
    let args = cli::parse();
    let config = config::read_config("config.toml").unwrap();

    //Logger::try_with_env_or_str("trace").unwrap()
    Logger::try_with_env_or_str("warn,hedged_staking=debug,NOTIFICATION=info").unwrap()
        .log_to_file_and_writer(
            FileSpec::try_from("./vfhedgedstaking.log").unwrap(),
            Box::new(notifications::TelegramLogWriter { config: config.notifications.to_owned() }))
        .append()
        .duplicate_to_stdout(Duplicate::Trace)
        .adaptive_format_for_stdout(AdaptiveFormat::Detailed)
        .format_for_stdout(flexi_logger::colored_detailed_format)
        .format_for_files(flexi_logger::detailed_format)
        .start().unwrap();

    if config.strategy.panics_to_log {
        // NOTE: this initialization must be after parsing config files and setup logging,
        // because bad config towards to `panic!()`, which doesn't outputs anywhere.
        log_panics::init();
    }

    //print!("{:?}", config);
    assert!(config.strategy.initial_ratio_percent <= dec!(100));
    assert!(config.strategy.high_ratio_percent <= dec!(100));
    assert!(config.strategy.low_ratio_percent <= config.strategy.high_ratio_percent);
    assert!(config.strategy.low_ratio_percent <= config.strategy.initial_ratio_percent);
    assert!(config.strategy.high_ratio_percent >= config.strategy.initial_ratio_percent);

    monitoring::run_metrics_web_server().await;

    // TODO: make correct error handling
    match args.action {
        cli::Action::Run | cli::Action::Monitoring => {
            loop {
                match strategy::strategy(&mut make_ctx(&config).await, args.action == cli::Action::Monitoring).await {
                    Ok(_) => { break },
                    Err(err) => {
                        error!("{}", err.to_string());
                        let timeout = 60;
                        info!("Take a pause ({} secs) in the hope that things will get better next time...", timeout);
                        sleep(Duration::from_secs(60)).await;
                    },
                }
            }
        },
        cli::Action::Only{action_name, value} => {
            match strategy::run_action_by_name(action_name, &mut make_ctx(&config).await, value).await {
                Ok(_) => { },
                Err(err) => error!(">>> {:?}", err),
            }
        }
    }

}

