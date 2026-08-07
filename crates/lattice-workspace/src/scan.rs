//! Reading a repo that has no `lattice.json` yet.
//!
//! [`scan_workspaces`] walks the tree for directories holding a manifest
//! Lattice recognizes; [`scan_engine_pins`] reads the tool versions the repo
//! already records in its native files. Both report what is on disk and stop
//! there — deciding what goes in the config is the caller's job.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use ignore::WalkBuilder;
use indexmap::IndexMap;
use lattice_config::{is_well_known_engine, EngineMap};

use crate::{detect_drivers, ECOSYSTEM_MARKERS};

/// How far below the root the walk goes. Deep enough for `apps/web/packages/ui`,
/// shallow enough that a large tree does not stall the scan.
const MAX_SCAN_DEPTH: usize = 5;

/// Directories that never hold a workspace worth proposing: dependency trees,
/// build output, and tool state. Hidden directories are skipped by the walker.
const SKIP_DIRS: &[&str] = &[
	"node_modules",
	"target",
	"dist",
	"build",
	"out",
	"vendor",
	"venv",
	"__pycache__",
	"coverage",
	"testdata",
	"fixtures",
];

/// Extensions matched by name-less .NET project files.
const DOTNET_EXTS: &[&str] = &["sln", "csproj", "fsproj", "vbproj"];

/// A directory that looks like a workspace because it holds a manifest Lattice
/// recognizes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceCandidate {
	/// Proposed workspace name, unique across the returned set.
	pub name: String,
	/// Path relative to the repo root, forward-slashed. The root itself is `"."`.
	pub path: String,
	/// The manifest that made this directory a candidate.
	pub marker: String,
	/// The task driver detection resolves here, when it resolves to one.
	pub driver: Option<String>,
	/// Whether to propose this one pre-selected. A candidate with no resolved
	/// driver, and a repo root that only exists to declare members, are offered
	/// alongside the rest but start unselected.
	pub default_selected: bool,
}

/// A tool version the repo already pins in one of its own files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnginePin {
	/// A well-known engine name.
	pub engine: String,
	/// The constraint to write, taken verbatim from the file.
	pub version: String,
	/// The file the version came from, for display.
	pub source: String,
}

/// Every directory under `root` that holds a recognized manifest, sorted by
/// path. Skips [`SKIP_DIRS`], hidden directories, and gitignored paths.
pub fn scan_workspaces(root: &Path) -> Vec<WorkspaceCandidate> {
	let walker = WalkBuilder::new(root)
		.max_depth(Some(MAX_SCAN_DEPTH))
		// Honor .gitignore even when the tree is not a git repo yet.
		.require_git(false)
		.parents(false)
		.filter_entry(|entry| {
			entry.depth() == 0
				|| entry
					.file_name()
					.to_str()
					.map(|name| !SKIP_DIRS.contains(&name))
					.unwrap_or(true)
		})
		.build();

	let mut found: Vec<WorkspaceCandidate> = Vec::new();
	for entry in walker.flatten() {
		if !entry.file_type().is_some_and(|t| t.is_dir()) {
			continue;
		}
		let path = entry.path();
		let Some(marker) = marker_for(path) else {
			continue;
		};
		let rel = relative_path(root, path);
		let driver = detect_drivers(path, &EngineMap::new()).ok().map(|d| d.tool);
		found.push(WorkspaceCandidate {
			name: suggested_name(root, path),
			path: rel,
			marker,
			// Declaring a workspace whose driver stays ambiguous halts the very
			// next run, so propose it without pre-selecting it.
			default_selected: driver.is_some(),
			driver,
		});
	}

	found.sort_by(|a, b| a.path.cmp(&b.path));

	// A root that declares members is not itself a workspace often enough that
	// proposing it selected would be wrong; leave it offered but unchecked.
	if found.len() > 1 {
		if let Some(root_entry) = found.iter_mut().find(|c| c.path == ".") {
			root_entry.default_selected = false;
		}
	}

	disambiguate_names(&mut found);
	found
}

