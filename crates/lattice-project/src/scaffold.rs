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
use lattice_workspace::scan::{EnginePin, WorkspaceCandidate};

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

	fn candidate(name: &str, path: &str, marker: &str) -> WorkspaceCandidate {
		WorkspaceCandidate {
			name: name.to_string(),
			path: path.to_string(),
			marker: marker.to_string(),
			driver: None,
			default_selected: true,
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
}
