//! Workspace discovery and task-driver resolution.
//!
//! Lattice resolves the task *driver* (the tool that runs tasks) for each
//! workspace by walking an evidence ladder and stopping at the first tier that
//! gives an unambiguous answer:
//!
//! * (a) a declaration in `lattice.json` `engines`, which always wins;
//! * (b) a dev-authored native declaration file (`packageManager`,
//!   `.tool-versions`, `.nvmrc`, `rust-toolchain.toml`, `./gradlew`, …);
//! * (c) a tool-unique lockfile or artifact (`bun.lockb`, `pnpm-lock.yaml`,
//!   `Cargo.lock`, `poetry.lock`, …);
//! * (d) otherwise, stop and ask ([`AmbiguityError`]). A bare generic ecosystem
//!   marker (a lone `package.json`, `pom.xml`, …) is not enough on its own.
//!
//! Tools carry a [`Role`]. Tools with *different* roles compose into a stack
//! (a node runtime plus a pnpm package-manager); only tools competing for the
//! *same* role are a conflict.

use std::path::{Path, PathBuf};

use anyhow::{bail, Result};
use indexmap::IndexMap;
use lattice_config::{EngineMap, LatticeConfig};

pub mod toolchain;

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
    pub role: Role,
    /// Files that identify this tool when present in a workspace dir.
    pub fingerprint: &'static [&'static str],
    /// Command that prints the tool's version.
    pub version_cmd: &'static str,
    /// Invoke template with a `{task}` placeholder (invoke form via [`DriverSpec::invoke`]).
    invoke_tpl: &'static str,
}

impl DriverSpec {
    pub fn invoke(&self, task: &str) -> String {
        self.invoke_tpl.replace("{task}", task)
    }
}

