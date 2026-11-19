//! What a workspace's manifests say it is called and what it depends on.
//!
//! Dependency order between workspaces comes from here rather than from
//! `lattice.json`: a `workspace:*` in `package.json`, a `path` dependency in
//! `Cargo.toml`, a `require` in `go.mod` or a uv workspace source already says
//! which package needs which. Edges between languages are the one thing no
//! manifest can say, and those are written in `overrides`.

use std::path::{Path, PathBuf};

use crate::read;

/// A package name within one ecosystem. The same string names different
/// packages in npm and in Cargo, so the ecosystem is part of the key.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PackageKey {
	Npm(String),
	Cargo(String),
	Go(String),
	Python(String),
	Composer(String),
}

#[derive(Debug, Clone)]
pub enum DepRef {
	Package(PackageKey),
	/// A dependency given as a directory, canonicalized where it exists.
	Dir(PathBuf),
}

#[derive(Debug, Clone, Default)]
pub struct Identity {
	/// The name the manifest gives the package, when it gives one.
	pub name: Option<String>,
	pub keys: Vec<PackageKey>,
	pub deps: Vec<DepRef>,
}

pub fn identity(root: &Path, dir: &Path) -> Identity {
	let mut id = Identity::default();
	npm(dir, &mut id);
	cargo(root, dir, &mut id);
	python(dir, &mut id);
	composer(dir, &mut id);
	go(dir, &mut id);
	id
}

fn claim_name(id: &mut Identity, name: &str) {
	if id.name.is_none() && !name.trim().is_empty() {
		id.name = Some(name.to_string());
	}
}

fn npm(dir: &Path, id: &mut Identity) {
	let Some(pkg) = read::json(&dir.join("package.json")) else {
		return;
	};
	if let Some(name) = pkg.get("name").and_then(|n| n.as_str()) {
		claim_name(id, name);
		id.keys.push(PackageKey::Npm(name.to_string()));
	}
	for section in [
		"dependencies",
		"devDependencies",
		"peerDependencies",
		"optionalDependencies",
	] {
		for dep in read::json_keys(pkg.get(section)) {
			id.deps.push(DepRef::Package(PackageKey::Npm(dep)));
		}
	}
}

fn cargo(root: &Path, dir: &Path, id: &mut Identity) {
	let Some(manifest) = read::toml(&dir.join("Cargo.toml")) else {
		return;
	};
	if let Some(name) = read::toml_path(&manifest, &["package", "name"]).and_then(|n| n.as_str()) {
		claim_name(id, name);
		id.keys.push(PackageKey::Cargo(name.to_string()));
	}

	let root_deps = read::toml(&root.join("Cargo.toml")).and_then(|m| {
		read::toml_path(&m, &["workspace", "dependencies"])
			.and_then(|d| d.as_table())
			.cloned()
	});

	let mut tables: Vec<&toml::Table> = Vec::new();
	for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
		if let Some(t) = manifest.get(section).and_then(|t| t.as_table()) {
			tables.push(t);
		}
	}
	if let Some(targets) = manifest.get("target").and_then(|t| t.as_table()) {
		for target in targets.values().filter_map(|t| t.as_table()) {
			for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
				if let Some(t) = target.get(section).and_then(|t| t.as_table()) {
					tables.push(t);
				}
			}
		}
	}

	for table in tables {
		for (key, spec) in table {
			let Some(spec) = spec.as_table() else {
				continue;
			};
			if let Some(path) = spec.get("path").and_then(|p| p.as_str()) {
				id.deps.push(DepRef::Dir(canonical(&dir.join(path))));
			} else if spec.get("workspace").and_then(|w| w.as_bool()) == Some(true) {
				let inherited = root_deps
					.as_ref()
					.and_then(|deps| deps.get(key))
					.and_then(|d| d.as_table())
					.and_then(|d| d.get("path"))
					.and_then(|p| p.as_str());
				if let Some(path) = inherited {
					id.deps.push(DepRef::Dir(canonical(&root.join(path))));
				}
			}
		}
	}
}

fn python(dir: &Path, id: &mut Identity) {
	let Some(manifest) = read::toml(&dir.join("pyproject.toml")) else {
		return;
	};
	if let Some(name) = read::toml_path(&manifest, &["project", "name"]).and_then(|n| n.as_str()) {
		claim_name(id, name);
		id.keys.push(PackageKey::Python(python_name(name)));
	}

	let mut requirements =
		read::toml_strings(read::toml_path(&manifest, &["project", "dependencies"]));
	for section in [
		&["project", "optional-dependencies"][..],
		&["dependency-groups"][..],
	] {
		if let Some(groups) = read::toml_path(&manifest, section).and_then(|g| g.as_table()) {
			for group in groups.values() {
				requirements.extend(read::toml_strings(Some(group)));
			}
		}
	}
	for requirement in requirements {
		let name = requirement
			.split(|c: char| " <>=!~;[(@".contains(c))
			.next()
			.unwrap_or("");
		if !name.is_empty() {
			id.deps
				.push(DepRef::Package(PackageKey::Python(python_name(name))));
		}
	}

	if let Some(sources) =
		read::toml_path(&manifest, &["tool", "uv", "sources"]).and_then(|s| s.as_table())
	{
		for spec in sources.values().filter_map(|s| s.as_table()) {
			if let Some(path) = spec.get("path").and_then(|p| p.as_str()) {
				id.deps.push(DepRef::Dir(canonical(&dir.join(path))));
			}
		}
	}
}

