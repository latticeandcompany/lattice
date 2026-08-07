//! The engine gradient: host / validate / provision.
//!
//! The developer declares an `installCmd`; Lattice runs it into a
//! content-addressed toolchain directory under `./.lattice/toolchains/`,
//! version-checks the result, pins it, and hands the runner a `PATH` prefix
//! that activates it for the duration of a single task.

use std::path::{Path, PathBuf};
use std::process::Command;

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
pub struct ResolvedToolchains {
	/// Directories to prepend to a task's `PATH`, in order.
	pub path_prepend: Vec<PathBuf>,
	/// A stable identity string over `(engine, version, installHash)`, sorted.
	pub identity: String,
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
	if cfg!(windows) {
		let mut c = Command::new("cmd");
		c.arg("/C").arg(command);
		c
	} else {
		let mut c = Command::new("sh");
		c.arg("-c").arg(command);
		c
	}
}

/// Prepend `dir` to a `PATH` value using the platform's separator.
fn prepend_to_path(dir: &Path) -> std::ffi::OsString {
	let existing = std::env::var_os("PATH").unwrap_or_default();
	let mut paths: Vec<PathBuf> = vec![dir.to_path_buf()];
	paths.extend(std::env::split_paths(&existing));
	std::env::join_paths(paths).unwrap_or(existing)
}

/// Run a command string through the platform shell, optionally prepending
/// `extra_path` to `PATH` and setting extra env. Returns combined stdout+stderr.
fn run_capture(
	cmd: &str,
	extra_path: Option<&Path>,
	extra_env: &[(String, String)],
) -> Result<(bool, String)> {
	let mut command = shell_command(cmd);
	if let Some(p) = extra_path {
		command.env("PATH", prepend_to_path(p));
	}
	for (k, v) in extra_env {
		command.env(k, v);
	}
	let out = command
		.output()
		.with_context(|| format!("failed to spawn `{cmd}`"))?;
	let mut combined = String::from_utf8_lossy(&out.stdout).into_owned();
	combined.push_str(&String::from_utf8_lossy(&out.stderr));
	Ok((out.status.success(), combined))
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
		let bin_dir = dir.join(&pins.bin);
		if !bin_dir.is_dir() {
			continue;
		}
		// Re-verify the version if we can; this does not reinstall.
		if let (Some(vc), Some(cons)) = (version_cmd, constraint) {
			if let Ok((ok, out)) = run_capture(vc, Some(&bin_dir), &[]) {
				if !ok {
					continue;
				}
				match parse_version(&out) {
					Some(v) if satisfies(&v, cons) => {}
					_ => continue,
				}
			}
		}
		return Some((dir, pins));
	}
	None
}

