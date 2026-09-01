//! Reading a repo that has no `lattice.json` yet.
//!
//! [`scan_workspaces`] walks the tree for directories holding a manifest
//! Lattice recognizes; [`scan_engine_pins`] reads the tool versions the repo
//! already records in its native files. Both report what is on disk and stop
//! there — deciding what goes in the config is the caller's job.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

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
	/// The tasks the directory's driver already knows how to run.
	#[serde(default)]
	pub declared: DeclaredTasks,
}

/// What a directory's own task configuration says, read verbatim.
///
/// Only the driver's file is read. A `turbo.json` workspace runs
/// `turbo run <task>`, so its `package.json` scripts are not tasks Lattice can
/// drive there, and proposing them would write a config whose every run fails.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DeclaredTasks {
	pub tasks: Vec<TaskCandidate>,
	/// Files outside any workspace that every task's result depends on.
	pub global_dependencies: Vec<String>,
	/// Environment variables every task's result depends on.
	pub global_env: Vec<String>,
	/// The file the declarations came from, for display. Empty when none were.
	pub source: String,
}

/// One task a directory's task configuration declares.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskCandidate {
	pub name: String,
	/// What the file says about the task. `None` where the file lists names and
	/// nothing else, the way `package.json` scripts do.
	#[serde(default)]
	pub detail: Option<TaskDetail>,
}

/// The parts of a declared task that carry over to a Lattice pipeline task.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TaskDetail {
	pub depends_on: Option<Vec<String>>,
	pub inputs: Option<Vec<String>>,
	pub ignore: Option<Vec<String>>,
	pub outputs: Option<Vec<String>>,
	pub env: Option<Vec<String>>,
	pub persistent: Option<bool>,
	pub cache: Option<bool>,
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
			declared: scan_declared_tasks(path, driver.as_deref()),
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

/// The tasks `driver` can already run in `path`, read from the file the driver
/// itself reads.
///
/// Empty for a driver that takes the task name straight on its command line —
/// `cargo`, `go`, `gradle` and the rest publish no list of what they will
/// accept, so there is nothing to read and nothing to propose.
pub fn scan_declared_tasks(path: &Path, driver: Option<&str>) -> DeclaredTasks {
	let Some(driver) = driver else {
		return DeclaredTasks::default();
	};
	match driver {
		"turbo" => turbo_tasks(path),
		"nx" => json_keys(path, "nx.json", "targetDefaults", true),
		"npm" | "pnpm" | "yarn" | "bun" => json_keys(path, "package.json", "scripts", false),
		"composer" => json_keys(path, "composer.json", "scripts", false),
		"deno" => {
			if path.join("deno.jsonc").exists() {
				json_keys(path, "deno.jsonc", "tasks", true)
			} else {
				json_keys(path, "deno.json", "tasks", true)
			}
		}
		"just" => just_recipes(path),
		"task" => taskfile_tasks(path),
		"pdm" => toml_keys(path, "pyproject.toml", "tool.pdm.scripts"),
		"poetry" => toml_keys(path, "pyproject.toml", "tool.poetry.scripts"),
		"uv" => toml_keys(path, "pyproject.toml", "project.scripts"),
		"pipenv" => toml_keys(path, "Pipfile", "scripts"),
		_ => DeclaredTasks::default(),
	}
}

/// A `turbo.json`'s task map, with each task's own cache settings carried over.
/// `tasks` is the current key; `pipeline` is what the same map was called
/// before, and a repo that has not migrated still runs.
fn turbo_tasks(path: &Path) -> DeclaredTasks {
	let Some(json) = read_json(&path.join("turbo.json"), true) else {
		return DeclaredTasks::default();
	};
	let map = json
		.get("tasks")
		.or_else(|| json.get("pipeline"))
		.and_then(Value::as_object);

	let mut declared = DeclaredTasks {
		global_dependencies: string_list(json.get("globalDependencies")),
		global_env: string_list(json.get("globalEnv")),
		source: "turbo.json".to_string(),
		..Default::default()
	};
	for (name, value) in map.into_iter().flatten() {
		// A `web#build` entry configures one package's task, not a task of the
		// repo, and Lattice has no name for it.
		if name.contains('#') {
			continue;
		}
		declared.tasks.push(TaskCandidate {
			name: name.clone(),
			detail: Some(turbo_detail(value)),
		});
	}
	declared
}

