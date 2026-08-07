use std::collections::HashSet;
use std::path::Path;

use anyhow::{bail, Result};
use clap::Args;
use console::style;
use dialoguer::{theme::ColorfulTheme, Input, MultiSelect, Select};
use serde_json::{json, Map, Value};

use lattice_output::{logo_for, teal, Theme};
use lattice_workspace::scan::{scan_engine_pins, scan_workspaces, EnginePin, WorkspaceCandidate};

use crate::cli::BIN_VERSION;
use lattice_config::schema::SCHEMA_JSON;

/// Lines that init ensures are present in `.gitignore`: the cache, provisioned
/// toolchains, and the installed binaries are all per-machine artifacts. The
/// committed `.lattice/schema.json` stays tracked and is not ignored.
const GITIGNORE_LINES: &[&str] = &[".lattice/cache/", ".lattice/toolchains/", ".lattice/bin/"];

#[derive(Args, Debug)]
#[command(long_about = "Scaffold a lattice.json in the current directory.\n\n\
Reads the repo first: every directory holding a manifest becomes a proposed \
workspace, and every tool version the repo already pins becomes a proposed \
engine. On a terminal you confirm the two lists; with --yes (or no TTY) it \
writes what it found. Also writes a committed .lattice/schema.json and ensures \
.gitignore covers local artifacts.")]
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
			bail!("lattice.json already exists (use --force to overwrite)");
		}

		// Never hang a pipeline: with no TTY, or with `-y`/`--yes`, take the
		// scan's own proposal instead of prompting.
		let tty = console::user_attended();

		// Lead an interactive init with the branded mark; skip it for pipes/CI
		// so scripted `init --yes` output stays clean.
		if tty {
			println!("{}", logo_for(theme));
			println!();
		}

		let candidates = scan_workspaces(&cwd);
		let pins = scan_engine_pins(&cwd);

		let config = if self.yes || !tty {
			let proposed: Vec<&WorkspaceCandidate> =
				candidates.iter().filter(|c| c.default_selected).collect();
			let pins: Vec<&EnginePin> = pins.iter().collect();
			if proposed.is_empty() && pins.is_empty() {
				default_skeleton(BIN_VERSION)
			} else {
				build_config(&proposed, &pins)
			}
		} else {
			confirm_scan(&candidates, &pins)?
		};

		write_artifacts(&cwd, &config)?;
		print_success(&config, &candidates);
		Ok(())
	}
}

/// The skeleton written when a scan of a bare directory turns up nothing and
/// there is no one to ask (`--yes`, or a pipeline).
pub fn default_skeleton(version: &str) -> Value {
	json!({
		"$schema": ".lattice/schema.json",
		"latticeVersion": version,
		"workspaces": [],
		"tasks": {
			"build": { "dependsOn": ["^build"], "outputs": ["dist/**"] }
		}
	})
}

/// Idempotently ensure each line in `lines` is present in `.gitignore` content.
/// Existing content and ordering are preserved; only missing lines are appended.
pub fn ensure_gitignore_lines(existing: &str, lines: &[&str]) -> String {
	let present: HashSet<&str> = existing.lines().map(|l| l.trim()).collect();
	let missing: Vec<&&str> = lines.iter().filter(|l| !present.contains(**l)).collect();
	if missing.is_empty() {
		return existing.to_string();
	}

	let mut out = existing.to_string();
	if !out.is_empty() && !out.ends_with('\n') {
		out.push('\n');
	}
	for line in missing {
		out.push_str(line);
		out.push('\n');
	}
	out
}

/// Assemble the config from the chosen workspaces and engine pins.
pub fn build_config(workspaces: &[&WorkspaceCandidate], pins: &[&EnginePin]) -> Value {
	let mut root = Map::new();
	root.insert("$schema".into(), json!(".lattice/schema.json"));
	root.insert("latticeVersion".into(), json!(BIN_VERSION));

	let ws: Vec<Value> = workspaces
		.iter()
		.map(|c| json!({ "name": c.name, "path": c.path }))
		.collect();
	root.insert("workspaces".into(), Value::Array(ws));

	if !pins.is_empty() {
		let mut engines = Map::new();
		for pin in pins {
			engines.insert(pin.engine.clone(), json!(pin.version));
		}
		root.insert("engines".into(), Value::Object(engines));
	}

	let mut tasks = Map::new();
	if !workspaces.is_empty() {
		let mut build = Map::new();
		build.insert("dependsOn".into(), json!(["^build"]));
		// Only claim an output directory the evidence supports; a repo with no
		// `package.json` workspace has no reason to cache `dist/`.
		if workspaces.iter().any(|c| c.marker == "package.json") {
			build.insert("outputs".into(), json!(["dist/**"]));
		}
		tasks.insert("build".into(), Value::Object(build));
	}
	root.insert("tasks".into(), Value::Object(tasks));

	Value::Object(root)
}