static DRIVERS: &[DriverSpec] = &[
    DriverSpec {
        tool: "node",
        role: Role::Runtime,
        fingerprint: &[".nvmrc"],
        version_cmd: "node --version",
        invoke_tpl: "node {task}",
    },
    DriverSpec {
        tool: "deno",
        role: Role::TaskRunner,
        fingerprint: &["deno.json", "deno.jsonc", "deno.lock"],
        version_cmd: "deno --version",
        invoke_tpl: "deno task {task}",
    },
    DriverSpec {
        tool: "bun",
        role: Role::PackageManager,
        fingerprint: &["bun.lockb", "bun.lock"],
        version_cmd: "bun --version",
        invoke_tpl: "bun run {task}",
    },
    DriverSpec {
        tool: "pnpm",
        role: Role::PackageManager,
        fingerprint: &["pnpm-lock.yaml"],
        version_cmd: "pnpm --version",
        invoke_tpl: "pnpm run {task}",
    },
    DriverSpec {
        tool: "yarn",
        role: Role::PackageManager,
        fingerprint: &["yarn.lock"],
        version_cmd: "yarn --version",
        invoke_tpl: "yarn {task}",
    },
    DriverSpec {
        tool: "npm",
        role: Role::PackageManager,
        fingerprint: &["package-lock.json"],
        version_cmd: "npm --version",
        invoke_tpl: "npm run {task}",
    },
    DriverSpec {
        tool: "cargo",
        role: Role::BuildTool,
        fingerprint: &["Cargo.lock", "rust-toolchain.toml", "rust-toolchain"],
        version_cmd: "cargo --version",
        invoke_tpl: "cargo {task}",
    },
    DriverSpec {
        tool: "go",
        role: Role::BuildTool,
        fingerprint: &["go.sum"],
        version_cmd: "go version",
        invoke_tpl: "go {task}",
    },
    DriverSpec {
        tool: "uv",
        role: Role::PackageManager,
        fingerprint: &["uv.lock"],
        version_cmd: "uv --version",
        invoke_tpl: "uv run {task}",
    },
    DriverSpec {
        tool: "poetry",
        role: Role::PackageManager,
        fingerprint: &["poetry.lock"],
        version_cmd: "poetry --version",
        invoke_tpl: "poetry run {task}",
    },
    DriverSpec {
        tool: "pdm",
        role: Role::PackageManager,
        fingerprint: &["pdm.lock"],
        version_cmd: "pdm --version",
        invoke_tpl: "pdm run {task}",
    },
    DriverSpec {
        tool: "pipenv",
        role: Role::PackageManager,
        fingerprint: &["Pipfile.lock"],
        version_cmd: "pipenv --version",
        invoke_tpl: "pipenv run {task}",
    },
    DriverSpec {
        tool: "python",
        role: Role::Runtime,
        fingerprint: &[".python-version"],
        version_cmd: "python --version",
        invoke_tpl: "python -m {task}",
    },
    DriverSpec {
        tool: "bundler",
        role: Role::PackageManager,
        fingerprint: &["Gemfile.lock"],
        version_cmd: "bundle --version",
        invoke_tpl: "bundle exec {task}",
    },
    DriverSpec {
        tool: "rake",
        role: Role::TaskRunner,
        fingerprint: &["Rakefile"],
        version_cmd: "rake --version",
        invoke_tpl: "rake {task}",
    },
    DriverSpec {
        tool: "ruby",
        role: Role::Runtime,
        fingerprint: &[".ruby-version"],
        version_cmd: "ruby --version",
        invoke_tpl: "ruby {task}",
    },
    DriverSpec {
        tool: "gradle",
        role: Role::BuildTool,
        fingerprint: &["gradlew"],
        version_cmd: "gradle --version",
        invoke_tpl: "./gradlew {task}",
    },
    DriverSpec {
        tool: "maven",
        role: Role::BuildTool,
        fingerprint: &["mvnw"],
        version_cmd: "mvn --version",
        invoke_tpl: "./mvnw {task}",
    },
    DriverSpec {
        tool: "java",
        role: Role::Runtime,
        fingerprint: &[".java-version"],
        version_cmd: "java -version",
        invoke_tpl: "java {task}",
    },
    DriverSpec {
        tool: "dotnet",
        role: Role::BuildTool,
        fingerprint: &["global.json"],
        version_cmd: "dotnet --version",
        invoke_tpl: "dotnet {task}",
    },
    DriverSpec {
        tool: "pod",
        role: Role::PackageManager,
        fingerprint: &["Podfile", "Podfile.lock"],
        version_cmd: "pod --version",
        invoke_tpl: "pod {task}",
    },
    DriverSpec {
        tool: "composer",
        role: Role::PackageManager,
        fingerprint: &["composer.lock"],
        version_cmd: "composer --version",
        invoke_tpl: "composer {task}",
    },
    DriverSpec {
        tool: "mix",
        role: Role::TaskRunner,
        fingerprint: &["mix.lock"],
        version_cmd: "mix --version",
        invoke_tpl: "mix {task}",
    },
    DriverSpec {
        tool: "dart",
        role: Role::PackageManager,
        fingerprint: &["pubspec.lock"],
        version_cmd: "dart --version",
        invoke_tpl: "dart pub {task}",
    },
    DriverSpec {
        tool: "swift",
        role: Role::BuildTool,
        fingerprint: &["Package.resolved"],
        version_cmd: "swift --version",
        invoke_tpl: "swift {task}",
    },
    DriverSpec {
        tool: "stack",
        role: Role::BuildTool,
        fingerprint: &["stack.yaml.lock"],
        version_cmd: "stack --version",
        invoke_tpl: "stack {task}",
    },
    DriverSpec {
        tool: "cabal",
        role: Role::BuildTool,
        fingerprint: &["cabal.project.freeze"],
        version_cmd: "cabal --version",
        invoke_tpl: "cabal {task}",
    },
    DriverSpec {
        tool: "just",
        role: Role::TaskRunner,
        fingerprint: &["justfile", ".justfile"],
        version_cmd: "just --version",
        invoke_tpl: "just {task}",
    },
    DriverSpec {
        tool: "task",
        role: Role::TaskRunner,
        fingerprint: &["Taskfile.yml", "Taskfile.yaml"],
        version_cmd: "task --version",
        invoke_tpl: "task {task}",
    },
    DriverSpec {
        tool: "turbo",
        role: Role::TaskRunner,
        fingerprint: &["turbo.json"],
        version_cmd: "turbo --version",
        invoke_tpl: "turbo run {task}",
    },
    DriverSpec {
        tool: "nx",
        role: Role::TaskRunner,
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

/// How a driver was selected (which rung of the evidence ladder).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Evidence {
    /// Declared in `lattice.json` `engines` (the escape hatch).
    Declaration,
    /// A dev-authored native declaration file (carries a description).
    NativeFile(String),
    /// A tool-unique lockfile/artifact (carries the file name).
    Lockfile(String),
}

impl Evidence {
    /// Precedence rank: declaration beats native file beats lockfile.
    fn rank(&self) -> u8 {
        match self {
            Evidence::Declaration => 2,
            Evidence::NativeFile(_) => 1,
            Evidence::Lockfile(_) => 0,
        }
    }
}

/// The detected/declared task driver for a workspace.
#[derive(Debug, Clone)]
pub struct DriverResolution {
    pub tool: String,
    pub role: Role,
    pub via: Evidence,
}

/// A discovered workspace with its engines, driver, and commands resolved.
#[derive(Debug, Clone)]
pub struct Workspace {
    pub name: String,
    pub path: PathBuf,
    pub auto: bool,
    pub depends_on: Vec<String>,
    /// Resolved (root-merged) engine constraints.
    pub engines: EngineMap,
    /// The detected/declared task driver (`None` for pure manual scripts).
    pub driver: Option<DriverResolution>,
    /// Resolved `task_name -> shell command`.
    pub commands: IndexMap<String, String>,
}

impl Workspace {
    pub fn command_for(&self, task: &str) -> Option<&str> {
        self.commands.get(task).map(|s| s.as_str())
    }
}

/// Raised when Lattice cannot unambiguously pick a task driver: either two
/// tools compete for the same role, or a workspace shows only a bare generic
/// marker with no tool-unique signal. Renders a copy-pasteable fix.
#[derive(Debug, Clone)]
pub struct AmbiguityError {
    pub workspace: String,
    pub candidates: Vec<String>,
    pub suggested_fix: String,
}

impl std::fmt::Display for AmbiguityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "workspace '{}' has an ambiguous or undeclared task driver.",
            self.workspace
        )?;
        if self.candidates.is_empty() {
            writeln!(
                f,
                "No task driver could be detected (no lockfile, wrapper, or \
                 native declaration)."
            )?;
        } else {
            writeln!(f, "Candidate tools seen: {}", self.candidates.join(", "))?;
        }
        write!(
            f,
            "Declare the task driver explicitly by adding to this workspace \
             in lattice.json:\n  {}",
            self.suggested_fix
        )
    }
}

