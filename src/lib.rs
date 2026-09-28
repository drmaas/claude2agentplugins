pub mod agent_plugins;
pub mod claude;
pub mod cli;
pub mod convert;
pub mod cursor;
pub mod error;
pub mod marketplace;
pub mod opencode;
pub mod validate;

use crate::cli::Cli;
use clap::Parser;
use error::Result;

pub fn run() -> Result<()> {
    let cli = Cli::parse();
    cli.execute()
}
