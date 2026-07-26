use anyhow::Result;
use clap::Args;
use console::style;

use lattice_output::{logo, paint_teal, wordmark, ROSETTE};

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
                r#"{{"version":"{}","target":"{}"}}"#,
                BIN_VERSION,
                std::env::consts::ARCH
            );
        } else {
            // Branded splash: teal rosette mark, then the ink `lattice` wordmark
            // lockup with version + target and the tagline (BRAND.md §2/§6).
            println!("{}", logo());
            println!(
                "{} {}  {}  {}",
                paint_teal(ROSETTE),
                wordmark(),
                style(BIN_VERSION).bold(),
                style(format!("({})", std::env::consts::ARCH)).dim()
            );
            println!(
                "{}",
                style("Local-first build tool for polyglot monorepos.").dim()
            );
        }
        Ok(())
    }
}