/// PEP 503: names compare case-insensitively with runs of `-`, `_` and `.` equal.
fn python_name(name: &str) -> String {
	let mut out = String::new();
	let mut last_sep = false;
	for c in name.trim().chars() {
		if matches!(c, '-' | '_' | '.') {
			if !last_sep {
				out.push('-');
			}
			last_sep = true;
		} else {
			out.extend(c.to_lowercase());
			last_sep = false;
		}
	}
	out
}

fn composer(dir: &Path, id: &mut Identity) {
	let Some(manifest) = read::json(&dir.join("composer.json")) else {
		return;
	};
	if let Some(name) = manifest.get("name").and_then(|n| n.as_str()) {
		claim_name(id, name);
		id.keys.push(PackageKey::Composer(name.to_string()));
	}
	for section in ["require", "require-dev"] {
		for dep in read::json_keys(manifest.get(section)) {
			id.deps.push(DepRef::Package(PackageKey::Composer(dep)));
		}
	}
}

/// Go names a module by import path, which makes a poor workspace name — the
/// directory stands in for it — but is exactly what another module requires.
fn go(dir: &Path, id: &mut Identity) {
	let Some(raw) = read::text(&dir.join("go.mod")) else {
		return;
	};
	let mut in_require = false;
	let mut in_replace = false;
	for line in raw.lines() {
		let line = line.split("//").next().unwrap_or("").trim();
		if let Some(module) = line.strip_prefix("module ") {
			id.keys
				.push(PackageKey::Go(module.trim().trim_matches('"').to_string()));
			continue;
		}
		if line == ")" {
			in_require = false;
			in_replace = false;
			continue;
		}
		if line == "require (" {
			in_require = true;
			continue;
		}
		if line == "replace (" {
			in_replace = true;
			continue;
		}
		let require = line.strip_prefix("require ").or(in_require.then_some(line));
		if let Some(entry) = require {
			if let Some(module) = entry.split_whitespace().next() {
				id.deps
					.push(DepRef::Package(PackageKey::Go(module.to_string())));
			}
			continue;
		}
		let replace = line.strip_prefix("replace ").or(in_replace.then_some(line));
		if let Some((_, target)) = replace.and_then(|r| r.split_once("=>")) {
			let target = target.split_whitespace().next().unwrap_or("");
			if target.starts_with('.') {
				id.deps.push(DepRef::Dir(canonical(&dir.join(target))));
			}
		}
	}
}

pub(crate) fn canonical(path: &Path) -> PathBuf {
	path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
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
	fn cargo_path_and_inherited_workspace_deps_are_directories() {
		let dir = tempfile::TempDir::new().unwrap();
		let root = dir.path();
		write(
			root,
			"Cargo.toml",
			"[workspace]\nmembers=[\"a\",\"b\",\"c\"]\n[workspace.dependencies]\nc = { path = \"c\" }\n",
		);
		write(root, "b/Cargo.toml", "[package]\nname=\"b\"\n");
		write(root, "c/Cargo.toml", "[package]\nname=\"c\"\n");
		write(
			root,
			"a/Cargo.toml",
			"[package]\nname=\"a\"\n[dependencies]\nb = { path = \"../b\" }\nc = { workspace = true }\nserde = \"1\"\n",
		);

		let id = identity(root, &root.join("a"));
		assert_eq!(id.name.as_deref(), Some("a"));
		let dirs: Vec<PathBuf> = id
			.deps
			.iter()
			.filter_map(|d| match d {
				DepRef::Dir(p) => Some(p.clone()),
				_ => None,
			})
			.collect();
		assert_eq!(
			dirs,
			[canonical(&root.join("b")), canonical(&root.join("c"))]
		);
	}

	#[test]
	fn python_requirement_names_are_normalized() {
		let dir = tempfile::TempDir::new().unwrap();
		write(
			dir.path(),
			"pyproject.toml",
			"[project]\nname = \"My_App\"\ndependencies = [\"Shared.Lib>=1.0\", \"requests[socks]\"]\n",
		);
		let id = identity(dir.path(), dir.path());
		assert!(id.keys.contains(&PackageKey::Python("my-app".into())));
		assert!(id
			.deps
			.iter()
			.any(|d| matches!(d, DepRef::Package(PackageKey::Python(n)) if n == "shared-lib")));
	}

	#[test]
	fn go_requires_in_both_forms() {
		let dir = tempfile::TempDir::new().unwrap();
		write(
			dir.path(),
			"go.mod",
			"module example.com/api\n\nrequire example.com/lib v0.0.0\nrequire (\n\texample.com/util v1.0.0 // indirect\n)\n",
		);
		let id = identity(dir.path(), dir.path());
		assert_eq!(id.name, None);
		let deps: Vec<String> = id
			.deps
			.iter()
			.filter_map(|d| match d {
				DepRef::Package(PackageKey::Go(m)) => Some(m.clone()),
				_ => None,
			})
			.collect();
		assert_eq!(deps, ["example.com/lib", "example.com/util"]);
	}
}
