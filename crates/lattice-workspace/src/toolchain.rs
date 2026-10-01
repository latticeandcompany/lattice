//! The engine gradient: host / validate / provision.
//!
//! The developer declares an `installCmd`; Lattice runs it into a
//! content-addressed toolchain directory under `./.lattice/toolchains/`,
//! version-checks the result, pins it, and hands the runner a `PATH` prefix
//! that activates it for the duration of a single task.

use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use lattice_config::{EngineMap, EngineSpec};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// How a single engine should be satisfied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineMode {
	/// No constraint and no install command: trust whatever is on `PATH`.
	HostPath,
	/// A version constraint but no install command: check the host tool
	/// satisfies it, install nothing.
	ValidateOnly { constraint: Option<String> },
	/// An install command: provision into `.lattice/toolchains`, version-check,
	/// pin, and prepend its bin dir.
	Provisioned {
		install_cmd: String,
		version_cmd: Option<String>,
		constraint: Option<String>,
		bin: String,
	},
}

/// The version command for an engine: explicit `versionCmd` wins, else the
/// built-in rule from [`lattice_config::WELL_KNOWN_ENGINES`].
fn version_cmd_for(name: &str, spec: &EngineSpec) -> Option<String> {
	spec.version_cmd()
		.map(String::from)
		.or_else(|| lattice_config::builtin_version_cmd(name).map(String::from))
}

/// [`version_cmd_for`] as seen from `cwd`: a Gradle or Maven project's wrapper
/// is the version of the tool its build runs, whatever is on `PATH`.
fn version_cmd_in(
	name: &str,
	spec: &EngineSpec,
	root: &Path,
	cwd: Option<&Path>,
) -> Option<String> {
	if spec.version_cmd().is_none() {
		let wrapper = match name {
			"gradle" => Some(("gradlew", "gradle")),
			"maven" => Some(("mvnw", "mvn")),
			_ => None,
		};
		if let (Some((script, tool)), Some(dir)) = (wrapper, cwd) {
			let found = crate::tasks::wrapper(dir, root, script, tool);
			if found != tool {
				return Some(format!("{found} --version"));
			}
		}
	}
	version_cmd_for(name, spec)
}

/// Classify one engine spec into an [`EngineMode`].
///
/// * object with `installCmd` → [`EngineMode::Provisioned`];
/// * any version constraint (string or object) → [`EngineMode::ValidateOnly`];
/// * neither → [`EngineMode::HostPath`].
pub fn classify(name: &str, spec: &EngineSpec) -> EngineMode {
	if let Some(install_cmd) = spec.install_cmd() {
		EngineMode::Provisioned {
			install_cmd: install_cmd.to_string(),
			version_cmd: version_cmd_for(name, spec),
			constraint: spec.version().map(String::from),
			bin: spec.bin().unwrap_or("bin").to_string(),
		}
	} else if let Some(constraint) = spec.version() {
		EngineMode::ValidateOnly {
			constraint: Some(constraint.to_string()),
		}
	} else {
		EngineMode::HostPath
	}
}

/// The `PATH` prefix and cache-key identity for a resolved engine map.
#[derive(Debug)]
pub struct ResolvedToolchains {
	/// Directories to prepend to a task's `PATH`, in order.
	pub path_prepend: Vec<PathBuf>,
	/// A stable identity string over `(engine, version, installHash)`, sorted.
	pub identity: String,
	/// What each engine resolved to, for telling the developer.
	pub engines: Vec<ResolvedEngine>,
}

/// One engine as a run will use it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedEngine {
	pub name: String,
	/// `None` when nothing checked it: no constraint, or no version to read.
	pub version: Option<String>,
	/// The provisioned bin directory, or the host binary a check found.
	pub location: Option<PathBuf>,
	pub provisioned: bool,
}

impl ResolvedEngine {
	/// `node 22.11.0`, `node 22.11.0 (installed by Lattice)`, `cargo (unchecked)`.
	pub fn describe(&self) -> String {
		match (&self.version, self.provisioned) {
			(Some(v), true) => format!("{} {v} (installed by Lattice)", self.name),
			(Some(v), false) => format!("{} {v}", self.name),
			(None, true) => format!("{} (installed by Lattice)", self.name),
			(None, false) => format!("{} (unchecked)", self.name),
		}
	}
}

/// The pin record written to `pins.json` in each provisioned toolchain dir.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolchainPins {
	pub engine: String,
	pub version: String,
	pub install_hash: String,
	pub bin: String,
}

/// Recorded as the version of a provisioned engine that has no version command
/// and no constraint to check. There is nothing to ask, so nothing is claimed —
/// the install hash is what identifies such a toolchain.
const UNKNOWN_VERSION: &str = "unknown";

/// Prefix of a staging directory inside an engine's toolchain dir.
const STAGING_PREFIX: &str = "tmp-";

/// How old a staging directory has to be before it is treated as the remains of
/// a run that died. Long enough that no live install is ever caught by the sweep.
const STALE_STAGING: Duration = Duration::from_secs(24 * 60 * 60);

/// First 8 hex chars of `sha256(install_cmd)`.
fn install_hash8(install_cmd: &str) -> String {
	let mut h = Sha256::new();
	h.update(install_cmd.as_bytes());
	let digest = h.finalize();
	let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
	hex[..8].to_string()
}

/// Build a [`Command`] that runs `command` through the platform shell, the same
/// way the runner hands a task's command over: `sh -c` on unix, `cmd /C` on
/// windows. A version check or an `installCmd` is a command string like any
/// other, and running it through a shell the platform does not have means every
/// engine in the config fails to resolve.
fn shell_command(command: &str) -> Command {
	// `/S /C` with a raw argument, not `.arg(command)`: Rust escapes an embedded
	// quote the way the MSVC runtime parses it, and `cmd` does not read `\"` as an
	// escape. A `versionCmd` or `installCmd` containing a quote — a quoted install
	// path, say — would otherwise arrive mangled. With `/S`, `cmd` strips the first
	// and last quote of the rest and takes what is between them verbatim.
	#[cfg(windows)]
	{
		use std::os::windows::process::CommandExt;
		let mut c = Command::new("cmd");
		c.raw_arg(format!("/S /C \"{command}\""));
		c
	}
	#[cfg(not(windows))]
	{
		let mut c = Command::new("sh");
		c.arg("-c").arg(command);
		c
	}
}