impl std::error::Error for AmbiguityError {}

/// The plausible ecosystem package-managers/build-tools for the generic marker
/// present in `path`, if any. Used only to populate an [`AmbiguityError`].
fn ecosystem_candidates(path: &Path) -> Vec<&'static str> {
    if path.join("package.json").exists() {
        vec!["pnpm", "npm", "yarn", "bun"]
    } else if path.join("pom.xml").exists() {
        vec!["maven"]
    } else if path.join("build.gradle").exists() || path.join("build.gradle.kts").exists() {
        vec!["gradle"]
    } else if path.join("pyproject.toml").exists()
        || path.join("setup.py").exists()
        || path.join("requirements.txt").exists()
    {
        vec!["uv", "poetry"]
    } else if path.join("Gemfile").exists() {
        vec!["bundler", "rake"]
    } else {
        vec![]
    }
}

/// Build a copy-pasteable `"engines"` snippet naming `tool`.
fn suggested_fix_for(tool: &str) -> String {
    format!("\"engines\": {{ \"{tool}\": \">=0.0.0\" }}")
}

/// Read the `scripts` (JS) / `tasks` (deno) map keys from a manifest, if any.
fn manifest_script_names(path: &Path, tool: &str) -> Option<Vec<String>> {
    let (file, key) = match tool {
        "npm" | "pnpm" | "yarn" | "bun" => ("package.json", "scripts"),
        "deno" => {
            if path.join("deno.jsonc").exists() {
                ("deno.jsonc", "tasks")
            } else {
                ("deno.json", "tasks")
            }
        }
        _ => return None,
    };
    let raw = std::fs::read_to_string(path.join(file)).ok()?;
    let json: serde_json::Value = serde_json::from_str(&raw).ok()?;
    let map = json.get(key)?.as_object()?;
    Some(map.keys().cloned().collect())
}

/// Insert a candidate tool with its evidence, keeping the highest-ranked
/// evidence per tool (declaration > native file > lockfile).
fn add_candidate(cands: &mut IndexMap<String, Evidence>, tool: &str, ev: Evidence) {
    match cands.get(tool) {
        Some(existing) if existing.rank() >= ev.rank() => {}
        _ => {
            cands.insert(tool.to_string(), ev);
        }
    }
}

