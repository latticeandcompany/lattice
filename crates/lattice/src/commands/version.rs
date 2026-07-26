use anyhow::Result;
use clap::Args;
use console::style;

use lattice_output::teal;

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
            // Branded: teal rosette + ink `lattice` wordmark (BRAND.md §2).
            println!(
                "{} {} {} {}",
                teal().apply_to("◆"),
                style("lattice").bold(),
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