/// Prepend `dir` to a `PATH` value using the platform's separator.
///
/// Falling back to the inherited `PATH` would drop the pin and run whatever
/// version of the tool the machine happens to have — the one thing pinning
/// exists to prevent — so a `PATH` that cannot be built is an error.
fn prepend_to_path(dir: &Path) -> Result<std::ffi::OsString> {
	let existing = std::env::var_os("PATH").unwrap_or_default();
	let mut paths: Vec<PathBuf> = vec![dir.to_path_buf()];
	paths.extend(std::env::split_paths(&existing));
	std::env::join_paths(paths).with_context(|| {
		format!(
			"the pinned toolchain cannot be put on PATH, because a directory in it contains \
			 a character PATH cannot hold: {}",
			dir.display()
		)
	})
}

/// What a command printed, and how it ended.
struct Capture {
	ok: bool,
	code: Option<i32>,
	out: String,
}

impl Capture {
	/// The shell could not find the program at all.
	fn not_found(&self) -> bool {
		self.code == Some(127)
			|| self
				.out
				.contains("is not recognized as an internal or external command")
	}
}

/// Run a command string through the platform shell, optionally prepending
/// `extra_path` to `PATH`, in `cwd`, with extra env. Output is stdout then
/// stderr.
fn run_capture(
	cmd: &str,
	extra_path: Option<&Path>,
	cwd: Option<&Path>,
	extra_env: &[(String, String)],
) -> Result<Capture> {
	let mut command = shell_command(cmd);
	if let Some(p) = extra_path {
		command.env("PATH", prepend_to_path(p)?);
	}
	if let Some(dir) = cwd {
		command.current_dir(dir);
	}
	for (k, v) in extra_env {
		command.env(k, v);
	}
	let out = command
		.output()
		.with_context(|| format!("failed to spawn `{cmd}`"))?;
	let mut combined = String::from_utf8_lossy(&out.stdout).into_owned();
	combined.push_str(&String::from_utf8_lossy(&out.stderr));
	Ok(Capture {
		ok: out.status.success(),
		code: out.status.code(),
		out: combined,
	})
}

/// Where the program a command names lives: the first directory on `PATH`
/// (after `extra`) that holds it, or the file itself for a relative path like
/// `./gradlew`.
fn which(cmd: &str, extra: Option<&Path>, cwd: Option<&Path>) -> Option<PathBuf> {
	let program = cmd.split_whitespace().next()?;
	if program.contains('/') || program.contains('\\') {
		let path = cwd
			.map(|c| c.join(program))
			.unwrap_or_else(|| PathBuf::from(program));
		let tidy: PathBuf = path.components().collect();
		return tidy.is_file().then_some(tidy);
	}
	let exts: &[&str] = if cfg!(windows) {
		&["", ".exe", ".cmd", ".bat"]
	} else {
		&[""]
	};
	let path = std::env::var_os("PATH").unwrap_or_default();
	extra
		.map(Path::to_path_buf)
		.into_iter()
		.chain(std::env::split_paths(&path))
		.flat_map(|dir| {
			exts.iter()
				.map(move |ext| dir.join(format!("{program}{ext}")))
		})
		.find(|candidate| candidate.is_file())
}

/// An engine failure laid out so each fact is on its own labeled line.
fn engine_error(name: &str, headline: &str, rows: &[(&str, String)], fix: &str) -> anyhow::Error {
	let mut message = format!("engine '{name}' {headline}");
	for (label, value) in rows {
		let value = value.trim();
		if value.is_empty() {
			continue;
		}
		let mut lines = value.lines();
		message.push_str(&format!("\n  {label:<8} {}", lines.next().unwrap_or("")));
		let rest: Vec<&str> = lines.collect();
		let shown = rest.len().min(20);
		let skipped = rest.len() - shown;
		if skipped > 0 {
			message.push_str(&format!("\n  {:<8} … {skipped} more lines", ""));
		}
		for line in &rest[rest.len() - shown..] {
			message.push_str(&format!("\n  {:<8} {line}", ""));
		}
	}
	message.push_str(&format!("\n  {:<8} {fix}", "fix"));
	anyhow::anyhow!(message)
}

fn located(capture_path: Option<PathBuf>, version: impl std::fmt::Display) -> String {
	match capture_path {
		Some(path) => format!("{version} at {}", path.display()),
		None => version.to_string(),
	}
}

/// Tolerantly parse a version out of arbitrary tool output:
/// `"v20.11.1"`, `"go1.22"`, `"rustc 1.75.0 (..)"`, `"1.75"`, …
pub fn parse_version(raw: &str) -> Option<semver::Version> {
	let start = raw.find(|c: char| c.is_ascii_digit())?;
	let rest = &raw[start..];
	let end = rest
		.find(|c: char| !(c.is_ascii_digit() || c == '.'))
		.unwrap_or(rest.len());
	let nums = &rest[..end];
	let parts: Vec<u64> = nums
		.split('.')
		.filter(|s| !s.is_empty())
		.filter_map(|s| s.parse().ok())
		.collect();
	if parts.is_empty() {
		return None;
	}
	Some(semver::Version::new(
		parts.first().copied().unwrap_or(0),
		parts.get(1).copied().unwrap_or(0),
		parts.get(2).copied().unwrap_or(0),
	))
}

/// [`parse_version`] for engine `name`, starting after the words its output puts
/// before the version where an earlier number is something else: `swift
/// --version` on macOS leads with the driver's version, `elixir --version` and
/// `mix --version` with Erlang/OTP's, and a first Maven wrapper run with
/// download URLs through `maven2`.
pub fn parse_engine_version(name: &str, raw: &str) -> Option<semver::Version> {
	let marker = match name {
		"swift" => "Swift version ",
		"elixir" => "Elixir ",
		"mix" => "Mix ",
		"maven" => "Apache Maven ",
		_ => return parse_version(raw),
	};
	raw.find(marker)
		.and_then(|at| parse_version(&raw[at + marker.len()..]))
		.or_else(|| parse_version(raw))
}

