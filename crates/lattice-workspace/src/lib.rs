//! Workspace discovery: which directories are workspaces, which tool runs
//! each one's tasks, what those tasks are, and which workspaces depend on
//! which.
//!
//! [`members`] finds the directories, from the repo's own workspace files plus
//! `lattice.json`. [`detect_driver`] picks the tool for each from the evidence
//! its files hold — a declaration in `overrides` first, then role (a task runner
//! over a package manager over a build tool), then how deliberate the evidence
//! is (a `packageManager` field over a lockfile over a manifest). [`tasks`]
//! lists what that tool can run there, and [`manifest`] reads the dependencies
//! that order the workspaces.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use indexmap::IndexMap;
use lattice_config::{
	builtin_task_defaults, closest_field, resolve_engines, EngineMap, LatticeConfig, PipelineTask,
	WorkspaceOverride, NO_DRIVER,
};

pub mod manifest;
pub mod members;
mod read;
pub mod scan;
pub mod tasks;
pub mod toolchain;

pub use read::rel_path;

/// The kind of job a tool does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
	Runtime,
	PackageManager,
	BuildTool,
	TaskRunner,
}

impl Role {
	/// Higher rank == more authoritative as a *task driver*. A pure `Runtime`
	/// (node/python/ruby/java) cannot run named tasks on its own.
	fn drive_rank(self) -> u8 {
		match self {
			Role::Runtime => 0,
			Role::BuildTool => 1,
			Role::PackageManager => 2,
			Role::TaskRunner => 3,
		}
	}
}

/// A built-in, extensible registry of known task drivers.
pub struct DriverRegistry;

/// One known tool: how to recognize it, version-check it, and invoke a task.
pub struct DriverSpec {
	pub tool: &'static str,
	/// Every job this tool does. A tool that fills more than one role drives
	/// with its highest-ranked one ([`DriverSpec::drive_role`]).
	pub roles: &'static [Role],
	/// Files that identify this tool when present in a workspace dir. Empty for
	/// a tool with no artifact only it produces: it can still be named in
	/// `engines` or a `.tool-versions` file, which is stronger evidence anyway.
	pub fingerprint: &'static [&'static str],
	/// Command that prints the tool's version.
	pub version_cmd: &'static str,
	/// The ecosystem this tool belongs to, as a stable lowercase slug. A front
	/// end keys a language mark off it. `None` for the language-agnostic task
	/// runners, which sit above whatever the workspace actually is.
	pub language: Option<&'static str>,
	/// Invoke template with a `{task}` placeholder (invoke form via [`DriverSpec::invoke`]).
	invoke_tpl: &'static str,
}

impl DriverSpec {
	pub fn invoke(&self, task: &str) -> String {
		self.invoke_tpl.replace("{task}", task)
	}

	/// The role this tool competes for as a task driver: its highest-ranked one.
	pub fn drive_role(&self) -> Role {
		self.roles
			.iter()
			.copied()
			.max_by_key(|r| r.drive_rank())
			.expect("every driver declares at least one role")
	}
}

