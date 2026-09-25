mod cli;
mod commands;
mod drift;
mod release;

use std::process::ExitCode;

use clap::Parser;
use cli::Cli;

#[tokio::main]
async fn main() -> ExitCode {
	let cli = Cli::parse();
	match cli.execute().await {
		Ok(()) => ExitCode::SUCCESS,
		Err(err) => {
			eprintln!("{} {err}", console::style("error:").red().bold());
			for cause in err.chain().skip(1) {
				for line in cause.to_string().lines() {
					eprintln!("  {line}");
				}
			}
			ExitCode::FAILURE
		}
	}
}
