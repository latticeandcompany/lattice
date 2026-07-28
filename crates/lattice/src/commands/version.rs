use anyhow::Result;
use clap::Args;

use lattice_output::splash;

use crate::cli::BIN_VERSION;

#[derive(Args, Debug)]
pub struct VersionArgs {
	/// Output version information as JSON.
	#[arg(long)]
	pub json: bool,
}

impl VersionArgs {
	pub async fn execute(&self) -> Result<()> {
		if self.json {
			println!(
				r#"{{"version":"{}","target":"{}","arch":"{}"}}"#,
				BIN_VERSION,
				env!("LATTICE_TARGET"),
				std::env::consts::ARCH
			);
		} else {
			println!("{}", splash(BIN_VERSION));
		}
		Ok(())
	}
}