static DRIVERS: &[DriverSpec] = &[
	DriverSpec {
		tool: "node",
		language: Some("node"),
		roles: &[Role::Runtime],
		fingerprint: &[".nvmrc"],
		version_cmd: "node --version",
		invoke_tpl: "node {task}",
	},
	DriverSpec {
		tool: "deno",
		language: Some("node"),
		roles: &[Role::Runtime, Role::PackageManager, Role::TaskRunner],
		fingerprint: &["deno.json", "deno.jsonc", "deno.lock"],
		version_cmd: "deno --version",
		invoke_tpl: "deno task {task}",
	},
	DriverSpec {
		tool: "bun",
		language: Some("node"),
		roles: &[Role::Runtime, Role::PackageManager],
		fingerprint: &["bun.lockb", "bun.lock"],
		version_cmd: "bun --version",
		invoke_tpl: "bun run {task}",
	},
	DriverSpec {
		tool: "pnpm",
		language: Some("node"),
		roles: &[Role::PackageManager],
		fingerprint: &["pnpm-lock.yaml"],
		version_cmd: "pnpm --version",
		invoke_tpl: "pnpm run {task}",
	},
	DriverSpec {
		tool: "yarn",
		language: Some("node"),
		roles: &[Role::PackageManager],
		fingerprint: &["yarn.lock"],
		version_cmd: "yarn --version",
		invoke_tpl: "yarn {task}",
	},
	DriverSpec {
		tool: "npm",
		language: Some("node"),
		roles: &[Role::PackageManager],
		fingerprint: &["package-lock.json", "npm-shrinkwrap.json"],
		version_cmd: "npm --version",
		invoke_tpl: "npm run {task}",
	},
	DriverSpec {
		tool: "cargo",
		language: Some("rust"),
		roles: &[Role::BuildTool],
		fingerprint: &["Cargo.lock", "rust-toolchain.toml", "rust-toolchain"],
		version_cmd: "cargo --version",
		invoke_tpl: "cargo {task}",
	},
	DriverSpec {
		tool: "go",
		language: Some("go"),
		roles: &[Role::BuildTool],
		fingerprint: &["go.sum"],
		version_cmd: "go version",
		invoke_tpl: "go {task}",
	},
	DriverSpec {
		tool: "uv",
		language: Some("python"),
		roles: &[Role::PackageManager],
		fingerprint: &["uv.lock"],
		version_cmd: "uv --version",
		invoke_tpl: "uv run {task}",
	},
	DriverSpec {
		tool: "poetry",
		language: Some("python"),
		roles: &[Role::PackageManager],
		fingerprint: &["poetry.lock"],
		version_cmd: "poetry --version",
		invoke_tpl: "poetry run {task}",
	},
	DriverSpec {
		tool: "pdm",
		language: Some("python"),
		roles: &[Role::PackageManager],
		fingerprint: &["pdm.lock"],
		version_cmd: "pdm --version",
		invoke_tpl: "pdm run {task}",
	},
	DriverSpec {
		tool: "pipenv",
		language: Some("python"),
		roles: &[Role::PackageManager],
		fingerprint: &["Pipfile.lock"],
		version_cmd: "pipenv --version",
		invoke_tpl: "pipenv run {task}",
	},
	DriverSpec {
		tool: "pip",
		language: Some("python"),
		roles: &[Role::PackageManager],
		// A `requirements.txt` names no tool — pip, uv, and pip-tools all read it
		// — so pip is only ever selected by declaration.
		fingerprint: &[],
		version_cmd: "pip --version",
		invoke_tpl: "pip {task}",
	},
	DriverSpec {
		tool: "python",
		language: Some("python"),
		roles: &[Role::Runtime],
		fingerprint: &[".python-version"],
		version_cmd: "python --version",
		invoke_tpl: "python -m {task}",
	},
	DriverSpec {
		tool: "bundler",
		language: Some("ruby"),
		roles: &[Role::PackageManager],
		fingerprint: &["Gemfile.lock"],
		version_cmd: "bundle --version",
		invoke_tpl: "bundle exec {task}",
	},
	DriverSpec {
		tool: "rake",
		language: Some("ruby"),
		roles: &[Role::TaskRunner],
		fingerprint: &["Rakefile"],
		version_cmd: "rake --version",
		invoke_tpl: "rake {task}",
	},
	DriverSpec {
		tool: "ruby",
		language: Some("ruby"),
		roles: &[Role::Runtime],
		fingerprint: &[".ruby-version"],
		version_cmd: "ruby --version",
		invoke_tpl: "ruby {task}",
	},
	DriverSpec {
		tool: "gradle",
		language: Some("java"),
		roles: &[Role::BuildTool],
		fingerprint: &["gradlew"],
		version_cmd: "gradle --version",
		invoke_tpl: "./gradlew {task}",
	},
	DriverSpec {
		tool: "maven",
		language: Some("java"),
		roles: &[Role::BuildTool],
		fingerprint: &["mvnw"],
		version_cmd: "mvn --version",
		invoke_tpl: "./mvnw {task}",
	},
	DriverSpec {
		tool: "java",
		language: Some("java"),
		roles: &[Role::Runtime],
		fingerprint: &[".java-version"],
		version_cmd: "java -version",
		invoke_tpl: "java {task}",
	},
	DriverSpec {
		tool: "kotlin",
		language: Some("kotlin"),
		roles: &[Role::Runtime],
		// Kotlin projects are driven by gradle or maven; nothing on disk names
		// the Kotlin toolchain itself except a `.tool-versions` entry.
		fingerprint: &[],
		version_cmd: "kotlinc -version",
		invoke_tpl: "kotlin {task}",
	},
	DriverSpec {
		tool: "dotnet",
		language: Some("dotnet"),
		roles: &[Role::BuildTool],
		fingerprint: &["global.json"],
		version_cmd: "dotnet --version",
		invoke_tpl: "dotnet {task}",
	},
	DriverSpec {
		tool: "nuget",
		language: Some("dotnet"),
		roles: &[Role::PackageManager],
		// `packages.lock.json` is deliberately absent: an SDK-style project can
		// have one and still be driven by `dotnet`. `packages.config` is the
		// legacy layout nuget.exe alone restores.
		fingerprint: &["packages.config"],
		version_cmd: "nuget help",
		invoke_tpl: "nuget {task}",
	},
	DriverSpec {
		tool: "pod",
		language: Some("swift"),
		roles: &[Role::PackageManager],
		fingerprint: &["Podfile", "Podfile.lock"],
		version_cmd: "pod --version",
		invoke_tpl: "pod {task}",
	},
	DriverSpec {
		tool: "swift",
		language: Some("swift"),
		roles: &[Role::BuildTool],
		fingerprint: &["Package.resolved"],
		version_cmd: "swift --version",
		invoke_tpl: "swift {task}",
	},
	DriverSpec {
		tool: "composer",
		language: Some("php"),
		roles: &[Role::PackageManager],
		fingerprint: &["composer.lock"],
		version_cmd: "composer --version",
		invoke_tpl: "composer {task}",
	},
	DriverSpec {
		tool: "mix",
		language: Some("elixir"),
		roles: &[Role::PackageManager, Role::TaskRunner],
		fingerprint: &["mix.lock"],
		version_cmd: "mix --version",
		invoke_tpl: "mix {task}",
	},
	DriverSpec {
		tool: "dart",
		language: Some("dart"),
		roles: &[Role::PackageManager],
		fingerprint: &["pubspec.lock"],
		version_cmd: "dart --version",
		invoke_tpl: "dart pub {task}",
	},
	DriverSpec {
		tool: "stack",
		language: Some("haskell"),
		roles: &[Role::BuildTool],
		fingerprint: &["stack.yaml.lock"],
		version_cmd: "stack --version",
		invoke_tpl: "stack {task}",
	},
	DriverSpec {
		tool: "cabal",
		language: Some("haskell"),
		roles: &[Role::BuildTool],
		fingerprint: &["cabal.project.freeze"],
		version_cmd: "cabal --version",
		invoke_tpl: "cabal {task}",
	},
	DriverSpec {
		tool: "just",
		language: None,
		roles: &[Role::TaskRunner],
		fingerprint: &["justfile", ".justfile"],
		version_cmd: "just --version",
		invoke_tpl: "just {task}",
	},
	DriverSpec {
		tool: "task",
		language: None,
		roles: &[Role::TaskRunner],
		fingerprint: &["Taskfile.yml", "Taskfile.yaml"],
		version_cmd: "task --version",
		invoke_tpl: "task {task}",
	},
	DriverSpec {
		tool: "turbo",
		language: Some("node"),
		roles: &[Role::TaskRunner],
		fingerprint: &["turbo.json"],
		version_cmd: "turbo --version",
		invoke_tpl: "turbo run {task}",
	},
	DriverSpec {
		tool: "nx",
		language: Some("node"),
		roles: &[Role::TaskRunner],
		fingerprint: &["nx.json"],
		version_cmd: "nx --version",
		invoke_tpl: "nx run {task}",
	},
];

impl DriverRegistry {
	pub fn get(tool: &str) -> Option<&'static DriverSpec> {
		DRIVERS.iter().find(|d| d.tool == tool)
	}

	pub fn known() -> &'static [DriverSpec] {
		DRIVERS
	}
}

/// Why a driver was chosen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Evidence {
	/// Named by `driver` in an override.
	Declaration,
	/// A file a developer writes to say which tool they use: `packageManager`,
	/// `.tool-versions`, a wrapper script, a `[tool.poetry]` table.
	NativeFile(String),
	/// A lockfile or artifact only one tool produces.
	Lockfile(String),
	/// A manifest only one tool reads, such as `Cargo.toml` or `go.mod`.
	Manifest(String),
}

impl Evidence {
	fn rank(&self) -> u8 {
		match self {
			Evidence::Declaration => 3,
			Evidence::NativeFile(_) => 2,
			Evidence::Lockfile(_) => 1,
			Evidence::Manifest(_) => 0,
		}
	}
}

#[derive(Debug, Clone)]
pub struct DriverResolution {
	pub tool: String,
	pub role: Role,
	pub via: Evidence,
}

/// Where a workspace's task came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskOrigin {
	/// The workspace's own files or its tool's verbs; carries the file or tool.
	Native(String),
	/// A command written in `overrides`.
	Config,
}

/// One task as it runs in one workspace: the command, and every layer of
/// settings — Lattice's defaults, the root `tasks` map, the workspace's
/// override — already stacked.
#[derive(Debug, Clone)]
pub struct WorkspaceTask {
	pub command: String,
	pub config: PipelineTask,
	pub origin: TaskOrigin,
	args_after_separator: bool,
}