/// Provision (if needed) and resolve every engine in `engines` into a `PATH`
/// prefix + cache identity. Memoized by content hash: identical toolchains
/// install once.
pub fn provision_and_resolve(
	root: &Path,
	engines: &EngineMap,
	log: &mut dyn FnMut(&str),
) -> Result<ResolvedToolchains> {
	let mut path_prepend = Vec::new();
	let mut identity_parts: Vec<String> = Vec::new();

	for (name, spec) in engines {
		match classify(name, spec) {
			EngineMode::HostPath => {
				identity_parts.push(format!("{name}=host"));
			}
			EngineMode::ValidateOnly { constraint } => {
				let vc = version_cmd_for(name, spec);
				let version = if let Some(vc) = &vc {
					let (ok, out) = run_capture(vc, None, &[])?;
					if !ok {
						bail!(
							"engine '{name}': version command `{vc}` failed:\n{}",
							out.trim()
						);
					}
					match parse_version(&out) {
						Some(v) => v,
						None => bail!(
							"engine '{name}': could not parse version from `{vc}` output: {}",
							out.trim()
						),
					}
				} else {
					bail!(
						"engine '{name}' has a version constraint but no way to check it \
                         (not a well-known engine and no `versionCmd`)"
					);
				};
				if let Some(cons) = &constraint {
					if !satisfies(&version, cons) {
						bail!(
                            "engine '{name}' {version} on PATH does not satisfy constraint '{cons}'"
                        );
					}
				}
				identity_parts.push(format!("{name}={version}@host"));
			}
			EngineMode::Provisioned {
				install_cmd,
				version_cmd,
				constraint,
				bin,
			} => {
				let hash = install_hash8(&install_cmd);
				let engine_dir = root.join(".lattice").join("toolchains").join(name);

				// Reuse an existing verified pin (install once).
				if let Some((dir, pins)) = find_verified_pin(
					&engine_dir,
					&hash,
					constraint.as_deref(),
					version_cmd.as_deref(),
				) {
					path_prepend.push(dir.join(&pins.bin));
					identity_parts.push(format!("{name}={}@{hash}", pins.version));
					continue;
				}

				std::fs::create_dir_all(&engine_dir).with_context(|| {
					format!("failed to create toolchain dir {}", engine_dir.display())
				})?;
				let target = engine_dir.join(format!("tmp-{hash}"));
				if target.exists() {
					std::fs::remove_dir_all(&target).ok();
				}
				std::fs::create_dir_all(&target)?;

				log(&format!(
					"provisioning engine '{name}' via installCmd into {}",
					target.display()
				));

				// $LATTICE_TOOLCHAIN_DIR: set as env and literal-substituted so
				// both `... $LATTICE_TOOLCHAIN_DIR ...` and env-var reads work.
				let target_str = target.to_string_lossy().into_owned();
				let substituted = install_cmd.replace("$LATTICE_TOOLCHAIN_DIR", &target_str);
				let env = vec![("LATTICE_TOOLCHAIN_DIR".to_string(), target_str.clone())];
				let (ok, out) = run_capture(&substituted, None, &env)?;
				if !ok {
					bail!("engine '{name}': installCmd failed:\n{}", out.trim());
				}

				let tmp_bin = target.join(&bin);

				// Version-check the freshly installed tool.
				let version = if let Some(vc) = &version_cmd {
					let (ok, out) = run_capture(vc, Some(&tmp_bin), &[])?;
					if !ok {
						bail!(
							"engine '{name}': version command `{vc}` failed after install:\n{}",
							out.trim()
						);
					}
					match parse_version(&out) {
						Some(v) => {
							if let Some(cons) = &constraint {
								if !satisfies(&v, cons) {
									bail!(
										"engine '{name}' provisioned {v} does not satisfy '{cons}'"
									);
								}
							}
							v.to_string()
						}
						None => "0.0.0".to_string(),
					}
				} else {
					"0.0.0".to_string()
				};

				// Move into the content-addressed, versioned final dir.
				let final_dir = engine_dir.join(format!("{version}-{hash}"));
				if final_dir.exists() {
					std::fs::remove_dir_all(&final_dir).ok();
				}
				std::fs::rename(&target, &final_dir).with_context(|| {
					format!(
						"failed to move toolchain into place: {} -> {}",
						target.display(),
						final_dir.display()
					)
				})?;

				let pins = ToolchainPins {
					engine: name.clone(),
					version: version.clone(),
					install_hash: hash.clone(),
					bin: bin.clone(),
				};
				std::fs::write(
					final_dir.join("pins.json"),
					serde_json::to_string_pretty(&pins)?,
				)?;

				path_prepend.push(final_dir.join(&bin));
				identity_parts.push(format!("{name}={version}@{hash}"));
			}
		}
	}

	identity_parts.sort();
	Ok(ResolvedToolchains {
		path_prepend,
		identity: identity_parts.join(";"),
	})
}

#[cfg(test)]
mod tests {
	use super::*;
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
		serde_json::from_value(json!({
			"faketool": {
				"version": ">=1.0.0",
				"installCmd": lattice_testkit::install_fake_tool("faketool", "1.2.3"),
				"versionCmd": "faketool",
				"bin": "bin"
			}
		}))
		.unwrap()
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
}
