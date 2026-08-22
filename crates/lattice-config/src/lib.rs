pub mod schema;

use anyhow::{anyhow, bail, Context, Result};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

pub const CONFIG_FILE: &str = "lattice.json";

/// The canonical engine table, as `(engine name, version command)`: every name
/// accepted in the bare-string engine form, paired with the command Lattice runs
/// to read that tool's version. A string-form engine whose name is absent here
/// is rejected; it must use the object form with an explicit `versionCmd`.
pub const WELL_KNOWN_ENGINES: &[(&str, &str)] = &[
	// JavaScript and TypeScript
	("node", "node --version"),
	("deno", "deno --version"),
	("bun", "bun --version"),
	("pnpm", "pnpm --version"),
	("yarn", "yarn --version"),
	("npm", "npm --version"),
	// Rust
	("rust", "rustc --version"),
	("cargo", "cargo --version"),
	// Go
	("go", "go version"),
	// Python
	("python", "python --version"),
	("python3", "python3 --version"),
	("pip", "pip --version"),
	("uv", "uv --version"),
	("poetry", "poetry --version"),
	("pdm", "pdm --version"),
	("pipenv", "pipenv --version"),
	// Ruby
	("ruby", "ruby --version"),
	("bundler", "bundle --version"),
	("rake", "rake --version"),
	// The JVM
	("java", "java -version"),
	("kotlin", "kotlinc -version"),
	("gradle", "gradle --version"),
	("maven", "mvn --version"),
	// .NET
	("dotnet", "dotnet --version"),
	// nuget.exe has no --version flag; `help` prints "NuGet Version: x.y.z" first.
	("nuget", "nuget help"),
	// Swift and Objective-C
	("swift", "swift --version"),
	("pod", "pod --version"),
	// PHP
	("php", "php --version"),
	("composer", "composer --version"),
	// Elixir
	("elixir", "elixir --version"),
	("mix", "mix --version"),
	// Dart
	("dart", "dart --version"),
	// Haskell
	("haskell", "ghc --version"),
	("ghc", "ghc --version"),
	("stack", "stack --version"),
	("cabal", "cabal --version"),
	// Language-agnostic task runners
	("just", "just --version"),
	("task", "task --version"),
	("turbo", "turbo --version"),
	("nx", "nx --version"),
];

/// The built-in version command for a well-known engine, if there is one.
pub fn builtin_version_cmd(name: &str) -> Option<&'static str> {
	WELL_KNOWN_ENGINES
		.iter()
		.find(|(engine, _)| *engine == name)
		.map(|(_, cmd)| *cmd)
}

pub fn is_well_known_engine(name: &str) -> bool {
	builtin_version_cmd(name).is_some()
}

/// Every well-known engine name, in table order.
pub fn well_known_engine_names() -> Vec<&'static str> {
	WELL_KNOWN_ENGINES
		.iter()
		.map(|(engine, _)| *engine)
		.collect()
}

/// Files that record resolved dependency state. Each one present in a workspace
/// is hashed into that workspace's cache keys, and its mtime decides whether
/// `lattice setup` reinstalls dependencies. Order is fixed: it is part of the
/// cache key.
pub const LOCKFILES: &[&str] = &[
	"package-lock.json",
	"yarn.lock",
	"pnpm-lock.yaml",
	"bun.lockb",
	"bun.lock",
	"Cargo.lock",
	"go.sum",
	"poetry.lock",
	"uv.lock",
	"Gemfile.lock",
	"npm-shrinkwrap.json",
	"deno.lock",
	"pdm.lock",
	"Pipfile.lock",
	"requirements.txt",
	"Podfile.lock",
	"packages.lock.json",
	"composer.lock",
	"mix.lock",
	"pubspec.lock",
	"Package.resolved",
	"stack.yaml.lock",
	"cabal.project.freeze",
];

/// Files that can define what a task command actually does. A task's resolved
/// command is usually an indirection — `npm run build` names a script in
/// `package.json`, `make test` names a target in a `Makefile` — so the command
/// string alone does not pin the work. Each of these present in a workspace is
/// hashed into that workspace's cache keys. Order is fixed: it is part of the
/// cache key.
pub const MANIFESTS: &[&str] = &[
	"package.json",
	"Cargo.toml",
	"go.mod",
	"pyproject.toml",
	"setup.py",
	"Gemfile",
	"Rakefile",
	"pom.xml",
	"build.gradle",
	"build.gradle.kts",
	"composer.json",
	"mix.exs",
	"pubspec.yaml",
	"Package.swift",
	"stack.yaml",
	"cabal.project",
	"deno.json",
	"deno.jsonc",
	"Makefile",
	"makefile",
	"GNUmakefile",
	"Justfile",
	"justfile",
	"Taskfile.yml",
	"Taskfile.yaml",
];

fn default_true() -> bool {
	true
}

/// Name-keyed map of engine constraints. Declaration order is preserved.
pub type EngineMap = IndexMap<String, EngineSpec>;

/// An engine constraint. Either a bare version-constraint string, or a detailed
/// object with an explicit version command / install command / bin dir.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum EngineSpec {
	/// A bare version constraint string (e.g. `">=20.0.0"`).
	Version(String),
	/// A detailed engine specification.
	Detailed(EngineSpecObject),
}

/// Hand-written rather than `#[serde(untagged)]`: an untagged enum buffers the
/// input and reports only that no variant matched, which would bury the unknown
/// field error [`EngineSpecObject`] raises. Dispatching on the JSON type instead
/// lets that error through with its key and position intact.
impl<'de> Deserialize<'de> for EngineSpec {
	fn deserialize<D: serde::Deserializer<'de>>(
		deserializer: D,
	) -> std::result::Result<Self, D::Error> {
		struct SpecVisitor;

		impl<'de> serde::de::Visitor<'de> for SpecVisitor {
			type Value = EngineSpec;

			fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
				f.write_str("a version constraint string or an engine object")
			}

			fn visit_str<E: serde::de::Error>(self, v: &str) -> std::result::Result<EngineSpec, E> {
				Ok(EngineSpec::Version(v.to_string()))
			}

			fn visit_map<A: serde::de::MapAccess<'de>>(
				self,
				map: A,
			) -> std::result::Result<EngineSpec, A::Error> {
				EngineSpecObject::deserialize(serde::de::value::MapAccessDeserializer::new(map))
					.map(EngineSpec::Detailed)
			}
		}

		deserializer.deserialize_any(SpecVisitor)
	}
}

impl EngineSpec {
	pub fn version(&self) -> Option<&str> {
		match self {
			EngineSpec::Version(v) => Some(v.as_str()),
			EngineSpec::Detailed(o) => o.version.as_deref(),
		}
	}

	pub fn version_cmd(&self) -> Option<&str> {
		match self {
			EngineSpec::Version(_) => None,
			EngineSpec::Detailed(o) => o.version_cmd.as_deref(),
		}
	}

	pub fn install_cmd(&self) -> Option<&str> {
		match self {
			EngineSpec::Version(_) => None,
			EngineSpec::Detailed(o) => o.install_cmd.as_deref(),
		}
	}

	/// The bin dir relative to the toolchain install, if any.
	pub fn bin(&self) -> Option<&str> {
		match self {
			EngineSpec::Version(_) => None,
			EngineSpec::Detailed(o) => o.bin.as_deref(),
		}
	}

	pub fn is_detailed(&self) -> bool {
		matches!(self, EngineSpec::Detailed(_))
	}
}