impl WorkspaceTask {
	/// A task written out in full, as an override would.
	pub fn new(command: impl Into<String>, config: PipelineTask) -> Self {
		WorkspaceTask {
			command: command.into(),
			config,
			origin: TaskOrigin::Config,
			args_after_separator: false,
		}
	}

	/// The command with `args` passed through to it.
	pub fn command_with_args(&self, args: &[String]) -> String {
		tasks::append_args(&self.command, args, self.args_after_separator)
	}
}

#[derive(Debug, Clone)]
pub struct Workspace {
	pub name: String,
	pub path: PathBuf,
	/// Repo-relative, forward-slashed, `"."` for the root.
	pub rel_path: String,
	/// Every workspace this one depends on, inferred and declared.
	pub depends_on: Vec<String>,
	/// The subset of [`Workspace::depends_on`] that came from manifests.
	pub inferred_deps: Vec<String>,
	/// Root engines with the workspace's own layered on top.
	pub engines: EngineMap,
	pub driver: Option<DriverResolution>,
	pub tasks: IndexMap<String, WorkspaceTask>,
	/// Something discovery could not read, said once per run.
	pub problems: Vec<String>,
}

impl Workspace {
	/// A workspace with no driver and no tasks, for a caller that builds one by
	/// hand rather than discovering it.
	pub fn new(name: impl Into<String>, path: impl Into<PathBuf>) -> Self {
		let name = name.into();
		Workspace {
			rel_path: name.clone(),
			name,
			path: path.into(),
			depends_on: Vec::new(),
			inferred_deps: Vec::new(),
			engines: EngineMap::new(),
			driver: None,
			tasks: IndexMap::new(),
			problems: Vec::new(),
		}
	}

	pub fn with_task(mut self, name: &str, command: &str, config: PipelineTask) -> Self {
		self.tasks
			.insert(name.to_string(), WorkspaceTask::new(command, config));
		self
	}

	pub fn with_deps(mut self, deps: &[&str]) -> Self {
		self.depends_on = deps.iter().map(|d| d.to_string()).collect();
		self
	}

	pub fn task(&self, name: &str) -> Option<&WorkspaceTask> {
		self.tasks.get(name)
	}

	pub fn command_for(&self, task: &str) -> Option<&str> {
		self.tasks.get(task).map(|t| t.command.as_str())
	}
}

/// Lattice could not tell which tool runs a workspace's tasks.
#[derive(Debug, Clone)]
pub struct AmbiguityError {
	pub workspace: String,
	pub candidates: Vec<String>,
	pub suggested_fix: String,
}

impl std::fmt::Display for AmbiguityError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		if self.candidates.is_empty() {
			writeln!(
				f,
				"Lattice cannot tell what runs the tasks in workspace '{}': it has no lockfile, \
				 wrapper or manifest Lattice recognizes.",
				self.workspace
			)?;
		} else {
			writeln!(
				f,
				"Lattice cannot tell which of {} runs the tasks in workspace '{}': there is no \
				 lockfile or packageManager field to go by.",
				self.candidates.join(", "),
				self.workspace
			)?;
		}
		write!(f, "Say which in lattice.json:\n  {}", self.suggested_fix)
	}
}

impl std::error::Error for AmbiguityError {}

/// Files that name a language but not always a tool, and the tools that read
/// each. One candidate means the file is enough to pick the driver on its own.
pub(crate) static ECOSYSTEM_MARKERS: &[(&str, &[&str])] = &[
	("package.json", &["npm", "pnpm", "yarn", "bun"]),
	("Cargo.toml", &["cargo"]),
	("go.mod", &["go"]),
	("pyproject.toml", &["uv", "poetry", "pdm", "pipenv"]),
	("Pipfile", &["pipenv"]),
	("requirements.txt", &["pip"]),
	("setup.py", &["pip"]),
	("Gemfile", &["bundler"]),
	("pom.xml", &["maven"]),
	("build.gradle", &["gradle"]),
	("build.gradle.kts", &["gradle"]),
	("settings.gradle", &["gradle"]),
	("settings.gradle.kts", &["gradle"]),
	("composer.json", &["composer"]),
	("mix.exs", &["mix"]),
	("pubspec.yaml", &["dart"]),
	("Package.swift", &["swift"]),
	("stack.yaml", &["stack"]),
	("cabal.project", &["cabal"]),
];

/// Extensions of project files that carry the project's name rather than a
/// fixed one, so they are matched by extension.
const NAMED_PROJECT_EXTS: &[(&str, &str)] = &[
	("sln", "dotnet"),
	("slnx", "dotnet"),
	("csproj", "dotnet"),
	("fsproj", "dotnet"),
	("vbproj", "dotnet"),
	("cabal", "cabal"),
];

fn ecosystem_candidates(path: &Path) -> Vec<&'static str> {
	for (marker, candidates) in ECOSYSTEM_MARKERS {
		if path.join(marker).exists() {
			return candidates.to_vec();
		}
	}
	named_project_tool(path).into_iter().collect()
}

fn named_project_tool(path: &Path) -> Option<&'static str> {
	let entries = std::fs::read_dir(path).ok()?;
	let mut found: Vec<&'static str> = entries
		.flatten()
		.filter_map(|e| {
			let ext = e.path().extension()?.to_str()?.to_string();
			NAMED_PROJECT_EXTS
				.iter()
				.find(|(x, _)| *x == ext)
				.map(|(_, tool)| *tool)
		})
		.collect();
	found.sort_unstable();
	found.into_iter().next()
}

fn override_fix(workspace: &str, tool: &str) -> String {
	format!("\"overrides\": {{ \"{workspace}\": {{ \"driver\": \"{tool}\" }} }}")
}

fn add_candidate(cands: &mut IndexMap<String, Evidence>, tool: &str, ev: Evidence) {
	match cands.get(tool) {
		Some(existing) if existing.rank() >= ev.rank() => {}
		_ => {
			cands.insert(tool.to_string(), ev);
		}
	}
}