fn turbo_detail(value: &Value) -> TaskDetail {
	let (inputs, ignore) = split_negated(string_list(value.get("inputs")));
	let (outputs, _) = split_negated(string_list(value.get("outputs")));
	TaskDetail {
		// A `$MY_VAR` entry is the retired way of declaring an env dependency,
		// and a `web#build` entry names a package's task. Neither is a task of
		// the repo, so neither can be depended on here.
		depends_on: some_list(
			string_list(value.get("dependsOn"))
				.into_iter()
				.filter(|d| !d.starts_with('$') && !d.contains('#'))
				.collect(),
		),
		// `$TURBO_DEFAULT$` re-includes the files the pattern list replaced,
		// which is already what Lattice does with a task that declares none.
		inputs: some_list(
			inputs
				.into_iter()
				.filter(|i| !i.starts_with('$'))
				.collect::<Vec<_>>(),
		),
		ignore: some_list(ignore),
		outputs: some_list(outputs),
		env: some_list(string_list(value.get("env"))),
		persistent: value.get("persistent").and_then(Value::as_bool),
		cache: value.get("cache").and_then(Value::as_bool),
	}
}

/// The keys of the `key` object in a JSON `file`, as tasks the file names and
/// says nothing else about.
fn json_keys(path: &Path, file: &str, key: &str, jsonc: bool) -> DeclaredTasks {
	let Some(json) = read_json(&path.join(file), jsonc) else {
		return DeclaredTasks::default();
	};
	let Some(map) = json.get(key).and_then(Value::as_object) else {
		return DeclaredTasks::default();
	};
	named(file, map.keys().cloned())
}

/// The keys of one TOML table, read a line at a time.
///
/// A whole TOML parser would be a dependency for four table lookups, and the
/// tables these drivers keep their scripts in are flat: `name = "command"`, one
/// per line, until the next header.
fn toml_keys(path: &Path, file: &str, section: &str) -> DeclaredTasks {
	let Ok(raw) = std::fs::read_to_string(path.join(file)) else {
		return DeclaredTasks::default();
	};
	let header = format!("[{section}]");
	let mut names = Vec::new();
	let mut inside = false;
	for line in raw.lines() {
		let line = line.trim();
		if line.starts_with('[') {
			inside = line == header;
			continue;
		}
		if !inside || line.is_empty() || line.starts_with('#') {
			continue;
		}
		let Some((key, _)) = line.split_once('=') else {
			continue;
		};
		let key = key.trim().trim_matches('"').trim_matches('\'');
		if !key.is_empty() {
			names.push(key.to_string());
		}
	}
	named(file, names)
}

/// A justfile's recipe names: the lines that start in the first column and
/// carry a `:`. Assignments (`x := y`), settings and attributes look similar
/// enough to be worth ruling out by hand.
fn just_recipes(path: &Path) -> DeclaredTasks {
	let file = if path.join("justfile").exists() {
		"justfile"
	} else {
		".justfile"
	};
	let Ok(raw) = std::fs::read_to_string(path.join(file)) else {
		return DeclaredTasks::default();
	};

	let mut names = Vec::new();
	for line in raw.lines() {
		if line.starts_with([' ', '\t']) || line.trim().is_empty() {
			continue;
		}
		if line.starts_with(['#', '[', '@']) || line.contains(":=") {
			continue;
		}
		let Some((head, _)) = line.split_once(':') else {
			continue;
		};
		// `export x = y`, `set shell = [...]`, `alias b := build`, `mod sub`.
		let Some(name) = head.split_whitespace().next() else {
			continue;
		};
		if name.contains('=') || matches!(name, "export" | "set" | "alias" | "mod" | "import") {
			continue;
		}
		if name
			.chars()
			.all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
		{
			names.push(name.to_string());
		}
	}
	named(file, names)
}

