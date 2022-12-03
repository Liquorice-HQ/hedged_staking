/// Monitoring of strategy working
///

use prometheus::{register_gauge, Gauge, register_counter, register_int_counter, gather, opts, Counter, IntCounter, Encoder};
use warp::Filter;
use warp::*;
use lazy_static::*;

use crate::types::*;
use crate::consts::*;

pub static METRIC_PREFIX: &str = "hedgedstaking";

pub fn prefix(s: &str) -> String {
    format!("{}_{}", METRIC_PREFIX, s)
}

pub fn prefix_balance(s: String) -> String {
    prefix(format!("balance_{}", s).as_str())
}

pub fn prefix_event(s: String) -> String {
    prefix(format!("event_{}", s).as_str())
}

pub fn prefix_expense(s: &str) -> String {
    prefix(format!("expense_{}", s).as_str())
}

lazy_static! {
    pub static ref PRICE: Gauge =
        register_gauge!(opts!(
                prefix("price"), // cha
                format!("{}/{} price on {}", OPERATING_COIN, TRANSFERRING_COIN, E2_NAME)
                )).unwrap();

    pub static ref E1_E2_RATIO: Gauge =
        register_gauge!(opts!(
                prefix(format!("{}_{}_ratio", E1_NAME, E2_NAME).as_str()),
                format!("{}/{} ratio", E1_NAME, E2_NAME)
                )).unwrap();

    pub static ref E1_E2_RATIO_HIGH: Gauge =
        register_gauge!(opts!(
                prefix(format!("{}_{}_ratio_high", E1_NAME, E2_NAME).as_str()),
                format!("{}/{} ratio high bound", E1_NAME, E2_NAME)
                )).unwrap();

    pub static ref E1_E2_RATIO_LOW: Gauge =
        register_gauge!(opts!(
                prefix(format!("{}_{}_ratio_low", E1_NAME, E2_NAME).as_str()),
                format!("{}/{} ratio low bound", E1_NAME, E2_NAME)
                )).unwrap();

    pub static ref E1_BALANCE_TOTAL: Gauge =
        register_gauge!(opts!(
                prefix_balance(format!("{}_total", E1_NAME)),
                format!("{} total balance in {}", E1_NAME, TRANSFERRING_COIN)
                )).unwrap();

    pub static ref E1_BALANCE_FREE: Gauge =
        register_gauge!(opts!(
                prefix_balance(format!("{}_free", E1_NAME)),
                format!("{} free balance in {}", E1_NAME, TRANSFERRING_COIN)
                )).unwrap();

    pub static ref E1_BALANCE_SHORTED: Gauge =
        register_gauge!(opts!(
                prefix_balance(format!("{}_shorted", E1_NAME)),
                format!("{} shorted in {}", E1_NAME, OPERATING_COIN)
                )).unwrap();

    pub static ref E2_BALANCE_TOTAL: Gauge =
        register_gauge!(opts!(
                prefix_balance(format!("{}_total", E2_NAME)),
                format!("{} total balance in {}", E2_NAME, TRANSFERRING_COIN)
                )).unwrap();

    pub static ref E2_BALANCE_TRANSFERRING: Gauge =
        register_gauge!(opts!(
                prefix_balance(format!("{}_{}", E2_NAME, TRANSFERRING_COIN)),
                format!("{} {} balance", E2_NAME, TRANSFERRING_COIN)
                )).unwrap();

    pub static ref E2_BALANCE_INTERMEDIATE: Gauge =
        register_gauge!(opts!(
                prefix_balance(format!("{}_{}", E2_NAME, INTERMEDIATE_COIN)),
                format!("{} {} balance", E2_NAME, INTERMEDIATE_COIN)
                )).unwrap();

    pub static ref E2_BALANCE_UNSTAKED: Gauge =
        register_gauge!(opts!(
                prefix_balance(format!("{}_unstaked_{}", E2_NAME, OPERATING_COIN)),
                format!("{} unstaked {} balance", E2_NAME, OPERATING_COIN)
                )).unwrap();

    pub static ref E2_BALANCE_STAKED: Gauge =
        register_gauge!(opts!(
                prefix_balance(format!("{}_staked_{}", E2_NAME, OPERATING_COIN)),
                format!("{} staked {} balance", E2_NAME, OPERATING_COIN)
                )).unwrap();

    pub static ref EE_BALANCE_TOTAL: Gauge =
        register_gauge!(opts!(
                prefix_balance(format!("{}_total", EE_NAME)),
                format!("{} total balance in {}", EE_NAME, TRANSFERRING_COIN)
                )).unwrap();

    pub static ref EE_BALANCE_TRANSFERRING: Gauge =
        register_gauge!(opts!(
                prefix_balance(format!("{}_{}", EE_NAME, TRANSFERRING_COIN)),
                format!("{} {} balance", EE_NAME, TRANSFERRING_COIN)
                )).unwrap();

    pub static ref EE_BALANCE_OPERATIONAL: Gauge =
        register_gauge!(opts!(
                prefix_balance(format!("{}_{}", EE_NAME, OPERATING_COIN)),
                format!("{} {} balance", EE_NAME, OPERATING_COIN)
                )).unwrap();

    pub static ref WALLET_BALANCE_TOTAL: Gauge =
        register_gauge!(opts!(
                prefix_balance(format!("{}_{}", WALLET_NAME, TRANSFERRING_COIN)),
                format!("{} {} balance", WALLET_NAME, TRANSFERRING_COIN)
                )).unwrap();

    pub static ref WALLET_BALANCE_GAS: Gauge =
        register_gauge!(opts!(
                prefix_balance(format!("{}_{}", WALLET_NAME, GAS_COIN)),
                format!("{} {} balance", WALLET_NAME, GAS_COIN)
                )).unwrap();

    pub static ref EVENT_ABOVE_HIGH: IntCounter =
        register_int_counter!(opts!(
                prefix_event("above_high".to_string()),
                "New 'above high' event"
                )).unwrap();

    // NOTE: Counter values can be obtained by query: curl --request GET http://172.16.57.3:9090/api/v1/query\?query\="expense"
    // E1 <--> Wallet (USDC, ETH)

    pub static ref E1_TO_WALLET_PRIM_EXPENSE: Counter =
        register_counter!(opts!(
                prefix_expense("e1_to_wallet_prim"),
                "E1->Wallet expense"
                )).unwrap();

    pub static ref WALLET_TO_E1_PRIM_EXPENSE: Counter =
        register_counter!(opts!(
                prefix_expense("wallet_to_e1_prim"),
                "Wallet->E1 expense"
                )).unwrap();

    pub static ref WALLET_TO_E1_GAS_EXPENSE: Counter =
        register_counter!(opts!(
                prefix_expense("wallet_to_e1_gas"),
                "Wallet->E1 expense"
                )).unwrap();

    // EE <--> Wallet (PRIM, ETH)

    pub static ref EE_TO_WALLET_PRIM_EXPENSE: Counter =
        register_counter!(opts!(
                prefix_expense("ee_to_wallet_prim"),
                "EE->Wallet expense"
                )).unwrap();

    pub static ref WALLET_TO_EE_PRIM_EXPENSE: Counter =
        register_counter!(opts!(
                prefix_expense("wallet_to_ee_prim"),
                "Wallet->EE expense"
                )).unwrap();

    pub static ref WALLET_TO_EE_GAS_EXPENSE: Counter =
        register_counter!(opts!(
                prefix_expense("wallet_to_ee_gas"),
                "Wallet->E1 expense"
                )).unwrap();

    // EE <--> E2 (SEC)

    pub static ref EE_TO_E2_SEC_EXPENSE: Counter =
        register_counter!(opts!(
                prefix_expense("ee_to_e2_sec"),
                "EE->E2 expense"
                )).unwrap();

    pub static ref E2_TO_EE_SEC_EXPENSE: Counter =
        register_counter!(opts!(
                prefix_expense("e2_to_ee_sec"),
                "E2->EE expense"
                )).unwrap();

}


