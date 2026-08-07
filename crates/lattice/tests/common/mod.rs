//! Shared helpers for the black-box E2E CLI tests.
//!
//! Every test builds a fully self-contained repo inside its own
//! [`tempfile::TempDir`] and drives the compiled `lattice` binary through
//! `assert_cmd`. Nothing touches the real repo, `$HOME`, or global state. Task
//! bodies come from `lattice_testkit`, which spells them for whichever shell will
//! run them, so no real language toolchains are required and the suites mean the
//! same thing on every platform.

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

	pub fn config(&self, json: &str) {
		self.write("lattice.json", json);
	}

	/// Write `lattice.json` from a template, replacing each `@name@` token with
	/// the matching command, JSON-escaped and quoted.
	///
	/// A task body has to be spelled for the shell that will run it, and on
	/// Windows that means backslashes — which are not valid in a JSON string as
	/// written. Substituting through here escapes them, and keeps the config
	/// readable instead of turning it into a `format!` with doubled braces.
	pub fn config_from(&self, template: &str, cmds: &[(&str, String)]) {
		let mut text = template.to_string();
		for (name, cmd) in cmds {
			let token = format!("@{name}@");
			assert!(
				text.contains(&token),
				"the template has no {token} to substitute"
			);
			text = text.replace(&token, &lattice_testkit::json(cmd));
		}
		self.config(&text);
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

	/// Install a workspace binary into `bin/` under the fixture, named `as_name`
	/// plus the platform's executable suffix.
	///
	/// Used to put a stand-in tool on a test's `PATH`. A copied program works
	/// wherever it was built, which a shell script dropped on disk does not.
	pub fn install_stub_bin(&self, built: &str, as_name: &str) {
		let source = assert_cmd::cargo::cargo_bin(built);
		assert!(
			source.exists(),
			"{} has not been built; `cargo test --workspace` builds every member's \
             binaries, so run that (or `cargo build --workspace --all-targets`) rather \
             than this test target alone",
			source.display()
		);
		let bin_dir = self.join("bin");
		std::fs::create_dir_all(&bin_dir).expect("mkdir bin");
		let dest = bin_dir.join(format!("{as_name}{}", std::env::consts::EXE_SUFFIX));
		std::fs::remove_file(&dest).ok();
		std::fs::copy(&source, &dest).expect("copy the stub binary");
		wait_until_executable(&dest);
	}

	/// A `PATH` with the fixture's `bin/` in front of the host's.
	pub fn path_with_stub_bin(&self) -> std::ffi::OsString {
		let host = std::env::var_os("PATH").unwrap_or_default();
		let mut dirs = vec![self.join("bin")];
		dirs.extend(std::env::split_paths(&host));
		std::env::join_paths(dirs).expect("join PATH")
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

	/// Install the compiled binary as `.lattice/bin/lattice-<version>` and point
	/// `.lattice/bin/lattice` at it, which is what the installer produces and what
	/// makes the drift check consider a binary its own to replace.
	pub fn install_managed(&self, version: &str) -> PathBuf {
		let bin_dir = self.join(".lattice/bin");
		std::fs::create_dir_all(&bin_dir).expect("mkdir .lattice/bin");
		let versioned = bin_dir.join(format!("lattice-{version}{}", std::env::consts::EXE_SUFFIX));
		let source = assert_cmd::cargo::cargo_bin("lattice");
		// Replace rather than overwrite: on macOS, writing over a binary the kernel
		// has already loaded invalidates its cached signature and the next exec of
		// it is killed.
		std::fs::remove_file(&versioned).ok();
		std::fs::copy(&source, &versioned).expect("copy the built binary");
		wait_until_executable(&versioned);
		let link = bin_dir.join(format!("lattice{}", std::env::consts::EXE_SUFFIX));
		std::fs::remove_file(&link).ok();
		#[cfg(unix)]
		std::os::unix::fs::symlink(versioned.file_name().unwrap(), &link).expect("symlink");
		#[cfg(not(unix))]
		std::fs::copy(&versioned, &link).expect("copy to the stable path");
		versioned
	}

	/// A command running the managed binary at `.lattice/bin/lattice-<version>`,
	/// with the same deterministic env as [`Fixture::lattice`].
	pub fn managed_lattice(&self, version: &str) -> Command {
		let bin = self.join(&format!(
			".lattice/bin/lattice-{version}{}",
			std::env::consts::EXE_SUFFIX
		));
		let mut cmd = Command::new(bin);
		cmd.current_dir(self.dir.path())
			.env("CI", "1")
			.env("NO_COLOR", "1")
			.env_remove("LATTICE_NO_VERSION_CHECK");
		cmd
	}

	/// Whether the stable `.lattice/bin/lattice` path resolves to `version`'s
	/// install.
	///
	/// Unix links the two and Windows copies, so this asks the question each way
	/// rather than reading a symlink that only one platform has.
	pub fn stable_points_at(&self, version: &str) -> bool {
		let stable = self.join(&format!(
			".lattice/bin/lattice{}",
			std::env::consts::EXE_SUFFIX
		));
		let versioned = self.join(&format!(
			".lattice/bin/lattice-{version}{}",
			std::env::consts::EXE_SUFFIX
		));
		#[cfg(unix)]
		{
			let target = std::fs::read_link(&stable).expect("read the stable symlink");
			target == std::path::Path::new(versioned.file_name().unwrap())
		}
		#[cfg(not(unix))]
		{
			match (std::fs::read(&stable), std::fs::read(&versioned)) {
				(Ok(a), Ok(b)) => a == b,
				_ => false,
			}
		}
	}
}

/// Block until an exec of `bin` stops failing with `ETXTBSY`.
///
/// The copy above holds a write handle on the new binary for as long as it takes.
/// Tests run as threads of one process, so a `fork` from a sibling test during
/// that window leaves the child holding a reference to that handle too, and the
/// kernel refuses to exec a file anyone can still write to. Waiting for the
/// sibling to get out of the way here keeps that race out of every test that
/// installs a managed binary and immediately runs it.
fn wait_until_executable(bin: &Path) {
	for _ in 0..100 {
		match std::process::Command::new(bin).arg("--version").output() {
			Err(e) if e.kind() == std::io::ErrorKind::ExecutableFileBusy => {
				std::thread::sleep(std::time::Duration::from_millis(20));
			}
			_ => return,
		}
	}
	panic!("{} was still busy after 2s", bin.display());
}

/// A release, published to a directory and served over `file://`.
///
/// The download, checksum, extract and link path is the same code whether the
/// base URL is GitHub or a directory, so the tests exercise all of it without a
/// network. The published binary is the compiled `release-stub`, which prints the
/// name it was invoked as — so a handover is observable in stdout, and on a
/// platform that cannot execute a shell script.
pub struct FakeRelease {
	dir: TempDir,
}

impl FakeRelease {
	pub fn new() -> Self {
		FakeRelease {
			dir: TempDir::new().expect("create temp dir"),
		}
	}

	pub fn base_url(&self) -> String {
		format!("file://{}", self.dir.path().display())
	}

	/// The triple this test binary was compiled for, which is the asset the CLI
	/// will look for.
	pub fn host_target() -> &'static str {
		env!("LATTICE_TARGET")
	}

	/// Publish `version` for the host target: an archive carrying the stub binary,
	/// and a checksums file listing it.
	pub fn publish(&self, version: &str) {
		self.publish_inner(version, None);
	}

	/// Publish `version` with a checksums file that does not describe the archive,
	/// standing in for a tampered or truncated download.
	pub fn publish_with_wrong_digest(&self, version: &str) {
		self.publish_inner(version, Some("0".repeat(64)));
	}

	fn publish_inner(&self, version: &str, digest_override: Option<String>) {
		let target = Self::host_target();
		let asset = format!("lattice-{version}-{target}.tar.gz");
		let dir = self.dir.path().join(format!("v{version}"));
		std::fs::create_dir_all(&dir).expect("mkdir release dir");

		let stub = assert_cmd::cargo::cargo_bin("release-stub");
		assert!(
			stub.exists(),
			"{} has not been built; `cargo test --workspace` builds every member's \
             binaries, so run that rather than this test target alone",
			stub.display()
		);
		let payload = std::fs::read(&stub).expect("read the stub binary");
		let archive_path = dir.join(&asset);
		write_archive(
			&archive_path,
			&format!("lattice-{version}-{target}"),
			&payload,
		);

		let digest = digest_override.unwrap_or_else(|| sha256_hex(&archive_path));
		std::fs::write(
			dir.join(format!("lattice-{version}-checksums.txt")),
			format!("{digest}  {asset}\n"),
		)
		.expect("write checksums");
	}

	/// A `file://` URL for a release-API response naming `version`, for
	/// `lattice upgrade latest`.
	pub fn latest_url(&self, version: &str) -> String {
		let path = self.dir.path().join("latest.json");
		std::fs::write(&path, format!(r#"{{"tag_name":"v{version}"}}"#))
			.expect("write latest.json");
		format!("file://{}", path.display())
	}

	/// A `file://` URL for a `/releases` list response, newest entry first. Each
	/// entry is a version and whether it is a pre-release.
	pub fn list_url(&self, releases: &[(&str, bool)]) -> String {
		let entries: Vec<String> = releases
			.iter()
			.map(|(version, prerelease)| {
				format!(r#"{{"tag_name":"v{version}","draft":false,"prerelease":{prerelease}}}"#)
			})
			.collect();
		let path = self.dir.path().join("releases.json");
		std::fs::write(&path, format!("[{}]", entries.join(","))).expect("write releases.json");
		format!("file://{}", path.display())
	}

	/// A latest-release URL that resolves to nothing, standing in for the 404
	/// GitHub answers with while every release so far is a pre-release.
	pub fn missing_latest_url(&self) -> String {
		format!(
			"file://{}",
			self.dir.path().join("no-such-latest.json").display()
		)
	}

	/// Delete a published version, so a later run that still succeeds proves it
	/// did not download anything.
	pub fn unpublish(&self, version: &str) {
		std::fs::remove_dir_all(self.dir.path().join(format!("v{version}")))
			.expect("remove the published version");
	}
}

/// A `.tar.gz` holding `<prefix>/lattice`, executable, next to the license and
/// completions a real release archive also carries.
fn write_archive(path: &Path, prefix: &str, binary: &[u8]) {
	let file = std::fs::File::create(path).expect("create archive");
	let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::fast());
	let mut builder = tar::Builder::new(encoder);

	let bin_name = format!("lattice{}", std::env::consts::EXE_SUFFIX);
	let mut header = tar::Header::new_gnu();
	header.set_size(binary.len() as u64);
	header.set_mode(0o755);
	header.set_cksum();
	builder
		.append_data(&mut header, format!("{prefix}/{bin_name}"), binary)
		.expect("append the binary");

	for (name, body) in [
		("LICENSE", &b"ISC"[..]),
		("completions/lattice.bash", &b"# completions"[..]),
	] {
		let mut header = tar::Header::new_gnu();
		header.set_size(body.len() as u64);
		header.set_mode(0o644);
		header.set_cksum();
		builder
			.append_data(&mut header, format!("{prefix}/{name}"), body)
			.expect("append an extra file");
	}
	builder.into_inner().expect("finish tar").finish().ok();
}

fn sha256_hex(path: &Path) -> String {
	use sha2::{Digest, Sha256};
	let bytes = std::fs::read(path).expect("read archive");
	let mut hasher = Sha256::new();
	hasher.update(&bytes);
	hex::encode(hasher.finalize())
}

/// Whether `curl` can fetch a `file://` URL, which the fake release relies on.
pub fn curl_supports_file() -> bool {
	std::process::Command::new("curl")
		.arg("-V")
		.output()
		.map(|out| String::from_utf8_lossy(&out.stdout).contains("file"))
		.unwrap_or(false)
}