/// Walk the evidence ladder for one workspace directory and its declared
/// engines, returning the single task driver or an [`AmbiguityError`].
///
/// (The `workspace` field of any error is set to the directory's file name;
/// [`discover_workspaces`] overwrites it with the configured name.)
pub fn detect_drivers(
    path: &Path,
    declared: &EngineMap,
) -> Result<DriverResolution, AmbiguityError> {
    let ws_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("workspace")
        .to_string();

    let mut cands: IndexMap<String, Evidence> = IndexMap::new();

    // (a) Declarations in lattice.json.
    for name in declared.keys() {
        if DriverRegistry::get(name).is_some() {
            add_candidate(&mut cands, name, Evidence::Declaration);
        }
    }

    // (b) Dev-authored native declaration files.
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
    if path.join("gradlew").exists() {
        add_candidate(&mut cands, "gradle", Evidence::NativeFile("gradlew".into()));
    }
    if path.join("mvnw").exists() {
        add_candidate(&mut cands, "maven", Evidence::NativeFile("mvnw".into()));
    }
    if path.join("deno.json").exists() || path.join("deno.jsonc").exists() {
        add_candidate(&mut cands, "deno", Evidence::NativeFile("deno.json".into()));
    }

    // (c) Tool-unique lockfiles/artifacts.
    for spec in DRIVERS {
        for fp in spec.fingerprint {
            // Skip the fingerprints already handled above as native files.
            if is_native_fingerprint(fp) {
                continue;
            }
            if path.join(fp).exists() {
                add_candidate(&mut cands, spec.tool, Evidence::Lockfile((*fp).to_string()));
            }
        }
    }

    // (d) Resolve.
    if cands.is_empty() {
        return Err(bare_marker_error(&ws_name, path));
    }

    // Group by role; find the highest driving role present.
    let mut top_rank = 0u8;
    for ev_tool in cands.keys() {
        if let Some(spec) = DriverRegistry::get(ev_tool) {
            top_rank = top_rank.max(spec.role.drive_rank());
        }
    }

    // A pure runtime cannot drive named tasks, so this is still ambiguity.
    if top_rank == Role::Runtime.drive_rank() {
        return Err(bare_marker_error(&ws_name, path));
    }

    let in_top: Vec<(&String, &Evidence)> = cands
        .iter()
        .filter(|(t, _)| {
            DriverRegistry::get(t)
                .map(|s| s.role.drive_rank() == top_rank)
                .unwrap_or(false)
        })
        .collect();

    let chosen = if in_top.len() == 1 {
        in_top[0]
    } else {
        // Same-role conflict: a declaration disambiguates if exactly one exists.
        let declared: Vec<_> = in_top
            .iter()
            .filter(|(_, ev)| matches!(**ev, Evidence::Declaration))
            .copied()
            .collect();
        if declared.len() == 1 {
            declared[0]
        } else {
            let mut candidates: Vec<String> = in_top.iter().map(|(t, _)| (*t).clone()).collect();
            candidates.sort();
            let fix = suggested_fix_for(&candidates[0]);
            return Err(AmbiguityError {
                workspace: ws_name,
                candidates,
                suggested_fix: fix,
            });
        }
    };

    let spec = DriverRegistry::get(chosen.0).expect("candidate is a known tool");
    Ok(DriverResolution {
        tool: chosen.0.clone(),
        role: spec.role,
        via: chosen.1.clone(),
    })
}

fn bare_marker_error(ws_name: &str, path: &Path) -> AmbiguityError {
    let candidates: Vec<String> = ecosystem_candidates(path)
        .into_iter()
        .map(String::from)
        .collect();
    let fix = suggested_fix_for(candidates.first().map(|s| s.as_str()).unwrap_or("node"));
    AmbiguityError {
        workspace: ws_name.to_string(),
        candidates,
        suggested_fix: fix,
    }
}

/// Fingerprints that are handled explicitly as native-declaration evidence
/// (so the generic fingerprint sweep doesn't double-count them as lockfiles).
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
            | "mvnw"
            | "deno.json"
            | "deno.jsonc"
    )
}

fn read_package_manager_field(path: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(path.join("package.json")).ok()?;
    let json: serde_json::Value = serde_json::from_str(&raw).ok()?;
    let pm = json.get("packageManager")?.as_str()?;
    // Value looks like "pnpm@8.6.0" — take the name before '@'.
    Some(pm.split('@').next().unwrap_or(pm).to_string())
}