/// Whether `v` satisfies a (possibly loose) constraint such as `">=20.0.0"`,
/// `"^1.75"`, or a bare `"1.22"`.
pub fn satisfies(v: &semver::Version, constraint: &str) -> bool {
	let c = constraint.trim();
	if c.is_empty() {
		return true;
	}
	if let Ok(req) = semver::VersionReq::parse(c) {
		return req.matches(v);
	}
	// Fallback: treat a bare/loose version as a lower bound.
	if let Some(min) = parse_version(c) {
		return *v >= min;
	}
	true
}

/// Locate an already-installed, verified pin for a provisioned engine so we
/// install only once. Scans for a `*-<hash>` dir carrying a matching
/// `pins.json` whose bin dir exists (and whose version still satisfies the
/// constraint when a version command is available).
fn find_verified_pin(
	engine_dir: &Path,
	install_hash: &str,
	constraint: Option<&str>,
	version_cmd: Option<&str>,
) -> Option<(PathBuf, ToolchainPins)> {
	// A constraint with no way to check it can never be confirmed, so no pin
	// counts as verified; provisioning then says so rather than trusting one.
	if constraint.is_some() && version_cmd.is_none() {
		return None;
	}
	let suffix = format!("-{install_hash}");
	let entries = std::fs::read_dir(engine_dir).ok()?;
	for entry in entries.flatten() {
		let dir = entry.path();
		let name = entry.file_name();
		let name = name.to_str().unwrap_or("");
		if !name.ends_with(&suffix) {
			continue;
		}
		let pins_raw = std::fs::read_to_string(dir.join("pins.json")).ok();
		let Some(pins_raw) = pins_raw else { continue };
		let Ok(pins) = serde_json::from_str::<ToolchainPins>(&pins_raw) else {
			continue;
		};
		if pins.install_hash != install_hash {
			continue;
		}
		let Ok(bin_rel) = checked_bin(&pins.engine, &pins.bin) else {
			continue;
		};
		let bin_dir = dir.join(bin_rel);
		if !bin_dir.is_dir() {
			continue;
		}
		// Re-verify the version if we can; this does not reinstall.
		if let (Some(vc), Some(cons)) = (version_cmd, constraint) {
			let captured = run_capture(vc, Some(&bin_dir), None, &[]);
			if !capture_confirms(&pins.engine, captured, cons) {
				continue;
			}
		}
		return Some((dir, pins));
	}
	None
}

/// Whether a version command's captured output proves the pin still satisfies
/// `constraint`. A run that never spawned, exited non-zero, or printed no
/// version proves nothing — none of those is a pin worth reusing.
fn capture_confirms(name: &str, captured: Result<Capture>, constraint: &str) -> bool {
	let Ok(c) = captured else {
		return false;
	};
	c.ok && parse_engine_version(name, &c.out).is_some_and(|v| satisfies(&v, constraint))
}

/// An engine's `bin` as a path relative to its toolchain directory.
///
/// An absolute `bin` replaces the toolchain path outright when joined, and one
/// climbing through `..` leaves `.lattice/toolchains` — either way the `PATH`
/// prefix would point at a directory Lattice never provisioned, while the
/// identity still claims a provisioned toolchain.
fn checked_bin<'a>(name: &str, bin: &'a str) -> Result<&'a Path> {
	let rel = Path::new(bin);
	let escapes = rel.components().any(|c| {
		matches!(
			c,
			Component::ParentDir | Component::RootDir | Component::Prefix(_)
		)
	});
	if rel.is_absolute() || escapes {
		bail!(
			"engine '{name}': bin '{bin}' has to be a relative path inside the engine's \
			 toolchain directory"
		);
	}
	Ok(rel)
}

/// A staging directory name no concurrent run can also be using. The staging
/// tree is cleared before use, so two runs sharing one delete each other's
/// half-installed files and promote a tree of mixed provenance.
fn staging_name(hash: &str) -> String {
	static NEXT: AtomicUsize = AtomicUsize::new(0);
	format!(
		"{STAGING_PREFIX}{hash}-{}-{}",
		std::process::id(),
		NEXT.fetch_add(1, Ordering::Relaxed)
	)
}

/// Delete staging directories older than `older_than`: a run killed before it
/// could promote or remove its own leaves one behind for good.
fn sweep_stale_staging(engine_dir: &Path, older_than: Duration) {
	let Ok(entries) = std::fs::read_dir(engine_dir) else {
		return;
	};
	for entry in entries.flatten() {
		if !entry
			.file_name()
			.to_string_lossy()
			.starts_with(STAGING_PREFIX)
		{
			continue;
		}
		let stale = entry
			.metadata()
			.and_then(|m| m.modified())
			.is_ok_and(|t| t.elapsed().is_ok_and(|age| age >= older_than));
		if stale {
			std::fs::remove_dir_all(entry.path()).ok();
		}
	}
}

