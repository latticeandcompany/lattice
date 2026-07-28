mod cli;
mod commands;
mod drift;
mod release;
mod schema;

use anyhow::Result;
use clap::Parser;
use cli::Cli;

#[tokio::main]
async fn main() -> Result<()> {
	let cli = Cli::parse();
	cli.execute().await
}
