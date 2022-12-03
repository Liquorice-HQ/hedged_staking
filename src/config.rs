//
use rust_decimal::Decimal;
use serde::Deserialize;

#[allow(unused_imports)]
use crate::types::*;


#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub kraken: KrakenConfig,
    pub dydx: DydxConfig,
    pub binance: BinanceConfig,
    pub wallet: WalletConfig,
    pub strategy: StrategyConfig,
    pub notifications: NotificationsConfig,
}


#[derive(Debug, Clone, Deserialize)]
pub struct KrakenConfig {
    pub key: String,
    pub secret: String,
    pub withdrawal_key: String,
    pub atom_withdrawal_key: String,
    pub usdc_account: String,
    pub atom_account: String,
}


#[derive(Debug, Clone, Deserialize)]
pub struct DydxConfig {
    pub key: String,
    pub secret: String,
    pub passphrase: String,
    pub stark_private_key: String,
}


#[derive(Debug, Clone, Deserialize)]
pub struct BinanceConfig {
    pub usdc_account: String,
    pub trading_key: String,
    pub trading_secret: String,
    pub funding_key: String,
    pub funding_secret: String,
}


#[derive(Debug, Clone, Deserialize)]
pub struct WalletConfig {
    pub key: String,
    pub secret: String,
}


#[derive(Debug, Clone, Deserialize)]
pub struct StrategyConfig {
    pub initial_ratio_percent: Decimal,
    pub low_ratio_percent: Decimal,
    pub high_ratio_percent: Decimal,
    //
    pub max_order_usdc: Value,
    pub max_order_std_dev_usdc: Value,
    pub max_slippage_usdc_1: Value,
    pub order_timeout: i64,
    //
    pub keypress_to_continue: bool,
    pub monitoring_timeout: u64,
    pub operations_timeout: u64,
    pub use_binance_for_exchange: bool,
    pub panics_to_log: bool,
}


#[derive(Debug, Clone, Deserialize)]
pub struct NotificationsConfig {
    pub telegram_enabled: bool,
    pub logs_telegram_token: String,
    pub logs_telegram_chat_id: i64,
    pub alerts_telegram_token: String,
    pub alerts_telegram_chat_id: i64,
}


impl From<toml::de::Error> for StrategyError {
    fn from(tde: toml::de::Error) -> Self {
        StrategyError::Misc { msg: format!("OpenLimitsError: {:?}", tde) }
    }
}


pub fn read_config(config_file_name: &str) -> Result<Config, StrategyError> {
    Ok(toml::from_str(&std::fs::read_to_string(config_file_name)?)?)
}