/// Run `install_cmd` into `staging` and return the version to record for what it
/// installed. A constraint is enforced here or nowhere: a tool whose version
/// cannot be read cannot be shown to satisfy one.
fn install_and_verify(
	staging: &Path,
	name: &str,
	install_cmd: &str,
	version_cmd: Option<&str>,
	constraint: Option<&str>,
	bin: &Path,
) -> Result<String> {
	// $LATTICE_TOOLCHAIN_DIR: set as env and literal-substituted so both
	// `... $LATTICE_TOOLCHAIN_DIR ...` and env-var reads work.
	let staging_str = staging.to_string_lossy().into_owned();
	let substituted = install_cmd.replace("$LATTICE_TOOLCHAIN_DIR", &staging_str);
	let env = vec![("LATTICE_TOOLCHAIN_DIR".to_string(), staging_str)];
	let installed = run_capture(&substituted, None, None, &env)?;
	if !installed.ok {
		let status = match installed.code {
			Some(code) => format!("exited with code {code}"),
			None => "was killed by a signal".to_string(),
		};
		return Err(engine_error(
			name,
			&format!("could not be installed: its installCmd {status}"),
			&[("ran", install_cmd.to_string()), ("output", installed.out)],
			"the command runs in a shell with $LATTICE_TOOLCHAIN_DIR set to an empty \
			 directory; it should put the tool there, with its executables under `bin`",
		));
	}

	let Some(vc) = version_cmd else {
		if let Some(cons) = constraint {
			return Err(engine_error(
				name,
				&format!("has the constraint '{cons}' but no way to check what was installed"),
				&[],
				&format!("'{name}' is not a well-known engine, so add a `versionCmd` to it"),
			));
		}
		return Ok(UNKNOWN_VERSION.to_string());
	};

	let bin_dir = staging.join(bin);
	let checked = run_capture(vc, Some(&bin_dir), None, &[])?;
	if !checked.ok {
		return Err(engine_error(
			name,
			"was installed, but its version command failed",
			&[("ran", vc.to_string()), ("output", checked.out)],
			&format!(
				"check that the installCmd puts `{}` in {}",
				vc.split_whitespace().next().unwrap_or(vc),
				bin.display()
			),
		));
	}
	match parse_engine_version(name, &checked.out) {
		Some(v) => {
			if let Some(cons) = constraint {
				if !satisfies(&v, cons) {
					return Err(engine_error(
						name,
						"installed a version that does not match its constraint",
						&[
							("wanted", cons.to_string()),
							("found", v.to_string()),
							("ran", vc.to_string()),
						],
						&format!("change the installCmd to install a version matching '{cons}'"),
					));
				}
			}
			Ok(v.to_string())
		}
		None => {
			if let Some(cons) = constraint {
				return Err(engine_error(
					name,
					"was installed, but Lattice could not read a version from it",
					&[
						("wanted", cons.to_string()),
						("ran", vc.to_string()),
						("output", checked.out),
					],
					"point `versionCmd` at a command that prints the version",
				));
			}
			Ok(UNKNOWN_VERSION.to_string())
		}
	}
}

/// Move a finished staging tree to its content-addressed home, reporting whether
/// this call is the one that put it there. A concurrent run may have promoted an
/// identical tree first, in which case that one stands. A directory with no
/// `pins.json` is the remains of a run that died mid-promotion, and is replaced.
fn promote_staging(staging: &Path, final_dir: &Path) -> Result<()> {
	// The pin has to be inside `staging` already, so the rename publishes a
	// finished tree. Promoting without it leaves the directory briefly pinless,
	// which is how another run reads the remains of one that died, so it would
	// delete a tree still in use. Enforced here because the window it opens is
	// microseconds wide and no timing test catches it reliably.
	anyhow::ensure!(
		staging.join("pins.json").is_file(),
		"internal error: refusing to promote {} without its pins.json",
		staging.display()
	);
	if final_dir.exists() && !final_dir.join("pins.json").is_file() {
		std::fs::remove_dir_all(final_dir).ok();
	}
	if !final_dir.exists() {
		match std::fs::rename(staging, final_dir) {
			Ok(()) => return Ok(()),
			Err(err) if !final_dir.join("pins.json").is_file() => {
				std::fs::remove_dir_all(staging).ok();
				return Err(err).with_context(|| {
					format!(
						"failed to move toolchain into place: {} -> {}",
						staging.display(),
						final_dir.display()
					)
				});
			}
			Err(_) => {}
		}
	}
	std::fs::remove_dir_all(staging).ok();
	Ok(())
}

/// Provision (if needed) and resolve every engine in `engines` into a `PATH`
/// prefix + cache identity, checking versions from the repo root.
pub fn provision_and_resolve(
	root: &Path,
	engines: &EngineMap,
	log: &mut dyn FnMut(&str),
) -> Result<ResolvedToolchains> {
	provision_and_resolve_in(root, engines, None, log)
}

