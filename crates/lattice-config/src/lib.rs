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

/// The marker [`parse_error`] recognizes to rewrite a duplicate-key failure into
/// the same shape as the unknown-key message. `serde_json` has no duplicate-key
/// option and `IndexMap`'s own deserializer takes the last value, so the check
/// has to happen in a visitor here, and the visitor is too deep to know the path
/// or the position.
const DUPLICATE_KEY: &str = "duplicate key `";

/// Deserialize a string-keyed map, rejecting a key that appears twice.
///
/// The last value would otherwise win in silence, and for a task or an engine
/// that changes what gets hashed, cached and run with nothing said about it.
fn unique_map<'de, D, V>(deserializer: D) -> std::result::Result<IndexMap<String, V>, D::Error>
where
	D: serde::Deserializer<'de>,
	V: Deserialize<'de>,
{
	struct UniqueMap<V>(std::marker::PhantomData<V>);

	impl<'de, V: Deserialize<'de>> serde::de::Visitor<'de> for UniqueMap<V> {
		type Value = IndexMap<String, V>;

		fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
			f.write_str("an object whose keys are each written once")
		}

		fn visit_map<A: serde::de::MapAccess<'de>>(
			self,
			mut map: A,
		) -> std::result::Result<Self::Value, A::Error> {
			let mut entries = IndexMap::new();
			while let Some(key) = map.next_key::<String>()? {
				let value = map.next_value()?;
				if entries.contains_key(&key) {
					return Err(serde::de::Error::custom(format!("{DUPLICATE_KEY}{key}`")));
				}
				entries.insert(key, value);
			}
			Ok(entries)
		}
	}

	deserializer.deserialize_map(UniqueMap(std::marker::PhantomData))
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

/// What `lattice.json` says about the workspaces one `overrides` key matches.
///
/// The key is a workspace name, or a glob over workspace paths when it holds a
/// `/` or a glob character.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceOverride {
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub name: Option<String>,
	/// The tool that runs this workspace's tasks, or `"none"` for a workspace
	/// whose every task is written out under `tasks`.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub driver: Option<String>,
	#[serde(
		default,
		deserialize_with = "unique_map",
		skip_serializing_if = "IndexMap::is_empty"
	)]
	pub engines: EngineMap,
	/// Workspaces this one depends on beyond what its manifest already says.
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub depends_on: Vec<String>,
	#[serde(
		default,
		deserialize_with = "unique_map",
		skip_serializing_if = "IndexMap::is_empty"
	)]
	pub tasks: IndexMap<String, TaskOverride>,
}

pub const NO_DRIVER: &str = "none";

/// A task in an override: a bare command, or the full task object.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum TaskOverride {
	Command(String),
	Detailed(PipelineTask),
}

/// Hand-written for the reason given on [`EngineSpec`]'s impl: an untagged enum
/// would swallow the unknown-field error the object form raises.
impl<'de> Deserialize<'de> for TaskOverride {
	fn deserialize<D: serde::Deserializer<'de>>(
		deserializer: D,
	) -> std::result::Result<Self, D::Error> {
		struct OverrideVisitor;

		impl<'de> serde::de::Visitor<'de> for OverrideVisitor {
			type Value = TaskOverride;

			fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
				f.write_str("a command string or a task object")
			}

			fn visit_str<E: serde::de::Error>(
				self,
				v: &str,
			) -> std::result::Result<TaskOverride, E> {
				Ok(TaskOverride::Command(v.to_string()))
			}

			fn visit_map<A: serde::de::MapAccess<'de>>(
				self,
				map: A,
			) -> std::result::Result<TaskOverride, A::Error> {
				PipelineTask::deserialize(serde::de::value::MapAccessDeserializer::new(map))
					.map(TaskOverride::Detailed)
			}
		}

		deserializer.deserialize_any(OverrideVisitor)
	}
}

impl TaskOverride {
	pub fn into_task(self) -> PipelineTask {
		match self {
			TaskOverride::Command(run) => PipelineTask {
				run: Some(run),
				..PipelineTask::default()
			},
			TaskOverride::Detailed(task) => task,
		}
	}
}

/// How a task behaves. Every field is optional so that layers — Lattice's
/// defaults, the root `tasks` map, a workspace override — can each set only
/// what they mean to, and [`PipelineTask::layer`] stacks them.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PipelineTask {
	/// The command, when a workspace override writes it out instead of letting
	/// Lattice find it in the workspace's own manifest.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub run: Option<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub depends_on: Option<Vec<String>>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub inputs: Option<Vec<String>>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub outputs: Option<Vec<String>>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub ignore: Option<Vec<String>>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub env: Option<Vec<String>>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub persistent: Option<bool>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub cache: Option<bool>,
	/// How long the task may run before it is killed. Absent means no limit.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub timeout: Option<Duration>,
}

