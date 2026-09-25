//! Turning a scan of a directory into the files that make it a Lattice repo.
//! Most repos need very little: the workspaces a workspace file already names,
//! and every task a manifest defines, are found without being written down.
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

use serde::Serialize;

use lattice_config::schema::SCHEMA_JSON;
use lattice_config::LatticeConfig;
use lattice_workspace::members;
use lattice_workspace::scan::{
	scan_engine_pins, scan_workspaces, DeclaredTasks, EnginePin, TaskDetail, WorkspaceCandidate,
};

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

/// The config written when there is nothing to say: every workspace, every
/// dependency and every task comes from the repo's own files.
pub fn default_skeleton(version: &str) -> Value {
	json!({
		"$schema": ".lattice/schema.json",
		"latticeVersion": version
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

/// What a scan of a repo without a `lattice.json` found.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Proposal {
	/// Directories the repo's own workspace files already name. Nothing needs
	/// writing for these; they are listed so a front end can show them.
	pub native: Vec<String>,
	/// Directories no workspace file names, which the config has to list.
	pub extra: Vec<WorkspaceCandidate>,
	pub pins: Vec<EnginePin>,
}

/// Read `root` for everything a config could say.
///
/// A directory inside another project is that project's business — a Gradle
/// subproject, a fixture package, a crate inside an app — so only the outermost
/// projects no workspace file names are proposed. The root is proposed only
/// when nothing else is: a root beside other projects usually holds the repo's
/// tooling, and one that names workspaces is not one itself.
pub fn propose(root: &Path) -> Proposal {
	let native: Vec<String> = members::find_members(root, &LatticeConfig::default())
		.unwrap_or_default()
		.into_iter()
		.filter(|m| matches!(m.origin, members::Origin::Native(_)))
		.map(|m| m.rel)
		.collect();

	let candidates = scan_workspaces(root);
	let root_candidate = candidates.iter().find(|c| c.path == ".");
	let root_is_one_build = root_candidate
		.and_then(|c| c.driver.as_deref())
		.is_some_and(|d| matches!(d, "gradle" | "maven" | "dotnet"));

	let inside = |path: &str, other: &str| {
		other != "." && path != other && path.starts_with(&format!("{other}/"))
	};
	let mut extra: Vec<WorkspaceCandidate> = candidates
		.iter()
		.filter(|c| c.path != "." && !root_is_one_build)
		.filter(|c| !native.contains(&c.path))
		.filter(|c| {
			!candidates.iter().any(|o| inside(&c.path, &o.path))
				&& !native.iter().any(|n| inside(&c.path, n))
		})
		.cloned()
		.collect();

	if let Some(root_candidate) = root_candidate {
		if native.is_empty() && !extra.is_empty() {
			let mut offered = root_candidate.clone();
			offered.default_selected = false;
			extra.insert(0, offered);
		}
	}

	Proposal {
		native,
		extra,
		pins: scan_engine_pins(root),
	}
}

/// `lattice.json` `workspaces` entries for `paths`: siblings that are every
/// project under their parent collapse into `parent/*`, so a service added
/// there later is picked up without editing the config.
pub fn workspace_patterns(paths: &[&str], all_candidates: &[&str]) -> Vec<String> {
	let parent = |p: &str| p.rsplit_once('/').map(|(dir, _)| dir.to_string());
	let mut out: Vec<String> = Vec::new();
	for path in paths {
		let pattern = match parent(path) {
			Some(dir) => {
				let siblings: Vec<&&str> = all_candidates
					.iter()
					.filter(|c| parent(c).as_deref() == Some(dir.as_str()))
					.collect();
				let all_chosen = siblings.iter().all(|s| paths.contains(s));
				if siblings.len() > 1 && all_chosen {
					format!("{dir}/*")
				} else {
					path.to_string()
				}
			}
			None => path.to_string(),
		};
		if !out.contains(&pattern) {
			out.push(pattern);
		}
	}
	out
}

/// Assemble the config from the chosen extra workspaces and engine pins.
///
/// `version` is written as `latticeVersion`. It is a parameter rather than the
/// running binary's own version because a front end may be scaffolding a repo
/// that pins a different one.
pub fn build_config(
	workspaces: &[&WorkspaceCandidate],
	all_candidates: &[&str],
	pins: &[&EnginePin],
	version: &str,
) -> Value {
	let mut root = Map::new();
	root.insert("$schema".into(), json!(".lattice/schema.json"));
	root.insert("latticeVersion".into(), json!(version));

	let paths: Vec<&str> = workspaces.iter().map(|c| c.path.as_str()).collect();
	let patterns = workspace_patterns(&paths, all_candidates);
	if !patterns.is_empty() {
		root.insert("workspaces".into(), json!(patterns));
	}

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

	let tasks = imported_tasks(workspaces);
	if !tasks.is_empty() {
		root.insert("tasks".into(), Value::Object(tasks));
	}

	Value::Object(root)
}

/// The task settings a workspace's own task runner already spells out, such as
/// a `turbo.json`'s outputs and dependencies. A task a file only names needs no
/// entry: it runs by name as it is.
fn imported_tasks(workspaces: &[&WorkspaceCandidate]) -> Map<String, Value> {
	let mut tasks: Map<String, Value> = Map::new();
	for c in workspaces {
		for task in &c.declared.tasks {
			if let Some(detail) = &task.detail {
				if !tasks.contains_key(&task.name) {
					tasks.insert(task.name.clone(), detail_value(detail));
				}
			}
		}
	}
	prune_depends_on(&mut tasks);
	tasks.retain(|_, v| v.as_object().is_some_and(|o| !o.is_empty()));
	tasks
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

/// Drop every `dependsOn` entry that names no imported task. A prerequisite
/// the user never chose to import is not one to halt a run over.
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
	let typed: LatticeConfig = serde_json::from_value(config.clone())?;
	Ok(lattice_config::to_config_text(&typed))
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

	fn write(root: &Path, rel: &str, contents: &str) {
		let path = root.join(rel);
		std::fs::create_dir_all(path.parent().unwrap()).unwrap();
		std::fs::write(path, contents).unwrap();
	}

	fn pin(engine: &str, version: &str) -> EnginePin {
		EnginePin {
			engine: engine.to_string(),
			version: version.to_string(),
			source: ".tool-versions".to_string(),
		}
	}

	#[test]
	fn the_skeleton_says_nothing_it_does_not_have_to() {
		let skeleton = default_skeleton("2.0.0");
		assert_eq!(skeleton["latticeVersion"], "2.0.0");
		assert!(skeleton.get("workspaces").is_none());
		assert!(skeleton.get("tasks").is_none());
		lattice_config::parse_config(&render_config(&skeleton).unwrap()).unwrap();
	}

	#[test]
	fn a_js_monorepo_needs_only_the_projects_its_workspace_file_misses() {
		let dir = tempfile::TempDir::new().unwrap();
		let root = dir.path();
		write(root, "package.json", r#"{"workspaces": ["packages/*"]}"#);
		write(root, "package-lock.json", "{}");
		write(root, "packages/ui/package.json", r#"{"name": "ui"}"#);
		write(root, "services/api/go.mod", "module api\n");
		write(root, "services/api/go.sum", "");
		write(root, "services/worker/go.mod", "module worker\n");
		write(root, "services/worker/go.sum", "");

		let p = propose(root);
		assert_eq!(p.native, ["packages/ui"]);
		let extra: Vec<&str> = p.extra.iter().map(|c| c.path.as_str()).collect();
		assert_eq!(extra, ["services/api", "services/worker"]);

		let chosen: Vec<&WorkspaceCandidate> = p.extra.iter().collect();
		let config = build_config(&chosen, &extra, &[], "2.0.0");
		assert_eq!(config["workspaces"], json!(["services/*"]));
		assert!(config.get("tasks").is_none());
	}

	#[test]
	fn a_gradle_build_is_proposed_as_one_project() {
		let dir = tempfile::TempDir::new().unwrap();
		let root = dir.path();
		write(root, "settings.gradle.kts", "include(\":app\")\n");
		write(root, "gradlew", "");
		write(root, "app/build.gradle.kts", "");
		let p = propose(root);
		assert!(p.extra.is_empty(), "{:?}", p.extra);
		assert!(p.native.is_empty());
	}

	#[test]
	fn one_of_two_siblings_is_written_literally() {
		assert_eq!(
			workspace_patterns(&["services/api"], &["services/api", "services/old"]),
			["services/api"]
		);
		assert_eq!(
			workspace_patterns(&["tools"], &["tools", "services/api"]),
			["tools"]
		);
	}

	#[test]
	fn a_pin_becomes_a_string_form_engine() {
		let node = pin("node", "22.11.0");
		let config = build_config(&[], &[], &[&node], "2.0.0");
		assert_eq!(config["engines"]["node"], "22.11.0");
		assert!(config.get("workspaces").is_none());
	}

	#[test]
	fn turbo_task_settings_carry_over_and_bare_names_do_not() {
		use lattice_workspace::scan::TaskCandidate;
		let mut c = WorkspaceCandidate {
			name: "web".into(),
			path: "web".into(),
			marker: "package.json".into(),
			driver: Some("turbo".into()),
			default_selected: true,
			declared: DeclaredTasks::default(),
		};
		c.declared.tasks = vec![
			TaskCandidate {
				name: "build".into(),
				detail: Some(TaskDetail {
					depends_on: Some(vec!["^build".into(), "codegen".into()]),
					outputs: Some(vec!["dist/**".into()]),
					..Default::default()
				}),
			},
			TaskCandidate {
				name: "lint".into(),
				detail: None,
			},
		];
		let config = build_config(&[&c], &["web"], &[], "2.0.0");
		assert_eq!(config["tasks"]["build"]["outputs"], json!(["dist/**"]));
		assert_eq!(config["tasks"]["build"]["dependsOn"], json!(["^build"]));
		assert!(config["tasks"].get("lint").is_none());
	}

	#[test]
	fn rendering_ends_in_exactly_one_newline() {
		let text = render_config(&default_skeleton("2.0.0")).unwrap();
		assert!(text.ends_with("}\n") && !text.ends_with("\n\n"));
	}

	#[test]
	fn writing_twice_leaves_gitignore_alone_the_second_time() {
		let dir = tempfile::TempDir::new().unwrap();
		let config = default_skeleton("2.0.0");
		let first = write_artifacts(dir.path(), &config).unwrap();
		assert!(first.contains(&".gitignore".to_string()));
		let second = write_artifacts(dir.path(), &config).unwrap();
		assert!(!second.contains(&".gitignore".to_string()));
	}

	#[test]
	fn every_ignored_path_is_a_machine_local_artifact() {
		for line in GITIGNORE_LINES {
			assert_ne!(*line, ".lattice/schema.json", "the schema is committed");
			assert_ne!(
				*line, ".lattice/",
				"ignoring all of .lattice hides the schema"
			);
		}
		assert!(GITIGNORE_LINES.contains(&".lattice/setup/"));
		assert!(GITIGNORE_LINES.contains(&LEGACY_SETUP_MARKER));
	}

	#[test]
	fn appending_ignore_lines_twice_does_not_duplicate_them() {
		let once = ensure_gitignore_lines("", GITIGNORE_LINES);
		assert_eq!(ensure_gitignore_lines(&once, GITIGNORE_LINES), once);
	}

	#[test]
	fn existing_ignore_content_is_kept_and_comes_first() {
		let out = ensure_gitignore_lines("node_modules/", &[".lattice/cache/"]);
		assert_eq!(out, "node_modules/\n.lattice/cache/\n");
	}
}
