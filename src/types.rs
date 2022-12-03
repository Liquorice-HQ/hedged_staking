use openlimits::binance::Binance;
use openlimits::dydx::Dydx;
use openlimits::errors::OpenLimitsError;
use openlimits::kraken::Kraken;
use rust_decimal::{Decimal, RoundingStrategy};
use rust_decimal::prelude::ToPrimitive;
use std::convert::From;
use derive_more::*;

//#[allow(unused_imports)]
use crate::config::*;

pub static E1_NAME: &str = "dYdX";
pub type FirstExchange = Dydx;


pub static E2_NAME: &str = "Kraken";
pub type SecondExchange = Kraken;

// "EE" for "Exchange for exchanges"
pub static EE_NAME: &str = "Binance";
pub type ExchangeExchange = Binance;

pub static WALLET_NAME: &str = "wallet";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WhichExchange {
    First,
    Second,
    Exchange,
    Wallet
}


pub struct Connections {
    pub e1: FirstExchange,
    pub e2: SecondExchange,
    pub ee_trade: ExchangeExchange,
    pub ee_funding: ExchangeExchange,
}


#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct E1Balances {
    pub total: PrimaryAsset,
    pub free: PrimaryAsset,
    pub operational_coins: SecondaryAsset,
}


#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct E2Balances {
    pub transferring_coins: PrimaryAsset,
    pub intermediate_coins: PrimaryAsset,
    pub staked_coins: SecondaryAsset,
    pub unstaked_coins: SecondaryAsset,
}


#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EEBalances {
    pub transferring_coins: PrimaryAsset,
    pub operational_coins: SecondaryAsset,
}


#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WalletBalances {
    pub transferring_coins: PrimaryAsset,
    pub gas_coins: Value,
}


#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Balances {
    pub e1: E1Balances,
    pub e2: E2Balances,
    pub ee: EEBalances,
    pub wallet: WalletBalances,
}


pub fn get_free_transferring_coins(balances: &Balances, we: WhichExchange) -> PrimaryAsset {
    match we {
        WhichExchange::First => balances.e1.free,
        WhichExchange::Second => balances.e2.transferring_coins,
        WhichExchange::Exchange => balances.ee.transferring_coins,
        WhichExchange::Wallet => balances.wallet.transferring_coins,
    }
}


/// Connections, etc.
///
pub struct Context {
    pub connections: Connections,

    pub short_min_limit: Value,
    pub transferring_min_limit: PrimaryAsset,
    pub staking_min_limit: SecondaryAsset,

    pub e1_eth_withdraw_address: String,
    pub e2_withdrawal_key: String,
    pub e2_withdrawal_key_operational: String,
    pub e2_operational_token_deposit_address: String,
    pub wallet_withdraw_address: String,
    pub balances: Option<Balances>,
    pub consts: StrategyConfig,
}


pub type TokenName<'a> = &'a str;


pub type Value = Decimal;

pub type Price = Decimal; // TODO maybe change to special type?


pub struct BuySellPrice {
    pub buy: Price,
    pub sell: Price,
}


pub fn avg_price(price: &BuySellPrice) -> Price {
    (price.buy + price.sell) / Decimal::TWO
}


#[derive(Debug, PartialOrd, PartialEq, Neg, From, Add, Sub, Mul, Div, Clone, Copy, Eq, Ord, Into, Deref, Default)]
pub struct PrimaryAsset(pub Value);


#[derive(Debug, PartialOrd, PartialEq, Neg, From, Add, Sub, Mul, Div, Clone, Copy, Eq, Ord, Into, Deref, Default)]
pub struct SecondaryAsset(pub Value);


#[derive(Debug, Display, Clone, Copy, PartialEq, Eq)]
pub enum Asset {
    Primary(PrimaryAsset),    // USDC
    Secondary(SecondaryAsset) // ATOM
}

impl From<i64> for PrimaryAsset {
    fn from(v: i64) -> PrimaryAsset {
        PrimaryAsset(Decimal::from(v))
    }
}


impl From<i64> for SecondaryAsset {
    fn from(v: i64) -> SecondaryAsset {
        SecondaryAsset(Decimal::from(v))
    }
}

pub trait SimpleOperations {
    fn round(&self, digs: u32) -> Self;
    fn to_f(&self) -> f64;
}


impl SimpleOperations for PrimaryAsset {
    fn round(&self, digs: u32) -> Self {
        PrimaryAsset(self.round_dp_with_strategy(digs, RoundingStrategy::ToZero))
    }
    fn to_f(&self) -> f64 {
        self.to_f64().unwrap()
    }
}


impl SimpleOperations for SecondaryAsset {
    fn round(&self, digs: u32) -> Self {
        SecondaryAsset(self.round_dp_with_strategy(digs, RoundingStrategy::ToZero))
    }
    fn to_f(&self) -> f64 {
        self.to_f64().unwrap()
    }
}


impl PrimaryAsset {
    pub fn to_sec(self, price: Price) -> SecondaryAsset {
        SecondaryAsset(*self / price)
    }
}


impl SecondaryAsset {
    pub fn to_prim(self, price: Price) -> PrimaryAsset {
        PrimaryAsset(*self * price)
    }
}

// --


#[derive(Debug)]
pub enum StrategyError {
    Misc { msg: String },
}


impl ToString for StrategyError {
    fn to_string(&self) -> String {
        match self {
            StrategyError::Misc{msg} => msg.to_owned(),
        }
    }
}


impl From<OpenLimitsError> for StrategyError {
    fn from(ole: OpenLimitsError) -> Self {
        StrategyError::Misc { msg: format!("OpenLimitsError: {:?}", ole) }
    }
}


impl From<std::io::Error> for StrategyError {
    fn from(sie: std::io::Error) -> Self {
        StrategyError::Misc { msg: format!("OpenLimitsError: {:?}", sie) }
    }
}


pub type StrategyResult<A> = Result<A, StrategyError>;


pub type ActionResult = StrategyResult<()>;