/// What Lattice assumes about a task by its name alone, before `lattice.json`
/// says anything. `build` waits on the builds of the workspaces it depends on;
/// the names that conventionally start a server keep running; the ones that
/// change something outside the workspace are never answered from the cache.
pub fn builtin_task_defaults(task: &str) -> PipelineTask {
	match task {
		"build" => PipelineTask {
			depends_on: Some(vec!["^build".to_string()]),
			..PipelineTask::default()
		},
		"dev" | "start" | "serve" | "watch" => PipelineTask {
			persistent: Some(true),
			..PipelineTask::default()
		},
		"clean" | "deploy" | "publish" | "release" => PipelineTask {
			cache: Some(false),
			..PipelineTask::default()
		},
		_ => PipelineTask::default(),
	}
}

impl PipelineTask {
	/// `self` with every field `over` sets replacing this one's. A list replaces
	/// rather than extends: an override that names `outputs` means those outputs.
	pub fn layer(mut self, over: &PipelineTask) -> PipelineTask {
		macro_rules! take {
			($($field:ident),*) => {
				$(if over.$field.is_some() {
					self.$field = over.$field.clone();
				})*
			};
		}
		take!(run, depends_on, inputs, outputs, ignore, env, persistent, cache, timeout);
		self
	}

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
	/// The longest timeout Lattice accepts. Past this the value stops behaving
	/// like a timeout at all: the seconds no longer fit the arithmetic the runner
	/// does on them, and an overflowed deadline is never reached, so an oversized
	/// timeout would silently mean no timeout.
	pub const MAX_SECS: u64 = 365 * 24 * 60 * 60;

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
		if seconds > Duration::MAX_SECS as f64 {
			bail!(
				"duration '{s}' is longer than the maximum of 365 days. Use a shorter \
				 duration, or leave `timeout` out to let the task run without a limit"
			);
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
				if v > Duration::MAX_SECS {
					return Err(serde::de::Error::custom(format!(
						"duration of {v} seconds is longer than the maximum of 365 days. Use a \
						 shorter duration, or leave `timeout` out to let the task run without a \
						 limit"
					)));
				}
				Ok(Duration(v))
			}

			fn visit_f64<E: serde::de::Error>(self, v: f64) -> std::result::Result<Duration, E> {
				if v.fract() != 0.0 {
					return Err(serde::de::Error::custom(format!(
						"duration {v} is not a whole number of seconds. Write a whole number of \
						 seconds, or a duration string such as \"90s\", \"1500ms\", or \"10m\""
					)));
				}
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
	#[serde(default, skip_serializing_if = "is_false")]
	pub loquacious: bool,
	#[serde(default = "default_true", skip_serializing_if = "is_true")]
	pub version_check: bool,
}

fn is_false(b: &bool) -> bool {
	!*b
}

fn is_true(b: &bool) -> bool {
	*b
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
	fn is_default(&self) -> bool {
		self.max_cache_size.is_none()
			&& self.cache_dir.is_none()
			&& !self.loquacious
			&& self.version_check
	}

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
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub lattice_version: Option<String>,
	/// Directories, or globs of them, that are workspaces on top of the ones the
	/// repo's own workspace files already name.
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub workspaces: Vec<String>,
	/// Globs of workspace paths to leave out, whoever named them.
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub exclude: Vec<String>,
	#[serde(
		default,
		deserialize_with = "unique_map",
		skip_serializing_if = "IndexMap::is_empty"
	)]
	pub engines: EngineMap,
	/// Repo-root-relative globs whose contents feed every task's cache key.
	///
	/// A task's `inputs` are relative to its own workspace, so a file above the
	/// workspace — a shared `tsconfig.base.json`, a schema directory, a root
	/// `.env` — cannot be named there in a way that means the same thing for
	/// every workspace. Anything listed here is hashed into all of them.
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub global_dependencies: Vec<String>,
	/// Environment variable names whose values feed every task's cache key.
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub global_env: Vec<String>,
	#[serde(
		default,
		deserialize_with = "unique_map",
		skip_serializing_if = "IndexMap::is_empty"
	)]
	pub tasks: IndexMap<String, PipelineTask>,
	#[serde(
		default,
		deserialize_with = "unique_map",
		skip_serializing_if = "IndexMap::is_empty"
	)]
	pub overrides: IndexMap<String, WorkspaceOverride>,
	#[serde(default, skip_serializing_if = "Settings::is_default")]
	pub settings: Settings,
}

