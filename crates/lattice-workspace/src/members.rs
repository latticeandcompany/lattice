//! Which directories are workspaces.
//!
//! The repo root's own workspace files name most of them — `package.json`
//! `workspaces`, `pnpm-workspace.yaml`, Cargo's `[workspace]`, `go.work`, uv's
//! `[tool.uv.workspace]`, deno's `workspace`. `lattice.json` adds directories
//! none of those know about (a Go service beside a JS monorepo) and `exclude`
//! takes any of them away.
//!
//! Only the root's files are expanded. A directory `lattice.json` names is one
//! workspace however it is organised inside: a nested monorepo, a Gradle build
//! with subprojects, a Cargo workspace in a subdirectory. Its own tool builds
//! it, and running two of that tool's builds over the same directory at once is
//! how locks and half-written outputs happen.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{bail, Context, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::WalkBuilder;
use lattice_config::LatticeConfig;

use crate::read;

/// Directories no workspace lives inside: dependency trees and build output.
/// Hidden directories and gitignored paths are skipped by the walker as well.
const SKIP_DIRS: &[&str] = &[
	"node_modules",
	"target",
	"dist",
	"build",
	"out",
	"vendor",
	"venv",
	"__pycache__",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
	/// Named by a workspace file at the repo root.
	Native(&'static str),
	/// Matched by `lattice.json` `workspaces`.
	Config,
	/// The repo root of a repo that names no workspaces at all.
	SingleProject,
}

/// A directory that is a workspace candidate, repo-relative and forward-slashed.
#[derive(Debug, Clone)]
pub struct Member {
	pub rel: String,
	pub origin: Origin,
}

pub fn find_members(root: &Path, config: &LatticeConfig) -> Result<Vec<Member>> {
	let mut found: BTreeMap<String, Origin> = BTreeMap::new();

	for (rel, file) in native_members(root)? {
		found.entry(rel).or_insert(Origin::Native(file));
	}

	for pattern in &config.workspaces {
		let matched = expand(root, pattern, |_| true)?;
		if matched.is_empty() {
			bail!(
				"`workspaces` entry '{pattern}' matches no directory. Check the spelling, or \
				 remove the entry"
			);
		}
		for rel in matched {
			found.entry(rel).or_insert(Origin::Config);
		}
	}

	if found.is_empty() {
		found.insert(".".to_string(), Origin::SingleProject);
	}

	let excluded = globs(&config.exclude, "exclude")?;
	Ok(found
		.into_iter()
		.filter(|(rel, _)| !excluded.is_match(rel))
		.map(|(rel, origin)| Member { rel, origin })
		.collect())
}

/// Every member the repo root's workspace files declare, with the file that
/// declared it.
fn native_members(root: &Path) -> Result<Vec<(String, &'static str)>> {
	let mut members = Vec::new();

	if let Some(pkg) = read::json(&root.join("package.json")) {
		let listed = pkg.get("workspaces").map(|w| match w.get("packages") {
			Some(packages) => read::json_strings(Some(packages)),
			None => read::json_strings(Some(w)),
		});
		if let Some(patterns) = listed {
			for rel in expand_all(root, &patterns, "package.json")? {
				members.push((rel, "package.json"));
			}
		}
	}

	if let Some(raw) = read::text(&root.join("pnpm-workspace.yaml")) {
		let patterns = pnpm_packages(&raw);
		for rel in expand_all(root, &patterns, "package.json")? {
			members.push((rel, "pnpm-workspace.yaml"));
		}
	}

	if let Some(manifest) = read::toml(&root.join("Cargo.toml")) {
		if let Some(workspace) = manifest.get("workspace").and_then(|w| w.as_table()) {
			let mut patterns = read::toml_strings(workspace.get("members"));
			patterns.extend(
				read::toml_strings(workspace.get("exclude"))
					.into_iter()
					.map(|p| format!("!{p}")),
			);
			for rel in expand_all(root, &patterns, "Cargo.toml")? {
				members.push((rel, "Cargo.toml"));
			}
			if manifest.contains_key("package") {
				members.push((".".to_string(), "Cargo.toml"));
			}
		}
	}

	if let Some(raw) = read::text(&root.join("go.work")) {
		for dir in go_work_uses(&raw) {
			if root.join(&dir).join("go.mod").is_file() {
				members.push((normalize(&dir), "go.work"));
			}
		}
	}

	if let Some(manifest) = read::toml(&root.join("pyproject.toml")) {
		if let Some(workspace) = read::toml_path(&manifest, &["tool", "uv", "workspace"]) {
			let mut patterns = read::toml_strings(workspace.get("members"));
			patterns.extend(
				read::toml_strings(workspace.get("exclude"))
					.into_iter()
					.map(|p| format!("!{p}")),
			);
			for rel in expand_all(root, &patterns, "pyproject.toml")? {
				members.push((rel, "pyproject.toml"));
			}
			if manifest.contains_key("project") {
				members.push((".".to_string(), "pyproject.toml"));
			}
		}
	}

	for file in ["deno.json", "deno.jsonc"] {
		if let Some(config) = read::json(&root.join(file)) {
			for pattern in read::json_strings(config.get("workspace")) {
				for rel in expand(root, &pattern, |dir| {
					dir.join("deno.json").is_file()
						|| dir.join("deno.jsonc").is_file()
						|| dir.join("package.json").is_file()
				})? {
					members.push((rel, "deno.json"));
				}
			}
		}
	}

	Ok(members)
}

/// Expand a workspace file's pattern list, where a `!` entry removes what the
/// entries before it matched, keeping the directories that hold `manifest`.
fn expand_all(root: &Path, patterns: &[String], manifest: &str) -> Result<Vec<String>> {
	let (include, exclude): (Vec<&String>, Vec<&String>) =
		patterns.iter().partition(|p| !p.starts_with('!'));
	let exclude: Vec<String> = exclude
		.iter()
		.map(|p| p.trim_start_matches('!').to_string())
		.collect();
	let excluded = globs(&exclude, manifest)?;

	let mut out = Vec::new();
	for pattern in include {
		for rel in expand(root, pattern, |dir| dir.join(manifest).is_file())? {
			if !excluded.is_match(&rel) && !out.contains(&rel) {
				out.push(rel);
			}
		}
	}
	Ok(out)
}

/// The directories under `root` a single pattern matches that `keep` accepts.
/// A pattern with no glob characters is looked up directly rather than walked
/// for, so naming a directory the walker would skip still works.
fn expand(root: &Path, pattern: &str, keep: impl Fn(&Path) -> bool) -> Result<Vec<String>> {
	let pattern = normalize(pattern);
	if !has_glob(&pattern) {
		let dir = root.join(&pattern);
		return Ok(if dir.is_dir() && keep(&dir) {
			vec![pattern]
		} else {
			Vec::new()
		});
	}

	let matcher = globs(std::slice::from_ref(&pattern), "workspaces")?;
	let depth = if pattern.contains("**") {
		None
	} else {
		Some(pattern.split('/').count())
	};
	let walker = WalkBuilder::new(root)
		.max_depth(depth)
		.require_git(false)
		.parents(false)
		// A directory the pattern names outright, like `packages/build` under
		// `packages/*`, is a candidate whatever it is called. Only the ones walked
		// through, and anything under `**`, are skipped for their name.
		.filter_entry(move |entry| {
			entry.depth() == 0
				|| Some(entry.depth()) == depth
				|| entry
					.file_name()
					.to_str()
					.is_some_and(|name| !SKIP_DIRS.contains(&name))
		})
		.build();

	let mut out = Vec::new();
	for entry in walker.flatten() {
		if !entry.file_type().is_some_and(|t| t.is_dir()) || entry.depth() == 0 {
			continue;
		}
		let rel = read::rel_path(root, entry.path());
		if matcher.is_match(&rel) && keep(entry.path()) {
			out.push(rel);
		}
	}
	out.sort();
	Ok(out)
}

fn globs(patterns: &[String], field: &str) -> Result<GlobSet> {
	let mut builder = GlobSetBuilder::new();
	for pattern in patterns {
		let glob = globset::GlobBuilder::new(&normalize(pattern))
			.literal_separator(true)
			.build()
			.or_else(|_| Glob::new(pattern))
			.with_context(|| format!("`{field}` entry '{pattern}' is not a valid glob"))?;
		builder.add(glob);
	}
	builder
		.build()
		.with_context(|| format!("`{field}` could not be compiled"))
}

pub(crate) fn has_glob(pattern: &str) -> bool {
	pattern.contains(['*', '?', '[', '{'])
}

/// `./apps/web/` and `apps/web` name the same directory.
pub fn normalize(pattern: &str) -> String {
	let trimmed = pattern
		.trim()
		.trim_start_matches("./")
		.trim_end_matches('/')
		.replace('\\', "/");
	if trimmed.is_empty() {
		".".to_string()
	} else {
		trimmed
	}
}

/// The `packages:` list of a `pnpm-workspace.yaml`. A handful of lines is all
/// that file ever holds, which is not worth a YAML parser.
fn pnpm_packages(raw: &str) -> Vec<String> {
	let mut out = Vec::new();
	let mut inside = false;
	for line in raw.lines() {
		let trimmed = line.trim();
		if trimmed.is_empty() || trimmed.starts_with('#') {
			continue;
		}
		if !line.starts_with([' ', '\t', '-']) {
			inside = trimmed.starts_with("packages:");
			continue;
		}
		if let Some(item) = trimmed.strip_prefix('-').filter(|_| inside) {
			let item = item.split(" #").next().unwrap_or(item).trim();
			out.push(item.trim_matches(['"', '\'']).to_string());
		}
	}
	out
}

/// The directories a `go.work` puts in the workspace, from both the one-line
/// `use ./a` form and the `use ( ... )` block.
fn go_work_uses(raw: &str) -> Vec<String> {
	let mut out = Vec::new();
	let mut in_block = false;
	for line in raw.lines() {
		let line = line.split("//").next().unwrap_or("").trim();
		if in_block {
			if line == ")" {
				in_block = false;
			} else if !line.is_empty() {
				out.push(line.trim_matches('"').to_string());
			}
			continue;
		}
		if let Some(rest) = line.strip_prefix("use") {
			let rest = rest.trim();
			if rest == "(" {
				in_block = true;
			} else if !rest.is_empty() {
				out.push(rest.trim_matches('"').to_string());
			}
		}
	}
	out
}

#[cfg(test)]
mod tests {
	use super::*;

	fn repo() -> tempfile::TempDir {
		tempfile::TempDir::new().unwrap()
	}

	fn write(root: &Path, rel: &str, contents: &str) {
		let path = root.join(rel);
		std::fs::create_dir_all(path.parent().unwrap()).unwrap();
		std::fs::write(path, contents).unwrap();
	}

	fn rels(root: &Path, config: &LatticeConfig) -> Vec<String> {
		find_members(root, config)
			.unwrap()
			.into_iter()
			.map(|m| m.rel)
			.collect()
	}

	#[test]
	fn package_json_workspaces_name_the_members_that_have_a_manifest() {
		let dir = repo();
		write(
			dir.path(),
			"package.json",
			r#"{"workspaces": ["packages/*"]}"#,
		);
		write(dir.path(), "packages/a/package.json", "{}");
		write(dir.path(), "packages/b/package.json", "{}");
		std::fs::create_dir_all(dir.path().join("packages/notes")).unwrap();

		assert_eq!(
			rels(dir.path(), &LatticeConfig::default()),
			["packages/a", "packages/b"]
		);
	}

	#[test]
	fn a_member_the_pattern_names_is_kept_whatever_it_is_called() {
		let dir = repo();
		let root = dir.path();
		write(root, "package.json", r#"{"workspaces": ["packages/*"]}"#);
		write(root, "packages/build/package.json", "{}");
		write(root, "packages/ui/package.json", "{}");
		write(root, "packages/ui/dist/package.json", "{}");
		assert_eq!(
			rels(root, &LatticeConfig::default()),
			["packages/build", "packages/ui"]
		);
	}

	#[test]
	fn pnpm_workspace_yaml_with_a_negation() {
		let dir = repo();
		write(
			dir.path(),
			"pnpm-workspace.yaml",
			"packages:\n  - 'apps/*'\n  - \"!apps/legacy\"\n",
		);
		write(dir.path(), "apps/web/package.json", "{}");
		write(dir.path(), "apps/legacy/package.json", "{}");

		assert_eq!(rels(dir.path(), &LatticeConfig::default()), ["apps/web"]);
	}

	#[test]
	fn a_virtual_cargo_workspace_names_its_members_but_not_itself() {
		let dir = repo();
		write(
			dir.path(),
			"Cargo.toml",
			"[workspace]\nmembers = [\"crates/*\"]\nexclude = [\"crates/skip\"]\n",
		);
		write(
			dir.path(),
			"crates/core/Cargo.toml",
			"[package]\nname = \"core\"\n",
		);
		write(
			dir.path(),
			"crates/skip/Cargo.toml",
			"[package]\nname = \"skip\"\n",
		);

		assert_eq!(rels(dir.path(), &LatticeConfig::default()), ["crates/core"]);
	}

	#[test]
	fn go_work_block_and_single_line_forms() {
		let dir = repo();
		write(
			dir.path(),
			"go.work",
			"go 1.22\n\nuse (\n\t./api\n\t./lib // shared\n)\nuse ./cli\n",
		);
		for m in ["api", "lib", "cli"] {
			write(dir.path(), &format!("{m}/go.mod"), "module x\n");
		}
		assert_eq!(
			rels(dir.path(), &LatticeConfig::default()),
			["api", "cli", "lib"]
		);
	}

	#[test]
	fn config_globs_add_to_native_members_and_exclude_removes() {
		let dir = repo();
		write(dir.path(), "package.json", r#"{"workspaces": ["web"]}"#);
		write(dir.path(), "web/package.json", "{}");
		write(dir.path(), "services/api/go.mod", "module api\n");
		write(dir.path(), "services/old/go.mod", "module old\n");

		let config = LatticeConfig {
			workspaces: vec!["services/*".into()],
			exclude: vec!["services/old".into()],
			..LatticeConfig::default()
		};
		assert_eq!(rels(dir.path(), &config), ["services/api", "web"]);
	}

	#[test]
	fn a_repo_that_names_nothing_is_one_project_at_the_root() {
		let dir = repo();
		write(dir.path(), "build.gradle.kts", "");
		let members = find_members(dir.path(), &LatticeConfig::default()).unwrap();
		assert_eq!(members.len(), 1);
		assert_eq!(members[0].rel, ".");
		assert_eq!(members[0].origin, Origin::SingleProject);
	}

	#[test]
	fn a_workspaces_entry_that_matches_nothing_is_an_error() {
		let dir = repo();
		let config = LatticeConfig {
			workspaces: vec!["apsp/*".into()],
			..LatticeConfig::default()
		};
		let err = find_members(dir.path(), &config).unwrap_err().to_string();
		assert!(err.contains("apsp/*"), "{err}");
	}

	#[test]
	fn a_star_does_not_cross_a_directory_boundary() {
		let dir = repo();
		write(dir.path(), "apps/web/package.json", "{}");
		write(dir.path(), "apps/web/nested/package.json", "{}");
		let config = LatticeConfig {
			workspaces: vec!["apps/*".into()],
			..LatticeConfig::default()
		};
		assert_eq!(rels(dir.path(), &config), ["apps/web"]);
	}
}
