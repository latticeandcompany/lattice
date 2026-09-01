//! Turning a scan of a directory into the files that make it a Lattice repo.
//!
//! Assembling the config and writing it are separate: a front end that wants to
//! show the user what it is about to write calls [`build_config`], renders the
//! result, and only then calls [`write_artifacts`]. Both front ends go through
//! here, so a config written from a window is byte-identical to one written by
//! `lattice init`.

use std::collections::HashSet;
use std::path::Path;

use anyhow::Result;
use serde_json::{json, Map, Value};

use lattice_config::schema::SCHEMA_JSON;
use lattice_workspace::scan::{DeclaredTasks, EnginePin, TaskDetail, WorkspaceCandidate};

/// The directory `lattice setup` writes its per-workspace install markers into,
/// one file per workspace. Named here because the line that ignores it has to
/// say the same thing.
pub const SETUP_MARKER_DIR: &str = ".lattice/setup";

/// Where `lattice setup` used to write its marker: one file of this name inside
/// every workspace directory. Still read, so upgrading mid-project does not
/// reinstall every workspace, and still ignored, so an old one left in a tree
/// stays untracked.
pub const LEGACY_SETUP_MARKER: &str = ".lattice-setup-marker";

/// Lines init ensures are present in `.gitignore`: the cache, provisioned
/// toolchains, installed binaries and setup markers are all per-machine
/// artifacts. The committed `.lattice/schema.json` stays tracked and is not
/// ignored, which is why `.lattice/` is listed a directory at a time.
pub const GITIGNORE_LINES: &[&str] = &[
	".lattice/cache/",
	".lattice/toolchains/",
	".lattice/bin/",
	".lattice/setup/",
	LEGACY_SETUP_MARKER,
];

/// The skeleton written when a scan turns up nothing and there is no one to ask.
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
///
/// `version` is written as `latticeVersion`. It is a parameter rather than the
/// running binary's own version because a front end may be scaffolding a repo
/// that pins a different one.
pub fn build_config(
	workspaces: &[&WorkspaceCandidate],
	pins: &[&EnginePin],
	version: &str,
) -> Value {
	let mut root = Map::new();
	root.insert("$schema".into(), json!(".lattice/schema.json"));
	root.insert("latticeVersion".into(), json!(version));

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

	let globals = union_globals(workspaces, |d| &d.global_dependencies);
	if !globals.is_empty() {
		root.insert("globalDependencies".into(), json!(globals));
	}
	let global_env = union_globals(workspaces, |d| &d.global_env);
	if !global_env.is_empty() {
		root.insert("globalEnv".into(), json!(global_env));
	}

	root.insert("tasks".into(), Value::Object(build_tasks(workspaces)));

	Value::Object(root)
}

/// The pipeline, taken from what the chosen workspaces' own task configuration
/// already declares.
///
/// A repo that runs eleven tasks through its task runner gets eleven tasks here,
/// not one: writing only `build` leaves every other task undeclared, and an
/// undeclared task is one `lattice run` refuses. Falling back to a lone `build`
/// is for the workspaces that declare no list to read — a Cargo or Go workspace
/// takes the task name on the command line and could name anything.
fn build_tasks(workspaces: &[&WorkspaceCandidate]) -> Map<String, Value> {
	let mut tasks: Map<String, Value> = Map::new();
	let mut named_only: HashSet<String> = HashSet::new();

	for c in workspaces {
		for task in &c.declared.tasks {
			match &task.detail {
				// Two workspaces declaring the same task is ordinary; the one that
				// says something about it is the one worth keeping.
				Some(detail) => {
					if !tasks.contains_key(&task.name) || named_only.remove(&task.name) {
						tasks.insert(task.name.clone(), detail_value(detail));
					}
				}
				None => {
					if !tasks.contains_key(&task.name) {
						named_only.insert(task.name.clone());
						tasks.insert(task.name.clone(), inferred_value(&task.name, workspaces));
					}
				}
			}
		}
	}

	if tasks.is_empty() && !workspaces.is_empty() {
		tasks.insert("build".into(), inferred_value("build", workspaces));
	}

	prune_depends_on(&mut tasks);
	tasks
}

/// What to write for a task the repo names but says nothing else about, as a
/// `package.json` script does. Only `build` gets a default, because only `build`
/// has one every ecosystem agrees on: a package is built after the packages it
/// depends on are.
fn inferred_value(name: &str, workspaces: &[&WorkspaceCandidate]) -> Value {
	if name != "build" {
		return Value::Object(Map::new());
	}
	let mut build = Map::new();
	build.insert("dependsOn".into(), json!(["^build"]));
	// Only claim an output directory the evidence supports; a repo with no
	// `package.json` workspace has no reason to cache `dist/`.
	if workspaces.iter().any(|c| c.marker == "package.json") {
		build.insert("outputs".into(), json!(["dist/**"]));
	}
	Value::Object(build)
}