impl LatticeConfig {
	/// Checks that need nothing but the file. Whether an override key or a
	/// `dependsOn` names a real workspace is only known once the workspaces are
	/// discovered, and is checked there.
	pub fn validate(&self) -> Result<()> {
		check_string_engines(&self.engines, "root")?;
		check_engine_bins(&self.engines, "root")?;
		for (key, over) in &self.overrides {
			let scope = format!("overrides.{key}");
			check_string_engines(&over.engines, &scope)?;
			check_engine_bins(&over.engines, &scope)?;
			if let Some(name) = &over.name {
				if name.trim().is_empty() {
					bail!(
						"`{scope}.name` is empty. Give the workspace a name, or leave the key out"
					);
				}
			}
		}
		if let Some(cache_dir) = &self.settings.cache_dir {
			if cache_dir.trim().is_empty() {
				bail!(
					"`settings.cacheDir` is empty. Name a directory, like \".lattice/cache\", \
					 or leave the key out to use that default"
				);
			}
			check_cache_dir(cache_dir)?;
		}
		for pattern in &self.workspaces {
			check_contained_pattern("workspaces", pattern)?;
		}
		for pattern in &self.exclude {
			check_contained_pattern("exclude", pattern)?;
		}
		for (task, cfg) in &self.tasks {
			if cfg.run.is_some() {
				bail!(
					"`tasks.{task}` has a `run`, but the root `tasks` map says how a task behaves \
					 in every workspace, not what it runs. Put the command under the workspace \
					 it belongs to: \"overrides\": {{ \"<workspace>\": {{ \"tasks\": {{ \"{task}\": \
					 \"<command>\" }} }} }}"
				);
			}
			for dep in cfg.depends_on.iter().flatten() {
				if dep == task {
					bail!("task '{task}' lists itself in `dependsOn`");
				}
			}
		}
		Ok(())
	}
}

/// A `workspaces` or `exclude` entry is matched against paths inside the repo,
/// so one that starts outside it can never match anything.
fn check_contained_pattern(field: &str, pattern: &str) -> Result<()> {
	if pattern.trim().is_empty() {
		bail!("`{field}` has an empty entry. Remove it, or write a directory like \"apps/*\"");
	}
	match contained_depth(pattern) {
		Ok(_) => Ok(()),
		Err(Escape::NotRelative) => bail!(
			"`{field}` entry '{pattern}' is not relative to the repo root. Write it relative to \
			 the repo root, like \"apps/*\""
		),
		Err(Escape::LeavesRoot) => bail!(
			"`{field}` entry '{pattern}' points outside the repo root. Every workspace has to be \
			 inside the repo"
		),
		Err(Escape::AmbiguousWhitespace) => bail!(
			"`{field}` entry '{pattern}' has leading or trailing whitespace around a directory \
			 name. Windows drops that whitespace and unix keeps it, so the entry would match a \
			 different directory on each. Remove it"
		),
	}
}

/// Why a declared path does not stay under the directory it is joined to.
enum Escape {
	NotRelative,
	LeavesRoot,
	/// Leading or trailing whitespace, which Windows strips from a path component
	/// and unix keeps, so the same string names two different directories.
	AmbiguousWhitespace,
}

/// How many directories deep `path` lands below the directory it is joined to,
/// or the reason it does not land below it at all. `Ok(0)` is the joined-to
/// directory itself.
///
/// Judged as text first, because `Path` answers for the platform it is running on
/// and a `lattice.json` is committed and shared. On unix `C:\Windows` is one
/// ordinary filename and `\etc` is a relative one; on Windows both resolve to a
/// drive root and leave the repo. A path that escapes anywhere has to be rejected
/// everywhere, or the same config means two things. `Path` is then consulted as
/// well, so a spelling the running platform treats as absolute is caught even if
/// the text rules do not enumerate it.
fn contained_depth(path: &str) -> std::result::Result<i32, Escape> {
	let bytes = path.as_bytes();
	let rooted = path.starts_with('/') || path.starts_with('\\');
	let drive_prefixed = bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':';
	let platform_rooted = Path::new(path).components().any(|c| {
		matches!(
			c,
			std::path::Component::RootDir | std::path::Component::Prefix(_)
		)
	});
	if rooted || drive_prefixed || platform_rooted {
		return Err(Escape::NotRelative);
	}

	// Both separators, for the same reason.
	let mut depth: i32 = 0;
	for part in path.split(['/', '\\']) {
		if part.trim() != part {
			return Err(Escape::AmbiguousWhitespace);
		}
		match part {
			"" | "." => {}
			".." => depth -= 1,
			_ => depth += 1,
		}
		if depth < 0 {
			return Err(Escape::LeavesRoot);
		}
	}
	Ok(depth)
}