/// The detailed object form of an engine constraint.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EngineSpecObject {
	pub version: Option<String>,
	pub version_cmd: Option<String>,
	pub install_cmd: Option<String>,
	pub bin: Option<String>,
}

/// One workspace: a directory with its own manifest, and the unit of task
/// running and caching.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceConfig {
	pub name: String,
	/// Literal directory path (relative to repo root).
	pub path: String,
	/// When true (default), infer engine and task commands from the native manifest.
	#[serde(default = "default_true")]
	pub auto: bool,
	/// Per-workspace toolchain constraints; overrides/pins the inferred engine.
	#[serde(default)]
	pub engines: EngineMap,
	/// Other workspaces (by name) this one depends on.
	#[serde(default)]
	pub depends_on: Option<Vec<String>>,
	/// Explicit script-name -> command overrides.
	#[serde(default)]
	pub scripts: IndexMap<String, String>,
}

/// A named task in the root task graph and how it relates across workspaces.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PipelineTask {
	pub depends_on: Option<Vec<String>>,
	pub inputs: Option<Vec<String>>,
	pub outputs: Option<Vec<String>>,
	pub ignore: Option<Vec<String>>,
	pub env: Option<Vec<String>>,
	pub persistent: Option<bool>,
	pub cache: Option<bool>,
	/// How long the task may run before it is killed. Absent means no limit.
	pub timeout: Option<Duration>,
}

impl PipelineTask {
	/// A persistent task is long-running (e.g. a dev server) and never cached.
	pub fn is_persistent(&self) -> bool {
		self.persistent == Some(true)
	}

	/// A task is cacheable unless it is persistent or has explicitly disabled caching.
	pub fn is_cacheable(&self) -> bool {
		!self.is_persistent() && self.cache != Some(false)
	}

	/// The timeout to enforce, if any. A persistent task runs until the run ends,
	/// so a timeout on one would only ever cut short the thing it was asked to
	/// keep alive.
	pub fn effective_timeout(&self) -> Option<std::time::Duration> {
		if self.is_persistent() {
			return None;
		}
		self.timeout.map(|d| d.as_std())
	}
}

/// A human duration such as `"90s"`, `"5m"`, `"1h"`, or a bare integer of seconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Duration(pub u64);

impl Duration {
	pub fn as_secs(&self) -> u64 {
		self.0
	}

	pub fn as_std(&self) -> std::time::Duration {
		std::time::Duration::from_secs(self.0)
	}

	/// Parse a human duration. Supports `ms`/`s`/`m`/`h` (case-insensitive) and a
	/// bare integer, interpreted as seconds. Sub-second values round up to one
	/// second: a timeout that rounds to zero would kill the task instantly.
	pub fn parse(s: &str) -> Result<Duration> {
		let trimmed = s.trim();
		if trimmed.is_empty() {
			bail!("duration is empty");
		}

		let split = trimmed
			.find(|c: char| !(c.is_ascii_digit() || c == '.'))
			.unwrap_or(trimmed.len());
		let (num_part, unit_part) = trimmed.split_at(split);
		let num_part = num_part.trim();
		let unit_part = unit_part.trim();

		if num_part.is_empty() {
			bail!("duration '{s}' does not start with a number");
		}

		let value: f64 = num_part
			.parse()
			.with_context(|| format!("could not read the number in duration '{s}'"))?;

		let seconds = match unit_part.to_ascii_lowercase().as_str() {
			"" | "s" | "sec" | "secs" => value,
			"ms" => value / 1000.0,
			"m" | "min" | "mins" => value * 60.0,
			"h" | "hr" | "hrs" => value * 3600.0,
			other => bail!("unknown duration unit '{other}' in '{s}'. Use ms, s, m, or h"),
		};

		if seconds <= 0.0 {
			bail!("duration '{s}' must be greater than zero");
		}
		Ok(Duration(seconds.ceil() as u64))
	}
}

impl FromStr for Duration {
	type Err = anyhow::Error;
	fn from_str(s: &str) -> Result<Self> {
		Duration::parse(s)
	}
}

impl fmt::Display for Duration {
	/// Render to a canonical human string using the largest exact unit.
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		let secs = self.0;
		for (unit, name) in [(3600u64, "h"), (60, "m")] {
			if secs.is_multiple_of(unit) && secs >= unit {
				return write!(f, "{}{}", secs / unit, name);
			}
		}
		write!(f, "{}s", secs)
	}
}

impl Serialize for Duration {
	fn serialize<S: serde::Serializer>(
		&self,
		serializer: S,
	) -> std::result::Result<S::Ok, S::Error> {
		serializer.serialize_str(&self.to_string())
	}
}

impl<'de> Deserialize<'de> for Duration {
	/// Accepts both `"90s"` and a bare number of seconds, since a timeout reads
	/// naturally either way.
	fn deserialize<D: serde::Deserializer<'de>>(
		deserializer: D,
	) -> std::result::Result<Self, D::Error> {
		struct DurationVisitor;

		impl serde::de::Visitor<'_> for DurationVisitor {
			type Value = Duration;

			fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
				f.write_str("a duration string such as \"90s\" or a number of seconds")
			}

			fn visit_str<E: serde::de::Error>(self, v: &str) -> std::result::Result<Duration, E> {
				Duration::parse(v).map_err(serde::de::Error::custom)
			}

			fn visit_u64<E: serde::de::Error>(self, v: u64) -> std::result::Result<Duration, E> {
				if v == 0 {
					return Err(serde::de::Error::custom(
						"duration must be greater than zero",
					));
				}
				Ok(Duration(v))
			}

			fn visit_f64<E: serde::de::Error>(self, v: f64) -> std::result::Result<Duration, E> {
				Duration::parse(&v.to_string()).map_err(serde::de::Error::custom)
			}
		}

		deserializer.deserialize_any(DurationVisitor)
	}
}

/// A human byte-size such as `"10GB"`, `"512MB"`, or a bare integer of bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheSize(pub u64);

impl CacheSize {
	pub fn as_bytes(&self) -> u64 {
		self.0
	}

	/// Parse a human byte-size. Supports `B`/`KB`/`MB`/`GB`/`TB` (base 1024,
	/// case-insensitive) and a bare integer (interpreted as bytes).
	pub fn parse(s: &str) -> Result<CacheSize> {
		let trimmed = s.trim();
		if trimmed.is_empty() {
			bail!("cache size is empty");
		}

		// Split into the leading numeric part and the trailing unit.
		let split = trimmed
			.find(|c: char| !(c.is_ascii_digit() || c == '.'))
			.unwrap_or(trimmed.len());
		let (num_part, unit_part) = trimmed.split_at(split);
		let num_part = num_part.trim();
		let unit_part = unit_part.trim();

		if num_part.is_empty() {
			bail!("cache size '{s}' does not start with a number");
		}

		let value: f64 = num_part
			.parse()
			.with_context(|| format!("could not read the number in cache size '{s}'"))?;

		let multiplier: u64 = match unit_part.to_ascii_uppercase().as_str() {
			"" | "B" => 1,
			"KB" | "K" => 1024,
			"MB" | "M" => 1024 * 1024,
			"GB" | "G" => 1024 * 1024 * 1024,
			"TB" | "T" => 1024u64 * 1024 * 1024 * 1024,
			other => bail!("unknown cache size unit '{other}' in '{s}'. Use B, KB, MB, GB, or TB"),
		};

		Ok(CacheSize((value * multiplier as f64) as u64))
	}
}