fn write_artifacts(cwd: &Path, config: &Value) -> Result<()> {
	let pretty = serde_json::to_string_pretty(config)?;
	std::fs::write(cwd.join("lattice.json"), format!("{pretty}\n"))?;

	let lattice_dir = cwd.join(".lattice");
	std::fs::create_dir_all(&lattice_dir)?;
	std::fs::write(lattice_dir.join("schema.json"), SCHEMA_JSON)?;

	let gi_path = cwd.join(".gitignore");
	let existing = std::fs::read_to_string(&gi_path).unwrap_or_default();
	let updated = ensure_gitignore_lines(&existing, GITIGNORE_LINES);
	if updated != existing {
		std::fs::write(&gi_path, updated)?;
	}
	Ok(())
}

fn print_success(config: &Value, candidates: &[WorkspaceCandidate]) {
	let check = teal().apply_to("✓");
	println!();
	println!("{} {}", check, style("wrote lattice.json").bold());
	println!("{} {}", check, style("wrote .lattice/schema.json").bold());
	println!("{} {}", check, style("updated .gitignore").bold());

	let written: Vec<&str> = config
		.get("workspaces")
		.and_then(Value::as_array)
		.map(|a| {
			a.iter()
				.filter_map(|w| w.get("path").and_then(Value::as_str))
				.collect()
		})
		.unwrap_or_default();

	// A workspace whose driver never resolved halts on its first run, so it is
	// found but not declared. Either way, say which directories those were.
	let (declared, held_back): (Vec<&WorkspaceCandidate>, Vec<&WorkspaceCandidate>) = candidates
		.iter()
		.filter(|c| c.driver.is_none())
		.partition(|c| written.contains(&c.path.as_str()));

	if !declared.is_empty() {
		println!();
		for c in &declared {
			println!(
				"{} no task driver resolved for {} — declare one in its {} or add {}",
				style("!").yellow(),
				style(&c.path).bold(),
				style("engines").bold(),
				style("scripts").bold()
			);
		}
	}
	if !held_back.is_empty() {
		let paths: Vec<&str> = held_back.iter().map(|c| c.path.as_str()).collect();
		println!();
		println!(
			"{} left out {}: no task driver resolved. Declare one in {} to add {}.",
			style("·").dim(),
			style(list_paths(&paths)).bold(),
			style("engines").bold(),
			if paths.len() == 1 { "it" } else { "them" }
		);
	}

	let has_workspaces = !written.is_empty();
	let has_engines = config
		.get("engines")
		.and_then(Value::as_object)
		.is_some_and(|m| !m.is_empty());

	println!();
	if has_workspaces {
		println!("next: {}", style("lattice run build").bold());
	} else if has_engines {
		println!("next: {}", style("lattice setup").bold());
	} else {
		println!(
			"next: declare a workspace in {}",
			style("lattice.json").bold()
		);
	}
}

/// Name up to three paths, then count the rest.
fn list_paths(paths: &[&str]) -> String {
	const SHOWN: usize = 3;
	if paths.len() <= SHOWN {
		return paths.join(", ");
	}
	format!(
		"{}, and {} more",
		paths[..SHOWN].join(", "),
		paths.len() - SHOWN
	)
}