/// [`provision_and_resolve`] for the workspace in `cwd`, where a build tool's
/// wrapper decides which version of it runs.
pub fn provision_and_resolve_in(
	root: &Path,
	engines: &EngineMap,
	cwd: Option<&Path>,
	log: &mut dyn FnMut(&str),
) -> Result<ResolvedToolchains> {
	let mut path_prepend = Vec::new();
	let mut identity_parts: Vec<String> = Vec::new();
	let mut engines_out: Vec<ResolvedEngine> = Vec::new();

	for (name, spec) in engines {
		match classify(name, spec) {
			EngineMode::HostPath => {
				identity_parts.push(format!("{name}=host"));
				engines_out.push(ResolvedEngine {
					name: name.clone(),
					version: None,
					location: None,
					provisioned: false,
				});
			}
			EngineMode::ValidateOnly { constraint } => {
				let Some(vc) = version_cmd_in(name, spec, root, cwd) else {
					return Err(engine_error(
						name,
						"has a version constraint but no way to check the installed version",
						&[],
						&format!(
							"'{name}' is not a well-known engine, so add a `versionCmd` to it"
						),
					));
				};
				let found = which(&vc, None, cwd);
				let checked = run_capture(&vc, None, cwd, &[])?;
				let cons = constraint.clone().unwrap_or_default();
				let install_hint = if vc.contains("gradlew") {
					format!(
						"move the project's wrapper to a version matching {cons}: \
						 `./gradlew wrapper --gradle-version <version>`"
					)
				} else if vc.contains("mvnw") {
					format!(
						"move the project's wrapper to a version matching {cons}: \
						 `./mvnw wrapper:wrapper -Dmaven=<version>`"
					)
				} else {
					format!(
						"install {name} {cons}, or give the engine an installCmd so Lattice installs \
						 it for this repo"
					)
				};
				if checked.not_found() || (!checked.ok && found.is_none()) {
					return Err(engine_error(
						name,
						"is not installed, or not on PATH",
						&[("wanted", cons.clone()), ("ran", vc.clone())],
						&install_hint,
					));
				}
				if !checked.ok {
					return Err(engine_error(
						name,
						"could not be checked: its version command failed",
						&[
							("wanted", cons.clone()),
							("ran", vc.clone()),
							("output", checked.out),
						],
						"fix the tool, or point `versionCmd` at a command that prints its version",
					));
				}
				let Some(version) = parse_engine_version(name, &checked.out) else {
					return Err(engine_error(
						name,
						"could not be checked: Lattice found no version in what it printed",
						&[
							("wanted", cons.clone()),
							("ran", vc.clone()),
							("output", checked.out),
						],
						"point `versionCmd` at a command that prints the version",
					));
				};
				if !satisfies(&version, &cons) {
					return Err(engine_error(
						name,
						"does not match the version this repo pins",
						&[
							("wanted", cons.clone()),
							("found", located(found.clone(), &version)),
							("ran", vc.clone()),
						],
						&install_hint,
					));
				}
				identity_parts.push(format!("{name}={version}@host"));
				engines_out.push(ResolvedEngine {
					name: name.clone(),
					version: Some(version.to_string()),
					location: found,
					provisioned: false,
				});
			}
			EngineMode::Provisioned {
				install_cmd,
				version_cmd,
				constraint,
				bin,
			} => {
				let hash = install_hash8(&install_cmd);
				let engine_dir = root.join(".lattice").join("toolchains").join(name);
				let bin_rel = checked_bin(name, &bin)?;

				// Reuse an existing verified pin (install once).
				if let Some((dir, pins)) = find_verified_pin(
					&engine_dir,
					&hash,
					constraint.as_deref(),
					version_cmd.as_deref(),
				) {
					let bin_dir = dir.join(checked_bin(name, &pins.bin)?);
					identity_parts.push(format!("{name}={}@{hash}", pins.version));
					engines_out.push(ResolvedEngine {
						name: name.clone(),
						version: (pins.version != UNKNOWN_VERSION).then(|| pins.version.clone()),
						location: Some(bin_dir.clone()),
						provisioned: true,
					});
					path_prepend.push(bin_dir);
					continue;
				}

				std::fs::create_dir_all(&engine_dir).with_context(|| {
					format!("failed to create toolchain dir {}", engine_dir.display())
				})?;
				sweep_stale_staging(&engine_dir, STALE_STAGING);

				let staging = engine_dir.join(staging_name(&hash));
				std::fs::create_dir_all(&staging)?;

				log(&format!(
					"provisioning engine '{name}' via installCmd into {}",
					staging.display()
				));

				let version = match install_and_verify(
					&staging,
					name,
					&install_cmd,
					version_cmd.as_deref(),
					constraint.as_deref(),
					bin_rel,
				) {
					Ok(version) => version,
					Err(err) => {
						std::fs::remove_dir_all(&staging).ok();
						return Err(err);
					}
				};

				// Move into the content-addressed, versioned final dir. The pin is
				// written into staging first, so the rename publishes a finished
				// tree in one step. Writing it afterwards left the directory
				// briefly pinless, which is exactly how a promotion reads the
				// remains of a run that died, so a second run provisioning the
				// same engine would delete a tree its owner was still using.
				let final_dir = engine_dir.join(format!("{version}-{hash}"));
				let pins = ToolchainPins {
					engine: name.clone(),
					version: version.clone(),
					install_hash: hash.clone(),
					bin: bin.clone(),
				};
				if let Err(err) = std::fs::write(
					staging.join("pins.json"),
					serde_json::to_string_pretty(&pins)?,
				) {
					std::fs::remove_dir_all(&staging).ok();
					return Err(err)
						.with_context(|| format!("failed to record the pin for engine '{name}'"));
				}
				promote_staging(&staging, &final_dir)?;

				path_prepend.push(final_dir.join(bin_rel));
				identity_parts.push(format!("{name}={version}@{hash}"));
				engines_out.push(ResolvedEngine {
					name: name.clone(),
					version: (version != UNKNOWN_VERSION).then(|| version.clone()),
					location: Some(final_dir.join(bin_rel)),
					provisioned: true,
				});
			}
		}
	}

	identity_parts.sort();
	Ok(ResolvedToolchains {
		path_prepend,
		identity: identity_parts.join(";"),
		engines: engines_out,
	})
}

#[cfg(test)]
mod tests {
	use super::*;

	fn cap(ok: bool, out: &str) -> Capture {
		Capture {
			ok,
			code: Some(if ok { 0 } else { 1 }),
			out: out.to_string(),
		}
	}

	#[cfg(unix)]
	#[test]
	fn a_tool_that_is_missing_says_so_and_how_to_fix_it() {
		let tmp = tempfile::tempdir().unwrap();
		let engines = fake_engine(serde_json::json!({
			"version": ">=1.0.0",
			"versionCmd": "definitely-not-a-real-tool-xyz --version"
		}));
		let err = provision_and_resolve(tmp.path(), &engines, &mut |_| {})
			.unwrap_err()
			.to_string();
		assert!(err.contains("is not installed, or not on PATH"), "{err}");
		assert!(err.contains("wanted   >=1.0.0"), "{err}");
		assert!(err.contains("fix      install"), "{err}");
	}

	#[test]
	fn a_version_mismatch_names_what_was_wanted_and_found() {
		let tmp = tempfile::tempdir().unwrap();
		let engines = fake_engine(serde_json::json!({
			"version": ">=99.0.0",
			"versionCmd": "echo fake 1.2.3"
		}));
		let err = provision_and_resolve(tmp.path(), &engines, &mut |_| {})
			.unwrap_err()
			.to_string();
		assert!(
			err.contains("does not match the version this repo pins"),
			"{err}"
		);
		assert!(err.contains("wanted   >=99.0.0"), "{err}");
		assert!(err.contains("found    1.2.3"), "{err}");
		assert!(err.contains("ran      "), "{err}");
	}