/// A Taskfile's task names: the keys one level under the top-level `tasks:`.
///
/// Read by indentation rather than parsed, for the same reason as [`toml_keys`].
/// Anything the shape does not fit is left alone, so a Taskfile written with
/// anchors or flow mappings proposes nothing instead of proposing nonsense.
fn taskfile_tasks(path: &Path) -> DeclaredTasks {
	let Some(file) = ["Taskfile.yml", "Taskfile.yaml"]
		.into_iter()
		.find(|f| path.join(f).exists())
	else {
		return DeclaredTasks::default();
	};
	let Ok(raw) = std::fs::read_to_string(path.join(file)) else {
		return DeclaredTasks::default();
	};

	let mut names = Vec::new();
	let mut in_tasks = false;
	let mut depth: Option<usize> = None;
	for line in raw.lines() {
		let trimmed = line.trim();
		if trimmed.is_empty() || trimmed.starts_with('#') {
			continue;
		}
		let indent = line.len() - line.trim_start().len();
		if !in_tasks {
			in_tasks = indent == 0 && trimmed == "tasks:";
			continue;
		}
		if indent == 0 {
			break;
		}
		let Some(name) = trimmed.strip_suffix(':') else {
			continue;
		};
		if *depth.get_or_insert(indent) != indent {
			continue;
		}
		let name = name.trim_matches('"').trim_matches('\'');
		if !name.is_empty() && !name.contains(' ') {
			names.push(name.to_string());
		}
	}
	named(file, names)
}

fn named(source: &str, names: impl IntoIterator<Item = String>) -> DeclaredTasks {
	DeclaredTasks {
		tasks: names
			.into_iter()
			.map(|name| TaskCandidate { name, detail: None })
			.collect(),
		source: source.to_string(),
		..Default::default()
	}
}

fn read_json(file: &Path, jsonc: bool) -> Option<Value> {
	let raw = std::fs::read_to_string(file).ok()?;
	let text = if jsonc {
		crate::strip_json_comments(&raw)
	} else {
		raw
	};
	serde_json::from_str(&text).ok()
}

fn string_list(value: Option<&Value>) -> Vec<String> {
	value
		.and_then(Value::as_array)
		.map(|a| {
			a.iter()
				.filter_map(Value::as_str)
				.map(String::from)
				.collect()
		})
		.unwrap_or_default()
}

/// Split a glob list into what it includes and what it excludes. Lattice spells
/// an exclusion as its own `ignore` list rather than a `!` in front of a
/// pattern, so a negation left in place would be read as a literal path.
fn split_negated(patterns: Vec<String>) -> (Vec<String>, Vec<String>) {
	let mut include = Vec::new();
	let mut exclude = Vec::new();
	for pattern in patterns {
		match pattern.strip_prefix('!') {
			Some(rest) => exclude.push(rest.to_string()),
			None => include.push(pattern),
		}
	}
	(include, exclude)
}

