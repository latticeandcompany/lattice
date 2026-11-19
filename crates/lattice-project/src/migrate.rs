//! Rewrite a 1.x `lattice.json` for this version.
//!
//! 1.x listed every workspace by name and path and every dependency edge by
//! hand. This version reads both from the repo's own manifests, so a faithful
//! migration is mostly deletion: it runs discovery against the repo and writes
//! down only where discovery would come out differently from what 1.x said —
//! a workspace no manifest names, a name the manifest spells otherwise, an edge
//! between languages, a command written out by hand.

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{Context, Result};
use indexmap::IndexMap;
use serde::Deserialize;

use lattice_config::{
	EngineMap, EngineSpec, LatticeConfig, PipelineTask, Settings, TaskOverride, WorkspaceOverride,
	NO_DRIVER,
};
use lattice_workspace::{detect_driver, discover_workspaces, members, DriverRegistry, Role};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct V1Config {
	#[serde(rename = "$schema")]
	schema: Option<String>,
	lattice_version: Option<String>,
	#[serde(default)]
	workspaces: Vec<V1Workspace>,
	#[serde(default)]
	engines: EngineMap,
	#[serde(default)]
	global_dependencies: Vec<String>,
	#[serde(default)]
	global_env: Vec<String>,
	#[serde(default)]
	tasks: IndexMap<String, PipelineTask>,
	#[serde(default)]
	settings: Settings,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct V1Workspace {
	name: String,
	path: String,
	#[serde(default = "yes")]
	auto: bool,
	#[serde(default)]
	engines: EngineMap,
	#[serde(default)]
	depends_on: Vec<String>,
	#[serde(default)]
	scripts: IndexMap<String, String>,
}

fn yes() -> bool {
	true
}

/// An `overrides` key that matches `rel` as a path. A bare `web` would be read
/// as a workspace name.
fn path_key(rel: &str) -> String {
	if lattice_workspace::is_path_key(rel) {
		rel.to_string()
	} else {
		format!("./{rel}")
	}
}

pub struct Migration {
	pub config: LatticeConfig,
	/// The new file, formatted.
	pub text: String,
	/// What changed that a reader of the diff would not guess, one line each.
	pub notes: Vec<String>,
}

/// 1.x's way of choosing a driver: name it in `engines` with a constraint that
/// matches any version. It pinned nothing.
fn is_placeholder(spec: &EngineSpec) -> bool {
	matches!(spec, EngineSpec::Version(v) if v.trim() == ">=0.0.0" || v.trim() == "*")
}

fn real_engines(engines: &EngineMap) -> EngineMap {
	engines
		.iter()
		.filter(|(_, spec)| !is_placeholder(spec))
		.map(|(k, v)| (k.clone(), v.clone()))
		.collect()
}

/// The driver 1.x picked for a workspace by declaration, where this version
/// would pick another.
///
/// Naming a tool in 1.x `engines` only ever broke a tie: role decided first, so
/// a declared build tool never beat a package manager the workspace's own
/// lockfile named, and a root `cargo` pin did not make a Node app a Cargo one.
/// So a declaration carries over only when detection now lands on a different
/// tool of the same role, or cannot decide between tools the declaration
/// names one of.
fn declared_driver(
	root: &Path,
	dir: &Path,
	ws: &V1Workspace,
	root_engines: &EngineMap,
) -> Option<String> {
	let declared: Vec<&String> = ws
		.engines
		.keys()
		.chain(root_engines.keys())
		.filter(|name| {
			DriverRegistry::get(name).is_some_and(|spec| spec.drive_role() != Role::Runtime)
		})
		.collect();
	match detect_driver(root, dir, &ws.name) {
		Ok(detected) => declared
			.into_iter()
			.find(|name| {
				**name != detected.tool
					&& DriverRegistry::get(name).is_some_and(|s| s.drive_role() == detected.role)
			})
			.cloned(),
		Err(ambiguous) => declared
			.into_iter()
			.find(|name| ambiguous.candidates.contains(name))
			.cloned(),
	}
}

/// Where this version's defaults for a task name change what a 1.x task did,
/// because the 1.x config left that field unset.
fn new_task_defaults(v1: &V1Config) -> Vec<String> {
	let mut names: Vec<&String> = v1.tasks.keys().collect();
	for ws in &v1.workspaces {
		names.extend(ws.scripts.keys());
	}
	let mut seen = BTreeSet::new();
	names.retain(|name| seen.insert(name.as_str()));

	let mut notes = Vec::new();
	for name in names {
		let default = lattice_config::builtin_task_defaults(name);
		let written = v1.tasks.get(name.as_str()).cloned().unwrap_or_default();
		if default.persistent == Some(true) && written.persistent.is_none() {
			notes.push(format!(
				"`{name}` is now a persistent task, as every dev, start, serve and watch is: \
				 never cached, and nothing can wait on it. Set \"persistent\": false on it \
				 to keep the 1.x behavior"
			));
		}
		if default.depends_on.is_some() && written.depends_on.is_none() {
			notes.push(format!(
				"`{name}` now waits on the {name} of every workspace it depends on \
				 (\"^{name}\"). Set \"dependsOn\": [] on it to keep the 1.x behavior"
			));
		}
		if default.cache == Some(false) && written.cache.is_none() {
			notes.push(format!(
				"`{name}` is no longer cached, as every clean, deploy, publish and release \
				 is. Set \"cache\": true on it to keep the 1.x behavior"
			));
		}
	}
	notes
}

pub fn migrate(root: &Path, v1_text: &str, version: &str) -> Result<Migration> {
	let v1: V1Config =
		serde_json::from_str(v1_text).context("lattice.json could not be read as a 1.x config")?;
	let mut notes = Vec::new();

	let mut config = LatticeConfig {
		schema: v1.schema.clone(),
		lattice_version: v1.lattice_version.as_ref().map(|_| version.to_string()),
		engines: real_engines(&v1.engines),
		global_dependencies: v1.global_dependencies.clone(),
		global_env: v1.global_env.clone(),
		tasks: v1.tasks.clone(),
		settings: v1.settings.clone(),
		..LatticeConfig::default()
	};
	for (name, spec) in &v1.engines {
		if is_placeholder(spec) {
			notes.push(format!(
				"dropped the `{name}` engine: its constraint matched any version, so it only \
				 chose a driver, and drivers are chosen with `driver` now"
			));
		}
	}

	notes.extend(new_task_defaults(&v1));

	let v1_paths: Vec<String> = v1
		.workspaces
		.iter()
		.map(|ws| members::normalize(&ws.path))
		.collect();
	let native: BTreeSet<String> = members::find_members(root, &config)?
		.into_iter()
		.filter(|m| m.origin != members::Origin::SingleProject)
		.map(|m| m.rel)
		.collect();
	for rel in &v1_paths {
		if !native.contains(rel) {
			config.workspaces.push(rel.clone());
		}
	}
	for rel in &native {
		if !v1_paths.contains(rel) {
			config.exclude.push(rel.clone());
			notes.push(format!(
				"excluded {rel}: the repo's workspace files name it, and the 1.x config did not"
			));
		}
	}

	let mut by_path: IndexMap<String, WorkspaceOverride> = IndexMap::new();
	for (ws, rel) in v1.workspaces.iter().zip(&v1_paths) {
		let mut over = WorkspaceOverride {
			engines: real_engines(&ws.engines),
			tasks: ws
				.scripts
				.iter()
				.map(|(k, v)| (k.clone(), TaskOverride::Command(v.clone())))
				.collect(),
			..WorkspaceOverride::default()
		};
		if !ws.auto {
			over.driver = Some(NO_DRIVER.to_string());
		} else {
			over.driver = declared_driver(root, &root.join(rel), ws, &v1.engines);
		}
		by_path.insert(path_key(rel), over);
	}
	config.overrides = by_path.clone();

	let discovered = discover_workspaces(root, &config)
		.context("the migrated config does not resolve against this repo")?;
	for (ws, rel) in v1.workspaces.iter().zip(&v1_paths) {
		let Some(found) = discovered.iter().find(|d| &d.rel_path == rel) else {
			continue;
		};
		if found.name != ws.name {
			by_path[&path_key(rel)].name = Some(ws.name.clone());
		}
	}
	config.overrides = by_path.clone();

	let discovered = discover_workspaces(root, &config)?;
	for (ws, rel) in v1.workspaces.iter().zip(&v1_paths) {
		let Some(found) = discovered.iter().find(|d| &d.rel_path == rel) else {
			continue;
		};
		let mut now_inferred = Vec::new();
		for dep in &ws.depends_on {
			if found.inferred_deps.contains(dep) {
				now_inferred.push(dep.clone());
			} else {
				by_path[&path_key(rel)].depends_on.push(dep.clone());
			}
		}
		if !now_inferred.is_empty() {
			notes.push(format!(
				"{}: dropped dependsOn {} — its manifest already says so",
				ws.name,
				now_inferred.join(", ")
			));
		}
	}

	config.overrides = by_path
		.into_iter()
		.zip(&v1.workspaces)
		.filter(|((_, over), _)| {
			over.name.is_some()
				|| over.driver.is_some()
				|| !over.engines.is_empty()
				|| !over.depends_on.is_empty()
				|| !over.tasks.is_empty()
		})
		.map(|((rel, over), ws)| {
			let key = if over.name.is_some() {
				rel
			} else {
				ws.name.clone()
			};
			(key, over)
		})
		.collect();

	let text = lattice_config::to_config_text(&config);
	let reparsed = lattice_config::parse_config(&text)
		.context("the migrated config does not parse; this is a bug in `lattice migrate`")?;
	discover_workspaces(root, &reparsed)
		.context("the migrated config does not resolve against this repo")?;

	Ok(Migration {
		config: reparsed,
		text,
		notes,
	})
}

#[cfg(test)]
mod tests {
	use super::*;

	fn write(root: &Path, rel: &str, contents: &str) {
		let path = root.join(rel);
		std::fs::create_dir_all(path.parent().unwrap()).unwrap();
		std::fs::write(path, contents).unwrap();
	}

	#[test]
	fn a_v1_cargo_workspace_shrinks_to_what_manifests_cannot_say() {
		let dir = tempfile::TempDir::new().unwrap();
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
			"[package]\nname = \"core\"\n",
		);
		write(
			root,
			"crates/cli/Cargo.toml",
			"[package]\nname = \"cli\"\n[dependencies]\ncore = { path = \"../core\" }\n",
		);
		write(
			root,
			"web/package.json",
			r#"{"scripts": {"build": "vite build"}}"#,
		);
		write(root, "web/package-lock.json", "{}");

		let v1 = r#"{
		  "latticeVersion": "1.1.1",
		  "workspaces": [
		    { "name": "core", "path": "crates/core" },
		    { "name": "cli", "path": "crates/cli", "dependsOn": ["core"] },
		    { "name": "site", "path": "web", "dependsOn": ["cli"], "scripts": { "dev": "vite" } }
		  ],
		  "engines": { "cargo": ">=1.80", "npm": ">=0.0.0" },
		  "tasks": { "build": { "dependsOn": ["^build"] }, "dev": { "persistent": true } }
		}"#;
		let m = migrate(root, v1, "2.0.0").unwrap();

		assert_eq!(m.config.lattice_version.as_deref(), Some("2.0.0"));
		assert_eq!(m.config.workspaces, ["web"]);
		assert!(m.config.engines.contains_key("cargo"));
		assert!(!m.config.engines.contains_key("npm"));
		assert!(
			!m.config.overrides.contains_key("cli"),
			"the edge is inferred"
		);

		let site = &m.config.overrides["./web"];
		assert_eq!(site.name.as_deref(), Some("site"));
		assert_eq!(site.depends_on, ["cli"]);
		assert!(matches!(&site.tasks["dev"], TaskOverride::Command(c) if c == "vite"));

		let ws = discover_workspaces(root, &m.config).unwrap();
		let names: Vec<&str> = ws.iter().map(|w| w.name.as_str()).collect();
		assert_eq!(names, ["cli", "core", "site"]);
		assert!(m
			.notes
			.iter()
			.any(|n| n.contains("cli: dropped dependsOn core")));
	}

	#[test]
	fn a_scoped_npm_workspace_keeps_its_overrides() {
		let dir = tempfile::TempDir::new().unwrap();
		let root = dir.path();
		write(root, "package.json", r#"{"workspaces": ["packages/*"]}"#);
		write(root, "package-lock.json", "{}");
		write(
			root,
			"packages/web/package.json",
			r#"{"name": "@acme/web", "scripts": {"build": "vite build"}}"#,
		);
		let v1 = r#"{
		  "workspaces": [
		    { "name": "@acme/web", "path": "packages/web", "scripts": { "e2e": "playwright test" } }
		  ]
		}"#;
		let m = migrate(root, v1, "2.0.0").unwrap();
		let web = &m.config.overrides["@acme/web"];
		assert!(matches!(&web.tasks["e2e"], TaskOverride::Command(c) if c == "playwright test"));
	}

	#[test]
	fn a_task_this_version_treats_differently_is_noted() {
		let dir = tempfile::TempDir::new().unwrap();
		std::fs::create_dir_all(dir.path().join("web")).unwrap();
		let v1 = r#"{
		  "workspaces": [{ "name": "web", "path": "web", "auto": false,
		                   "scripts": { "start": "node server.js", "deploy": "./ship" } }],
		  "tasks": { "build": {}, "start": { "persistent": false } }
		}"#;
		let notes = migrate(dir.path(), v1, "2.0.0").unwrap().notes;
		assert!(
			notes.iter().any(|n| n.starts_with("`build` now waits")),
			"{notes:?}"
		);
		assert!(
			notes
				.iter()
				.any(|n| n.starts_with("`deploy` is no longer cached")),
			"{notes:?}"
		);
		assert!(!notes.iter().any(|n| n.starts_with("`start`")), "{notes:?}");
	}

	#[test]
	fn a_manual_v1_workspace_keeps_its_commands_and_no_driver() {
		let dir = tempfile::TempDir::new().unwrap();
		std::fs::create_dir_all(dir.path().join("tools")).unwrap();
		let v1 = r#"{
		  "workspaces": [{ "name": "tools", "path": "tools", "auto": false,
		                   "scripts": { "build": "make" } }],
		  "tasks": { "build": {} }
		}"#;
		let m = migrate(dir.path(), v1, "2.0.0").unwrap();
		let over = &m.config.overrides["tools"];
		assert_eq!(over.driver.as_deref(), Some("none"));
		assert!(m.text.contains("\"build\": \"make\""), "{}", m.text);
		assert_eq!(m.config.lattice_version, None);
	}

	#[test]
	fn a_root_build_tool_pin_does_not_become_every_workspace_driver() {
		let dir = tempfile::TempDir::new().unwrap();
		let root = dir.path();
		write(root, "Cargo.toml", "[workspace]\nmembers = [\"core\"]\n");
		write(root, "core/Cargo.toml", "[package]\nname = \"core\"\n");
		write(
			root,
			"web/package.json",
			r#"{"scripts": {"build": "vite build"}}"#,
		);
		write(root, "web/package-lock.json", "{}");
		let v1 = r#"{
		  "workspaces": [{ "name": "core", "path": "core" }, { "name": "web", "path": "web" }],
		  "engines": { "cargo": ">=1.88.0", "npm": ">=0.0.0" }
		}"#;
		let m = migrate(root, v1, "2.0.0").unwrap();
		assert!(
			m.config.overrides.values().all(|o| o.driver.is_none()),
			"{}",
			m.text
		);
		let ws = discover_workspaces(root, &m.config).unwrap();
		let web = ws.iter().find(|w| w.name == "web").unwrap();
		assert_eq!(web.command_for("build"), Some("npm run build"));
		let core = ws.iter().find(|w| w.name == "core").unwrap();
		assert_eq!(core.command_for("build"), Some("cargo build"));
	}

	#[test]
	fn a_declaration_that_settled_an_ambiguity_carries_over() {
		let dir = tempfile::TempDir::new().unwrap();
		write(
			dir.path(),
			"tool/package.json",
			r#"{"scripts": {"build": "tsc"}}"#,
		);
		let v1 = r#"{
		  "workspaces": [{ "name": "tool", "path": "tool", "engines": { "pnpm": ">=0.0.0" } }]
		}"#;
		let m = migrate(dir.path(), v1, "2.0.0").unwrap();
		assert_eq!(m.config.overrides["tool"].driver.as_deref(), Some("pnpm"));
	}

	#[test]
	fn a_member_the_v1_config_left_out_stays_out() {
		let dir = tempfile::TempDir::new().unwrap();
		let root = dir.path();
		write(
			root,
			"Cargo.toml",
			"[workspace]\nmembers = [\"a\", \"b\"]\n",
		);
		write(root, "a/Cargo.toml", "[package]\nname = \"a\"\n");
		write(root, "b/Cargo.toml", "[package]\nname = \"b\"\n");
		let v1 = r#"{ "workspaces": [{ "name": "a", "path": "a" }] }"#;
		let m = migrate(root, v1, "2.0.0").unwrap();
		assert_eq!(m.config.exclude, ["b"]);
		assert!(m.config.workspaces.is_empty());
	}
}