/// The tool versions recorded in the repo's own files, most deliberate source
/// first. One entry per engine: the first source to name an engine wins.
pub fn scan_engine_pins(root: &Path) -> Vec<EnginePin> {
	let mut pins: IndexMap<String, EnginePin> = IndexMap::new();

	let mut add = |engine: &str, version: &str, source: &str| {
		let version = version.trim();
		if version.is_empty() || !is_well_known_engine(engine) {
			return;
		}
		pins.entry(engine.to_string()).or_insert_with(|| EnginePin {
			engine: engine.to_string(),
			version: version.to_string(),
			source: source.to_string(),
		});
	};

	for (tool, version) in read_tool_versions_pins(root) {
		add(&tool, &version, ".tool-versions");
	}
	if let Some(v) = read_trimmed(root, ".nvmrc") {
		add("node", v.trim_start_matches('v'), ".nvmrc");
	}
	if let Some(v) = read_rust_toolchain(root) {
		add("rust", &v, "rust-toolchain");
	}
	if let Some(v) = read_trimmed(root, ".python-version") {
		add("python", &v, ".python-version");
	}
	if let Some(v) = read_trimmed(root, ".ruby-version") {
		add("ruby", &v, ".ruby-version");
	}
	if let Some(v) = read_trimmed(root, ".java-version") {
		add("java", &v, ".java-version");
	}
	for (tool, version, field) in read_package_json_pins(root) {
		add(&tool, &version, &format!("package.json {field}"));
	}
	if let Some(v) = read_go_toolchain(root) {
		add("go", &v, "go.mod");
	}

	pins.into_values().collect()
}

fn marker_for(path: &Path) -> Option<String> {
	for (marker, _) in ECOSYSTEM_MARKERS {
		if path.join(marker).exists() {
			return Some((*marker).to_string());
		}
	}
	dotnet_project_file(path)
}

/// .NET projects carry the project's own name, so they match by extension.
fn dotnet_project_file(path: &Path) -> Option<String> {
	let entries = std::fs::read_dir(path).ok()?;
	let mut names: Vec<String> = entries
		.flatten()
		.filter(|e| {
			e.path()
				.extension()
				.and_then(|x| x.to_str())
				.is_some_and(|ext| DOTNET_EXTS.contains(&ext))
		})
		.filter_map(|e| e.file_name().to_str().map(String::from))
		.collect();
	names.sort();
	names.into_iter().next()
}

fn relative_path(root: &Path, path: &Path) -> String {
	let rel = path.strip_prefix(root).unwrap_or(path);
	let joined: Vec<String> = rel
		.components()
		.map(|c| c.as_os_str().to_string_lossy().into_owned())
		.collect();
	if joined.is_empty() {
		".".to_string()
	} else {
		joined.join("/")
	}
}

fn suggested_name(root: &Path, path: &Path) -> String {
	let dir = if path == root { root } else { path };
	dir.file_name()
		.and_then(|n| n.to_str())
		.map(String::from)
		.unwrap_or_else(|| "root".to_string())
}

/// Two directories can share a base name (`apps/web`, `sites/web`). Where they
/// do, name both after their full path so the config stays unambiguous.
fn disambiguate_names(candidates: &mut [WorkspaceCandidate]) {
	let mut counts: HashMap<String, usize> = HashMap::new();
	for c in candidates.iter() {
		*counts.entry(c.name.clone()).or_default() += 1;
	}
	for c in candidates.iter_mut() {
		if counts.get(&c.name).copied().unwrap_or(0) > 1 && c.path != "." {
			c.name = c.path.replace('/', "-");
		}
	}
}