/// Show what the scan found and let the user uncheck anything wrong. Falls
/// through to manual entry when the scan came up empty or was cleared.
fn confirm_scan(candidates: &[WorkspaceCandidate], pins: &[EnginePin]) -> Result<Value> {
	let theme = ColorfulTheme::default();

	let mut chosen_ws: Vec<&WorkspaceCandidate> = Vec::new();
	if !candidates.is_empty() {
		let labels: Vec<String> = candidates.iter().map(label_for_candidate).collect();
		let defaults: Vec<bool> = candidates.iter().map(|c| c.default_selected).collect();
		let picked = MultiSelect::with_theme(&theme)
			.with_prompt(plural(candidates.len(), "workspace", "workspaces"))
			.items(&labels)
			.defaults(&defaults)
			.interact()?;
		chosen_ws = picked.into_iter().map(|i| &candidates[i]).collect();
	}

	let mut chosen_pins: Vec<&EnginePin> = Vec::new();
	if !pins.is_empty() {
		let labels: Vec<String> = pins.iter().map(label_for_pin).collect();
		let picked = MultiSelect::with_theme(&theme)
			.with_prompt(plural(
				pins.len(),
				"pinned tool version",
				"pinned tool versions",
			))
			.items(&labels)
			.defaults(&vec![true; pins.len()])
			.interact()?;
		chosen_pins = picked.into_iter().map(|i| &pins[i]).collect();
	}

	if chosen_ws.is_empty() && chosen_pins.is_empty() {
		if candidates.is_empty() && pins.is_empty() {
			println!("Found no manifests and no pinned tool versions here.");
		}
		println!();
		return manual_entry(&theme);
	}

	Ok(build_config(&chosen_ws, &chosen_pins))
}

/// `apps/web    pnpm    package.json`
fn label_for_candidate(c: &WorkspaceCandidate) -> String {
	let driver = c.driver.as_deref().unwrap_or("—");
	format!("{:<28} {:<10} {}", c.path, driver, c.marker)
}

/// `node    22.11.0    .nvmrc`
fn label_for_pin(p: &EnginePin) -> String {
	format!("{:<12} {:<14} {}", p.engine, p.version, p.source)
}

fn plural(n: usize, one: &str, many: &str) -> String {
	let noun = if n == 1 { one } else { many };
	format!("found {n} {noun}")
}

/// The floor: a config with neither a workspace nor an engine does nothing, so
/// init will not write one. Keeps asking until there is at least one of either.
fn manual_entry(theme: &ColorfulTheme) -> Result<Value> {
	let mut workspaces: Vec<WorkspaceCandidate> = Vec::new();
	let mut pins: Vec<EnginePin> = Vec::new();

	loop {
		let have_any = !workspaces.is_empty() || !pins.is_empty();
		let mut options = vec!["a workspace to build", "a tool version to pin"];
		if have_any {
			options.push("nothing else — write the config");
		}
		let prompt = if have_any {
			"Add anything else?"
		} else {
			"What should Lattice manage here?"
		};

		let choice = Select::with_theme(theme)
			.with_prompt(prompt)
			.items(&options)
			.default(0)
			.interact()?;

		match choice {
			0 => workspaces.push(prompt_workspace(theme)?),
			1 => pins.push(prompt_pin(theme)?),
			_ => break,
		}
	}

	let ws: Vec<&WorkspaceCandidate> = workspaces.iter().collect();
	let pin_refs: Vec<&EnginePin> = pins.iter().collect();
	Ok(build_config(&ws, &pin_refs))
}

fn prompt_workspace(theme: &ColorfulTheme) -> Result<WorkspaceCandidate> {
	let name: String = Input::with_theme(theme)
		.with_prompt("workspace name")
		.validate_with(|input: &String| -> Result<(), &str> {
			if input.trim().is_empty() {
				Err("name must not be empty")
			} else {
				Ok(())
			}
		})
		.interact_text()?;
	let path: String = Input::with_theme(theme)
		.with_prompt("workspace path (a literal directory)")
		.with_initial_text(name.clone())
		.interact_text()?;

	let path = path.trim().to_string();
	Ok(WorkspaceCandidate {
		name: name.trim().to_string(),
		marker: String::new(),
		driver: lattice_workspace::detect_drivers(
			Path::new(&path),
			&lattice_config::EngineMap::new(),
		)
		.ok()
		.map(|d| d.tool),
		path,
		default_selected: true,
	})
}

fn prompt_pin(theme: &ColorfulTheme) -> Result<EnginePin> {
	let engines = lattice_config::well_known_engine_names();
	let idx = Select::with_theme(theme)
		.with_prompt("tool")
		.items(&engines)
		.default(0)
		.interact()?;
	let engine = engines[idx].to_string();
	let version: String = Input::with_theme(theme)
		.with_prompt(format!("version constraint for {engine} (e.g. >=20.0.0)"))
		.validate_with(|input: &String| -> Result<(), &str> {
			if input.trim().is_empty() {
				Err("a constraint must not be empty")
			} else {
				Ok(())
			}
		})
		.interact_text()?;

	Ok(EnginePin {
		engine,
		version: version.trim().to_string(),
		source: "you".to_string(),
	})
}