	#[test]
	fn a_failed_install_shows_its_output_and_status() {
		let tmp = tempfile::tempdir().unwrap();
		let engines = fake_engine(serde_json::json!({
			"installCmd": lattice_testkit::then([
				lattice_testkit::echo("downloading"),
				lattice_testkit::echo_err("no network"),
				lattice_testkit::exit(7),
			])
		}));
		let err = provision_and_resolve(tmp.path(), &engines, &mut |_| {})
			.unwrap_err()
			.to_string();
		assert!(err.contains("installCmd exited with code 7"), "{err}");
		assert!(err.contains("no network"), "{err}");
		assert!(err.contains("$LATTICE_TOOLCHAIN_DIR"), "{err}");
	}

	#[cfg(unix)]
	#[test]
	fn gradle_is_checked_through_the_wrapper_of_the_build_it_runs() {
		let tmp = tempfile::tempdir().unwrap();
		std::fs::write(tmp.path().join("gradlew"), "").unwrap();
		std::fs::create_dir_all(tmp.path().join("app")).unwrap();
		let spec = EngineSpec::Version(">=8".into());
		assert_eq!(
			version_cmd_in("gradle", &spec, tmp.path(), Some(&tmp.path().join("app"))).as_deref(),
			Some("../gradlew --version")
		);
		assert_eq!(
			version_cmd_in("gradle", &spec, tmp.path(), None).as_deref(),
			Some("gradle --version")
		);
	}

	#[test]
	fn a_resolved_engine_describes_where_it_came_from() {
		let e = ResolvedEngine {
			name: "node".into(),
			version: Some("22.11.0".into()),
			location: None,
			provisioned: true,
		};
		assert_eq!(e.describe(), "node 22.11.0 (installed by Lattice)");
	}
	use serde_json::json;
	use tempfile::TempDir;

	fn spec(v: serde_json::Value) -> EngineSpec {
		serde_json::from_value(v).unwrap()
	}

	#[test]
	fn classify_host_validate_provision() {
		// string version-only → ValidateOnly
		assert!(matches!(
			classify("node", &spec(json!(">=20.0.0"))),
			EngineMode::ValidateOnly { .. }
		));
		// object with version but no installCmd → ValidateOnly
		assert!(matches!(
			classify(
				"alpes",
				&spec(json!({ "version": ">=2", "versionCmd": "alp --version" }))
			),
			EngineMode::ValidateOnly { .. }
		));
		// object with installCmd → Provisioned
		assert!(matches!(
			classify(
				"alpes",
				&spec(json!({ "installCmd": "curl x | sh", "versionCmd": "alp --version" }))
			),
			EngineMode::Provisioned { .. }
		));
		// object with neither → HostPath
		assert!(matches!(
			classify("node", &spec(json!({}))),
			EngineMode::HostPath
		));
	}

	#[test]
	fn parse_version_tolerant() {
		assert_eq!(
			parse_version("v20.11.1").unwrap(),
			semver::Version::new(20, 11, 1)
		);
		assert_eq!(
			parse_version("go1.22").unwrap(),
			semver::Version::new(1, 22, 0)
		);
		assert_eq!(
			parse_version("rustc 1.75.0 (abc 2024)").unwrap(),
			semver::Version::new(1, 75, 0)
		);
		assert_eq!(
			parse_version("1.75").unwrap(),
			semver::Version::new(1, 75, 0)
		);
		assert!(parse_version("no digits here").is_none());

		let swift =
			"swift-driver version: 1.90.11.1 Apple Swift version 5.10 (swiftlang-5.10.0.13)";
		assert_eq!(
			parse_engine_version("swift", swift).unwrap().to_string(),
			"5.10.0"
		);
		let elixir =
			"Erlang/OTP 26 [erts-14.2] [64-bit]\n\nElixir 1.16.0 (compiled with Erlang/OTP 26)";
		assert_eq!(
			parse_engine_version("elixir", elixir).unwrap().to_string(),
			"1.16.0"
		);
		let mix = "Erlang/OTP 26 [erts-14.2]\n\nMix 1.16.0 (compiled with Erlang/OTP 26)";
		assert_eq!(
			parse_engine_version("mix", mix).unwrap().to_string(),
			"1.16.0"
		);
		let maven = "Downloading from central: https://repo.maven.apache.org/maven2/x\nApache Maven 3.9.6 (bc0240f)";
		assert_eq!(
			parse_engine_version("maven", maven).unwrap().to_string(),
			"3.9.6"
		);
		assert_eq!(
			parse_engine_version("swift", "5.9").unwrap().to_string(),
			"5.9.0"
		);
	}

	#[test]
	fn satisfies_matrix() {
		let v = semver::Version::new(20, 11, 1);
		assert!(satisfies(&v, ">=20.0.0"));
		assert!(!satisfies(&v, ">=21.0.0"));
		let r = semver::Version::new(1, 75, 0);
		assert!(satisfies(&r, "^1.75"));
		assert!(!satisfies(&r, "^1.76"));
		let g = semver::Version::new(1, 22, 3);
		assert!(satisfies(&g, "1.22"));
		assert!(satisfies(&g, "")); // empty == no constraint
	}

	fn fake_engines() -> EngineMap {
		// The installCmd provisions a stand-in `faketool` that prints a version.
		fake_engine(json!({
			"version": ">=1.0.0",
			"installCmd": lattice_testkit::install_fake_tool("faketool", "1.2.3"),
			"versionCmd": "faketool",
			"bin": "bin"
		}))
	}

	fn fake_engine(spec: serde_json::Value) -> EngineMap {
		serde_json::from_value(json!({ "faketool": spec })).unwrap()
	}

	/// An `installCmd` for a stand-in tool that prints no version at all.
	fn installs_without_a_version() -> String {
		lattice_testkit::install_fake_tool("faketool", "no-version-here")
	}

	fn engine_dir(root: &Path) -> PathBuf {
		root.join(".lattice").join("toolchains").join("faketool")
	}

	fn read_pins(bin: &Path) -> ToolchainPins {
		let raw = std::fs::read_to_string(bin.parent().unwrap().join("pins.json")).unwrap();
		serde_json::from_str(&raw).unwrap()
	}