fn read_tool_versions(path: &Path) -> Vec<String> {
    let mut names = Vec::new();
    // .tool-versions (asdf / mise): "node 20.11.1" per line.
    if let Ok(raw) = std::fs::read_to_string(path.join(".tool-versions")) {
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
    // mise.toml / .mise.toml: names appear under a [tools] table.
    for f in ["mise.toml", ".mise.toml"] {
        if let Ok(raw) = std::fs::read_to_string(path.join(f)) {
            let mut in_tools = false;
            for line in raw.lines() {
                let t = line.trim();
                if t.starts_with('[') {
                    in_tools = t.starts_with("[tools]");
                    continue;
                }
                if in_tools {
                    if let Some(key) = t.split('=').next() {
                        let key = key.trim().trim_matches('"');
                        if !key.is_empty() {
                            names.push(key.to_string());
                        }
                    }
                }
            }
        }
    }
    names
}

fn has_gemfile_ruby_directive(path: &Path) -> bool {
    std::fs::read_to_string(path.join("Gemfile"))
        .map(|s| s.lines().any(|l| l.trim_start().starts_with("ruby ")))
        .unwrap_or(false)
}

fn go_mod_has_toolchain(path: &Path) -> bool {
    std::fs::read_to_string(path.join("go.mod"))
        .map(|s| s.lines().any(|l| l.trim_start().starts_with("toolchain ")))
        .unwrap_or(false)
}

/// Resolve one task's command for a workspace. For JS/deno drivers the task
/// must exist in the manifest's `scripts`/`tasks` map; other drivers use the
/// tool's invoke form directly.
///
/// A `persistent` task (dev server, watcher) is never fabricated for a
/// direct-invoke driver — there is no universal `cargo dev`/`go dev`, so it runs
/// only where the workspace explicitly declares it (an explicit `scripts` entry,
/// applied before this is called, or a manifest script for JS/deno drivers).
pub fn infer_task_command(
    task: &str,
    ws_driver: &DriverResolution,
    path: &Path,
    persistent: bool,
) -> Option<String> {
    let spec = DriverRegistry::get(&ws_driver.tool)?;
    if let Some(names) = manifest_script_names(path, &ws_driver.tool) {
        if names.iter().any(|n| n == task) {
            Some(spec.invoke(task))
        } else {
            None
        }
    } else if persistent {
        None
    } else {
        Some(spec.invoke(task))
    }
}

/// Discover + validate + resolve commands for every configured workspace.
///
/// Strict failures: a workspace path that is not an existing directory; a
/// duplicate name or path; an `auto` workspace that hits the ambiguity halt.
pub fn discover_workspaces(root: &Path, config: &LatticeConfig) -> Result<Vec<Workspace>> {
    use std::collections::HashSet;

    let mut workspaces = Vec::new();
    let mut seen_names: HashSet<String> = HashSet::new();
    let mut seen_paths: HashSet<PathBuf> = HashSet::new();

    for ws_cfg in &config.workspaces {
        let path = root.join(&ws_cfg.path);

        if !path.is_dir() {
            bail!(
                "workspace path '{}' does not point to a directory; workspace \
                 paths are literal directories, not globs",
                ws_cfg.path
            );
        }

        let name = ws_cfg.name.clone();
        if !seen_names.insert(name.clone()) {
            bail!("duplicate workspace name '{}' in lattice.json", name);
        }
        let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
        if !seen_paths.insert(canonical) {
            bail!("duplicate workspace path '{}' in lattice.json", ws_cfg.path);
        }

        let engines = lattice_config::resolve_engines(&config.engines, &ws_cfg.engines);

        // Detect the driver for auto workspaces; a manual workspace with only
        // its own scripts has no driver.
        let driver = if ws_cfg.auto {
            match detect_drivers(&path, &engines) {
                Ok(d) => Some(d),
                Err(mut e) => {
                    e.workspace = name.clone();
                    bail!("{e}");
                }
            }
        } else {
            // A manual workspace may still name a driver, but never halts on it.
            detect_drivers(&path, &engines).ok()
        };

        // Commands: explicit scripts overrides always win; auto workspaces then
        // infer commands for each root task via the resolved driver.
        let mut commands: IndexMap<String, String> = IndexMap::new();
        for (task, cmd) in &ws_cfg.scripts {
            commands.insert(task.clone(), cmd.clone());
        }
        if ws_cfg.auto {
            if let Some(drv) = &driver {
                for (task, task_cfg) in &config.tasks {
                    if commands.contains_key(task) {
                        continue;
                    }
                    if let Some(cmd) =
                        infer_task_command(task, drv, &path, task_cfg.is_persistent())
                    {
                        commands.insert(task.clone(), cmd);
                    }
                }
            }
        }

        let depends_on = ws_cfg.depends_on.clone().unwrap_or_default();

        workspaces.push(Workspace {
            name,
            path,
            auto: ws_cfg.auto,
            depends_on,
            engines,
            driver,
            commands,
        });
    }

    Ok(workspaces)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;
    use tempfile::TempDir;

    fn engines(v: serde_json::Value) -> EngineMap {
        serde_json::from_value(v).unwrap()
    }

    fn write(dir: &Path, name: &str, contents: &str) {
        fs::write(dir.join(name), contents).unwrap();
    }

    #[test]
    fn tier_a_declaration_overrides_lockfile() {
        // A pnpm lockfile is present, but engines declares bun → bun wins, and
        // since they share a role the declaration is the tiebreaker.
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "pnpm-lock.yaml", "");
        write(tmp.path(), "package.json", "{}");
        let d = detect_drivers(tmp.path(), &engines(json!({ "bun": ">=1.1" }))).unwrap();
        assert_eq!(d.tool, "bun");
        assert_eq!(d.via, Evidence::Declaration);
    }

    #[test]
    fn tier_b_tool_versions_selects_named_tool() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), ".tool-versions", "pnpm 8.6.0\n");
        write(tmp.path(), "package.json", "{}");
        let d = detect_drivers(tmp.path(), &EngineMap::new()).unwrap();
        assert_eq!(d.tool, "pnpm");
        assert!(matches!(d.via, Evidence::NativeFile(_)));
    }

    #[test]
    fn tier_b_package_manager_field_selects() {
        let tmp = TempDir::new().unwrap();
        write(
            tmp.path(),
            "package.json",
            r#"{ "packageManager": "yarn@4.0.0" }"#,
        );
        let d = detect_drivers(tmp.path(), &EngineMap::new()).unwrap();
        assert_eq!(d.tool, "yarn");
    }

    #[test]
    fn tier_b_gradlew_selects_gradle() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "gradlew", "#!/bin/sh\n");
        write(tmp.path(), "build.gradle", "");
        let d = detect_drivers(tmp.path(), &EngineMap::new()).unwrap();
        assert_eq!(d.tool, "gradle");
    }

    #[test]
    fn tier_b_gemfile_ruby_directive_selects_ruby_but_rake_drives() {
        // Gemfile ruby directive (ruby runtime) + Rakefile (task runner) →
        // rake drives (higher role); no conflict.
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "Gemfile", "ruby \"3.2.0\"\n");
        write(tmp.path(), "Rakefile", "");
        let d = detect_drivers(tmp.path(), &EngineMap::new()).unwrap();
        assert_eq!(d.tool, "rake");
    }

    #[test]
    fn tier_c_bun_lockfile_selects() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "bun.lockb", "");
        write(tmp.path(), "package.json", "{}");
        let d = detect_drivers(tmp.path(), &EngineMap::new()).unwrap();
        assert_eq!(d.tool, "bun");
        assert_eq!(d.via, Evidence::Lockfile("bun.lockb".into()));
    }

    #[test]
    fn tier_c_cargo_lock_selects() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "Cargo.lock", "");
        write(tmp.path(), "Cargo.toml", "");
        let d = detect_drivers(tmp.path(), &EngineMap::new()).unwrap();
        assert_eq!(d.tool, "cargo");
    }

    #[test]
    fn tier_c_poetry_lock_selects() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "poetry.lock", "");
        write(tmp.path(), "pyproject.toml", "");
        let d = detect_drivers(tmp.path(), &EngineMap::new()).unwrap();
        assert_eq!(d.tool, "poetry");
    }

    #[test]
    fn tier_d_bare_package_json_is_ambiguous() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "package.json", r#"{ "name": "x" }"#);
        let err = detect_drivers(tmp.path(), &EngineMap::new()).unwrap_err();
        assert!(err.candidates.contains(&"pnpm".to_string()));
        let msg = format!("{err}");
        assert!(msg.contains("engines"), "fix must be copy-pasteable: {msg}");
    }

    #[test]
    fn same_role_conflict_is_ambiguous() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "bun.lockb", "");
        write(tmp.path(), "pnpm-lock.yaml", "");
        write(tmp.path(), "package.json", "{}");
        let err = detect_drivers(tmp.path(), &EngineMap::new()).unwrap_err();
        assert!(err.candidates.contains(&"bun".to_string()));
        assert!(err.candidates.contains(&"pnpm".to_string()));
        assert!(format!("{err}").contains("engines"));
    }

    #[test]
    fn role_composition_node_plus_pnpm_resolves_to_pnpm() {
        // node runtime (.nvmrc) + pnpm package-manager (lockfile) → pnpm drives,
        // no conflict. node is still in the engine map for provisioning.
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), ".nvmrc", "20.11.1\n");
        write(tmp.path(), "pnpm-lock.yaml", "");
        write(tmp.path(), "package.json", "{}");
        let d = detect_drivers(tmp.path(), &EngineMap::new()).unwrap();
        assert_eq!(d.tool, "pnpm");
        assert_eq!(d.role, Role::PackageManager);
    }

    #[test]
    fn js_driver_reads_real_package_json_scripts() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "pnpm-lock.yaml", "");
        write(
            tmp.path(),
            "package.json",
            r#"{ "scripts": { "build": "tsc", "start": "node ." } }"#,
        );
        let d = detect_drivers(tmp.path(), &EngineMap::new()).unwrap();
        assert_eq!(
            infer_task_command("build", &d, tmp.path(), false).as_deref(),
            Some("pnpm run build")
        );
        // A task not in package.json scripts is not invented.
        assert_eq!(infer_task_command("test", &d, tmp.path(), false), None);
    }

    #[test]
    fn direct_invoke_driver_never_fabricates_persistent_task() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "Cargo.lock", "");
        let d = detect_drivers(tmp.path(), &EngineMap::new()).unwrap();
        // A normal task uses the tool's invoke form directly...
        assert_eq!(
            infer_task_command("build", &d, tmp.path(), false).as_deref(),
            Some("cargo build")
        );
        // ...but a persistent task is never fabricated (there is no `cargo dev`).
        assert_eq!(infer_task_command("dev", &d, tmp.path(), true), None);
    }

    #[test]
    fn discover_builds_commands_and_overrides_win() {
        use lattice_config::WorkspaceConfig;
        let tmp = TempDir::new().unwrap();
        let ws_dir = tmp.path().join("app");
        fs::create_dir_all(&ws_dir).unwrap();
        write(&ws_dir, "pnpm-lock.yaml", "");
        write(
            &ws_dir,
            "package.json",
            r#"{ "scripts": { "build": "tsc" } }"#,
        );

        let mut config = LatticeConfig::default();
        config.tasks.insert("build".into(), Default::default());
        config.tasks.insert("deploy".into(), Default::default());
        let mut scripts = indexmap::IndexMap::new();
        scripts.insert("deploy".to_string(), "./deploy.sh".to_string());
        config.workspaces.push(WorkspaceConfig {
            name: "app".into(),
            path: "app".into(),
            auto: true,
            engines: EngineMap::new(),
            depends_on: None,
            scripts,
        });

        let ws = discover_workspaces(tmp.path(), &config).unwrap();
        assert_eq!(ws.len(), 1);
        assert_eq!(ws[0].command_for("build"), Some("pnpm run build"));
        assert_eq!(ws[0].command_for("deploy"), Some("./deploy.sh"));
        assert_eq!(ws[0].driver.as_ref().unwrap().tool, "pnpm");
    }

    #[test]
    fn auto_workspace_never_infers_persistent_task_command() {
        use lattice_config::{PipelineTask, WorkspaceConfig};
        let tmp = TempDir::new().unwrap();
        let ws_dir = tmp.path().join("core");
        fs::create_dir_all(&ws_dir).unwrap();
        // A cargo (non-manifest) driver: normally infers a command for any task.
        write(&ws_dir, "Cargo.lock", "");

        let mut config = LatticeConfig::default();
        config.tasks.insert("build".into(), Default::default());
        let dev = PipelineTask {
            persistent: Some(true),
            ..Default::default()
        };
        config.tasks.insert("dev".into(), dev);
        config.workspaces.push(WorkspaceConfig {
            name: "core".into(),
            path: "core".into(),
            auto: true,
            engines: EngineMap::new(),
            depends_on: None,
            scripts: Default::default(),
        });

        let ws = discover_workspaces(tmp.path(), &config).unwrap();
        // A normal task is still inferred; the persistent `dev` is not fabricated.
        assert_eq!(ws[0].command_for("build"), Some("cargo build"));
        assert_eq!(ws[0].command_for("dev"), None);
    }

    #[test]
    fn auto_workspace_runs_persistent_task_only_where_declared() {
        use lattice_config::{PipelineTask, WorkspaceConfig};
        let tmp = TempDir::new().unwrap();
        let ws_dir = tmp.path().join("web");
        fs::create_dir_all(&ws_dir).unwrap();
        write(&ws_dir, "Cargo.lock", "");

        let mut config = LatticeConfig::default();
        let dev = PipelineTask {
            persistent: Some(true),
            ..Default::default()
        };
        config.tasks.insert("dev".into(), dev);
        let mut scripts = indexmap::IndexMap::new();
        scripts.insert("dev".to_string(), "trunk serve".to_string());
        config.workspaces.push(WorkspaceConfig {
            name: "web".into(),
            path: "web".into(),
            auto: true,
            engines: EngineMap::new(),
            depends_on: None,
            scripts,
        });

        let ws = discover_workspaces(tmp.path(), &config).unwrap();
        // An explicitly declared persistent script is honored.
        assert_eq!(ws[0].command_for("dev"), Some("trunk serve"));
    }

    #[test]
    fn discover_halts_on_auto_ambiguity() {
        use lattice_config::WorkspaceConfig;
        let tmp = TempDir::new().unwrap();
        let ws_dir = tmp.path().join("app");
        fs::create_dir_all(&ws_dir).unwrap();
        write(&ws_dir, "package.json", "{}");

        let mut config = LatticeConfig::default();
        config.workspaces.push(WorkspaceConfig {
            name: "app".into(),
            path: "app".into(),
            auto: true,
            engines: EngineMap::new(),
            depends_on: None,
            scripts: Default::default(),
        });
        let err = discover_workspaces(tmp.path(), &config).unwrap_err();
        assert!(format!("{err:#}").contains("app"));
    }

    #[test]
    fn role_composition_python_runtime_plus_uv_resolves_to_uv() {
        // python runtime (.python-version) + uv package-manager (lockfile) → uv
        // drives; the runtime is not a conflict, it composes into the stack.
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), ".python-version", "3.12.1\n");
        write(tmp.path(), "uv.lock", "");
        write(tmp.path(), "pyproject.toml", "");
        let d = detect_drivers(tmp.path(), &EngineMap::new()).unwrap();
        assert_eq!(d.tool, "uv");
        assert_eq!(d.role, Role::PackageManager);
    }

    #[test]
    fn role_composition_turbo_over_pnpm_resolves_to_turbo() {
        // A meta task runner (turbo.json) composes above a package-manager
        // (pnpm-lock.yaml): turbo has the higher driving role, so it drives.
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "turbo.json", "{}");
        write(tmp.path(), "pnpm-lock.yaml", "");
        write(tmp.path(), "package.json", "{}");
        let d = detect_drivers(tmp.path(), &EngineMap::new()).unwrap();
        assert_eq!(d.tool, "turbo");
        assert_eq!(d.role, Role::TaskRunner);
        assert_eq!(d.via, Evidence::Lockfile("turbo.json".into()));
    }

    #[test]
    fn same_role_conflict_two_python_package_managers() {
        // pdm and pipenv are both package managers → no unique driver.
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "pdm.lock", "");
        write(tmp.path(), "Pipfile.lock", "");
        write(tmp.path(), "pyproject.toml", "");
        let err = detect_drivers(tmp.path(), &EngineMap::new()).unwrap_err();
        assert!(err.candidates.contains(&"pdm".to_string()));
        assert!(err.candidates.contains(&"pipenv".to_string()));
        assert!(format!("{err}").contains("engines"));
    }

    #[test]
    fn same_role_conflict_two_build_tools() {
        // stack and cabal are both Haskell build tools → conflict.
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "stack.yaml.lock", "");
        write(tmp.path(), "cabal.project.freeze", "");
        let err = detect_drivers(tmp.path(), &EngineMap::new()).unwrap_err();
        assert!(err.candidates.contains(&"stack".to_string()));
        assert!(err.candidates.contains(&"cabal".to_string()));
        assert!(format!("{err}").contains("engines"));
    }

    #[test]
    fn same_role_conflict_declaration_disambiguates() {
        // Two package-manager lockfiles, but a declaration names one → resolved.
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "pdm.lock", "");
        write(tmp.path(), "Pipfile.lock", "");
        write(tmp.path(), "pyproject.toml", "");
        let d = detect_drivers(tmp.path(), &engines(json!({ "pdm": ">=2.0" }))).unwrap();
        assert_eq!(d.tool, "pdm");
        assert_eq!(d.via, Evidence::Declaration);
    }

    #[test]
    fn tier_c_new_tool_lockfiles_select_their_driver() {
        for (file, tool) in [
            ("composer.lock", "composer"),
            ("mix.lock", "mix"),
            ("pubspec.lock", "dart"),
            ("Package.resolved", "swift"),
            ("stack.yaml.lock", "stack"),
            ("cabal.project.freeze", "cabal"),
            ("pdm.lock", "pdm"),
            ("Pipfile.lock", "pipenv"),
            ("justfile", "just"),
            ("Taskfile.yml", "task"),
            ("turbo.json", "turbo"),
            ("nx.json", "nx"),
        ] {
            let tmp = TempDir::new().unwrap();
            write(tmp.path(), file, "");
            let d = detect_drivers(tmp.path(), &EngineMap::new())
                .unwrap_or_else(|e| panic!("{file} should select {tool}: {e}"));
            assert_eq!(d.tool, tool, "{file} → {tool}");
        }
    }

    #[test]
    fn new_tool_invoke_forms() {
        assert_eq!(
            DriverRegistry::get("mix").unwrap().invoke("test"),
            "mix test"
        );
        assert_eq!(
            DriverRegistry::get("dart").unwrap().invoke("get"),
            "dart pub get"
        );
        assert_eq!(
            DriverRegistry::get("turbo").unwrap().invoke("build"),
            "turbo run build"
        );
        assert_eq!(
            DriverRegistry::get("nx").unwrap().invoke("build"),
            "nx run build"
        );
    }
}