impl FromStr for CacheSize {
	type Err = anyhow::Error;
	fn from_str(s: &str) -> Result<Self> {
		CacheSize::parse(s)
	}
}

impl fmt::Display for CacheSize {
	/// Render to a canonical human string using the largest exact unit.
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		let bytes = self.0;
		const TB: u64 = 1024 * 1024 * 1024 * 1024;
		const GB: u64 = 1024 * 1024 * 1024;
		const MB: u64 = 1024 * 1024;
		const KB: u64 = 1024;
		if bytes == 0 {
			return write!(f, "0B");
		}
		for (unit, name) in [(TB, "TB"), (GB, "GB"), (MB, "MB"), (KB, "KB")] {
			if bytes.is_multiple_of(unit) {
				return write!(f, "{}{}", bytes / unit, name);
			}
		}
		write!(f, "{}B", bytes)
	}
}

impl Serialize for CacheSize {
	fn serialize<S: serde::Serializer>(
		&self,
		serializer: S,
	) -> std::result::Result<S::Ok, S::Error> {
		serializer.serialize_str(&self.to_string())
	}
}

impl<'de> Deserialize<'de> for CacheSize {
	fn deserialize<D: serde::Deserializer<'de>>(
		deserializer: D,
	) -> std::result::Result<Self, D::Error> {
		let s = String::deserialize(deserializer)?;
		CacheSize::parse(&s).map_err(serde::de::Error::custom)
	}
}

/// Repo-wide knobs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Settings {
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub max_cache_size: Option<CacheSize>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub cache_dir: Option<String>,
	#[serde(default)]
	pub loquacious: bool,
	#[serde(default = "default_true")]
	pub version_check: bool,
}

impl Default for Settings {
	fn default() -> Self {
		Settings {
			max_cache_size: None,
			cache_dir: None,
			loquacious: false,
			version_check: true,
		}
	}
}

impl Settings {
	/// The cache directory, defaulting to `.lattice/cache`.
	pub fn cache_dir(&self) -> &str {
		self.cache_dir.as_deref().unwrap_or(".lattice/cache")
	}
}

/// The root `lattice.json` configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LatticeConfig {
	#[serde(rename = "$schema", skip_serializing_if = "Option::is_none")]
	pub schema: Option<String>,
	pub lattice_version: Option<String>,
	#[serde(default)]
	pub workspaces: Vec<WorkspaceConfig>,
	#[serde(default)]
	pub engines: EngineMap,
	/// Repo-root-relative globs whose contents feed every task's cache key.
	///
	/// A task's `inputs` are relative to its own workspace, so a file above the
	/// workspace — a shared `tsconfig.base.json`, a schema directory, a root
	/// `.env` — cannot be named there in a way that means the same thing for
	/// every workspace. Anything listed here is hashed into all of them.
	#[serde(default)]
	pub global_dependencies: Vec<String>,
	/// Environment variable names whose values feed every task's cache key.
	#[serde(default)]
	pub global_env: Vec<String>,
	#[serde(default)]
	pub tasks: IndexMap<String, PipelineTask>,
	#[serde(default)]
	pub settings: Settings,
}

impl LatticeConfig {
	/// Any engine (root or per-workspace) declared in string form whose key is
	/// not a well-known engine is an error. Workspace names must be unique,
	/// workspace paths must be non-empty and stay inside the repo, and every
	/// `dependsOn` must name something that exists.
	pub fn validate(&self) -> Result<()> {
		check_string_engines(&self.engines, "root")?;
		for ws in &self.workspaces {
			check_string_engines(&ws.engines, &format!("workspace '{}'", ws.name))?;
		}

		// Workspace names unique.
		let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
		for ws in &self.workspaces {
			if ws.path.trim().is_empty() {
				bail!("workspace '{}' has an empty path", ws.name);
			}
			check_contained_path(&ws.name, &ws.path)?;
			if !seen.insert(ws.name.as_str()) {
				bail!(
					"duplicate workspace name '{}'. Every workspace name must be unique",
					ws.name
				);
			}
		}

		self.check_workspace_deps()?;
		self.check_task_deps()?;

		Ok(())
	}

	/// A workspace `dependsOn` entry that names no declared workspace builds no
	/// edge at all, so `^task` expands to nothing and the run silently loses its
	/// ordering. Name the typo instead.
	fn check_workspace_deps(&self) -> Result<()> {
		let names: Vec<String> = self.workspaces.iter().map(|ws| ws.name.clone()).collect();
		for ws in &self.workspaces {
			for dep in ws.depends_on.iter().flatten() {
				if dep == &ws.name {
					bail!("workspace '{}' lists itself in `dependsOn`", ws.name);
				}
				if names.contains(dep) {
					continue;
				}
				let mut message = format!(
					"workspace '{}' depends on '{dep}', which is not a declared workspace",
					ws.name
				);
				if let Some(near) = closest_field(dep, &names) {
					message.push_str(&format!("\nDid you mean `{near}`?"));
				}
				if !names.is_empty() {
					message.push_str(&format!("\nDeclared workspaces: {}", names.join(", ")));
				}
				bail!(message);
			}
		}
		Ok(())
	}

	/// A task `dependsOn` entry naming a task the `tasks` map does not define
	/// resolves to no node, so the prerequisite is quietly dropped.
	fn check_task_deps(&self) -> Result<()> {
		let names: Vec<String> = self.tasks.keys().cloned().collect();
		for (task, cfg) in &self.tasks {
			for dep in cfg.depends_on.iter().flatten() {
				let bare = dep.strip_prefix('^').unwrap_or(dep);
				if bare == task && bare == dep {
					bail!("task '{task}' lists itself in `dependsOn`");
				}
				if names.iter().any(|n| n == bare) {
					continue;
				}
				let mut message = format!(
					"task '{task}' depends on '{dep}', but '{bare}' is not defined in `tasks`"
				);
				if let Some(near) = closest_field(bare, &names) {
					message.push_str(&format!("\nDid you mean `{near}`?"));
				}
				if !names.is_empty() {
					message.push_str(&format!("\nDefined tasks: {}", names.join(", ")));
				}
				bail!(message);
			}
		}
		Ok(())
	}
}

/// A workspace path has to stay inside the repo. Everything downstream — the
/// input walk, the output globs, the clear-before-restore a cache hit does —
/// treats it as the boundary of what a task may touch.
fn check_contained_path(name: &str, path: &str) -> Result<()> {
	// Judged as text rather than through `Path`, because `Path` answers for the
	// platform it is running on and a `lattice.json` is committed and shared. On
	// unix `C:\Windows` is one ordinary filename and `\etc` is a relative one; on
	// Windows both resolve to a drive root and leave the repo. A path that escapes
	// anywhere has to be rejected everywhere, or the same config means two things.
	let bytes = path.as_bytes();
	let rooted = path.starts_with('/') || path.starts_with('\\');
	let drive_prefixed = bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':';
	if rooted || drive_prefixed {
		bail!(
			"workspace '{name}' has a path '{path}' that is not relative to the repo \
			 root. Write every workspace path relative to the repo root"
		);
	}

	// Both separators, for the same reason.
	let mut depth: i32 = 0;
	for part in path.split(['/', '\\']) {
		match part {
			"" | "." => {}
			".." => depth -= 1,
			_ => depth += 1,
		}
		if depth < 0 {
			bail!(
				"workspace '{name}' has a path '{path}' that points outside the repo root. \
				 Every workspace path must stay inside the repo"
			);
		}
	}
	Ok(())
}