	#[test]
	fn provision_installs_pins_and_reuses() {
		let tmp = TempDir::new().unwrap();
		let engines = fake_engines();

		let mut installs = 0usize;
		let mut log = |m: &str| {
			if m.contains("provisioning") {
				installs += 1;
			}
		};
		let resolved = provision_and_resolve(tmp.path(), &engines, &mut log).unwrap();

		// Installed into .lattice/toolchains and path points at its bin dir.
		assert_eq!(resolved.path_prepend.len(), 1);
		let bin = &resolved.path_prepend[0];
		assert!(bin.ends_with("bin"));
		assert!(bin.is_dir());
		assert!(bin
			.join(lattice_testkit::fake_tool_file("faketool"))
			.exists());
		assert!(bin.starts_with(tmp.path().join(".lattice").join("toolchains")));

		// pins.json was written next to the bin dir.
		let pin_dir = bin.parent().unwrap();
		let pins: ToolchainPins =
			serde_json::from_str(&std::fs::read_to_string(pin_dir.join("pins.json")).unwrap())
				.unwrap();
		assert_eq!(pins.engine, "faketool");
		assert_eq!(pins.version, "1.2.3");
		assert_eq!(pins.bin, "bin");
		assert_eq!(installs, 1);

		// Identity is stable and content-addressed.
		assert!(resolved.identity.contains("faketool=1.2.3@"));

		// Re-running reuses the pin — installs exactly once.
		let mut installs2 = 0usize;
		let mut log2 = |m: &str| {
			if m.contains("provisioning") {
				installs2 += 1;
			}
		};
		let again = provision_and_resolve(tmp.path(), &engines, &mut log2).unwrap();
		assert_eq!(installs2, 0, "second run must reuse the pin");
		assert_eq!(again.identity, resolved.identity);
		assert_eq!(again.path_prepend, resolved.path_prepend);
	}

	/// A tool whose version cannot be read cannot be shown to satisfy a
	/// constraint. This used to provision happily, print `setup complete`, and pin
	/// `0.0.0` — a version nothing installed, feeding every cache key.
	#[test]
	fn an_unreadable_version_fails_the_constraint_it_cannot_be_checked_against() {
		let tmp = TempDir::new().unwrap();
		let engines = fake_engine(json!({
			"version": ">=99.0.0",
			"installCmd": installs_without_a_version(),
			"versionCmd": "faketool",
			"bin": "bin"
		}));

		let err = provision_and_resolve(tmp.path(), &engines, &mut |_| {}).unwrap_err();
		let msg = format!("{err:#}");
		assert!(msg.contains("could not read a version"), "{msg}");
		assert!(msg.contains(">=99.0.0"), "{msg}");

		// Nothing was pinned, and the staging directory went with the failure.
		let left: Vec<_> = std::fs::read_dir(engine_dir(tmp.path()))
			.unwrap()
			.flatten()
			.map(|e| e.file_name())
			.collect();
		assert!(
			left.is_empty(),
			"a refused install leaves nothing: {left:?}"
		);
	}

	/// The same config is strict without an `installCmd`, so it cannot be silent
	/// with one.
	#[test]
	fn a_constraint_with_no_version_command_is_refused_either_way() {
		let tmp = TempDir::new().unwrap();
		let validate_only = fake_engine(json!({ "version": ">=1.0.0" }));
		let provisioned = fake_engine(json!({
			"version": ">=1.0.0",
			"installCmd": installs_without_a_version(),
			"bin": "bin"
		}));

		for engines in [validate_only, provisioned] {
			let err = provision_and_resolve(tmp.path(), &engines, &mut |_| {}).unwrap_err();
			let msg = format!("{err:#}");
			assert!(msg.contains("add a `versionCmd`"), "{msg}");
		}
	}

	/// With nothing to check against there is nothing to claim either: the install
	/// hash is what identifies such a toolchain.
	#[test]
	fn an_engine_with_no_constraint_records_an_unknown_version() {
		let tmp = TempDir::new().unwrap();
		let engines = fake_engine(json!({
			"installCmd": installs_without_a_version(),
			"versionCmd": "faketool",
			"bin": "bin"
		}));

		let resolved = provision_and_resolve(tmp.path(), &engines, &mut |_| {}).unwrap();
		assert!(
			resolved.identity.starts_with("faketool=unknown@"),
			"{}",
			resolved.identity
		);
		assert_eq!(read_pins(&resolved.path_prepend[0]).version, "unknown");
	}

	/// A version check that never ran is not a check that passed. The pin lookup
	/// used to accept the pin outright when the command could not be spawned.
	#[test]
	fn a_version_check_that_did_not_run_confirms_nothing() {
		assert!(capture_confirms("tool", Ok(cap(true, "1.2.3")), ">=1.0.0"));
		assert!(!capture_confirms("tool", Ok(cap(true, "0.9.0")), ">=1.0.0"));
		assert!(!capture_confirms(
			"tool",
			Ok(cap(false, "1.2.3")),
			">=1.0.0"
		));
		assert!(!capture_confirms(
			"tool",
			Ok(cap(true, "no-version-here")),
			">=1.0.0"
		));
		assert!(!capture_confirms(
			"tool",
			Err(anyhow::anyhow!("failed to spawn `faketool`")),
			">=1.0.0"
		));
	}

