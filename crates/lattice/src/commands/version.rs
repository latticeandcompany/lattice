use anyhow::Result;
use clap::Args;
use console::style;

use lattice_config::find_root;

use crate::commands::dev::{linked_build, LinkedBuild};

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Args, Debug)]
pub struct VersionArgs {
    #[arg(long, help = "Output version as JSON")]
    pub json: bool,
}

impl VersionArgs {
    pub async fn execute(&self) -> Result<()> {
        // Determine which build the stable symlink resolves to, degrading gracefully
        // when we're not inside a lattice repo.
        let linked = std::env::current_dir()
            .ok()
            .and_then(|cwd| find_root(&cwd))
            .map(|root| linked_build(&root))
            .unwrap_or(LinkedBuild::None);

        if self.json {
            println!(
                r#"{{"version":"{}","target":"{}","linked":"{}"}}"#,
                VERSION,
                std::env::consts::ARCH,
                linked.describe()
            );
        } else {
            println!(
                "{} {} {}",
                style("lattice").bold().cyan(),
                style(VERSION).bold(),
                style(format!("({})", std::env::consts::ARCH)).dim()
            );
            println!(
                "{}",
                style("Cross-language monorepo task orchestrator").dim()
            );
            println!("{} {}", style("linked:").dim(), linked.describe());
        }
        Ok(())
    }
}