async fn metrics_handler() -> Result<impl Reply, Rejection> {
    let encoder = prometheus::TextEncoder::new();

    let mut buffer = Vec::new();
    if let Err(e) = encoder.encode(&gather(), &mut buffer) {
        eprintln!("could not encode custom metrics: {}", e);
    };
    let mut res = match String::from_utf8(buffer.clone()) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("custom metrics could not be from_utf8'd: {}", e);
            String::default()
        }
    };
    buffer.clear();

    let mut buffer = Vec::new();
    if let Err(e) = encoder.encode(&prometheus::gather(), &mut buffer) {
        eprintln!("could not encode prometheus metrics: {}", e);
    };
    let res_custom = match String::from_utf8(buffer.clone()) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("prometheus metrics could not be from_utf8'd: {}", e);
            String::default()
        }
    };
    buffer.clear();

    res.push_str(&res_custom);
    Ok(res)
}


pub async fn run_metrics_web_server() {
    let metrics_route = warp::path!("metrics").and_then(metrics_handler);
    tokio::task::spawn(
        warp::serve(metrics_route).run(([0,0,0,0], 8080))
        );
    // Initialzie counters to view on Grafana
    E1_TO_WALLET_PRIM_EXPENSE.reset();
    WALLET_TO_E1_PRIM_EXPENSE.reset();
    WALLET_TO_E1_GAS_EXPENSE.reset();
    EE_TO_WALLET_PRIM_EXPENSE.reset();
    WALLET_TO_EE_PRIM_EXPENSE.reset();
    WALLET_TO_EE_GAS_EXPENSE.reset();
    EE_TO_E2_SEC_EXPENSE.reset();
    E2_TO_EE_SEC_EXPENSE.reset();
}