/// Reject string-form engines whose key is not a well-known engine.
fn check_string_engines(engines: &EngineMap, scope: &str) -> Result<()> {
	for (name, spec) in engines {
		if matches!(spec, EngineSpec::Version(_)) && !is_well_known_engine(name) {
			return Err(anyhow!(
				"engine '{name}' in {scope} uses the string form, which carries only a version. \
                 '{name}' is not a well-known engine, so Lattice cannot version-check it on its \
                 own. Use the object form with a `versionCmd`, like this: \"{name}\": \
                 {{ \"version\": \">=1.0.0\", \"versionCmd\": \"{name} --version\" }}"
			));
		}
	}
	Ok(())
}

/// Merge root engines with per-workspace engines; workspace entries win per-key.
pub fn resolve_engines(root: &EngineMap, workspace: &EngineMap) -> EngineMap {
	let mut merged = root.clone();
	for (name, spec) in workspace {
		merged.insert(name.clone(), spec.clone());
	}
	merged
}

/// Read `lattice.json` from `root`, parse it, and run [`LatticeConfig::validate`].
pub fn load_config(root: &Path) -> Result<LatticeConfig> {
	let config_path = root.join(CONFIG_FILE);
	let content = std::fs::read_to_string(&config_path)
		.with_context(|| format!("failed to read {} from {}", CONFIG_FILE, root.display()))?;
	parse_config(&content)
}

/// Parse `lattice.json` text and run [`LatticeConfig::validate`].
///
/// Every field is checked against the config types, so an unrecognized key is an
/// error rather than a no-op. A misspelled `inputs` or `outputs` would otherwise
/// change what a task hashes and caches without saying anything.
pub fn parse_config(content: &str) -> Result<LatticeConfig> {
	let mut deserializer = serde_json::Deserializer::from_str(content);
	let config: LatticeConfig =
		serde_path_to_error::deserialize(&mut deserializer).map_err(parse_error)?;
	deserializer
		.end()
		.with_context(|| format!("failed to parse {CONFIG_FILE}"))?;
	config.validate()?;
	Ok(config)
}

/// An unknown key and the keys that would have been accepted in its place,
/// recovered from serde's message. Everything needed to say what to write
/// instead is in there; nothing else exposes it.
struct UnknownField {
	field: String,
	expected: Vec<String>,
}

impl UnknownField {
	/// Serde's wording is "unknown field `x`, expected one of `a`, `b`" — with
	/// "expected `a`" for a single field, and "there are no fields" for none.
	/// Returns `None` for any other message, which falls back to serde's own.
	fn parse(message: &str) -> Option<UnknownField> {
		let rest = message.strip_prefix("unknown field `")?;
		let (field, rest) = rest.split_once('`')?;
		Some(UnknownField {
			field: field.to_string(),
			expected: rest
				.split('`')
				.skip(1)
				.step_by(2)
				.map(str::to_string)
				.collect(),
		})
	}
}

/// The message for a parse failure. An unknown key gets named, placed, and
/// matched against the fields that belong there; anything else keeps serde's
/// own message and position.
fn parse_error(error: serde_path_to_error::Error<serde_json::Error>) -> anyhow::Error {
	let Some(unknown) = UnknownField::parse(&error.inner().to_string()) else {
		return anyhow::Error::new(error.into_inner())
			.context(format!("failed to parse {CONFIG_FILE}"));
	};

	let inner = error.inner();
	let position = format!("line {}, column {}", inner.line(), inner.column());
	let mut message = match container_path(error.path(), &unknown.field) {
		Some(path) => format!(
			"unknown field `{}` in {path} ({CONFIG_FILE} {position})",
			unknown.field
		),
		None => format!(
			"unknown field `{}` at the top level of {CONFIG_FILE} ({position})",
			unknown.field
		),
	};
	if let Some(near) = closest_field(&unknown.field, &unknown.expected) {
		message.push_str(&format!("\nDid you mean `{near}`?"));
	}
	if !unknown.expected.is_empty() {
		message.push_str(&format!(
			"\nFields accepted here: {}",
			unknown.expected.join(", ")
		));
	}
	anyhow!(message)
}

/// The container `field` was found in, written the way it reads in the file:
/// `tasks.build`, `workspaces[0]`, `engines.node`. `None` at the top level.
fn container_path(path: &serde_path_to_error::Path, field: &str) -> Option<String> {
	use serde_path_to_error::Segment;

	let mut parts: Vec<String> = Vec::new();
	for segment in path.iter() {
		match segment {
			Segment::Seq { index } => match parts.last_mut() {
				Some(last) => last.push_str(&format!("[{index}]")),
				None => parts.push(format!("[{index}]")),
			},
			Segment::Map { key } => parts.push(key.clone()),
			Segment::Enum { variant } => parts.push(variant.clone()),
			_ => {}
		}
	}
	if parts.last().map(String::as_str) == Some(field) {
		parts.pop();
	}
	if parts.is_empty() {
		None
	} else {
		Some(parts.join("."))
	}
}

/// The accepted field closest to `field`, when one is close enough to be a
/// plausible typo rather than a coincidence.
fn closest_field<'a>(field: &str, expected: &'a [String]) -> Option<&'a str> {
	// One slip is worth pointing at for any key; two only once the key is long
	// enough that two edits still leave most of it intact.
	let budget = if field.chars().count() <= 4 { 1 } else { 2 };
	expected
		.iter()
		.map(|candidate| (edit_distance(field, candidate), candidate))
		.filter(|(distance, _)| *distance <= budget)
		.min_by_key(|(distance, candidate)| (*distance, candidate.len()))
		.map(|(_, candidate)| candidate.as_str())
}

/// Levenshtein distance, case-insensitively, so `Outputs` reads as one edit from
/// `outputs` rather than seven.
fn edit_distance(a: &str, b: &str) -> usize {
	let a: Vec<char> = a.chars().flat_map(char::to_lowercase).collect();
	let b: Vec<char> = b.chars().flat_map(char::to_lowercase).collect();
	let mut previous: Vec<usize> = (0..=b.len()).collect();
	let mut current = vec![0usize; b.len() + 1];
	for (i, ca) in a.iter().enumerate() {
		current[0] = i + 1;
		for (j, cb) in b.iter().enumerate() {
			let substitution = previous[j] + usize::from(ca != cb);
			current[j + 1] = substitution.min(previous[j + 1] + 1).min(current[j] + 1);
		}
		std::mem::swap(&mut previous, &mut current);
	}
	previous[b.len()]
}