fn detail_value(detail: &TaskDetail) -> Value {
	let mut task = Map::new();
	let mut list = |key: &str, value: &Option<Vec<String>>| {
		if let Some(value) = value {
			task.insert(key.into(), json!(value));
		}
	};
	list("dependsOn", &detail.depends_on);
	list("inputs", &detail.inputs);
	list("ignore", &detail.ignore);
	list("outputs", &detail.outputs);
	list("env", &detail.env);
	if let Some(persistent) = detail.persistent {
		task.insert("persistent".into(), json!(persistent));
	}
	if let Some(cache) = detail.cache {
		task.insert("cache".into(), json!(cache));
	}
	Value::Object(task)
}

/// Drop every `dependsOn` entry that names no task in the written pipeline.
/// Lattice refuses to load a config that depends on a task it does not define,
/// and a prerequisite the user never chose to import is not one to halt over.
fn prune_depends_on(tasks: &mut Map<String, Value>) {
	let names: HashSet<String> = tasks.keys().cloned().collect();
	for (name, task) in tasks.iter_mut() {
		let Some(deps) = task.get_mut("dependsOn").and_then(Value::as_array_mut) else {
			continue;
		};
		deps.retain(|dep| {
			let Some(dep) = dep.as_str() else {
				return false;
			};
			match dep.strip_prefix('^') {
				Some(bare) => names.contains(bare),
				None => dep != name && names.contains(dep),
			}
		});
		if deps.is_empty() {
			task.as_object_mut()
				.expect("a task is an object")
				.remove("dependsOn");
		}
	}
}

fn union_globals(
	workspaces: &[&WorkspaceCandidate],
	pick: impl Fn(&DeclaredTasks) -> &Vec<String>,
) -> Vec<String> {
	let mut seen = HashSet::new();
	let mut out = Vec::new();
	for value in workspaces.iter().flat_map(|c| pick(&c.declared)) {
		if seen.insert(value.clone()) {
			out.push(value.clone());
		}
	}
	out
}

/// The text [`write_artifacts`] would write for `config`, so a caller can show it
/// first. Pretty-printed with a trailing newline.
pub fn render_config(config: &Value) -> Result<String> {
	Ok(format!("{}\n", serde_json::to_string_pretty(config)?))
}

/// Write `lattice.json`, the committed schema copy, and the `.gitignore` lines.
/// Returns the repo-relative paths that changed.
pub fn write_artifacts(dir: &Path, config: &Value) -> Result<Vec<String>> {
	let mut written = Vec::new();

	std::fs::write(dir.join("lattice.json"), render_config(config)?)?;
	written.push("lattice.json".to_string());

	let lattice_dir = dir.join(".lattice");
	std::fs::create_dir_all(&lattice_dir)?;
	std::fs::write(lattice_dir.join("schema.json"), SCHEMA_JSON)?;
	written.push(".lattice/schema.json".to_string());

	let gitignore = dir.join(".gitignore");
	let existing = std::fs::read_to_string(&gitignore).unwrap_or_default();
	let updated = ensure_gitignore_lines(&existing, GITIGNORE_LINES);
	if updated != existing {
		std::fs::write(&gitignore, updated)?;
		written.push(".gitignore".to_string());
	}

	Ok(written)
}

#[cfg(test)]
mod tests {
	use super::*;
	use lattice_workspace::scan::TaskCandidate;

	fn candidate(name: &str, path: &str, marker: &str) -> WorkspaceCandidate {
		WorkspaceCandidate {
			name: name.to_string(),
			path: path.to_string(),
			marker: marker.to_string(),
			driver: None,
			default_selected: true,
			declared: DeclaredTasks::default(),
		}
	}

	#[test]
	fn the_declared_version_is_the_one_that_gets_written() {
		// Not the running binary's version: a front end may be scaffolding a repo
		// that pins something else, and writing the wrong pin here is silent.
		let config = build_config(&[], &[], "9.9.9");
		assert_eq!(config["latticeVersion"], json!("9.9.9"));
	}

	#[test]
	fn a_dist_output_needs_a_package_json_to_justify_it() {
		let node = candidate("web", "apps/web", "package.json");
		let rust = candidate("core", "crates/core", "Cargo.toml");

		let with_node = build_config(&[&node], &[], "1.0.0");
		assert_eq!(with_node["tasks"]["build"]["outputs"], json!(["dist/**"]));

		let without = build_config(&[&rust], &[], "1.0.0");
		assert!(without["tasks"]["build"].get("outputs").is_none());
	}

