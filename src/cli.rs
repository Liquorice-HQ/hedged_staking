/// Command line parsing
///

use clap::Parser;
use clap::builder::IntoResettable;
use clap::builder::Resettable;
use clap::builder::Str;

use crate::utils::*;
use crate::types::*;


struct OwnVersion {
}


impl IntoResettable<Str> for OwnVersion {
    fn into_resettable(self) -> clap::builder::Resettable<Str> {
        Resettable::Value(Str::from(get_long_version_string()))
    }
}


/// Simple program to greet a person
#[derive(Parser, Debug)]
#[clap(author, version = OwnVersion {}, about, long_about = None)]
pub struct CliArgs {
    #[clap(subcommand)]
    pub action: Action,
    pub config: Option<std::path::PathBuf>,
}


#[derive(clap::Subcommand, PartialEq, Debug)]
pub enum Action {
    /// Run main strategy
    Run,
    /// Monitoring only
    Monitoring,
    /// Run only specified action
    Only { action_name: String, value: Option<Value> },
}


pub fn parse() -> CliArgs { CliArgs::parse() }