/// The evidence one directory holds on its own.
fn direct_evidence(path: &Path) -> IndexMap<String, Evidence> {
	let mut cands: IndexMap<String, Evidence> = IndexMap::new();

	if path.join(".nvmrc").exists() {
		add_candidate(&mut cands, "node", Evidence::NativeFile(".nvmrc".into()));
	}
	if let Some(pm) = read_package_manager_field(path) {
		if DriverRegistry::get(&pm).is_some() {
			add_candidate(
				&mut cands,
				&pm,
				Evidence::NativeFile("package.json packageManager".into()),
			);
		}
	}
	for tool in read_tool_versions(path) {
		if DriverRegistry::get(&tool).is_some() {
			add_candidate(
				&mut cands,
				&tool,
				Evidence::NativeFile(".tool-versions".into()),
			);
		}
	}
	if path.join("rust-toolchain.toml").exists() || path.join("rust-toolchain").exists() {
		add_candidate(
			&mut cands,
			"cargo",
			Evidence::NativeFile("rust-toolchain".into()),
		);
	}
	if path.join(".python-version").exists() {
		add_candidate(
			&mut cands,
			"python",
			Evidence::NativeFile(".python-version".into()),
		);
	}
	if let Some(pyproject) = read::toml(&path.join("pyproject.toml")) {
		for tool in ["poetry", "pdm", "uv"] {
			if read::toml_path(&pyproject, &["tool", tool]).is_some() {
				add_candidate(
					&mut cands,
					tool,
					Evidence::NativeFile(format!("pyproject.toml [tool.{tool}]")),
				);
			}
		}
	}
	if path.join(".ruby-version").exists() || has_gemfile_ruby_directive(path) {
		add_candidate(
			&mut cands,
			"ruby",
			Evidence::NativeFile("ruby-version".into()),
		);
	}
	if path.join(".java-version").exists() {
		add_candidate(
			&mut cands,
			"java",
			Evidence::NativeFile(".java-version".into()),
		);
	}
	if go_mod_has_toolchain(path) {
		add_candidate(
			&mut cands,
			"go",
			Evidence::NativeFile("go.mod toolchain".into()),
		);
	}
	for (wrapper, tool) in [("gradlew", "gradle"), ("mvnw", "maven")] {
		if path.join(wrapper).exists() {
			add_candidate(&mut cands, tool, Evidence::NativeFile(wrapper.into()));
		}
	}
	if path.join("deno.json").exists() || path.join("deno.jsonc").exists() {
		add_candidate(&mut cands, "deno", Evidence::NativeFile("deno.json".into()));
	}

	for spec in DRIVERS {
		for fp in spec.fingerprint {
			if is_native_fingerprint(fp) {
				continue;
			}
			if path.join(fp).exists() {
				add_candidate(&mut cands, spec.tool, Evidence::Lockfile((*fp).to_string()));
			}
		}
	}
	cands
}

fn top_rank(cands: &IndexMap<String, Evidence>) -> u8 {
	cands
		.keys()
		.filter_map(|t| DriverRegistry::get(t))
		.map(|s| s.drive_role().drive_rank())
		.max()
		.unwrap_or(0)
}

/// The single tool that runs `path`'s tasks, or why there is not one.
///
/// Evidence in the directory decides first, by role and then by how deliberate
/// it is. A workspace inside a JS or Python monorepo has no lockfile of its own
/// — the package manager wrote one at the root — so failing that, the nearest
/// directory above it that has one decides, among the tools that read this
/// workspace's manifest. Failing that, a manifest only one tool reads
/// (`Cargo.toml`, `go.mod`, `pom.xml`, a `build.gradle`) is enough.
pub fn detect_driver(
	root: &Path,
	path: &Path,
	name: &str,
) -> Result<DriverResolution, AmbiguityError> {
	let cands = direct_evidence(path);
	let rank = top_rank(&cands);
	if rank > Role::Runtime.drive_rank() {
		return choose(name, &cands, rank);
	}

	let ecosystem = ecosystem_candidates(path);
	if ecosystem.len() > 1 {
		for ancestor in path.ancestors().skip(1) {
			if !ancestor.starts_with(root) {
				break;
			}
			let above: IndexMap<String, Evidence> = direct_evidence(ancestor)
				.into_iter()
				.filter(|(tool, _)| ecosystem.contains(&tool.as_str()))
				.map(|(tool, ev)| {
					let rel = read::rel_path(path, ancestor);
					let described = match ev {
						Evidence::Lockfile(f) => Evidence::Lockfile(format!("{rel}/{f}")),
						Evidence::NativeFile(f) => Evidence::NativeFile(format!("{rel}/{f}")),
						other => other,
					};
					(tool, described)
				})
				.collect();
			if !above.is_empty() {
				let rank = top_rank(&above);
				return choose(name, &above, rank);
			}
			if ancestor == root {
				break;
			}
		}
	}

	match ecosystem.as_slice() {
		[tool] => {
			let spec = DriverRegistry::get(tool).expect("marker tools are registered");
			let marker = ECOSYSTEM_MARKERS
				.iter()
				.find(|(m, c)| c == &[*tool] && path.join(m).exists())
				.map(|(m, _)| m.to_string())
				.unwrap_or_else(|| format!("{tool} project file"));
			Ok(DriverResolution {
				tool: tool.to_string(),
				role: spec.drive_role(),
				via: Evidence::Manifest(marker),
			})
		}
		[] => {
			// A runtime pin with nothing else says which language, not which tool.
			Err(AmbiguityError {
				workspace: name.to_string(),
				candidates: Vec::new(),
				suggested_fix: format!(
					"\"overrides\": {{ \"{name}\": {{ \"tasks\": {{ \"build\": \"<command>\" }} }} }}"
				),
			})
		}
		several => {
			let candidates: Vec<String> = several.iter().map(|t| t.to_string()).collect();
			Err(AmbiguityError {
				workspace: name.to_string(),
				suggested_fix: override_fix(name, &candidates[0]),
				candidates,
			})
		}
	}
}

fn choose(
	name: &str,
	cands: &IndexMap<String, Evidence>,
	rank: u8,
) -> Result<DriverResolution, AmbiguityError> {
	let in_top: Vec<(&String, &Evidence)> = cands
		.iter()
		.filter(|(t, _)| {
			DriverRegistry::get(t).is_some_and(|s| s.drive_role().drive_rank() == rank)
		})
		.collect();
	let best = in_top.iter().map(|(_, ev)| ev.rank()).max().unwrap_or(0);
	let strongest: Vec<&(&String, &Evidence)> =
		in_top.iter().filter(|(_, ev)| ev.rank() == best).collect();
	if strongest.len() != 1 {
		let mut candidates: Vec<String> = in_top.iter().map(|(t, _)| (*t).clone()).collect();
		candidates.sort();
		return Err(AmbiguityError {
			workspace: name.to_string(),
			suggested_fix: override_fix(name, &candidates[0]),
			candidates,
		});
	}
	let (tool, via) = strongest[0];
	let spec = DriverRegistry::get(tool).expect("candidate is a known tool");
	Ok(DriverResolution {
		tool: (*tool).clone(),
		role: spec.drive_role(),
		via: (*via).clone(),
	})
}

/// Fingerprints [`direct_evidence`] reads as native files rather than as
/// lockfiles, so the sweep does not count them twice.
fn is_native_fingerprint(fp: &str) -> bool {
	matches!(
		fp,
		".nvmrc"
			| "rust-toolchain.toml"
			| "rust-toolchain"
			| ".python-version"
			| ".ruby-version"
			| ".java-version"
			| "gradlew"
			| "mvnw" | "deno.json"
			| "deno.jsonc"
	)
}

fn read_package_manager_field(path: &Path) -> Option<String> {
	let json = read::json(&path.join("package.json"))?;
	let pm = json.get("packageManager")?.as_str()?;
	Some(pm.split('@').next().unwrap_or(pm).to_string())
}