/// `settings.cacheDir` is joined to the repo root and then owned outright:
/// `lattice prune` removes cache archives and partial writes found there. It has
/// to stay inside the repo, and it cannot be the repo root itself.
fn check_cache_dir(path: &str) -> Result<()> {
	match contained_depth(path) {
		Ok(0) => bail!(
			"`settings.cacheDir` is '{path}', which is the repo root itself. Point it at a \
			 directory of its own, like \".lattice/cache\" — `lattice prune` deletes cache \
			 archives and partial writes in whatever directory it names"
		),
		Ok(_) => Ok(()),
		Err(Escape::NotRelative) => bail!(
			"`settings.cacheDir` is '{path}', which is not relative to the repo root. Write \
			 it relative to the repo root, like \".lattice/cache\""
		),
		Err(Escape::LeavesRoot) => bail!(
			"`settings.cacheDir` is '{path}', which points outside the repo root. The cache \
			 directory must stay inside the repo"
		),
		Err(Escape::AmbiguousWhitespace) => bail!(
			"`settings.cacheDir` is '{path}', which has leading or trailing whitespace around \
			 a directory name. Windows drops that whitespace and unix keeps it, so the path \
			 would name a different directory on each. Remove it"
		),
	}
}

/// An engine's `bin` is joined to the toolchain install directory and every
/// resolved bin dir is prepended to a task's PATH. An absolute `bin` would
/// replace the install path outright and put a directory Lattice never
/// provisioned in front of every command.
fn check_engine_bins(engines: &EngineMap, scope: &str) -> Result<()> {
	for (name, spec) in engines {
		let Some(bin) = spec.bin() else { continue };
		if bin.trim().is_empty() {
			bail!(
				"engine '{name}' in {scope} has an empty `bin`. Leave `bin` out to use the \
				 default of \"bin\", or name a directory inside the toolchain install"
			);
		}
		match contained_depth(bin) {
			Ok(_) => {}
			Err(Escape::NotRelative) => bail!(
				"engine '{name}' in {scope} has a `bin` of '{bin}', which is not relative to \
				 the toolchain install. Write `bin` as a path inside the toolchain \
				 directory, like \"bin\""
			),
			Err(Escape::LeavesRoot) => bail!(
				"engine '{name}' in {scope} has a `bin` of '{bin}', which points outside the \
				 toolchain install. Write `bin` as a path inside the toolchain directory, \
				 like \"bin\""
			),
			Err(Escape::AmbiguousWhitespace) => bail!(
				"engine '{name}' in {scope} has a `bin` of '{bin}', which has leading or \
				 trailing whitespace around a directory name. Windows drops that whitespace \
				 and unix keeps it, so the path would name a different directory on each. \
				 Remove it"
			),
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
	if is_v1_config(content) {
		bail!(
			"{CONFIG_FILE} is written for Lattice 1.x, which listed every workspace as an object \
			 with a name and a path. Run `lattice migrate` to rewrite it for this version"
		);
	}
	let mut deserializer = serde_json::Deserializer::from_str(content);
	let config: LatticeConfig =
		serde_path_to_error::deserialize(&mut deserializer).map_err(parse_error)?;
	deserializer
		.end()
		.with_context(|| format!("failed to parse {CONFIG_FILE}"))?;
	config.validate()?;
	Ok(config)
}

/// JSON the way people write `lattice.json` by hand: two-space indents, fields
/// in the order the type declares them, and a list of short values kept on one
/// line.
pub fn to_config_text<T: Serialize>(value: &T) -> String {
	let pretty = serde_json::to_string_pretty(value).unwrap_or_default();
	let lines: Vec<&str> = pretty.lines().collect();
	let mut out = String::new();
	let mut i = 0;
	while i < lines.len() {
		let line = lines[i];
		if line.ends_with('[') {
			let close = (i + 1..lines.len()).find(|&j| lines[j].trim_start().starts_with(']'));
			if let Some(close) = close {
				let items: Vec<&str> = lines[i + 1..close]
					.iter()
					.map(|l| l.trim().trim_end_matches(','))
					.collect();
				let scalar = |item: &&str| !item.starts_with(['{', '[', '}', ']']);
				let joined = format!("{}{}{}", line, items.join(", "), lines[close].trim_start());
				if items.iter().all(scalar) && joined.len() <= 80 {
					out.push_str(&joined);
					out.push('\n');
					i = close + 1;
					continue;
				}
			}
		}
		out.push_str(line);
		out.push('\n');
		i += 1;
	}
	out
}

/// Whether `content` is a 1.x config: one whose `workspaces` holds objects.
/// Text that is not JSON at all is left for the real parse to report.
pub fn is_v1_config(content: &str) -> bool {
	let Ok(value) = serde_json::from_str::<serde_json::Value>(content) else {
		return false;
	};
	value
		.get("workspaces")
		.and_then(|w| w.as_array())
		.is_some_and(|entries| entries.iter().any(|e| e.is_object()))
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
	if let Some(key) = duplicate_key(&error.inner().to_string()) {
		return duplicate_key_error(&error, &key);
	}
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

/// The duplicated key out of a [`unique_map`] failure, whatever `serde_json`
/// appended to the message.
fn duplicate_key(message: &str) -> Option<String> {
	let rest = message.strip_prefix(DUPLICATE_KEY)?;
	let (key, _) = rest.split_once('`')?;
	Some(key.to_string())
}

fn duplicate_key_error(
	error: &serde_path_to_error::Error<serde_json::Error>,
	key: &str,
) -> anyhow::Error {
	let inner = error.inner();
	let position = format!("line {}, column {}", inner.line(), inner.column());
	let where_it_is = match container_path(error.path(), key) {
		Some(path) => format!("in {path}"),
		None => format!("at the top level of {CONFIG_FILE}"),
	};
	anyhow!(
		"duplicate key `{key}` {where_it_is} ({CONFIG_FILE} {position})\nKeep one of them: the \
		 second replaces the first, so only the last would take effect"
	)
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
/// Shared with `lattice-workspace`, which runs the same check over declared
/// task names, so a near miss reads the same wherever it is caught.
pub fn closest_field<'a>(field: &str, expected: &'a [String]) -> Option<&'a str> {
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

/// Edit distance, case-insensitively, counting a swap of two neighbouring
/// characters as one edit: `tset` is one slip from `test`, not two, and
/// `Outputs` is none from `outputs`.
fn edit_distance(a: &str, b: &str) -> usize {
	let a: Vec<char> = a.chars().flat_map(char::to_lowercase).collect();
	let b: Vec<char> = b.chars().flat_map(char::to_lowercase).collect();
	let mut d = vec![vec![0usize; b.len() + 1]; a.len() + 1];
	for (i, row) in d.iter_mut().enumerate() {
		row[0] = i;
	}
	for j in 0..=b.len() {
		d[0][j] = j;
	}
	for i in 1..=a.len() {
		for j in 1..=b.len() {
			let cost = usize::from(a[i - 1] != b[j - 1]);
			let mut best = (d[i - 1][j] + 1)
				.min(d[i][j - 1] + 1)
				.min(d[i - 1][j - 1] + cost);
			if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
				best = best.min(d[i - 2][j - 2] + 1);
			}
			d[i][j] = best;
		}
	}
	d[a.len()][b.len()]
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

	/// The same with the cause chain, for a message serde raises under this
	/// crate's parse context rather than one the crate rewrites outright.
	fn parse_failure_chain(content: &str) -> String {
		format!(
			"{:#}",
			parse_config(content).expect_err("this config must be rejected")
		)
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

	#[test]
	fn schema_rejects_v1_workspace_objects() {
		let old = json!({ "workspaces": [{ "name": "a", "path": "a" }] });
		assert!(!compiled_schema().is_valid(&old));
	}

	#[test]
	fn config_text_keeps_short_lists_on_one_line() {
		let config = parse_config(
			r#"{"tasks": {"build": {"dependsOn": ["^build"]}, "clean": {}}, "workspaces": ["apps/*", "crates/*"]}"#,
		)
		.unwrap();
		let text = to_config_text(&config);
		assert_eq!(
			text,
			"{\n  \"workspaces\": [\"apps/*\", \"crates/*\"],\n  \"tasks\": {\n    \"build\": {\n      \"dependsOn\": [\"^build\"]\n    },\n    \"clean\": {}\n  }\n}\n"
		);
	}

	#[test]
	fn a_v1_config_is_pointed_at_migrate() {
		let message = parse_failure(r#"{ "workspaces": [{ "name": "a", "path": "a" }] }"#);
		assert!(message.contains("Lattice 1.x"), "{message}");
		assert!(message.contains("lattice migrate"), "{message}");
	}

	#[test]
	fn workspace_globs_and_overrides_parse() {
		let config = parse_config(
			r#"{
			  "workspaces": ["apps/*", "services/api"],
			  "exclude": ["apps/legacy"],
			  "overrides": {
			    "web": { "driver": "pnpm", "dependsOn": ["api"],
			             "tasks": { "build": "vite build", "e2e": { "run": "playwright test", "cache": false } } },
			    "services/*": { "engines": { "go": ">=1.22" } }
			  }
			}"#,
		)
		.unwrap();
		assert_eq!(config.workspaces, ["apps/*", "services/api"]);
		let web = &config.overrides["web"];
		assert_eq!(web.driver.as_deref(), Some("pnpm"));
		assert!(matches!(&web.tasks["build"], TaskOverride::Command(c) if c == "vite build"));
		let e2e = web.tasks["e2e"].clone().into_task();
		assert_eq!(e2e.run.as_deref(), Some("playwright test"));
		assert_eq!(e2e.cache, Some(false));
	}

	#[test]
	fn a_run_in_the_root_task_map_is_sent_to_an_override() {
		let message = parse_failure(r#"{ "tasks": { "build": { "run": "make" } } }"#);
		assert!(message.contains("`tasks.build` has a `run`"), "{message}");
		assert!(message.contains("\"overrides\""), "{message}");
	}

	#[test]
	fn an_unknown_override_field_is_named_with_its_key() {
		let message = parse_failure(r#"{ "overrides": { "web": { "dependOn": ["a"] } } }"#);
		assert!(
			message.contains("unknown field `dependOn` in overrides.web"),
			"{message}"
		);
		assert!(message.contains("Did you mean `dependsOn`?"), "{message}");
	}

	#[test]
	fn layering_replaces_only_what_the_upper_layer_sets() {
		let base = builtin_task_defaults("build");
		let over = PipelineTask {
			outputs: Some(vec!["out/**".into()]),
			..Default::default()
		};
		let layered = base.layer(&over);
		assert_eq!(layered.depends_on, Some(vec!["^build".to_string()]));
		assert_eq!(layered.outputs, Some(vec!["out/**".to_string()]));
	}

	#[test]
	fn builtin_defaults_by_task_name() {
		assert!(builtin_task_defaults("dev").is_persistent());
		assert!(!builtin_task_defaults("clean").is_cacheable());
		assert_eq!(builtin_task_defaults("lint"), PipelineTask::default());
	}

	#[test]
	fn schema_rejects_old_projects_key() {
		let validator = compiled_schema();
		let old = json!({
			"workspaces": ["a"],
			"projects": {}
		});
		assert!(
			!validator.is_valid(&old),
			"a top-level `projects` key must be rejected by the schema"
		);
	}

	#[test]
	fn schema_allows_self_reference() {
		let validator = compiled_schema();
		let ok = json!({
			"$schema": ".lattice/schema.json",
			"workspaces": ["apps/web"]
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

	/// `/etc` is not "absolute" on Windows — it names no drive — but `join`
	/// resolves it to the drive root all the same. Every spelling that leaves the
	/// repo has to be rejected on every platform.
	#[test]
	fn validate_rejects_a_workspace_pattern_outside_the_repo() {
		for path in [
			"../outside",
			"apps/../../outside",
			"/etc",
			"\\etc",
			"C:\\Windows",
			"C:etc",
		] {
			let config: LatticeConfig =
				serde_json::from_value(json!({ "workspaces": [path] })).unwrap();
			assert!(config.validate().is_err(), "'{path}' must be rejected");
			let config: LatticeConfig =
				serde_json::from_value(json!({ "exclude": [path] })).unwrap();
			assert!(
				config.validate().is_err(),
				"exclude '{path}' must be rejected"
			);
		}
	}

	#[test]
	fn validate_allows_a_pattern_that_dips_and_returns() {
		let config: LatticeConfig =
			serde_json::from_value(json!({ "workspaces": ["apps/../apps/*"] })).unwrap();
		config
			.validate()
			.expect("the pattern stays inside the repo");
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
				"Fields accepted here: $schema, latticeVersion, workspaces, exclude, engines, \
				 globalDependencies, globalEnv, tasks, overrides, settings"
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
	fn a_workspaces_entry_that_is_not_a_string_is_rejected() {
		let message = parse_failure_chain(r#"{ "workspaces": ["apps/*", 3] }"#);
		assert!(message.contains("invalid type"), "{message}");
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
		assert_eq!(edit_distance("tset", "test"), 1);
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
					"workspaces": ["apps/*"],
					"exclude": ["apps/old"],
					"engines": { "node": ">=20" },
					"globalDependencies": ["a.json"],
					"globalEnv": ["CI"],
					"tasks": { "build": {} },
					"overrides": { "web": {} },
					"settings": { "loquacious": true }
				})),
			),
			(
				"an override",
				schema_properties(&defs["override"], "$defs/override"),
				accepted_keys::<WorkspaceOverride>(json!({
					"name": "web",
					"driver": "pnpm",
					"engines": { "node": ">=20" },
					"dependsOn": ["api"],
					"tasks": { "build": "x" }
				})),
			),
			(
				"a task",
				schema_properties(&defs["pipelineTask"], "$defs/pipelineTask"),
				accepted_keys::<PipelineTask>(json!({
					"run": "make",
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
					"loquacious": true,
					"versionCheck": false
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
			"workspaces": ["apps/*"],
			"exclude": ["apps/old"],
			"overrides": {
				"web": {
					"name": "site",
					"driver": "npm",
					"engines": { "node": ">=20.0.0" },
					"dependsOn": ["api"],
					"tasks": { "build": "npm run build", "e2e": { "run": "playwright test", "cache": false } }
				}
			},
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

	/// A duplicate key inside `tasks`, `engines` or `scripts` is last-wins in
	/// `serde_json`, so the first declaration disappears and the task quietly
	/// hashes and caches something else.
	#[test]
	fn a_duplicate_task_key_is_rejected() {
		let message =
			parse_failure(r#"{ "tasks": { "build": { "outputs": ["dist/**"] }, "build": {} } }"#);
		assert!(
			message.contains("duplicate key `build` in tasks"),
			"must name the key and where it is: {message}"
		);
		assert!(
			message.contains("lattice.json line 1"),
			"must give the position: {message}"
		);
	}

	#[test]
	fn a_duplicate_engine_key_is_rejected() {
		let message = parse_failure(
			"{\n  \"engines\": {\n    \"node\": \">=20\",\n    \"node\": \">=99\"\n  }\n}",
		);
		assert!(
			message.contains("duplicate key `node` in engines"),
			"{message}"
		);
		assert!(
			message.contains("line 5"),
			"must point at the closing line of the object: {message}"
		);
	}

	#[test]
	fn a_duplicate_override_task_is_rejected() {
		let message = parse_failure(
			r#"{ "overrides": { "a": { "tasks": { "build": "one", "build": "two" } } } }"#,
		);
		assert!(
			message.contains("duplicate key `build` in overrides.a.tasks"),
			"{message}"
		);
	}

	#[test]
	fn a_duplicate_struct_field_is_still_rejected() {
		let message =
			parse_failure_chain(r#"{ "overrides": { "a": { "name": "x", "name": "y" } } }"#);
		assert!(message.contains("duplicate field `name`"), "{message}");
	}

	#[test]
	fn distinct_keys_in_the_same_map_still_parse() {
		let config = parse_config(
			r#"{ "engines": { "node": ">=20", "cargo": ">=1.88" },
			     "tasks": { "build": {}, "test": {} },
			     "overrides": { "a": { "tasks": { "build": "x", "test": "y" } } } }"#,
		)
		.expect("distinct keys must still parse");
		assert_eq!(config.engines.len(), 2);
		assert_eq!(config.tasks.len(), 2);
		assert_eq!(config.overrides["a"].tasks.len(), 2);
	}

	/// An oversized timeout used to saturate the float-to-int cast to `u64::MAX`,
	/// and a deadline that far out overflows rather than arriving — so asking for
	/// a very long timeout produced no timeout at all.
	#[test]
	fn an_oversized_timeout_is_rejected_rather_than_disabled() {
		for text in [
			r#"{ "tasks": { "build": { "timeout": 1e30 } } }"#,
			r#"{ "tasks": { "build": { "timeout": "99999999999999999999s" } } }"#,
			r#"{ "tasks": { "build": { "timeout": 18446744073709551615 } } }"#,
			r#"{ "tasks": { "build": { "timeout": "99999h" } } }"#,
		] {
			let message = parse_failure_chain(text);
			assert!(
				message.contains("longer than the maximum of 365 days"),
				"{text} must be rejected, not silently turned into no timeout: {message}"
			);
		}

		assert!(Duration::parse("365d").is_err(), "`d` is not a unit");
		assert_eq!(
			Duration::parse(&format!("{}s", Duration::MAX_SECS))
				.unwrap()
				.as_secs(),
			Duration::MAX_SECS,
			"the maximum itself must be accepted"
		);
		assert!(Duration::parse(&format!("{}s", Duration::MAX_SECS + 1)).is_err());
	}

	/// The schema declares the number form as an integer, so the parser has to
	/// reject a fraction rather than round it to a value the user did not write.
	#[test]
	fn a_fractional_number_timeout_is_rejected() {
		let message = parse_failure_chain(r#"{ "tasks": { "build": { "timeout": 1.5 } } }"#);
		assert!(
			message.contains("not a whole number of seconds"),
			"{message}"
		);

		let whole: Duration = serde_json::from_str("90.0").expect("a whole float is still seconds");
		assert_eq!(whole.as_secs(), 90);

		let schema = compiled_schema();
		assert!(
			!schema.is_valid(&json!({ "tasks": { "build": { "timeout": 1.5 } } })),
			"the schema must reject the same value the parser does"
		);
		assert!(
			!schema.is_valid(&json!({ "tasks": { "build": { "timeout": 31536001 } } })),
			"the schema must reject a timeout over the maximum too"
		);
		assert!(
			schema.is_valid(&json!({ "tasks": { "build": { "timeout": 31536000 } } })),
			"the maximum itself must validate"
		);
	}

	/// `lattice prune` owns whatever directory `cacheDir` names: it deletes cache
	/// archives and partial writes there. An absolute or escaping value pointed
	/// that at somebody else's files.
	#[test]
	fn validate_rejects_a_cache_dir_outside_the_repo() {
		for dir in [
			"/tmp/lattice",
			"\\tmp",
			"C:\\cache",
			"../cache",
			"a/../../cache",
		] {
			let config: LatticeConfig =
				serde_json::from_value(json!({ "settings": { "cacheDir": dir } })).unwrap();
			assert!(
				config.validate().is_err(),
				"cacheDir '{dir}' must be rejected"
			);
		}
	}

	/// `.` is inside the repo and still wrong: it makes the repo root the cache
	/// directory, so pruning deletes every stray archive sitting there.
	#[test]
	fn validate_rejects_a_cache_dir_that_is_the_repo_root() {
		for dir in [".", "./", "a/.."] {
			let config: LatticeConfig =
				serde_json::from_value(json!({ "settings": { "cacheDir": dir } })).unwrap();
			let message = config
				.validate()
				.expect_err("the repo root must be rejected as a cache dir")
				.to_string();
			assert!(
				message.contains("the repo root itself"),
				"must say why: {message}"
			);
		}

		let empty: LatticeConfig =
			serde_json::from_value(json!({ "settings": { "cacheDir": "  " } })).unwrap();
		assert!(
			empty.validate().is_err(),
			"an empty cacheDir must be rejected"
		);
	}

	#[test]
	fn validate_accepts_a_cache_dir_inside_the_repo() {
		let config: LatticeConfig =
			serde_json::from_value(json!({ "settings": { "cacheDir": ".cache/lattice" } }))
				.unwrap();
		config
			.validate()
			.expect("a contained cacheDir must be accepted");
	}

	/// Windows strips whitespace around a path component and unix keeps it — the
	/// same string, two directories.
	#[test]
	fn validate_rejects_a_workspace_pattern_whose_shape_is_ambiguous() {
		for path in ["apps/web ", " apps/web", "apps/ web", "apps/web/ "] {
			let config: LatticeConfig =
				serde_json::from_value(json!({ "workspaces": [path] })).unwrap();
			let message = config
				.validate()
				.expect_err("whitespace around a directory name must be rejected")
				.to_string();
			assert!(message.contains("whitespace"), "'{path}': {message}");
		}
	}

	/// An engine `bin` is joined to the toolchain install and then prepended to
	/// every task's PATH. An absolute one replaced the install path outright and
	/// still reported a provisioned toolchain.
	#[test]
	fn validate_rejects_an_engine_bin_that_escapes_the_toolchain() {
		for bin in [
			"/usr/bin",
			"\\usr\\bin",
			"C:\\Windows",
			"../../..",
			"bin/../../..",
		] {
			let config: LatticeConfig = serde_json::from_value(json!({
				"engines": { "node": { "version": ">=20", "bin": bin } }
			}))
			.unwrap();
			let message = config
				.validate()
				.expect_err("an escaping bin must be rejected")
				.to_string();
			assert!(
				message.contains("engine 'node' in root"),
				"must name the engine and where it is declared: {message}"
			);
		}
	}

	#[test]
	fn validate_rejects_an_engine_bin_in_an_override_too() {
		let config: LatticeConfig = serde_json::from_value(json!({
			"overrides": {
				"web": { "engines": { "node": { "version": ">=20", "bin": "/usr/bin" } } }
			}
		}))
		.unwrap();
		let message = config
			.validate()
			.expect_err("an override engine bin must be checked too")
			.to_string();
		assert!(
			message.contains("engine 'node' in overrides.web"),
			"{message}"
		);
	}

	#[test]
	fn validate_rejects_an_empty_engine_bin() {
		let config: LatticeConfig = serde_json::from_value(json!({
			"engines": { "node": { "version": ">=20", "bin": "" } }
		}))
		.unwrap();
		let message = config
			.validate()
			.expect_err("an empty bin must be rejected")
			.to_string();
		assert!(message.contains("empty `bin`"), "{message}");
	}

	#[test]
	fn validate_accepts_an_engine_bin_inside_the_toolchain() {
		for bin in ["bin", ".", "usr/local/bin", "bin/../libexec"] {
			let config: LatticeConfig = serde_json::from_value(json!({
				"engines": { "node": { "version": ">=20", "bin": bin } }
			}))
			.unwrap();
			config
				.validate()
				.unwrap_or_else(|e| panic!("bin '{bin}' must be accepted: {e}"));
		}
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