fn some_list(list: Vec<String>) -> Option<Vec<String>> {
	(!list.is_empty()).then_some(list)
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
///
/// A path name can collide in turn — `a/b-c` and `a-b/c` both flatten to
/// `a-b-c` — and a config with two workspaces of one name does not load at all,
/// so the last word is a counter. `candidates` is in path order, which keeps the
/// numbering stable between scans.
fn disambiguate_names(candidates: &mut [WorkspaceCandidate]) {
	let mut counts: HashMap<&str, usize> = HashMap::new();
	for c in candidates.iter() {
		*counts.entry(c.name.as_str()).or_default() += 1;
	}
	let shared: Vec<String> = counts
		.into_iter()
		.filter(|(_, n)| *n > 1)
		.map(|(name, _)| name.to_string())
		.collect();

	let mut taken: HashSet<String> = HashSet::new();
	for c in candidates.iter_mut() {
		if shared.contains(&c.name) && c.path != "." {
			c.name = c.path.replace('/', "-");
		}
		if !taken.insert(c.name.clone()) {
			let mut n = 2;
			let mut unique = format!("{}-{n}", c.name);
			while !taken.insert(unique.clone()) {
				n += 1;
				unique = format!("{}-{n}", c.name);
			}
			c.name = unique;
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

	/// The path-based fallback name can collide in turn — `a/b-c` and `a-b/c` both
	/// flatten to `a-b-c` — and `lattice init` writing two workspaces of one name
	/// produces a config the next command refuses to load.
	#[test]
	fn colliding_path_names_are_still_unique() {
		let tmp = TempDir::new().unwrap();
		for dir in ["a/b-c", "z/b-c", "a-b/c", "z/c"] {
			write(tmp.path(), &format!("{dir}/package.json"), "{}");
			write(tmp.path(), &format!("{dir}/package-lock.json"), "{}");
		}

		let found = scan_workspaces(tmp.path());
		let names: Vec<&str> = found.iter().map(|c| c.name.as_str()).collect();
		assert_eq!(names.len(), 4);
		let unique: HashSet<&str> = names.iter().copied().collect();
		assert_eq!(unique.len(), names.len(), "duplicate name in {names:?}");
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
	fn names(declared: &DeclaredTasks) -> Vec<&str> {
		declared.tasks.iter().map(|t| t.name.as_str()).collect()
	}

	fn detail_of<'a>(declared: &'a DeclaredTasks, name: &str) -> &'a TaskDetail {
		declared
			.tasks
			.iter()
			.find(|t| t.name == name)
			.and_then(|t| t.detail.as_ref())
			.unwrap_or_else(|| panic!("{name} was declared with detail"))
	}

	/// The whole point of reading `turbo.json`: a repo that runs twelve tasks
	/// through it gets twelve tasks, not the one Lattice used to assume.
	#[test]
	fn a_turbo_repo_contributes_every_task_it_declares() {
		let tmp = TempDir::new().unwrap();
		write(
			tmp.path(),
			"turbo.json",
			r#"{
				"globalDependencies": ["tsconfig.base.json"],
				"globalEnv": ["CI"],
				"tasks": {
					// A comment is legal here, and turbo.json in the wild has them.
					"build": { "dependsOn": ["^build"], "outputs": ["dist/**", "!dist/tmp/**"] },
					"dev": { "persistent": true, "cache": false },
					"lint": {},
					"web#deploy": { "dependsOn": ["build"] }
				}
			}"#,
		);

		let declared = scan_declared_tasks(tmp.path(), Some("turbo"));

		assert_eq!(names(&declared), vec!["build", "dev", "lint"]);
		assert_eq!(declared.global_dependencies, vec!["tsconfig.base.json"]);
		assert_eq!(declared.global_env, vec!["CI"]);
		assert_eq!(declared.source, "turbo.json");

		let build = detail_of(&declared, "build");
		assert_eq!(
			build.depends_on.as_deref(),
			Some(["^build".to_string()].as_slice())
		);
		// Lattice spells an exclusion as its own list, so the `!` pattern moves
		// rather than being written where it would read as a literal path.
		assert_eq!(
			build.outputs.as_deref(),
			Some(["dist/**".to_string()].as_slice())
		);

		let dev = detail_of(&declared, "dev");
		assert_eq!(dev.persistent, Some(true));
		assert_eq!(dev.cache, Some(false));
		assert_eq!(detail_of(&declared, "lint"), &TaskDetail::default());
	}

	/// `pipeline` is what the task map was called before, and a repo that never
	/// migrated is still a repo Lattice has to read.
	#[test]
	fn the_older_name_for_the_turbo_task_map_is_read_too() {
		let tmp = TempDir::new().unwrap();
		write(
			tmp.path(),
			"turbo.json",
			r#"{ "pipeline": { "build": {}, "test": {} } }"#,
		);
		assert_eq!(
			names(&scan_declared_tasks(tmp.path(), Some("turbo"))),
			vec!["build", "test"]
		);
	}

	/// A `$TURBO_DEFAULT$` input and a `web#build` dependency name things that do
	/// not exist in a Lattice config; writing either produces one that fails to
	/// load.
	#[test]
	fn declarations_lattice_has_no_word_for_are_dropped() {
		let tmp = TempDir::new().unwrap();
		write(
			tmp.path(),
			"turbo.json",
			r#"{ "tasks": { "build": {
				"dependsOn": ["^build", "web#compile", "$LEGACY_ENV"],
				"inputs": ["$TURBO_DEFAULT$", "src/**", "!src/**/*.test.ts"]
			} } }"#,
		);

		let declared = scan_declared_tasks(tmp.path(), Some("turbo"));
		let build = detail_of(&declared, "build");
		assert_eq!(
			build.depends_on.as_deref(),
			Some(["^build".to_string()].as_slice())
		);
		assert_eq!(
			build.inputs.as_deref(),
			Some(["src/**".to_string()].as_slice())
		);
		assert_eq!(
			build.ignore.as_deref(),
			Some(["src/**/*.test.ts".to_string()].as_slice())
		);
	}

	#[test]
	fn a_package_manager_contributes_its_script_names_and_nothing_more() {
		let tmp = TempDir::new().unwrap();
		write(
			tmp.path(),
			"package.json",
			r#"{ "scripts": { "build": "tsc", "test": "vitest" } }"#,
		);
		let declared = scan_declared_tasks(tmp.path(), Some("pnpm"));
		assert_eq!(names(&declared), vec!["build", "test"]);
		assert!(declared.tasks.iter().all(|t| t.detail.is_none()));
	}

	/// Every ecosystem that publishes a task list gets read, not just the node
	/// one. A driver that publishes none contributes nothing, which is the honest
	/// answer: `cargo` accepts any subcommand and names none of them anywhere.
	#[test]
	fn every_ecosystem_that_declares_a_task_list_is_read() {
		let tmp = TempDir::new().unwrap();
		let root = tmp.path();
		write(
			root,
			"composer.json",
			r#"{ "scripts": { "phpstan": "x" } }"#,
		);
		write(root, "deno.jsonc", r#"{ "tasks": { "serve": "x" } }"#);
		write(
			root,
			"nx.json",
			r#"{ "targetDefaults": { "package": {} } }"#,
		);
		write(root, "Pipfile", "[scripts]\nfmt = \"black .\"\n");
		write(
			root,
			"pyproject.toml",
			"[project]\nname = \"x\"\n\n[tool.pdm.scripts]\ntypecheck = \"mypy .\"\n\n[tool.poetry.scripts]\nserve = \"app:main\"\n",
		);
		write(
			root,
			"justfile",
			"# a comment\nexport FOO := \"bar\"\nalias b := build\n\nbuild target=\"debug\":\n    cargo build\n\n@quiet:\n    echo hi\n\ntest:\n    cargo test\n",
		);
		write(
			root,
			"Taskfile.yml",
			"version: '3'\n\nvars:\n  GREETING: hi\n\ntasks:\n  build:\n    cmds:\n      - go build\n  test:\n    cmds:\n      - go test\n",
		);

		let cases = [
			("composer", vec!["phpstan"]),
			("deno", vec!["serve"]),
			("nx", vec!["package"]),
			("pipenv", vec!["fmt"]),
			("pdm", vec!["typecheck"]),
			("poetry", vec!["serve"]),
			("just", vec!["build", "test"]),
			("task", vec!["build", "test"]),
		];
		for (driver, expected) in cases {
			let declared = scan_declared_tasks(root, Some(driver));
			assert_eq!(names(&declared), expected, "{driver}");
		}

		for driver in ["cargo", "go", "gradle", "maven", "dotnet", "swift"] {
			let declared = scan_declared_tasks(root, Some(driver));
			assert!(declared.tasks.is_empty(), "{driver} declares no list");
			assert!(declared.source.is_empty(), "{driver}");
		}
	}

	#[test]
	fn a_file_that_will_not_parse_proposes_nothing_rather_than_guessing() {
		let tmp = TempDir::new().unwrap();
		write(tmp.path(), "turbo.json", "{ this is not json");
		assert!(scan_declared_tasks(tmp.path(), Some("turbo"))
			.tasks
			.is_empty());
	}

	#[test]
	fn a_scanned_candidate_carries_the_tasks_its_driver_can_run() {
		let tmp = TempDir::new().unwrap();
		write(tmp.path(), "package.json", "{}");
		write(tmp.path(), "turbo.json", r#"{ "tasks": { "lint": {} } }"#);

		let found = scan_workspaces(tmp.path());
		assert_eq!(found.len(), 1);
		assert_eq!(found[0].driver.as_deref(), Some("turbo"));
		assert_eq!(names(&found[0].declared), vec!["lint"]);
	}
}