fn read_tool_versions(path: &Path) -> Vec<String> {
	let mut names = Vec::new();
	if let Some(raw) = read::text(&path.join(".tool-versions")) {
		for line in raw.lines() {
			let line = line.trim();
			if line.is_empty() || line.starts_with('#') {
				continue;
			}
			if let Some(tool) = line.split_whitespace().next() {
				names.push(tool.to_string());
			}
		}
	}
	for f in ["mise.toml", ".mise.toml"] {
		if let Some(tools) = read::toml(&path.join(f))
			.and_then(|m| m.get("tools").and_then(|t| t.as_table()).cloned())
		{
			names.extend(tools.keys().cloned());
		}
	}
	names
}

fn has_gemfile_ruby_directive(path: &Path) -> bool {
	read::text(&path.join("Gemfile"))
		.is_some_and(|s| s.lines().any(|l| l.trim_start().starts_with("ruby ")))
}

fn go_mod_has_toolchain(path: &Path) -> bool {
	read::text(&path.join("go.mod"))
		.is_some_and(|s| s.lines().any(|l| l.trim_start().starts_with("toolchain ")))
}

/// Drop `//` line and `/* */` block comments from JSONC text, leaving string
/// literals — and anything comment-shaped inside one — exactly as they were.
pub(crate) fn strip_json_comments(raw: &str) -> String {
	let mut out = String::with_capacity(raw.len());
	let mut chars = raw.chars().peekable();
	let mut in_string = false;
	let mut escaped = false;
	while let Some(c) = chars.next() {
		if in_string {
			out.push(c);
			if escaped {
				escaped = false;
			} else if c == '\\' {
				escaped = true;
			} else if c == '"' {
				in_string = false;
			}
			continue;
		}
		match c {
			'"' => {
				in_string = true;
				out.push(c);
			}
			'/' if chars.peek() == Some(&'/') => {
				for c in chars.by_ref() {
					if c == '\n' {
						out.push(c);
						break;
					}
				}
			}
			'/' if chars.peek() == Some(&'*') => {
				chars.next();
				let mut prev = '\0';
				for c in chars.by_ref() {
					if prev == '*' && c == '/' {
						break;
					}
					if c == '\n' {
						out.push(c);
					}
					prev = c;
				}
			}
			_ => out.push(c),
		}
	}
	out
}

/// Where a package manager puts the executables it installed for a project.
/// Most ecosystems reach their local tools through the driver itself
/// (`bundle exec`, `./gradlew`); these are the ones where a task names a bare
/// executable in a directory nobody types.
const DEPENDENCY_BIN_DIRS: &[&str] = &[
	"node_modules/.bin",
	"vendor/bin",
	".venv/bin",
	".venv/Scripts",
	"venv/bin",
	"venv/Scripts",
];

/// The directories holding executables installed for the workspace, nearest
/// first, from `ws_path` up to `root` — the order a package manager resolves in
/// when a workspace and the repo root both install a tool.
pub fn dependency_bin_dirs(root: &Path, ws_path: &Path) -> Vec<PathBuf> {
	let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
	let start = ws_path
		.canonicalize()
		.unwrap_or_else(|_| ws_path.to_path_buf());

	let mut dirs = Vec::new();
	for dir in start.ancestors() {
		if !dir.starts_with(&root) {
			break;
		}
		for rel in DEPENDENCY_BIN_DIRS {
			let bin = rel
				.split('/')
				.fold(dir.to_path_buf(), |p, part| p.join(part));
			if bin.is_dir() {
				dirs.push(bin);
			}
		}
	}
	dirs
}

/// Whether an `overrides` key is matched against paths rather than names. A
/// scoped npm name like `@acme/ui` holds a slash and is still a name.
pub fn is_path_key(key: &str) -> bool {
	if members::has_glob(key) {
		return true;
	}
	key == "." || (key.contains('/') && !key.starts_with('@'))
}

/// The overrides that apply to one workspace, folded in declaration order.
#[derive(Default)]
struct Merged {
	name: Option<String>,
	driver: Option<String>,
	engines: EngineMap,
	depends_on: Vec<String>,
	tasks: IndexMap<String, PipelineTask>,
}

impl Merged {
	fn fold(&mut self, over: &WorkspaceOverride) {
		if over.name.is_some() {
			self.name = over.name.clone();
		}
		if over.driver.is_some() {
			self.driver = over.driver.clone();
		}
		for (k, v) in &over.engines {
			self.engines.insert(k.clone(), v.clone());
		}
		for dep in &over.depends_on {
			if !self.depends_on.contains(dep) {
				self.depends_on.push(dep.clone());
			}
		}
		for (task, value) in &over.tasks {
			let layer = value.clone().into_task();
			let merged = self
				.tasks
				.shift_remove(task)
				.unwrap_or_default()
				.layer(&layer);
			self.tasks.insert(task.clone(), merged);
		}
	}
}

struct Found {
	rel: String,
	dir: PathBuf,
	derived_name: String,
	identity: manifest::Identity,
	merged: Merged,
	origin: members::Origin,
}

