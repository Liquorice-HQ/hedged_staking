use clap::crate_version;
use pause_console::*;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use termion::{color, style};

use crate::types::*;


pub const ONE:Decimal = Decimal::ONE;
pub const ONE_S:SecondaryAsset = SecondaryAsset(ONE);


pub const ZERO:Decimal = Decimal::ZERO;
pub const ZERO_P:PrimaryAsset = PrimaryAsset(ZERO);
pub const ZERO_S:SecondaryAsset = SecondaryAsset(ZERO);


pub fn press_enter_to_continue() {
    pause_console!(format!(
        "{}{}Press Enter to continue...{}",
        color::Fg(color::Red), style::Bold, style::Reset).as_str()); 
}


#[allow(dead_code)]
pub fn wait_for_human_action(msg: String) {
    pause_console!(format!(
            "{}{}ACTION REQUIRED:{} {}. Press Enter when done...{}",
            color::Fg(color::Red), style::Bold, style::NoBold,
            msg,
            style::Reset
            ).as_str());
}


pub fn percent_to_decimal(percent: Decimal) -> Decimal {
    percent / dec!(100)
}


#[allow(dead_code)] // TODO remove dead_code
pub fn copysign(value: Decimal, sign_carrier: Decimal) -> Decimal {
    let mut v = value;
    v.set_sign_positive(sign_carrier >= ZERO);
    v
}


pub fn get_long_version_string() -> String {
    let git_dirty = env!("BUILD_GIT_DIRTY");
    let git_dirty_flag = if git_dirty.is_empty() { "" } else { " *" };
    format!("ver {}\nbuild timestamp: {}\ngit rev: {}{}",
        crate_version!(),
        env!("BUILD_TIMESTAMP"),
        env!("BUILD_GIT_VERSION"),
        git_dirty_flag)
}

