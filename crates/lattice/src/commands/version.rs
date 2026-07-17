use anyhow::Result;
use clap::Args;
use console::style;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Args, Debug)]
pub struct VersionArgs {
    #[arg(long, help = "Output version as JSON")]
    pub json: bool,
}

impl VersionArgs {
    pub async fn execute(&self) -> Result<()> {
        if self.json {
            println!(
                r#"{{"version":"{}","target":"{}"}}"#,
                VERSION,
                std::env::consts::ARCH
            );
        } else {
            println!(
                "{} {} {}",
                style("lattice").bold().cyan(),
                style(VERSION).bold(),
                style(format!("({})", std::env::consts::ARCH)).dim()
            );
            println!("{}", style("Cross-language monorepo task orchestrator").dim());
        }
        Ok(())
    }
}
