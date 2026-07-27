use anyhow::{bail, Result};
use clap::Args;
use console::style;

use lattice_cache::{CacheStore, LocalStore};
use lattice_config::{find_root, CacheSize};
use lattice_output::{paint_teal, ROSETTE};

#[derive(Args, Debug)]
#[command(
    long_about = "Evict cache artifacts (oldest first) until the local cache is under a \
size limit.\n\nThe limit comes from --max-size, or falls back to settings.maxCacheSize in \
lattice.json."
)]
pub struct PruneArgs {
    /// Upper bound on the cache size (e.g. "10GB"). Defaults to settings.maxCacheSize.
    #[arg(long, value_name = "SIZE")]
    pub max_size: Option<String>,
}

impl PruneArgs {
    pub async fn execute(&self) -> Result<()> {
        let cwd = std::env::current_dir()?;
        let root = find_root(&cwd).ok_or_else(|| {
            anyhow::anyhow!("No lattice.json found in this directory or any parent.")
        })?;
        let config = lattice_config::load_config(&root)?;

        let max = match &self.max_size {
            Some(s) => CacheSize::parse(s)?,
            None => match config.settings.max_cache_size {
                Some(cs) => cs,
                None => bail!(
                    "no max cache size set (pass --max-size or set settings.maxCacheSize in lattice.json)"
                ),
            },
        };

        let store = LocalStore::new(root.join(config.settings.cache_dir()));
        let report = store.prune(max.as_bytes())?;

        println!(
            "{} removed {} artifact{}, freed {}",
            paint_teal(ROSETTE),
            style(report.removed).bold(),
            if report.removed == 1 { "" } else { "s" },
            style(CacheSize(report.bytes_freed)).bold()
        );
        Ok(())
    }
}