/// Every workspace in the repo, with its driver, tasks and dependencies
/// resolved. See [`members`] for which directories count.
pub fn discover_workspaces(root: &Path, config: &LatticeConfig) -> Result<Vec<Workspace>> {
	let canonical_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());

	let mut path_matchers: Vec<(usize, globset::GlobMatcher)> = Vec::new();
	for (i, key) in config.overrides.keys().enumerate() {
		if is_path_key(key) {
			let glob = globset::GlobBuilder::new(&members::normalize(key))
				.literal_separator(true)
				.build()
				.with_context(|| format!("`overrides` key '{key}' is not a valid glob"))?;
			path_matchers.push((i, glob.compile_matcher()));
		}
	}
	let mut used = vec![false; config.overrides.len()];

	let mut found: Vec<Found> = Vec::new();
	for member in members::find_members(root, config)? {
		let dir = if member.rel == "." {
			root.to_path_buf()
		} else {
			root.join(&member.rel)
		};
		let canonical = manifest::canonical(&dir);
		if !canonical.starts_with(&canonical_root) {
			bail!(
				"workspace '{}' resolves to {}, which is outside the repo root. A workspace \
				 directory has to be inside the repo",
				member.rel,
				canonical.display()
			);
		}
		let identity = manifest::identity(root, &dir);
		let derived_name = identity.name.clone().unwrap_or_else(|| {
			dir.file_name()
				.or_else(|| canonical.file_name())
				.map(|n| n.to_string_lossy().into_owned())
				.unwrap_or_else(|| "root".to_string())
		});
		found.push(Found {
			rel: member.rel,
			dir,
			derived_name,
			identity,
			merged: Merged::default(),
			origin: member.origin,
		});
	}

	for f in &mut found {
		for (i, (key, over)) in config.overrides.iter().enumerate() {
			let hit = if is_path_key(key) {
				path_matchers
					.iter()
					.any(|(j, m)| *j == i && m.is_match(&f.rel))
			} else {
				*key == f.derived_name
			};
			if hit {
				used[i] = true;
				f.merged.fold(over);
			}
		}
	}
	// A name key may also name the workspace by the name a path override gave it.
	for f in &mut found {
		let Some(renamed) = f.merged.name.clone() else {
			continue;
		};
		for (i, (key, over)) in config.overrides.iter().enumerate() {
			if !is_path_key(key) && *key == renamed && renamed != f.derived_name {
				used[i] = true;
				f.merged.fold(over);
			}
		}
	}

	let known_names: Vec<String> = found
		.iter()
		.map(|f| {
			f.merged
				.name
				.clone()
				.unwrap_or_else(|| f.derived_name.clone())
		})
		.collect();
	for (i, key) in config.overrides.keys().enumerate() {
		if used[i] {
			continue;
		}
		let mut message = if is_path_key(key) {
			format!("`overrides` key '{key}' matches no workspace path")
		} else {
			format!("`overrides` key '{key}' names no workspace")
		};
		if let Some(near) = closest_field(key, &known_names) {
			message.push_str(&format!("\nDid you mean `{near}`?"));
		}
		if !known_names.is_empty() {
			message.push_str(&format!("\nWorkspaces: {}", known_names.join(", ")));
		}
		bail!(message);
	}

	let mut workspaces: Vec<Workspace> = Vec::new();
	let mut keys: Vec<(Vec<manifest::PackageKey>, PathBuf)> = Vec::new();
	let mut dep_refs: Vec<Vec<manifest::DepRef>> = Vec::new();
	for f in found {
		let name = f
			.merged
			.name
			.clone()
			.unwrap_or_else(|| f.derived_name.clone());
		let driver = match f.merged.driver.as_deref() {
			Some(NO_DRIVER) => None,
			Some(tool) => {
				let Some(spec) = DriverRegistry::get(tool) else {
					let known: Vec<String> = DRIVERS.iter().map(|d| d.tool.to_string()).collect();
					let mut message = format!(
						"workspace '{name}' names driver '{tool}', which Lattice does not know"
					);
					if let Some(near) = closest_field(tool, &known) {
						message.push_str(&format!("\nDid you mean `{near}`?"));
					}
					message.push_str(&format!(
						"\nUse one of {}, or \"none\" and write each task's command under \
						 `tasks`",
						known.join(", ")
					));
					bail!(message);
				};
				Some(DriverResolution {
					tool: tool.to_string(),
					role: spec.drive_role(),
					via: Evidence::Declaration,
				})
			}
			None => match detect_driver(root, &f.dir, &name) {
				Ok(d) => Some(d),
				Err(_) if f.merged.tasks.values().any(|t| t.run.is_some()) => None,
				// A glob in lattice.json can match a directory of notes or assets,
				// and a repo root can hold nothing but other projects; a directory
				// with nothing Lattice recognizes is simply not a workspace.
				Err(e)
					if e.candidates.is_empty()
						&& matches!(
							f.origin,
							members::Origin::Config | members::Origin::SingleProject
						) =>
				{
					continue
				}
				Err(e) => bail!("{e}"),
			},
		};

		let mut problems = Vec::new();
		let native = match &driver {
			Some(d) => {
				let n = tasks::native_tasks(&d.tool, &f.dir, root, f.identity.name.as_deref());
				if let Some(problem) = &n.problem {
					problems.push(format!("{name}: {problem}, so it runs none of its scripts"));
				}
				n.tasks
			}
			None => IndexMap::new(),
		};
		let tool = driver.as_ref().map(|d| d.tool.as_str()).unwrap_or("");
		let resolved = resolve_tasks(config, &f.merged, native, tool, &f.dir, root);

		keys.push((f.identity.keys.clone(), manifest::canonical(&f.dir)));
		dep_refs.push(f.identity.deps.clone());
		workspaces.push(Workspace {
			name,
			path: f.dir,
			rel_path: f.rel,
			depends_on: f.merged.depends_on.clone(),
			inferred_deps: Vec::new(),
			engines: resolve_engines(&config.engines, &f.merged.engines),
			driver,
			tasks: resolved,
			problems,
		});
	}

	disambiguate(&mut workspaces, config)?;
	infer_dependencies(&mut workspaces, &keys, &dep_refs);
	check_declared_dependencies(&workspaces)?;
	Ok(workspaces)
}

/// Stack every layer of settings for each task the workspace has a command for.
fn resolve_tasks(
	config: &LatticeConfig,
	merged: &Merged,
	native: IndexMap<String, tasks::NativeTask>,
	tool: &str,
	dir: &Path,
	root: &Path,
) -> IndexMap<String, WorkspaceTask> {
	let mut names: Vec<String> = native.keys().cloned().collect();
	for name in merged.tasks.keys() {
		if !names.contains(name) {
			names.push(name.clone());
		}
	}

	let mut out = IndexMap::new();
	for name in names {
		let over = merged.tasks.get(&name);
		let (command, origin, after_sep) = match over.and_then(|o| o.run.clone()) {
			Some(run) => (run, TaskOrigin::Config, false),
			None => match native.get(&name) {
				Some(n) => (
					n.command.clone(),
					TaskOrigin::Native(n.source.clone()),
					n.args_after_separator,
				),
				None => continue,
			},
		};
		let mut layered = builtin_task_defaults(&name);
		if name == "build" && origin != TaskOrigin::Config {
			layered.outputs = tasks::default_build_outputs(tool, dir, root);
		}
		if let Some(root_task) = config.tasks.get(&name) {
			layered = layered.layer(root_task);
		}
		if let Some(over) = over {
			layered = layered.layer(over);
		}
		layered.run = None;
		out.insert(
			name,
			WorkspaceTask {
				command,
				config: layered,
				origin,
				args_after_separator: after_sep,
			},
		);
	}
	out
}

/// Two workspaces can derive the same name — `apps/web` and `sites/web`, two
/// packages with one `name`. Those are named by their paths instead. A clash
/// between names someone chose on purpose is theirs to fix.
fn disambiguate(workspaces: &mut [Workspace], config: &LatticeConfig) -> Result<()> {
	let mut counts: HashMap<String, usize> = HashMap::new();
	for ws in workspaces.iter() {
		*counts.entry(ws.name.clone()).or_default() += 1;
	}
	let renamed: HashSet<String> = config
		.overrides
		.values()
		.filter_map(|o| o.name.clone())
		.collect();
	for ws in workspaces.iter_mut() {
		if counts[&ws.name] > 1 {
			if renamed.contains(&ws.name) {
				bail!(
					"two workspaces are named '{}'. Give each a different `name` in `overrides`",
					ws.name
				);
			}
			ws.name = ws.rel_path.clone();
		}
	}
	let mut seen = HashSet::new();
	for ws in workspaces.iter() {
		if !seen.insert(ws.name.clone()) {
			bail!(
				"two workspaces are named '{}'. Give one a different `name` in `overrides`",
				ws.name
			);
		}
	}
	Ok(())
}