	#[test]
	fn engines_are_omitted_entirely_when_nothing_is_pinned() {
		let config = build_config(
			&[&candidate("web", "apps/web", "package.json")],
			&[],
			"1.0.0",
		);
		assert!(config.get("engines").is_none());
	}

	#[test]
	fn a_pin_becomes_a_string_form_engine() {
		let pin = EnginePin {
			engine: "node".to_string(),
			version: ">=26".to_string(),
			source: ".nvmrc".to_string(),
		};
		let config = build_config(&[], &[&pin], "1.0.0");
		assert_eq!(config["engines"]["node"], json!(">=26"));
	}

	#[test]
	fn rendering_ends_in_exactly_one_newline() {
		let text = render_config(&default_skeleton("1.0.0")).unwrap();
		assert!(text.ends_with("}\n"));
		assert!(!text.ends_with("}\n\n"));
	}

	#[test]
	fn writing_twice_leaves_gitignore_alone_the_second_time() {
		let dir = tempfile::tempdir().unwrap();
		let config = default_skeleton("1.0.0");

		let first = write_artifacts(dir.path(), &config).unwrap();
		assert!(first.contains(&".gitignore".to_string()));

		let second = write_artifacts(dir.path(), &config).unwrap();
		assert!(
			!second.contains(&".gitignore".to_string()),
			"an unchanged .gitignore must not be reported as written"
		);
	}

	#[test]
	fn every_ignored_path_is_a_machine_local_artifact() {
		let out = ensure_gitignore_lines("", GITIGNORE_LINES);
		for line in GITIGNORE_LINES {
			assert!(out.contains(line), "{line} missing from {out:?}");
		}
		// The schema copy is committed, so it must never be ignored.
		assert!(!out.contains(".lattice/schema.json"));
	}

	#[test]
	fn the_setup_marker_directory_is_ignored() {
		// The ignore line has to be the marker directory `setup` actually writes
		// into, plus the trailing slash that makes it a directory pattern.
		let expected = format!("{SETUP_MARKER_DIR}/");
		assert!(
			GITIGNORE_LINES.iter().any(|l| *l == expected),
			"{expected} missing from {GITIGNORE_LINES:?}"
		);

		let dir = tempfile::tempdir().unwrap();
		write_artifacts(dir.path(), &default_skeleton("1.0.0")).unwrap();
		let ignored = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
		assert!(
			ignored.lines().any(|l| l.trim() == expected),
			"the setup marker directory is missing from .gitignore:\n{ignored}"
		);
	}

	#[test]
	fn the_legacy_in_workspace_marker_stays_ignored() {
		// Someone upgrading has one of these in every workspace already. Dropping
		// the line would turn them all into untracked files in the source tree.
		assert!(GITIGNORE_LINES.contains(&LEGACY_SETUP_MARKER));

		let dir = tempfile::tempdir().unwrap();
		write_artifacts(dir.path(), &default_skeleton("1.0.0")).unwrap();
		let ignored = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
		assert!(
			ignored.lines().any(|l| l.trim() == LEGACY_SETUP_MARKER),
			"the legacy setup marker is missing from .gitignore:\n{ignored}"
		);
	}

	fn pin(engine: &str, version: &str) -> EnginePin {
		EnginePin {
			engine: engine.to_string(),
			version: version.to_string(),
			source: ".tool-versions".to_string(),
		}
	}

	#[test]
	fn the_skeleton_declares_a_build_task_and_no_workspaces() {
		let v = default_skeleton("1.2.3");
		assert_eq!(v["$schema"], json!(".lattice/schema.json"));
		assert_eq!(v["latticeVersion"], json!("1.2.3"));
		assert_eq!(v["workspaces"], json!([]));
		assert_eq!(v["tasks"]["build"]["dependsOn"], json!(["^build"]));
		assert_eq!(v["tasks"]["build"]["outputs"], json!(["dist/**"]));
	}