fn read_trimmed(root: &Path, file: &str) -> Option<String> {
	let raw = std::fs::read_to_string(root.join(file)).ok()?;
	let first = raw.lines().find(|l| !l.trim().is_empty())?;
	let trimmed = first.trim();
	if trimmed.is_empty() {
		None
	} else {
		Some(trimmed.to_string())
	}
}

fn read_tool_versions_pins(root: &Path) -> Vec<(String, String)> {
	let Ok(raw) = std::fs::read_to_string(root.join(".tool-versions")) else {
		return Vec::new();
	};
	raw.lines()
		.filter(|l| !l.trim_start().starts_with('#'))
		.filter_map(|line| {
			let mut parts = line.split_whitespace();
			let tool = parts.next()?;
			let version = parts.next()?;
			Some((tool.to_string(), version.to_string()))
		})
		.collect()
}

/// `rust-toolchain.toml` holds `channel = "1.83.0"` under `[toolchain]`; the
/// legacy `rust-toolchain` file holds the bare channel. Read by line rather
/// than by TOML parse: one key is all this needs.
fn read_rust_toolchain(root: &Path) -> Option<String> {
	if let Ok(raw) = std::fs::read_to_string(root.join("rust-toolchain.toml")) {
		for line in raw.lines() {
			let line = line.trim();
			let Some(value) = line.strip_prefix("channel") else {
				continue;
			};
			let value = value.trim_start().strip_prefix('=')?.trim();
			return Some(value.trim_matches('"').trim_matches('\'').to_string());
		}
		return None;
	}
	read_trimmed(root, "rust-toolchain")
}

/// `packageManager` pins one tool exactly (`"pnpm@8.6.0"`); the `engines` block
/// carries constraints for others.
fn read_package_json_pins(root: &Path) -> Vec<(String, String, String)> {
	let Ok(raw) = std::fs::read_to_string(root.join("package.json")) else {
		return Vec::new();
	};
	let Ok(json) = serde_json::from_str::<serde_json::Value>(&raw) else {
		return Vec::new();
	};

	let mut out = Vec::new();
	if let Some(pm) = json.get("packageManager").and_then(|v| v.as_str()) {
		if let Some((name, version)) = pm.split_once('@') {
			// Corepack allows a "pnpm@8.6.0+sha512..." integrity suffix.
			let version = version.split('+').next().unwrap_or(version);
			out.push((
				name.to_string(),
				version.to_string(),
				"packageManager".to_string(),
			));
		}
	}
	if let Some(engines) = json.get("engines").and_then(|v| v.as_object()) {
		for (name, value) in engines {
			if let Some(constraint) = value.as_str() {
				out.push((name.clone(), constraint.to_string(), "engines".to_string()));
			}
		}
	}
	out
}