fn infer_dependencies(
	workspaces: &mut [Workspace],
	keys: &[(Vec<manifest::PackageKey>, PathBuf)],
	dep_refs: &[Vec<manifest::DepRef>],
) {
	let names: Vec<String> = workspaces.iter().map(|w| w.name.clone()).collect();
	for (i, refs) in dep_refs.iter().enumerate() {
		let mut inferred: Vec<String> = Vec::new();
		for dep in refs {
			let target = keys.iter().position(|(ks, dir)| match dep {
				manifest::DepRef::Package(key) => ks.contains(key),
				manifest::DepRef::Dir(path) => dir == path,
			});
			if let Some(j) = target.filter(|j| *j != i) {
				if !inferred.contains(&names[j]) {
					inferred.push(names[j].clone());
				}
			}
		}
		inferred.sort();
		let ws = &mut workspaces[i];
		for dep in &inferred {
			if !ws.depends_on.contains(dep) {
				ws.depends_on.push(dep.clone());
			}
		}
		ws.inferred_deps = inferred;
	}
}

/// A `dependsOn` in an override that names no workspace would build no edge,
/// and the run would lose its ordering without a word.
fn check_declared_dependencies(workspaces: &[Workspace]) -> Result<()> {
	let names: Vec<String> = workspaces.iter().map(|w| w.name.clone()).collect();
	for ws in workspaces {
		for dep in &ws.depends_on {
			if dep == &ws.name {
				bail!("workspace '{}' lists itself in `dependsOn`", ws.name);
			}
			if names.contains(dep) {
				continue;
			}
			let mut message = format!(
				"workspace '{}' depends on '{dep}', which is not a workspace",
				ws.name
			);
			if let Some(near) = closest_field(dep, &names) {
				message.push_str(&format!("\nDid you mean `{near}`?"));
			}
			message.push_str(&format!("\nWorkspaces: {}", names.join(", ")));
			bail!(message);
		}
	}
	Ok(())
}

/// What a run should say about the tasks it asked for that some workspaces do
/// not have.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct SkippedTasks {
	/// Worth seeing on every run: a manifest discovery could not read, and a
	/// workspace that declares a name one typo away from the task.
	pub warnings: Vec<String>,
	/// Which workspaces sat a task out, for a run that asked to hear everything.
	pub notes: Vec<String>,
}

pub fn skipped_task_notes(workspaces: &[Workspace], tasks: &[String]) -> SkippedTasks {
	let mut out = SkippedTasks::default();
	for ws in workspaces {
		out.warnings.extend(ws.problems.iter().cloned());
	}
	for task in tasks {
		let mut without: Vec<&str> = Vec::new();
		for ws in workspaces {
			if ws.tasks.contains_key(task) {
				continue;
			}
			let declared: Vec<String> = ws.tasks.keys().cloned().collect();
			if let Some(near) = closest_field(task, &declared) {
				out.warnings.push(format!(
					"{} has no '{task}' task. Did you mean '{near}'?",
					ws.name
				));
			} else {
				without.push(&ws.name);
			}
		}
		if !without.is_empty() && without.len() < workspaces.len() {
			out.notes.push(format!(
				"no '{task}' task in {}, so they sit this one out",
				without.join(", ")
			));
		}
	}
	out
}

#[cfg(test)]
mod tests {
	use super::*;
	use tempfile::TempDir;

	fn write(root: &Path, rel: &str, contents: &str) {
		let path = root.join(rel);
		std::fs::create_dir_all(path.parent().unwrap()).unwrap();
		std::fs::write(path, contents).unwrap();
	}

	fn config(json: &str) -> LatticeConfig {
		lattice_config::parse_config(json).unwrap()
	}

