//! Shared helpers for the black-box E2E CLI tests.
//!
//! Every test builds a fully self-contained repo inside its own
//! [`tempfile::TempDir`] and drives the compiled `lattice` binary through
//! `assert_cmd`. Nothing touches the real repo, `$HOME`, or global state. Task
//! bodies are plain `sh`/`echo`/`printf` commands, so no real language
//! toolchains are required.

// Each integration-test binary pulls in this module but uses only a subset of
// the helpers; silence the resulting per-binary dead-code warnings so the suite
// stays clean under `clippy --tests -- -D warnings`.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use tempfile::TempDir;

pub struct Fixture {
	dir: TempDir,
}

impl Fixture {
	pub fn new() -> Self {
		Fixture {
			dir: TempDir::new().expect("create temp dir"),
		}
	}

	pub fn root(&self) -> &Path {
		self.dir.path()
	}

	pub fn join(&self, rel: &str) -> PathBuf {
		self.dir.path().join(rel)
	}

	pub fn exists(&self, rel: &str) -> bool {
		self.join(rel).exists()
	}

	pub fn mkdir(&self, rel: &str) {
		std::fs::create_dir_all(self.join(rel)).expect("mkdir");
	}

	pub fn write(&self, rel: &str, contents: &str) {
		let path = self.join(rel);
		if let Some(parent) = path.parent() {
			std::fs::create_dir_all(parent).expect("create parents");
		}
		std::fs::write(&path, contents).expect("write file");
	}

	/// Write an executable file (creating parents) at a repo-relative path.
	/// Used to drop stand-in tools that a task resolves off `PATH`.
	#[cfg(unix)]
	pub fn write_exec(&self, rel: &str, contents: &str) {
		use std::os::unix::fs::PermissionsExt;
		self.write(rel, contents);
		let path = self.join(rel);
		let mut perms = std::fs::metadata(&path).expect("stat").permissions();
		perms.set_mode(0o755);
		std::fs::set_permissions(&path, perms).expect("chmod");
	}

	pub fn config(&self, json: &str) {
		self.write("lattice.json", json);
	}

	pub fn read(&self, rel: &str) -> String {
		std::fs::read_to_string(self.join(rel)).expect("read file")
	}

	/// A `lattice` command pre-pointed at this fixture, with deterministic env:
	/// `CI=1` + no TTY (assert_cmd pipes) forces the plain line output, and
	/// `NO_COLOR=1` keeps stdout ANSI-free and greppable.
	pub fn lattice(&self) -> Command {
		let mut cmd = Command::cargo_bin("lattice").expect("cargo_bin lattice");
		cmd.current_dir(self.dir.path())
			.env("CI", "1")
			.env("NO_COLOR", "1")
			.env_remove("LATTICE_NO_VERSION_CHECK");
		cmd
	}

	/// Recursively collect every file path under a repo-relative directory.
	/// Returns an empty vec if the directory does not exist.
	pub fn files_under(&self, rel: &str) -> Vec<PathBuf> {
		fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
			if let Ok(entries) = std::fs::read_dir(dir) {
				for e in entries.flatten() {
					let p = e.path();
					if p.is_dir() {
						walk(&p, out);
					} else {
						out.push(p);
					}
				}
			}
		}
		let mut out = Vec::new();
		walk(&self.join(rel), &mut out);
		out
	}

	/// The `*.tar.gz` cache artifacts currently in `.lattice/cache`.
	pub fn cache_tarballs(&self) -> Vec<PathBuf> {
		self.files_under(".lattice/cache")
			.into_iter()
			.filter(|p| p.extension().map(|x| x == "gz").unwrap_or(false))
			.collect()
	}
}