	/// Two runs provisioning one engine at once each need staging of their own: a
	/// shared directory is cleared before use, so one run deletes the other's
	/// half-installed files and what gets promoted is a mix of both.
	#[test]
	fn concurrent_provisions_do_not_clear_each_others_staging() {
		let tmp = TempDir::new().unwrap();
		let root = tmp.path().to_path_buf();

		let runs: Vec<_> = (0..4)
			.map(|_| {
				let root = root.clone();
				std::thread::spawn(move || {
					provision_and_resolve(&root, &fake_engines(), &mut |_| {})
						.map(|resolved| resolved.path_prepend)
				})
			})
			.collect();

		// Join every run before asserting. Checking one run's directory while the
		// others are still promoting into it races against the test itself.
		let prepends: Vec<_> = runs
			.into_iter()
			.map(|run| run.join().unwrap().expect("every concurrent run installs"))
			.collect();

		for prepend in &prepends {
			let bin = &prepend[0];
			assert!(
				bin.join(lattice_testkit::fake_tool_file("faketool"))
					.is_file(),
				"{} is missing the tool that was installed into it",
				bin.display()
			);
			assert_eq!(read_pins(bin).version, "1.2.3");
		}

		let staged: Vec<String> = std::fs::read_dir(engine_dir(tmp.path()))
			.unwrap()
			.flatten()
			.map(|e| e.file_name().to_string_lossy().into_owned())
			.filter(|name| name.starts_with(STAGING_PREFIX))
			.collect();
		assert!(staged.is_empty(), "staging left behind: {staged:?}");
	}

	/// Promoting a tree without its pin is what let a second run mistake a live
	/// toolchain for the remains of a dead one and delete it. The ordering is a
	/// correctness requirement, so it is checked rather than merely intended.
	#[test]
	fn a_staging_tree_without_its_pin_is_never_promoted() {
		let tmp = TempDir::new().unwrap();
		let staging = tmp.path().join("tmp-abc-1-1");
		std::fs::create_dir_all(staging.join("bin")).unwrap();

		let err = promote_staging(&staging, &tmp.path().join("1.2.3-abc")).unwrap_err();
		assert!(
			format!("{err:#}").contains("without its pins.json"),
			"{err:#}"
		);
	}

	/// The loser of a race adopts the published tree instead of replacing it.
	#[test]
	fn promotion_leaves_a_published_tree_alone() {
		let tmp = TempDir::new().unwrap();
		let final_dir = tmp.path().join("1.2.3-abc");
		std::fs::create_dir_all(final_dir.join("bin")).unwrap();
		std::fs::write(final_dir.join("pins.json"), "{}").unwrap();
		std::fs::write(final_dir.join("bin/faketool"), "the winner").unwrap();

		let staging = tmp.path().join("tmp-abc-2-1");
		std::fs::create_dir_all(staging.join("bin")).unwrap();
		std::fs::write(staging.join("pins.json"), "{}").unwrap();
		std::fs::write(staging.join("bin/faketool"), "the loser").unwrap();

		promote_staging(&staging, &final_dir).unwrap();

		assert!(!staging.exists(), "the loser cleans up after itself");
		assert_eq!(
			std::fs::read_to_string(final_dir.join("bin/faketool")).unwrap(),
			"the winner",
			"a published tree survives another run's promotion"
		);
	}

	/// A run that is killed leaves its staging directory behind for good, so the
	/// next one sweeps whatever is old enough to belong to nobody.
	#[test]
	fn stale_staging_is_swept_and_live_staging_is_not() {
		let tmp = TempDir::new().unwrap();
		let stale = tmp.path().join(staging_name("deadbeef"));
		let promoted = tmp.path().join("1.2.3-deadbeef");
		std::fs::create_dir_all(&stale).unwrap();
		std::fs::create_dir_all(&promoted).unwrap();

		sweep_stale_staging(tmp.path(), Duration::ZERO);
		assert!(!stale.exists());
		assert!(promoted.is_dir(), "a promoted toolchain is not staging");

		let live = tmp.path().join(staging_name("deadbeef"));
		std::fs::create_dir_all(&live).unwrap();
		sweep_stale_staging(tmp.path(), STALE_STAGING);
		assert!(live.is_dir(), "a running install must not be swept");
	}

	/// Degrading to the inherited `PATH` would run whatever version of the tool
	/// the machine happens to have, which is the one thing a pin exists to
	/// prevent. A repo path holding a character `PATH` cannot carry has to fail.
	///
	/// The path never has to exist: building the value is what fails.
	#[test]
	fn a_path_that_cannot_be_built_fails_instead_of_dropping_the_pin() {
		let root = PathBuf::from(format!("repo{}one", lattice_testkit::unjoinable_char()));

		let err = prepend_to_path(&root).unwrap_err();
		let msg = format!("{err:#}");
		assert!(msg.contains("a character PATH cannot hold"), "{msg}");
	}

	/// The version check is the first thing that needs the pinned `PATH`, so
	/// provisioning stops there rather than reporting a tool it never ran.
	///
	/// Unix only, because the repo has to sit at such a path for this to be
	/// reachable, and the character Windows refuses in a `PATH` entry is also
	/// illegal in a Windows file name.
	#[test]
	#[cfg(unix)]
	fn provisioning_stops_when_the_pinned_path_cannot_be_built() {
		let tmp = TempDir::new().unwrap();
		let root = tmp
			.path()
			.join(format!("repo{}one", lattice_testkit::unjoinable_char()));
		std::fs::create_dir_all(&root).unwrap();

		let err = provision_and_resolve(&root, &fake_engines(), &mut |_| {}).unwrap_err();
		let msg = format!("{err:#}");
		assert!(msg.contains("cannot be put on PATH"), "{msg}");
	}

	/// `bin` is joined onto the toolchain directory, so an absolute one replaces
	/// it outright and `..` climbs out of `.lattice/toolchains` — either way the
	/// PATH prefix points somewhere Lattice never provisioned while the identity
	/// still claims a provisioned toolchain.
	#[test]
	fn a_bin_outside_the_toolchain_directory_is_refused() {
		let tmp = TempDir::new().unwrap();
		for bad in ["/usr/bin", "../../..", "bin/../../elsewhere"] {
			let engines = fake_engine(json!({
				"installCmd": lattice_testkit::install_fake_tool("faketool", "1.2.3"),
				"versionCmd": "faketool",
				"bin": bad
			}));
			let err = provision_and_resolve(tmp.path(), &engines, &mut |_| {}).unwrap_err();
			let msg = format!("{err:#}");
			assert!(msg.contains("relative path inside"), "bin '{bad}': {msg}");
		}
		assert!(
			!tmp.path().join(".lattice").exists(),
			"a refused bin installs nothing"
		);
	}
}