	fn find<'a>(workspaces: &'a [Workspace], name: &str) -> &'a Workspace {
		workspaces
			.iter()
			.find(|w| w.name == name)
			.unwrap_or_else(|| panic!("no workspace '{name}'"))
	}

	fn pnpm_monorepo() -> TempDir {
		let dir = TempDir::new().unwrap();
		let root = dir.path();
		write(root, "package.json", r#"{"private": true}"#);
		write(root, "pnpm-workspace.yaml", "packages:\n  - packages/*\n");
		write(root, "pnpm-lock.yaml", "");
		write(
			root,
			"packages/ui/package.json",
			r#"{"name": "@acme/ui", "scripts": {"build": "tsc", "lint": "eslint ."}}"#,
		);
		write(
			root,
			"packages/web/package.json",
			r#"{"name": "web", "scripts": {"build": "vite build", "dev": "vite"},
			    "dependencies": {"@acme/ui": "workspace:*", "react": "19"}}"#,
		);
		dir
	}

	#[test]
	fn a_pnpm_monorepo_needs_no_config_at_all() {
		let dir = pnpm_monorepo();
		let ws = discover_workspaces(dir.path(), &config("{}")).unwrap();

		assert_eq!(ws.len(), 2, "the root declares members and is not one");
		let web = find(&ws, "web");
		assert_eq!(web.driver.as_ref().unwrap().tool, "pnpm");
		assert_eq!(web.depends_on, ["@acme/ui"]);
		assert_eq!(web.inferred_deps, ["@acme/ui"]);
		assert_eq!(web.command_for("build"), Some("pnpm run build"));

		let build = &web.task("build").unwrap().config;
		assert_eq!(
			build.depends_on.as_deref(),
			Some(&["^build".to_string()][..])
		);
		assert_eq!(build.outputs.as_deref(), Some(&["dist/**".to_string()][..]));
		assert_eq!(web.task("dev").unwrap().config.persistent, Some(true));
		assert!(find(&ws, "@acme/ui").task("dev").is_none());
	}

	#[test]
	fn root_tasks_and_overrides_layer_over_the_defaults() {
		let dir = pnpm_monorepo();
		let ws = discover_workspaces(
			dir.path(),
			&config(
				r#"{
				  "tasks": { "build": { "outputs": ["out/**"], "env": ["API_URL"] } },
				  "overrides": {
				    "web": { "tasks": { "build": { "outputs": ["dist/web/**"] }, "e2e": "playwright test" } },
				    "packages/*": { "tasks": { "lint": { "cache": false } } }
				  }
				}"#,
			),
		)
		.unwrap();

		let web = find(&ws, "web");
		let build = &web.task("build").unwrap().config;
		assert_eq!(
			build.outputs.as_deref(),
			Some(&["dist/web/**".to_string()][..])
		);
		assert_eq!(build.env.as_deref(), Some(&["API_URL".to_string()][..]));
		assert_eq!(
			build.depends_on.as_deref(),
			Some(&["^build".to_string()][..])
		);

		let e2e = web.task("e2e").unwrap();
		assert_eq!(e2e.command, "playwright test");
		assert_eq!(e2e.origin, TaskOrigin::Config);

		let ui = find(&ws, "@acme/ui");
		assert_eq!(ui.task("lint").unwrap().config.cache, Some(false));
		assert!(ui.task("e2e").is_none());
	}

	#[test]
	fn a_scoped_npm_name_is_an_override_key_like_any_other_name() {
		let dir = pnpm_monorepo();
		let ws = discover_workspaces(
			dir.path(),
			&config(r#"{"overrides": {"@acme/ui": {"tasks": {"lint": {"cache": false}}}}}"#),
		)
		.unwrap();
		let lint = &find(&ws, "@acme/ui").task("lint").unwrap().config;
		assert_eq!(lint.cache, Some(false));
	}

	#[test]
	fn an_override_key_that_matches_nothing_says_what_it_might_have_meant() {
		let dir = pnpm_monorepo();
		let err = discover_workspaces(dir.path(), &config(r#"{"overrides": {"weeb": {}}}"#))
			.unwrap_err()
			.to_string();
		assert!(err.contains("'weeb' names no workspace"), "{err}");
		assert!(err.contains("Did you mean `web`?"), "{err}");
	}

	#[test]
	fn a_cross_language_edge_is_declared_and_checked() {
		let dir = pnpm_monorepo();
		write(
			dir.path(),
			"services/api/go.mod",
			"module example.com/api\n",
		);
		let ws = discover_workspaces(
			dir.path(),
			&config(
				r#"{"workspaces": ["services/*"], "overrides": {"web": {"dependsOn": ["api"]}}}"#,
			),
		)
		.unwrap();
		let web = find(&ws, "web");
		assert!(web.depends_on.contains(&"api".to_string()));
		assert_eq!(find(&ws, "api").driver.as_ref().unwrap().tool, "go");

		let err = discover_workspaces(
			dir.path(),
			&config(r#"{"overrides": {"web": {"dependsOn": ["apii"]}}}"#),
		)
		.unwrap_err()
		.to_string();
		assert!(err.contains("'apii', which is not a workspace"), "{err}");
	}

	#[test]
	fn cargo_members_are_named_by_crate_and_ordered_by_path_deps() {
		let dir = TempDir::new().unwrap();
		let root = dir.path();
		write(
			root,
			"Cargo.toml",
			"[workspace]\nmembers = [\"crates/*\"]\n",
		);
		write(root, "Cargo.lock", "");
		write(
			root,
			"crates/core/Cargo.toml",
			"[package]\nname = \"acme-core\"\n",
		);
		write(
			root,
			"crates/cli/Cargo.toml",
			"[package]\nname = \"acme-cli\"\n[dependencies]\nacme-core = { path = \"../core\" }\n",
		);
		let ws = discover_workspaces(root, &config("{}")).unwrap();
		let cli = find(&ws, "acme-cli");
		assert_eq!(cli.driver.as_ref().unwrap().tool, "cargo");
		assert_eq!(cli.depends_on, ["acme-core"]);
		assert_eq!(cli.command_for("lint"), Some("cargo clippy"));
		assert_eq!(cli.command_for("clean"), Some("cargo clean -p acme-cli"));
	}

	#[test]
	fn a_gradle_build_is_one_workspace_however_many_subprojects_it_has() {
		let dir = TempDir::new().unwrap();
		let root = dir.path();
		write(
			root,
			"settings.gradle.kts",
			"include(\":core\", \":app\")\n",
		);
		write(root, "gradlew", "");
		write(root, "core/build.gradle.kts", "");
		write(root, "app/build.gradle.kts", "plugins { application }\n");
		let ws = discover_workspaces(root, &config("{}")).unwrap();
		assert_eq!(ws.len(), 1);
		assert_eq!(ws[0].rel_path, ".");
		assert_eq!(ws[0].driver.as_ref().unwrap().tool, "gradle");
		let build = ws[0].task("build").unwrap();
		assert!(
			build.command.ends_with("gradlew build"),
			"{}",
			build.command
		);
		assert_eq!(
			build.config.outputs.as_deref(),
			Some(
				&[
					"build/**".to_string(),
					"app/build/**".to_string(),
					"core/build/**".to_string()
				][..]
			)
		);
	}

	#[test]
	fn a_package_json_with_nothing_to_go_by_asks_for_a_driver() {
		let dir = TempDir::new().unwrap();
		write(dir.path(), "tools/gen/package.json", "{}");
		let err = discover_workspaces(dir.path(), &config(r#"{"workspaces": ["tools/*"]}"#))
			.unwrap_err()
			.to_string();
		assert!(err.contains("npm, pnpm, yarn, bun"), "{err}");
		assert!(
			err.contains(r#""overrides": { "gen": { "driver": "npm" } }"#),
			"{err}"
		);

		let ws = discover_workspaces(
			dir.path(),
			&config(r#"{"workspaces": ["tools/*"], "overrides": {"gen": {"driver": "yarn"}}}"#),
		)
		.unwrap();
		assert_eq!(ws[0].driver.as_ref().unwrap().via, Evidence::Declaration);
	}

	#[test]
	fn a_workspace_with_no_driver_runs_only_what_its_override_writes() {
		let dir = TempDir::new().unwrap();
		std::fs::create_dir_all(dir.path().join("scripts")).unwrap();
		let ws = discover_workspaces(
			dir.path(),
			&config(
				r#"{"workspaces": ["scripts"], "overrides": {"scripts": {"driver": "none", "tasks": {"build": "make"}}}}"#,
			),
		)
		.unwrap();
		assert!(ws[0].driver.is_none());
		assert_eq!(ws[0].command_for("build"), Some("make"));
		assert_eq!(
			ws[0].task("build").unwrap().config.outputs,
			None,
			"a written-out command gets no guessed outputs"
		);
	}

	#[test]
	fn two_packages_that_share_a_directory_name_are_named_by_path() {
		let dir = TempDir::new().unwrap();
		write(dir.path(), "apps/web/go.mod", "module a\n");
		write(dir.path(), "sites/web/go.mod", "module b\n");
		let ws = discover_workspaces(
			dir.path(),
			&config(r#"{"workspaces": ["apps/*", "sites/*"]}"#),
		)
		.unwrap();
		let names: Vec<&str> = ws.iter().map(|w| w.name.as_str()).collect();
		assert_eq!(names, ["apps/web", "sites/web"]);
	}

	#[test]
	fn skipped_tasks_name_who_sat_out_and_catch_a_typo() {
		let dir = pnpm_monorepo();
		let ws = discover_workspaces(dir.path(), &config("{}")).unwrap();
		let skipped = skipped_task_notes(&ws, &["lint".to_string()]);
		assert_eq!(
			skipped.notes,
			["no 'lint' task in web, so they sit this one out"]
		);
		assert!(skipped.warnings.is_empty());
		let skipped = skipped_task_notes(&ws, &["biuld".to_string()]);
		assert!(
			skipped
				.warnings
				.iter()
				.any(|n| n.contains("Did you mean 'build'?")),
			"{skipped:?}"
		);
	}
}
