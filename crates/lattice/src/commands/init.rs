use anyhow::{bail, Result};
use clap::Args;
use console::style;
use dialoguer::{theme::ColorfulTheme, MultiSelect};

use lattice_output::{logo_for, teal, Theme};
use lattice_project::scaffold::{self, Proposal};
use lattice_project::Project;
use lattice_workspace::scan::{EnginePin, WorkspaceCandidate};

use crate::cli::BIN_VERSION;

#[derive(Args, Debug)]
#[command(long_about = "Create a lattice.json in the current directory.\n\n\
Lattice reads the repo first. Workspaces your package.json, pnpm-workspace.yaml, \
Cargo.toml, go.work or pyproject.toml already name need no entry, and neither do the \
tasks their manifests define. What is left — a project no workspace file knows about, \
a tool version the repo already pins — is proposed, and on a terminal you confirm it. \
With --yes, or with no terminal attached, Lattice writes what it found. It also writes \
.lattice/schema.json for you to commit and adds Lattice's local files to .gitignore.")]
pub struct InitArgs {
	/// Overwrite an existing lattice.json.
	#[arg(long)]
	pub force: bool,

	/// Write what the scan finds without prompting.
	#[arg(short, long)]
	pub yes: bool,
}

impl InitArgs {
	pub async fn execute(&self, theme: Theme) -> Result<()> {
		let cwd = std::env::current_dir()?;
		let config_path = cwd.join("lattice.json");
		if config_path.exists() && !self.force {
			let text = std::fs::read_to_string(&config_path).unwrap_or_default();
			if lattice_config::is_v1_config(&text) {
				bail!(
					"lattice.json already exists, written for Lattice 1.x. Run `lattice migrate` \
					 to rewrite it, or pass --force to start over"
				);
			}
			bail!("lattice.json already exists. Pass --force to overwrite it");
		}

		let tty = console::user_attended();
		if tty {
			println!("{}", logo_for(theme));
			println!();
		}

		let proposal = scaffold::propose(&cwd);
		let all: Vec<&str> = proposal.extra.iter().map(|c| c.path.as_str()).collect();
		let (workspaces, pins) = if self.yes || !tty {
			let workspaces: Vec<&WorkspaceCandidate> = proposal
				.extra
				.iter()
				.filter(|c| c.default_selected)
				.collect();
			(workspaces, proposal.pins.iter().collect())
		} else {
			confirm(&proposal)?
		};

		let mut config = scaffold::build_config(&workspaces, &all, &pins, BIN_VERSION);
		if let Some(root) = &proposal.single_project {
			scaffold::import_settings(&mut config, &[root]);
		}
		scaffold::write_artifacts(&cwd, &config)?;
		print_result(&cwd, &proposal);
		Ok(())
	}
}

fn confirm(proposal: &Proposal) -> Result<(Vec<&WorkspaceCandidate>, Vec<&EnginePin>)> {
	let theme = ColorfulTheme::default();

	if !proposal.native.is_empty() {
		println!(
			"{} {} named by the repo's workspace files",
			teal().apply_to("✓"),
			plural(proposal.native.len(), "workspace", "workspaces"),
		);
	}

	let mut workspaces = Vec::new();
	if !proposal.extra.is_empty() {
		let labels: Vec<String> = proposal.extra.iter().map(candidate_label).collect();
		let defaults: Vec<bool> = proposal.extra.iter().map(|c| c.default_selected).collect();
		let picked = MultiSelect::with_theme(&theme)
			.with_prompt("Projects no workspace file names. Add them as workspaces?")
			.items(&labels)
			.defaults(&defaults)
			.interact()?;
		workspaces = picked.into_iter().map(|i| &proposal.extra[i]).collect();
	}

	let mut pins = Vec::new();
	if !proposal.pins.is_empty() {
		let labels: Vec<String> = proposal.pins.iter().map(pin_label).collect();
		let picked = MultiSelect::with_theme(&theme)
			.with_prompt("Tool versions the repo pins. Check them on every run?")
			.items(&labels)
			.defaults(&vec![true; proposal.pins.len()])
			.interact()?;
		pins = picked.into_iter().map(|i| &proposal.pins[i]).collect();
	}

	Ok((workspaces, pins))
}

fn print_result(root: &std::path::Path, proposal: &Proposal) {
	let check = teal().apply_to("✓");
	println!();
	println!("{check} {}", style("wrote lattice.json").bold());
	println!("{check} {}", style("wrote .lattice/schema.json").bold());

	let project = match Project::open_root(root) {
		Ok(project) => project,
		Err(err) => {
			println!();
			println!("{} {err:#}", style("!").yellow());
			return;
		}
	};

	println!();
	if project.workspaces.is_empty() {
		println!(
			"Lattice found no workspaces. List their directories under {} in lattice.json.",
			style("workspaces").bold()
		);
		return;
	}
	println!(
		"{}",
		style(format!(
			"{}:",
			plural(project.workspaces.len(), "workspace", "workspaces")
		))
		.bold()
	);
	let width = project
		.workspaces
		.iter()
		.map(|w| w.name.len())
		.max()
		.unwrap_or(0);
	for ws in &project.workspaces {
		let driver = ws.driver.as_ref().map(|d| d.tool.as_str()).unwrap_or("—");
		let tasks: Vec<&str> = ws.tasks.keys().map(String::as_str).take(6).collect();
		let more = ws.tasks.len().saturating_sub(tasks.len());
		let tail = if more > 0 {
			format!(" +{more}")
		} else {
			String::new()
		};
		println!(
			"  {:<width$}  {:<8} {}{}",
			ws.name,
			style(driver).dim(),
			tasks.join(" "),
			style(tail).dim(),
		);
	}

	let skipped: Vec<&str> = proposal
		.extra
		.iter()
		.filter(|c| c.driver.is_none())
		.map(|c| c.path.as_str())
		.collect();
	if !skipped.is_empty() {
		println!();
		println!(
			"{} Lattice cannot tell which tool runs {}. Set one with {} in {}.",
			style("·").dim(),
			skipped.join(", "),
			style("\"driver\"").bold(),
			style("overrides").bold()
		);
	}

	println!();
	let first = if project
		.workspaces
		.iter()
		.any(|w| w.tasks.contains_key("build"))
	{
		"lattice run build"
	} else {
		"lattice run <task>"
	};
	println!("next: {}", style(first).bold());
}

/// `services/api    go`
fn candidate_label(c: &WorkspaceCandidate) -> String {
	format!("{:<32} {}", c.path, c.driver.as_deref().unwrap_or("?"))
}

/// `node    22.11.0    .nvmrc`
fn pin_label(p: &EnginePin) -> String {
	format!("{:<12} {:<14} {}", p.engine, p.version, p.source)
}

fn plural(n: usize, one: &str, many: &str) -> String {
	format!("{n} {}", if n == 1 { one } else { many })
}
