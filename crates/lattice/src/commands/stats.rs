use anyhow::Result;
use clap::Args;
use console::style;

use lattice_cache::{CacheStore, LocalStore, Savings};
use lattice_config::find_root;
use lattice_output::{banner_line, fmt_bytes, fmt_count, fmt_span, paint_teal};

/// How far back the recent-window line looks.
const RECENT_DAYS: i64 = 7;

#[derive(Args, Debug)]
#[command(
	long_about = "Report what this repo's cache has done for it: task time saved, how \
many runs and hits it took, and how much room the cache uses.\n\nThe record is a \
ledger appended once per run, kept alongside the cache. Clearing the cache clears \
the record with it, and it is per-machine: nothing about it is committed."
)]
pub struct StatsArgs {}

impl StatsArgs {
	pub async fn execute(&self) -> Result<()> {
		let cwd = std::env::current_dir()?;
		let root = find_root(&cwd).ok_or_else(|| {
			anyhow::anyhow!(
				"no lattice.json found in this directory or any parent. \
                 Run `lattice init` to create one"
			)
		})?;
		lattice_config::schema::ensure_schema(&root);
		let config = lattice_config::load_config(&root)?;

		let store = LocalStore::new(root.join(config.settings.cache_dir()));
		let runs = store.recorded_runs()?;
		let all = Savings::of(&runs);

		let since = all
			.since
			.map(|at| format!("since {}", at.format("%Y-%m-%d")))
			.unwrap_or_default();
		println!("{}", banner_line(format!("stats  {since}").trim_end()));
		println!();

		if all.runs == 0 {
			println!(
				"  No runs recorded yet. Run a task and this fills in — every run \
appends one line."
			);
			return Ok(());
		}

		let recent = Savings::recent(&runs, RECENT_DAYS);
		let usage = store.usage()?;

		row(
			"saved",
			&format!("{} of task time", paint_teal(&fmt_span(all.saved_ms))),
		);
		row(
			"runs",
			&format!(
				"{} · {} of {} tasks cached{}",
				fmt_count(all.runs),
				fmt_count(all.hits),
				fmt_count(all.tasks),
				all.hit_rate()
					.map(|r| format!(" ({r:.0}%)"))
					.unwrap_or_default()
			),
		);
		row(
			"cache",
			&format!(
				"{} · {} {}",
				fmt_bytes(usage.bytes),
				fmt_count(usage.entries),
				if usage.entries == 1 {
					"entry"
				} else {
					"entries"
				}
			),
		);
		row(
			&format!("last {RECENT_DAYS}d"),
			&format!(
				"{} saved across {} run{}",
				fmt_span(recent.saved_ms),
				fmt_count(recent.runs),
				if recent.runs == 1 { "" } else { "s" }
			),
		);
		Ok(())
	}
}

/// One label/value line, labels padded to a common column so the values line up.
///
/// Padded before it is styled, not after: a dim label carries escape bytes that
/// width formatting counts and a terminal does not, so padding the styled string
/// leaves every value in a different column once color is on.
fn row(label: &str, value: &str) {
	println!("  {} {}", style(format!("{label:<10}")).dim(), value);
}