	#[test]
	fn scanned_workspaces_keep_their_scan_order() {
		let web = candidate("web", "apps/web", "package.json");
		let api = candidate("api", "services/api", "Cargo.toml");
		let node = pin("node", "22.11.0");

		let v = build_config(&[&web, &api], &[&node], "1.0.0");
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
	fn an_engine_only_config_declares_no_task_at_all() {
		// Nothing to build, so claiming a build task would be inventing one.
		let node = pin("node", "22.11.0");
		let v = build_config(&[], &[&node], "1.0.0");
		assert_eq!(v["workspaces"], json!([]));
		assert_eq!(v["tasks"], json!({}));
		assert_eq!(v["engines"]["node"], json!("22.11.0"));
	}

	#[test]
	fn appending_ignore_lines_twice_does_not_duplicate_them() {
		let once = ensure_gitignore_lines("", GITIGNORE_LINES);
		let twice = ensure_gitignore_lines(&once, GITIGNORE_LINES);
		assert_eq!(once, twice);
		assert_eq!(twice.matches(".lattice/cache/").count(), 1);
	}

	#[test]
	fn existing_ignore_content_is_kept_and_comes_first() {
		let existing = "node_modules/\ntarget/\n";
		let out = ensure_gitignore_lines(existing, GITIGNORE_LINES);
		assert!(out.starts_with("node_modules/\ntarget/\n"));
		assert!(out.contains(".lattice/cache/"));
	}

	#[test]
	fn a_line_already_present_is_not_added_again() {
		let out = ensure_gitignore_lines(".lattice/cache/\n", GITIGNORE_LINES);
		assert_eq!(out.matches(".lattice/cache/").count(), 1);
		assert!(out.contains(".lattice/toolchains/"));
	}
	fn task(name: &str, detail: Option<TaskDetail>) -> TaskCandidate {
		TaskCandidate {
			name: name.to_string(),
			detail,
		}
	}

	fn list(items: &[&str]) -> Option<Vec<String>> {
		Some(items.iter().map(|s| s.to_string()).collect())
	}

	/// The bug this exists for: a repo whose task runner declares a dozen tasks
	/// used to get a config with one, and every other task it already ran was
	/// undeclared from the moment init finished.
	#[test]
	fn every_declared_task_reaches_the_config() {
		let mut root = candidate("app", ".", "package.json");
		root.declared = DeclaredTasks {
			tasks: vec![
				task(
					"build",
					Some(TaskDetail {
						depends_on: list(&["^build"]),
						outputs: list(&["dist/**"]),
						..Default::default()
					}),
				),
				task(
					"dev",
					Some(TaskDetail {
						persistent: Some(true),
						cache: Some(false),
						..Default::default()
					}),
				),
				task("lint", Some(TaskDetail::default())),
			],
			global_dependencies: vec!["tsconfig.base.json".into()],
			global_env: vec!["CI".into()],
			source: "turbo.json".into(),
		};

		let config = build_config(&[&root], &[], "1.0.0");

		assert_eq!(
			config["tasks"],
			json!({
				"build": { "dependsOn": ["^build"], "outputs": ["dist/**"] },
				"dev": { "persistent": true, "cache": false },
				"lint": {}
			})
		);
		assert_eq!(config["globalDependencies"], json!(["tsconfig.base.json"]));
		assert_eq!(config["globalEnv"], json!(["CI"]));
	}

	/// Lattice refuses to load a config whose task depends on a task it does not
	/// define, so importing half a task graph has to leave a config that runs.
	#[test]
	fn a_dependency_on_a_task_that_was_not_imported_is_dropped() {
		let mut ws = candidate("app", ".", "package.json");
		ws.declared = DeclaredTasks {
			tasks: vec![task(
				"zip",
				Some(TaskDetail {
					depends_on: list(&["^build", "build", "zip"]),
					..Default::default()
				}),
			)],
			source: "turbo.json".into(),
			..Default::default()
		};

		let config = build_config(&[&ws], &[], "1.0.0");
		assert_eq!(config["tasks"], json!({ "zip": {} }));
	}

	/// Two workspaces naming the same task is ordinary in a monorepo. The one
	/// that says something about it is the one worth keeping, whichever came
	/// first.
	#[test]
	fn the_workspace_that_describes_a_shared_task_wins() {
		let mut scripts = candidate("web", "apps/web", "package.json");
		scripts.declared = DeclaredTasks {
			tasks: vec![task("build", None), task("test", None)],
			source: "package.json".into(),
			..Default::default()
		};
		let mut runner = candidate("root", ".", "package.json");
		runner.declared = DeclaredTasks {
			tasks: vec![task(
				"build",
				Some(TaskDetail {
					outputs: list(&["build/**"]),
					..Default::default()
				}),
			)],
			source: "turbo.json".into(),
			..Default::default()
		};

		let config = build_config(&[&scripts, &runner], &[], "1.0.0");
		assert_eq!(config["tasks"]["build"], json!({ "outputs": ["build/**"] }));
		assert_eq!(config["tasks"]["test"], json!({}));
	}

	/// A `cargo` or `go` workspace publishes no list of the tasks it accepts, so
	/// there is nothing to import and `build` remains the one safe proposal.
	#[test]
	fn a_workspace_that_declares_no_task_list_still_gets_a_build() {
		let rust = candidate("core", "crates/core", "Cargo.toml");
		let config = build_config(&[&rust], &[], "1.0.0");
		assert_eq!(
			config["tasks"],
			json!({ "build": { "dependsOn": ["^build"] } })
		);
	}
}
