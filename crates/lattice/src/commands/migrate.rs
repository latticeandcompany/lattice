use anyhow::{bail, Result};
use clap::Args;
use console::style;

use lattice_config::{find_root, CONFIG_FILE};
use lattice_output::teal;

use crate::cli::BIN_VERSION;

#[derive(Args, Debug)]
#[command(
	long_about = "Rewrite a Lattice 1.x lattice.json for this version.\n\n\
Lattice now reads workspaces and the dependencies between them from the repo's own \
manifests, so most of a 1.x config is no longer needed. `migrate` works out what \
this repo still has to say — a workspace no workspace file names, an edge between \
languages, a command written out by hand — and writes only that. Every workspace \
keeps the name it had."
)]
pub struct MigrateArgs {
	/// Print the new lattice.json instead of writing it.
	#[arg(long)]
	pub dry_run: bool,
}

impl MigrateArgs {
	pub async fn execute(&self) -> Result<()> {
		let cwd = std::env::current_dir()?;
		let Some(root) = find_root(&cwd) else {
			bail!("no lattice.json found in this directory or any parent");
		};
		let path = root.join(CONFIG_FILE);
		let text = std::fs::read_to_string(&path)?;
		if !lattice_config::is_v1_config(&text) {
			lattice_config::parse_config(&text)?;
			println!("lattice.json is already written for this version. Nothing to do.");
			return Ok(());
		}

		let migration = lattice_project::migrate::migrate(&root, &text, BIN_VERSION)?;
		if self.dry_run {
			print!("{}", migration.text);
			for note in &migration.notes {
				eprintln!("{} {note}", style("·").dim());
			}
			return Ok(());
		}

		std::fs::write(&path, &migration.text)?;
		lattice_config::schema::ensure_schema(&root);

		let before = text.lines().count();
		let after = migration.text.lines().count();
		println!(
			"{} {} {}",
			teal().apply_to("✓"),
			style("rewrote lattice.json").bold(),
			style(format!("{before} lines → {after}")).dim()
		);
		for note in &migration.notes {
			println!("{} {note}", style("·").dim());
		}
		Ok(())
	}
}