/// A `toolchain go1.22.0` line pins exactly; the `go 1.22` directive is the
/// language version and stands in when there is no toolchain line.
fn read_go_toolchain(root: &Path) -> Option<String> {
	let raw = std::fs::read_to_string(root.join("go.mod")).ok()?;
	let mut directive = None;
	for line in raw.lines() {
		let line = line.trim();
		if let Some(rest) = line.strip_prefix("toolchain ") {
			return Some(rest.trim().trim_start_matches("go").to_string());
		}
		if directive.is_none() {
			if let Some(rest) = line.strip_prefix("go ") {
				directive = Some(rest.trim().to_string());
			}
		}
	}
	directive
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::fs;
	use tempfile::TempDir;

	fn write(root: &Path, rel: &str, contents: &str) {
		let path = root.join(rel);
		fs::create_dir_all(path.parent().unwrap()).unwrap();
		fs::write(path, contents).unwrap();
	}

	#[test]
	fn finds_manifests_in_nested_directories() {
		let tmp = TempDir::new().unwrap();
		write(tmp.path(), "apps/web/package.json", "{}");
		write(tmp.path(), "apps/web/pnpm-lock.yaml", "");
		write(tmp.path(), "services/api/Cargo.toml", "[package]");
		write(tmp.path(), "tools/cli/go.mod", "module x");

		let found = scan_workspaces(tmp.path());
		let paths: Vec<&str> = found.iter().map(|c| c.path.as_str()).collect();
		assert_eq!(paths, vec!["apps/web", "services/api", "tools/cli"]);
		assert_eq!(found[0].driver.as_deref(), Some("pnpm"));
		assert_eq!(found[0].marker, "package.json");
	}

	#[test]
	fn skips_dependency_and_output_directories() {
		let tmp = TempDir::new().unwrap();
		write(tmp.path(), "apps/web/package.json", "{}");
		write(tmp.path(), "apps/web/node_modules/dep/package.json", "{}");
		write(tmp.path(), "target/debug/build/Cargo.toml", "[package]");
		write(tmp.path(), "dist/package.json", "{}");

		let paths: Vec<String> = scan_workspaces(tmp.path())
			.into_iter()
			.map(|c| c.path)
			.collect();
		assert_eq!(paths, vec!["apps/web"]);
	}

	#[test]
	fn skips_gitignored_and_hidden_directories() {
		let tmp = TempDir::new().unwrap();
		write(tmp.path(), ".gitignore", "generated/\n");
		write(tmp.path(), "generated/proto/package.json", "{}");
		write(tmp.path(), ".cache/pkg/package.json", "{}");
		write(tmp.path(), "apps/web/package.json", "{}");

		let paths: Vec<String> = scan_workspaces(tmp.path())
			.into_iter()
			.map(|c| c.path)
			.collect();
		assert_eq!(paths, vec!["apps/web"]);
	}

	#[test]
	fn root_only_repo_is_a_selected_candidate() {
		let tmp = TempDir::new().unwrap();
		write(tmp.path(), "Cargo.toml", "[package]");
		write(tmp.path(), "Cargo.lock", "");

		let found = scan_workspaces(tmp.path());
		assert_eq!(found.len(), 1);
		assert_eq!(found[0].path, ".");
		assert!(found[0].default_selected);
	}

	#[test]
	fn root_alongside_members_is_offered_unselected() {
		let tmp = TempDir::new().unwrap();
		write(tmp.path(), "Cargo.toml", "[workspace]");
		write(tmp.path(), "Cargo.lock", "");
		write(tmp.path(), "crates/a/Cargo.toml", "[package]");
		write(tmp.path(), "crates/a/Cargo.lock", "");

		let found = scan_workspaces(tmp.path());
		let root = found.iter().find(|c| c.path == ".").unwrap();
		let member = found.iter().find(|c| c.path == "crates/a").unwrap();
		assert!(!root.default_selected);
		assert!(member.default_selected);
	}

	#[test]
	fn a_candidate_with_no_driver_is_offered_unselected() {
		// A bare `Cargo.toml` with the lock at the repo root is not enough
		// evidence to drive tasks, and declaring it would halt the next run.
		let tmp = TempDir::new().unwrap();
		write(tmp.path(), "crates/a/Cargo.toml", "[package]");
		write(tmp.path(), "apps/web/package.json", "{}");
		write(tmp.path(), "apps/web/package-lock.json", "{}");

		let found = scan_workspaces(tmp.path());
		let bare = found.iter().find(|c| c.path == "crates/a").unwrap();
		let driven = found.iter().find(|c| c.path == "apps/web").unwrap();
		assert_eq!(bare.driver, None);
		assert!(
			!bare.default_selected,
			"an undriveable candidate is offered, not proposed"
		);
		assert_eq!(driven.driver.as_deref(), Some("npm"));
		assert!(driven.default_selected);
	}

	#[test]
	fn duplicate_base_names_become_path_names() {
		let tmp = TempDir::new().unwrap();
		write(tmp.path(), "apps/web/package.json", "{}");
		write(tmp.path(), "sites/web/package.json", "{}");
		write(tmp.path(), "tools/cli/go.mod", "module x");

		let found = scan_workspaces(tmp.path());
		let name_for = |p: &str| {
			found
				.iter()
				.find(|c| c.path == p)
				.map(|c| c.name.clone())
				.unwrap()
		};
		assert_eq!(name_for("apps/web"), "apps-web");
		assert_eq!(name_for("sites/web"), "sites-web");
		assert_eq!(name_for("tools/cli"), "cli");
	}

	#[test]
	fn dotnet_projects_are_found_by_extension() {
		let tmp = TempDir::new().unwrap();
		write(tmp.path(), "src/Api/Api.csproj", "<Project />");

		let found = scan_workspaces(tmp.path());
		assert_eq!(found.len(), 1);
		assert_eq!(found[0].marker, "Api.csproj");
	}

	#[test]
	fn reads_pins_from_native_files() {
		let tmp = TempDir::new().unwrap();
		write(tmp.path(), ".nvmrc", "v22.11.0\n");
		write(
			tmp.path(),
			"rust-toolchain.toml",
			"[toolchain]\nchannel = \"1.83.0\"\n",
		);
		write(tmp.path(), ".python-version", "3.12.1\n");

		let pins = scan_engine_pins(tmp.path());
		let find = |engine: &str| pins.iter().find(|p| p.engine == engine).unwrap();
		assert_eq!(find("node").version, "22.11.0");
		assert_eq!(find("node").source, ".nvmrc");
		assert_eq!(find("rust").version, "1.83.0");
		assert_eq!(find("python").version, "3.12.1");
	}

	#[test]
	fn tool_versions_wins_over_other_sources() {
		let tmp = TempDir::new().unwrap();
		write(tmp.path(), ".tool-versions", "node 20.11.1\nruby 3.3.0\n");
		write(tmp.path(), ".nvmrc", "22.11.0\n");

		let pins = scan_engine_pins(tmp.path());
		let node = pins.iter().find(|p| p.engine == "node").unwrap();
		assert_eq!(node.version, "20.11.1");
		assert_eq!(node.source, ".tool-versions");
		assert!(pins.iter().any(|p| p.engine == "ruby"));
	}

	#[test]
	fn reads_package_manager_and_engines_from_package_json() {
		let tmp = TempDir::new().unwrap();
		write(
			tmp.path(),
			"package.json",
			r#"{ "packageManager": "pnpm@8.6.0+sha512.abc", "engines": { "node": ">=20" } }"#,
		);

		let pins = scan_engine_pins(tmp.path());
		let pnpm = pins.iter().find(|p| p.engine == "pnpm").unwrap();
		assert_eq!(pnpm.version, "8.6.0");
		assert_eq!(pnpm.source, "package.json packageManager");
		let node = pins.iter().find(|p| p.engine == "node").unwrap();
		assert_eq!(node.version, ">=20");
	}

	#[test]
	fn go_toolchain_line_beats_the_language_directive() {
		let tmp = TempDir::new().unwrap();
		write(
			tmp.path(),
			"go.mod",
			"module x\n\ngo 1.21\n\ntoolchain go1.22.0\n",
		);

		let pins = scan_engine_pins(tmp.path());
		assert_eq!(
			pins.iter().find(|p| p.engine == "go").unwrap().version,
			"1.22.0"
		);
	}

	#[test]
	fn unknown_engine_names_are_dropped() {
		let tmp = TempDir::new().unwrap();
		write(
			tmp.path(),
			".tool-versions",
			"nodejs 20.11.1\nnode 20.11.1\n",
		);

		let pins = scan_engine_pins(tmp.path());
		assert_eq!(pins.len(), 1);
		assert_eq!(pins[0].engine, "node");
	}

	#[test]
	fn empty_repo_yields_nothing() {
		let tmp = TempDir::new().unwrap();
		assert!(scan_workspaces(tmp.path()).is_empty());
		assert!(scan_engine_pins(tmp.path()).is_empty());
	}
}