#[cfg(test)]
mod tests {
	use super::*;

	fn candidate(name: &str, path: &str, marker: &str) -> WorkspaceCandidate {
		WorkspaceCandidate {
			name: name.into(),
			path: path.into(),
			marker: marker.into(),
			driver: Some("pnpm".into()),
			default_selected: true,
		}
	}

	fn pin(engine: &str, version: &str) -> EnginePin {
		EnginePin {
			engine: engine.into(),
			version: version.into(),
			source: ".nvmrc".into(),
		}
	}

	#[test]
	fn default_skeleton_has_expected_shape() {
		let v = default_skeleton("1.2.3");
		assert_eq!(v["$schema"], json!(".lattice/schema.json"));
		assert_eq!(v["latticeVersion"], json!("1.2.3"));
		assert_eq!(v["workspaces"], json!([]));
		assert_eq!(v["tasks"]["build"]["dependsOn"], json!(["^build"]));
		assert_eq!(v["tasks"]["build"]["outputs"], json!(["dist/**"]));
	}

	#[test]
	fn config_carries_scanned_workspaces_and_engines() {
		let web = candidate("web", "apps/web", "package.json");
		let api = candidate("api", "services/api", "Cargo.toml");
		let node = pin("node", "22.11.0");

		let v = build_config(&[&web, &api], &[&node]);
		assert_eq!(
			v["workspaces"],
			json!([
				{ "name": "web", "path": "apps/web" },
				{ "name": "api", "path": "services/api" }
			])
		);
		assert_eq!(v["engines"]["node"], json!("22.11.0"));
		assert_eq!(v["tasks"]["build"]["dependsOn"], json!(["^build"]));
	}

	#[test]
	fn engines_key_is_absent_without_pins() {
		let web = candidate("web", "apps/web", "package.json");
		let v = build_config(&[&web], &[]);
		assert!(v.get("engines").is_none());
	}

	#[test]
	fn dist_outputs_only_follow_a_package_json_workspace() {
		let js = candidate("web", "apps/web", "package.json");
		let rust = candidate("api", "services/api", "Cargo.toml");

		let with_js = build_config(&[&js], &[]);
		assert_eq!(with_js["tasks"]["build"]["outputs"], json!(["dist/**"]));

		let without_js = build_config(&[&rust], &[]);
		assert!(without_js["tasks"]["build"].get("outputs").is_none());
	}

	#[test]
	fn engine_only_config_declares_no_build_task() {
		let node = pin("node", "22.11.0");
		let v = build_config(&[], &[&node]);
		assert_eq!(v["workspaces"], json!([]));
		assert_eq!(v["tasks"], json!({}));
		assert_eq!(v["engines"]["node"], json!("22.11.0"));
	}

	#[test]
	fn gitignore_appends_missing_lines() {
		let out = ensure_gitignore_lines("", GITIGNORE_LINES);
		assert!(out.contains(".lattice/cache/"));
		assert!(out.contains(".lattice/toolchains/"));
	}

	#[test]
	fn gitignore_is_idempotent() {
		let once = ensure_gitignore_lines("", GITIGNORE_LINES);
		let twice = ensure_gitignore_lines(&once, GITIGNORE_LINES);
		assert_eq!(once, twice, "appending twice must not duplicate lines");
		assert_eq!(twice.matches(".lattice/cache/").count(), 1);
		assert_eq!(twice.matches(".lattice/toolchains/").count(), 1);
	}

	#[test]
	fn gitignore_preserves_existing_lines() {
		let existing = "node_modules/\ntarget/\n";
		let out = ensure_gitignore_lines(existing, GITIGNORE_LINES);
		assert!(out.starts_with("node_modules/\ntarget/\n"));
		assert!(out.contains(".lattice/cache/"));
		assert!(out.contains(".lattice/toolchains/"));
	}

	#[test]
	fn gitignore_does_not_readd_present_line() {
		let existing = ".lattice/cache/\n";
		let out = ensure_gitignore_lines(existing, GITIGNORE_LINES);
		assert_eq!(out.matches(".lattice/cache/").count(), 1);
		assert!(out.contains(".lattice/toolchains/"));
	}

	#[test]
	fn bundled_schema_is_valid_json() {
		let parsed: Value = serde_json::from_str(SCHEMA_JSON).expect("schema.json is valid JSON");
		assert!(parsed.get("$defs").is_some());
	}
}