/// Walk up from `start` looking for a directory containing `lattice.json`.
pub fn find_root(start: &Path) -> Option<PathBuf> {
	let mut current = start.to_path_buf();
	loop {
		if current.join(CONFIG_FILE).exists() {
			return Some(current);
		}
		if !current.pop() {
			return None;
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use jsonschema::Validator;
	use serde_json::{json, Value};

	/// Repo root, resolved from this crate's manifest dir (`crates/lattice-config`).
	fn repo_root() -> PathBuf {
		Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
	}

	fn read_json(path: &Path) -> Value {
		let content = std::fs::read_to_string(path)
			.unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
		serde_json::from_str(&content)
			.unwrap_or_else(|e| panic!("failed to parse {} as JSON: {e}", path.display()))
	}

	/// The schema this crate compiles in, as JSON. Read from the constant rather
	/// than from disk so the tests check the copy that actually ships.
	fn schema_json() -> Value {
		serde_json::from_str(schema::SCHEMA_JSON).expect("the bundled schema must be valid JSON")
	}

	fn compiled_schema() -> Validator {
		let schema = schema_json();
		jsonschema::validator_for(&schema).expect("schema.json must be a valid JSON Schema")
	}

	/// The message a rejected config produces, as the user reads it.
	fn parse_failure(content: &str) -> String {
		parse_config(content)
			.expect_err("this config must be rejected")
			.to_string()
	}

	#[test]
	fn schema_validates_repo_config() {
		let validator = compiled_schema();
		let config = read_json(&repo_root().join(CONFIG_FILE));
		if let Err(error) = validator.validate(&config) {
			panic!("repo lattice.json failed schema validation: {error}");
		}
	}

	#[test]
	fn schema_validates_polyglot_example_config() {
		let validator = compiled_schema();
		let config = read_json(
			&repo_root()
				.join("examples")
				.join("polyglot")
				.join(CONFIG_FILE),
		);
		if let Err(error) = validator.validate(&config) {
			panic!("examples/polyglot/lattice.json failed schema validation: {error}");
		}
	}

	/// The example exists to show cross-workspace ordering. A `^task` with no
	/// workspace edge to expand into demonstrates nothing, so assert the pair.
	#[test]
	fn polyglot_example_has_an_edge_for_its_caret_task() {
		let dir = repo_root().join("examples").join("polyglot");
		let config = load_config(&dir).expect("examples/polyglot must load");

		let caret_tasks: Vec<&String> = config
			.tasks
			.iter()
			.filter(|(_, task)| {
				task.depends_on
					.iter()
					.flatten()
					.any(|dep| dep.starts_with('^'))
			})
			.map(|(name, _)| name)
			.collect();
		assert!(
			!caret_tasks.is_empty(),
			"examples/polyglot must keep a `^task` dependency to demonstrate ordering"
		);

		let edges: Vec<(&str, &Vec<String>)> = config
			.workspaces
			.iter()
			.filter_map(|ws| ws.depends_on.as_ref().map(|deps| (ws.name.as_str(), deps)))
			.filter(|(_, deps)| !deps.is_empty())
			.collect();
		assert!(
			!edges.is_empty(),
			"tasks {caret_tasks:?} use `^`, but no workspace in examples/polyglot declares \
			 `dependsOn`, so they expand to nothing"
		);

		let names: Vec<&str> = config
			.workspaces
			.iter()
			.map(|ws| ws.name.as_str())
			.collect();
		for (workspace, deps) in edges {
			for dep in deps {
				assert!(
					names.contains(&dep.as_str()),
					"workspace '{workspace}' depends on unknown workspace '{dep}'"
				);
			}
		}
	}

	#[test]
	fn schema_rejects_old_glob_string_workspaces() {
		let validator = compiled_schema();
		let old = json!({ "workspaces": ["apps/*"] });
		assert!(
			!validator.is_valid(&old),
			"glob-string workspaces must be rejected by the schema"
		);
	}

	#[test]
	fn schema_rejects_old_projects_key() {
		let validator = compiled_schema();
		let old = json!({
			"workspaces": [{ "name": "a", "path": "a" }],
			"projects": {}
		});
		assert!(
			!validator.is_valid(&old),
			"a top-level `projects` key must be rejected by the schema"
		);
	}

	#[test]
	fn schema_rejects_glob_key_on_workspace() {
		let validator = compiled_schema();
		let old = json!({
			"workspaces": [{ "name": "a", "path": "a", "glob": "a/*" }]
		});
		assert!(
			!validator.is_valid(&old),
			"a `glob` key on a workspace must be rejected by the schema"
		);
	}

	#[test]
	fn schema_requires_workspace_path() {
		let validator = compiled_schema();
		let bad = json!({ "workspaces": [{ "name": "no-path" }] });
		assert!(
			!validator.is_valid(&bad),
			"a workspace without `path` must be rejected by the schema"
		);
	}

	#[test]
	fn schema_requires_workspace_name() {
		let validator = compiled_schema();
		let bad = json!({ "workspaces": [{ "path": "apps/web" }] });
		assert!(
			!validator.is_valid(&bad),
			"a workspace without `name` must be rejected by the schema"
		);
	}

	#[test]
	fn schema_allows_self_reference() {
		let validator = compiled_schema();
		let ok = json!({
			"$schema": ".lattice/schema.json",
			"workspaces": [{ "name": "web", "path": "apps/web" }]
		});
		assert!(
			validator.is_valid(&ok),
			"a top-level `$schema` self-reference must be allowed"
		);
	}

	#[test]
	fn schema_allows_string_and_object_engines() {
		let validator = compiled_schema();
		let ok = json!({
			"engines": {
				"node": ">=20.0.0",
				"alpes": { "version": ">=2.6.7", "versionCmd": "alp --version" }
			}
		});
		assert!(
			validator.is_valid(&ok),
			"both string and object engine forms must be allowed by the schema"
		);
	}

	/// Every `lattice.json` in the tree: this repo's own, plus each example.
	fn shipped_config_dirs() -> Vec<PathBuf> {
		vec![
			repo_root(),
			repo_root().join("examples").join("polyglot"),
			repo_root().join("examples").join("nested-repo"),
		]
	}

	#[test]
	fn shipped_configs_load_and_validate() {
		for dir in shipped_config_dirs() {
			let config = load_config(&dir)
				.unwrap_or_else(|e| panic!("load_config failed for {}: {e:#}", dir.display()));
			let serialized =
				serde_json::to_string(&config).expect("LatticeConfig must serialize back to JSON");
			let reparsed: LatticeConfig = serde_json::from_str(&serialized)
				.expect("re-serialized LatticeConfig must parse again");
			assert_eq!(
				serde_json::to_value(&config).unwrap(),
				serde_json::to_value(&reparsed).unwrap(),
				"config from {} must round-trip through serde without loss",
				dir.display()
			);
			let validator = compiled_schema();
			let raw = read_json(&dir.join(CONFIG_FILE));
			assert!(
				validator.is_valid(&raw),
				"shipped config at {} must validate against the schema",
				dir.display()
			);
		}
	}

	#[test]
	fn engine_string_form_parses_to_version() {
		let engines: EngineMap = serde_json::from_value(json!({ "node": ">=20.0.0" })).unwrap();
		let spec = &engines["node"];
		assert!(matches!(spec, EngineSpec::Version(_)));
		assert_eq!(spec.version(), Some(">=20.0.0"));
		assert!(!spec.is_detailed());
		assert_eq!(spec.version_cmd(), None);
	}

	#[test]
	fn engine_object_form_parses_to_detailed() {
		let engines: EngineMap = serde_json::from_value(json!({
			"alpes": {
				"version": ">=2.6.7",
				"versionCmd": "alp --version",
				"installCmd": "curl ... | sh",
				"bin": "bin"
			}
		}))
		.unwrap();
		let spec = &engines["alpes"];
		assert!(spec.is_detailed());
		assert_eq!(spec.version(), Some(">=2.6.7"));
		assert_eq!(spec.version_cmd(), Some("alp --version"));
		assert_eq!(spec.install_cmd(), Some("curl ... | sh"));
		assert_eq!(spec.bin(), Some("bin"));
	}

	#[test]
	fn validate_rejects_unknown_string_engine() {
		let config: LatticeConfig =
			serde_json::from_value(json!({ "engines": { "alpes": ">=2" } })).unwrap();
		let err = config
			.validate()
			.expect_err("string-form unknown engine must be rejected");
		let msg = format!("{err}");
		assert!(
			msg.contains("alpes"),
			"error should mention the engine: {msg}"
		);
		assert!(
			msg.contains("versionCmd"),
			"error should suggest the object form: {msg}"
		);
	}

	#[test]
	fn validate_accepts_unknown_object_engine() {
		let config: LatticeConfig = serde_json::from_value(json!({
			"engines": { "alpes": { "version": ">=2", "versionCmd": "alp --version" } }
		}))
		.unwrap();
		config
			.validate()
			.expect("object-form engine with versionCmd must be accepted");
	}

	#[test]
	fn validate_accepts_well_known_string_engine() {
		let config: LatticeConfig =
			serde_json::from_value(json!({ "engines": { "node": ">=20" } })).unwrap();
		config
			.validate()
			.expect("well-known string engine must be accepted");
	}

	#[test]
	fn engine_table_is_unique_and_names_a_version_command() {
		let mut seen = std::collections::HashSet::new();
		for (engine, version_cmd) in WELL_KNOWN_ENGINES {
			assert!(seen.insert(*engine), "duplicate engine '{engine}' in table");
			assert!(
				version_cmd.split_whitespace().count() >= 2,
				"engine '{engine}' has no version arguments: '{version_cmd}'"
			);
			assert_eq!(builtin_version_cmd(engine), Some(*version_cmd));
		}
		assert_eq!(well_known_engine_names().len(), WELL_KNOWN_ENGINES.len());
	}

	#[test]
	fn python3_checks_python3_not_python() {
		// `python` on PATH may be a different interpreter entirely, so a
		// `python3` constraint has to be checked against `python3`.
		assert_eq!(builtin_version_cmd("python3"), Some("python3 --version"));
		assert_eq!(builtin_version_cmd("python"), Some("python --version"));
	}

	#[test]
	fn lockfile_table_is_unique() {
		let mut seen = std::collections::HashSet::new();
		for lf in LOCKFILES {
			assert!(seen.insert(*lf), "duplicate lockfile '{lf}' in table");
		}
	}

	#[test]
	fn validate_rejects_duplicate_workspace_names() {
		let config: LatticeConfig = serde_json::from_value(json!({
			"workspaces": [
				{ "name": "dup", "path": "a" },
				{ "name": "dup", "path": "b" }
			]
		}))
		.unwrap();
		assert!(
			config.validate().is_err(),
			"duplicate workspace names must be rejected"
		);
	}

	/// A `dependsOn` naming a workspace that does not exist builds no edge, so the
	/// run loses the ordering it was written to guarantee and says nothing.
	#[test]
	fn validate_rejects_a_workspace_dep_that_names_nothing() {
		let config: LatticeConfig = serde_json::from_value(json!({
			"workspaces": [
				{ "name": "lib", "path": "lib" },
				{ "name": "app", "path": "app", "dependsOn": ["libb"] }
			]
		}))
		.unwrap();
		let message = config
			.validate()
			.expect_err("an unknown workspace dep must be rejected")
			.to_string();
		assert!(message.contains("'libb'"), "must name the typo: {message}");
		assert!(
			message.contains("Did you mean `lib`?"),
			"must offer the near miss: {message}"
		);
	}

	#[test]
	fn validate_accepts_a_workspace_dep_that_resolves() {
		let config: LatticeConfig = serde_json::from_value(json!({
			"workspaces": [
				{ "name": "lib", "path": "lib" },
				{ "name": "app", "path": "app", "dependsOn": ["lib"] }
			]
		}))
		.unwrap();
		config
			.validate()
			.expect("a resolvable dep must be accepted");
	}

	#[test]
	fn validate_rejects_a_self_dependent_workspace() {
		let config: LatticeConfig = serde_json::from_value(json!({
			"workspaces": [{ "name": "app", "path": "app", "dependsOn": ["app"] }]
		}))
		.unwrap();
		assert!(config.validate().is_err());
	}

	/// Same failure mode one level up: a task dep that matches no task in the
	/// map resolves to no node, so the prerequisite is dropped in silence.
	#[test]
	fn validate_rejects_a_task_dep_that_names_nothing() {
		let config: LatticeConfig = serde_json::from_value(json!({
			"tasks": { "build": { "dependsOn": ["codegen"] } }
		}))
		.unwrap();
		let message = config
			.validate()
			.expect_err("an undefined task dep must be rejected")
			.to_string();
		assert!(message.contains("'codegen'"), "{message}");
		assert!(message.contains("Defined tasks: build"), "{message}");
	}

	/// `^build` is the same task in dependency workspaces, so it resolves against
	/// the task map exactly like a bare `build` does.
	#[test]
	fn validate_checks_the_caret_form_against_the_task_map() {
		let ok: LatticeConfig = serde_json::from_value(json!({
			"tasks": { "build": { "dependsOn": ["^build"] } }
		}))
		.unwrap();
		ok.validate()
			.expect("^build resolves to the declared build task");

		let bad: LatticeConfig = serde_json::from_value(json!({
			"tasks": { "build": { "dependsOn": ["^compile"] } }
		}))
		.unwrap();
		let message = bad
			.validate()
			.expect_err("^compile names nothing")
			.to_string();
		assert!(message.contains("'compile'"), "{message}");
	}

	/// `/etc` is not "absolute" on Windows — it names no drive — but `join`
	/// resolves it to the drive root all the same, so it leaves the repo just as
	/// an absolute path does. Every spelling that does that has to be rejected on
	/// every platform, or the guard holds only where it was written.
	#[test]
	fn validate_rejects_a_workspace_path_outside_the_repo() {
		for path in [
			"../outside",
			"apps/../../outside",
			"/etc",
			"\\etc",
			"C:\\Windows",
			"C:etc",
		] {
			let config: LatticeConfig =
				serde_json::from_value(json!({ "workspaces": [{ "name": "esc", "path": path }] }))
					.unwrap();
			assert!(
				config.validate().is_err(),
				"path '{path}' must be rejected: everything downstream treats the \
				 workspace dir as the boundary of what a task may touch"
			);
		}
	}

	#[test]
	fn validate_allows_a_path_that_dips_and_returns() {
		let config: LatticeConfig = serde_json::from_value(
			json!({ "workspaces": [{ "name": "a", "path": "apps/../apps/web" }] }),
		)
		.unwrap();
		config.validate().expect("the path stays inside the repo");
	}

	#[test]
	fn global_dependencies_and_env_parse() {
		let config = parse_config(
			r#"{ "globalDependencies": ["tsconfig.base.json", "proto/**"], "globalEnv": ["CI"] }"#,
		)
		.expect("both root keys must parse");
		assert_eq!(
			config.global_dependencies,
			["tsconfig.base.json", "proto/**"]
		);
		assert_eq!(config.global_env, ["CI"]);
	}

	#[test]
	fn global_keys_default_to_empty() {
		let config = parse_config("{}").unwrap();
		assert!(config.global_dependencies.is_empty());
		assert!(config.global_env.is_empty());
	}

	#[test]
	fn duration_parses_units() {
		assert_eq!(Duration::parse("90s").unwrap().as_secs(), 90);
		assert_eq!(Duration::parse("5m").unwrap().as_secs(), 300);
		assert_eq!(Duration::parse("1h").unwrap().as_secs(), 3600);
		assert_eq!(Duration::parse("30").unwrap().as_secs(), 30);
		// Anything under a second rounds up rather than to zero, which would kill
		// the task the instant it started.
		assert_eq!(Duration::parse("1ms").unwrap().as_secs(), 1);
		assert!(Duration::parse("0s").is_err());
		assert!(Duration::parse("later").is_err());
		assert!(Duration::parse("10 parsecs").is_err());
	}

	#[test]
	fn duration_round_trips_through_serde() {
		for (text, canonical) in [("90s", "90s"), ("5m", "5m"), ("3600", "1h")] {
			let d = Duration::parse(text).unwrap();
			assert_eq!(
				serde_json::to_string(&d).unwrap(),
				format!("\"{canonical}\"")
			);
			let back: Duration = serde_json::from_str(&format!("\"{canonical}\"")).unwrap();
			assert_eq!(back, d);
		}
		// A bare number of seconds reads naturally too.
		let n: Duration = serde_json::from_str("45").unwrap();
		assert_eq!(n.as_secs(), 45);
	}

	/// A persistent task is asked to keep running, so a timeout on one would only
	/// ever cut short the thing it exists to hold open.
	#[test]
	fn a_persistent_task_has_no_effective_timeout() {
		let dev = PipelineTask {
			persistent: Some(true),
			timeout: Some(Duration(30)),
			..Default::default()
		};
		assert_eq!(dev.effective_timeout(), None);

		let build = PipelineTask {
			timeout: Some(Duration(30)),
			..Default::default()
		};
		assert_eq!(
			build.effective_timeout(),
			Some(std::time::Duration::from_secs(30))
		);
	}

	#[test]
	fn resolve_engines_workspace_overrides_root() {
		let root: EngineMap =
			serde_json::from_value(json!({ "node": ">=18", "rust": ">=1.75" })).unwrap();
		let workspace: EngineMap =
			serde_json::from_value(json!({ "node": ">=20", "bun": ">=1.1" })).unwrap();
		let merged = resolve_engines(&root, &workspace);
		assert_eq!(merged["node"].version(), Some(">=20"), "workspace key wins");
		assert_eq!(
			merged["rust"].version(),
			Some(">=1.75"),
			"root-only key kept"
		);
		assert_eq!(
			merged["bun"].version(),
			Some(">=1.1"),
			"workspace-only key added"
		);
	}

	#[test]
	fn cache_size_parses_units() {
		assert_eq!(
			CacheSize::parse("10GB").unwrap().as_bytes(),
			10 * 1024 * 1024 * 1024
		);
		assert_eq!(
			CacheSize::parse("512MB").unwrap().as_bytes(),
			512 * 1024 * 1024
		);
		assert_eq!(CacheSize::parse("1024").unwrap().as_bytes(), 1024);
		assert_eq!(
			CacheSize::parse("2tb").unwrap().as_bytes(),
			2u64 * 1024 * 1024 * 1024 * 1024
		);
	}

	#[test]
	fn cache_size_round_trips_through_serde() {
		let cs = CacheSize::parse("10GB").unwrap();
		let json = serde_json::to_string(&cs).unwrap();
		assert_eq!(json, "\"10GB\"");
		let back: CacheSize = serde_json::from_str(&json).unwrap();
		assert_eq!(back, cs);
	}

	#[test]
	fn settings_defaults() {
		let s: Settings = serde_json::from_value(json!({})).unwrap();
		assert!(!s.loquacious);
		assert!(s.version_check);
		assert_eq!(s.cache_dir(), ".lattice/cache");
		assert!(s.max_cache_size.is_none());
	}

	#[test]
	fn settings_explicit_values() {
		let s: Settings =
			serde_json::from_value(json!({ "loquacious": true, "versionCheck": false })).unwrap();
		assert!(s.loquacious);
		assert!(!s.version_check);
	}

	#[test]
	fn tasks_preserve_declaration_order() {
		let config: LatticeConfig =
			serde_json::from_str(r#"{ "tasks": { "build": {}, "test": {}, "dev": {} } }"#).unwrap();
		let keys: Vec<&str> = config.tasks.keys().map(|s| s.as_str()).collect();
		assert_eq!(keys, vec!["build", "test", "dev"]);
	}

	#[test]
	fn pipeline_task_cacheability() {
		let persistent = PipelineTask {
			persistent: Some(true),
			..Default::default()
		};
		assert!(persistent.is_persistent());
		assert!(!persistent.is_cacheable());

		let no_cache = PipelineTask {
			cache: Some(false),
			..Default::default()
		};
		assert!(!no_cache.is_cacheable());

		let normal = PipelineTask::default();
		assert!(normal.is_cacheable());
	}

	#[test]
	fn unknown_top_level_key_is_rejected() {
		let message = parse_failure("{\n  \"workspaces\": [],\n  \"projects\": {}\n}");
		assert!(
			message.contains("unknown field `projects`"),
			"must name the key: {message}"
		);
		assert!(
			message.contains("at the top level of lattice.json"),
			"must say where it is: {message}"
		);
		assert!(
			message.contains("line 3"),
			"must give the position: {message}"
		);
		assert!(
			message.contains(
				"Fields accepted here: $schema, latticeVersion, workspaces, engines, \
				 globalDependencies, globalEnv, tasks, settings"
			),
			"must list what belongs there: {message}"
		);
		assert!(
			!message.contains("Did you mean"),
			"nothing valid resembles `projects`: {message}"
		);
	}

	#[test]
	fn a_misspelled_outputs_suggests_outputs() {
		let message = parse_failure(r#"{ "tasks": { "build": { "output": ["dist/**"] } } }"#);
		assert!(
			message.contains("unknown field `output` in tasks.build"),
			"must place the key inside its task: {message}"
		);
		assert!(
			message.contains("Did you mean `outputs`?"),
			"must offer the near miss: {message}"
		);
	}

	#[test]
	fn a_misspelled_inputs_suggests_inputs() {
		let message = parse_failure(r#"{ "tasks": { "test": { "input": ["src/**"] } } }"#);
		assert!(
			message.contains("unknown field `input` in tasks.test"),
			"{message}"
		);
		assert!(message.contains("Did you mean `inputs`?"), "{message}");
	}

	#[test]
	fn unknown_workspace_key_names_the_entry() {
		let message = parse_failure(
			r#"{ "workspaces": [ { "name": "a", "path": "a" },
			                     { "name": "b", "path": "b", "dependOn": ["a"] } ] }"#,
		);
		assert!(
			message.contains("unknown field `dependOn` in workspaces[1]"),
			"must index the offending entry: {message}"
		);
		assert!(message.contains("Did you mean `dependsOn`?"), "{message}");
	}

	#[test]
	fn unknown_settings_key_is_rejected() {
		let message = parse_failure(r#"{ "settings": { "logging": true } }"#);
		assert!(
			message.contains("unknown field `logging` in settings"),
			"{message}"
		);
		assert!(message.contains("loquacious"), "{message}");
	}

	#[test]
	fn unknown_engine_object_key_is_rejected() {
		let message = parse_failure(r#"{ "engines": { "node": { "versionCmnd": "node -v" } } }"#);
		assert!(
			message.contains("unknown field `versionCmnd` in engines.node"),
			"an untagged enum would report only that no variant matched: {message}"
		);
		assert!(message.contains("Did you mean `versionCmd`?"), "{message}");
	}

	#[test]
	fn an_engine_that_is_neither_form_names_both() {
		let error = parse_config(r#"{ "engines": { "node": 20 } }"#).expect_err("must be rejected");
		let message = format!("{error:#}");
		assert!(
			message.contains("expected a version constraint string or an engine object"),
			"{message}"
		);
	}

	/// The unknown-key message is the whole error. Wrapping it in a parse context
	/// would indent it under a `Caused by:` and bury the suggestion.
	#[test]
	fn an_unknown_key_is_not_wrapped_in_a_parse_context() {
		let error = parse_config(r#"{ "projects": {} }"#).expect_err("must be rejected");
		assert_eq!(
			format!("{error:#}"),
			error.to_string(),
			"the unknown-key error must have no cause chain"
		);
	}

	/// Malformed JSON keeps serde's own message and position.
	#[test]
	fn malformed_json_still_reports_a_parse_failure() {
		let error = parse_config("{ \"tasks\": ").expect_err("must be rejected");
		let rendered = format!("{error:#}");
		assert!(
			rendered.contains("failed to parse lattice.json"),
			"{rendered}"
		);
	}

	#[test]
	fn trailing_content_after_the_object_is_rejected() {
		let error = parse_config(r#"{ "tasks": {} } trailing"#).expect_err("must be rejected");
		assert!(
			format!("{error:#}").contains("failed to parse lattice.json"),
			"{error:#}"
		);
	}

	#[test]
	fn suggestions_stay_within_one_or_two_edits() {
		let expected = ["outputs".to_string(), "inputs".to_string()];
		assert_eq!(closest_field("output", &expected), Some("outputs"));
		assert_eq!(closest_field("Outputs", &expected), Some("outputs"));
		assert_eq!(closest_field("projects", &expected), None);
		// Four characters or fewer allow one edit only, so a short key does not
		// collect a suggestion by accident.
		assert_eq!(closest_field("env", &["ignore".to_string()]), None);
	}

	#[test]
	fn edit_distance_counts_edits() {
		assert_eq!(edit_distance("outputs", "outputs"), 0);
		assert_eq!(edit_distance("output", "outputs"), 1);
		assert_eq!(edit_distance("dependOn", "dependsOn"), 1);
		assert_eq!(edit_distance("OUTPUTS", "outputs"), 0);
		assert_eq!(edit_distance("", "abc"), 3);
	}

	/// Property names of a schema object, which must also forbid extras.
	fn schema_properties(node: &Value, label: &str) -> Vec<String> {
		assert_eq!(
			node.get("additionalProperties"),
			Some(&Value::Bool(false)),
			"{label} must set additionalProperties: false"
		);
		node.get("properties")
			.and_then(Value::as_object)
			.unwrap_or_else(|| panic!("{label} must declare properties"))
			.keys()
			.cloned()
			.collect()
	}

	/// The keys a config type accepts, read off a value carrying every one of
	/// them. `rename_all` applies in both directions, so what comes back out is
	/// what the deserializer will take in.
	fn accepted_keys<T: Serialize + for<'de> Deserialize<'de>>(full: Value) -> Vec<String> {
		let parsed: T = serde_json::from_value(full).expect("the full example must deserialize");
		match serde_json::to_value(&parsed).expect("must serialize") {
			Value::Object(map) => map.keys().cloned().collect(),
			other => panic!("expected an object, got {other}"),
		}
	}

	/// The bundled schema and the config types have to accept the same keys, or
	/// an editor and `lattice run` disagree about whether a file is valid.
	#[test]
	fn schema_and_config_types_accept_the_same_keys() {
		let schema = schema_json();
		let defs = &schema["$defs"];

		let cases = [
			(
				"the top level",
				schema_properties(&schema, "the root schema"),
				accepted_keys::<LatticeConfig>(json!({
					"$schema": ".lattice/schema.json",
					"latticeVersion": "1.0.0",
					"workspaces": [],
					"engines": {},
					"globalDependencies": [],
					"globalEnv": [],
					"tasks": {},
					"settings": {}
				})),
			),
			(
				"a workspace entry",
				schema_properties(&defs["workspace"], "$defs/workspace"),
				accepted_keys::<WorkspaceConfig>(json!({
					"name": "a",
					"path": "a",
					"auto": true,
					"engines": {},
					"dependsOn": [],
					"scripts": {}
				})),
			),
			(
				"a task",
				schema_properties(&defs["pipelineTask"], "$defs/pipelineTask"),
				accepted_keys::<PipelineTask>(json!({
					"dependsOn": [],
					"inputs": [],
					"outputs": [],
					"ignore": [],
					"env": [],
					"persistent": false,
					"cache": true,
					"timeout": "10m"
				})),
			),
			(
				"settings",
				schema_properties(&defs["settings"], "$defs/settings"),
				accepted_keys::<Settings>(json!({
					"maxCacheSize": "1GB",
					"cacheDir": ".lattice/cache",
					"loquacious": false,
					"versionCheck": true
				})),
			),
			(
				"an engine object",
				schema_properties(&defs["engineSpec"], "$defs/engineSpec"),
				accepted_keys::<EngineSpecObject>(json!({
					"version": ">=1.0.0",
					"versionCmd": "node --version",
					"installCmd": "curl example.com | sh",
					"bin": "bin"
				})),
			),
		];

		for (label, mut from_schema, mut from_types) in cases {
			from_schema.sort();
			from_types.sort();
			assert_eq!(
				from_schema, from_types,
				"the schema and the config type disagree about the keys of {label}"
			);
		}
	}

	/// Every key the schema accepts has to survive the parser too, in place, not
	/// just by name.
	#[test]
	fn a_config_using_every_key_parses() {
		let full = json!({
			"$schema": ".lattice/schema.json",
			"latticeVersion": "1.0.0",
			"workspaces": [{
				"name": "web",
				"path": "apps/web",
				"auto": false,
				"engines": { "node": ">=20.0.0" },
				"dependsOn": [],
				"scripts": { "build": "npm run build" }
			}],
			"engines": {
				"protoc": {
					"version": ">=25.0.0",
					"versionCmd": "protoc --version",
					"installCmd": "sh install.sh",
					"bin": "bin"
				}
			},
			"globalDependencies": ["tsconfig.base.json", "proto/**"],
			"globalEnv": ["NODE_ENV"],
			"tasks": {
				"build": {
					"dependsOn": ["^build"],
					"inputs": ["src/**/*"],
					"outputs": ["dist/**"],
					"ignore": ["**/*.test.*"],
					"env": ["DATABASE_URL"],
					"persistent": false,
					"cache": true,
					"timeout": "10m"
				}
			},
			"settings": {
				"maxCacheSize": "10GB",
				"cacheDir": ".lattice/cache",
				"loquacious": false,
				"versionCheck": true
			}
		});
		let text = serde_json::to_string_pretty(&full).unwrap();
		parse_config(&text).expect("a config using every documented key must parse");
		assert!(
			compiled_schema().is_valid(&full),
			"the same config must validate against the bundled schema"
		);
	}

	#[test]
	fn shipped_configs_reject_an_added_key() {
		for dir in shipped_config_dirs() {
			let Value::Object(mut raw) = read_json(&dir.join(CONFIG_FILE)) else {
				panic!("{} must hold a JSON object", dir.display());
			};
			raw.insert("projects".to_string(), json!({}));
			let text = serde_json::to_string(&Value::Object(raw)).unwrap();
			let message = parse_failure(&text);
			assert!(
				message.contains("unknown field `projects`"),
				"{}: {message}",
				dir.display()
			);
		}
	}
}
