//! Keeping an invocation honest about which version it is.
//!
//! `latticeVersion` in the nearest `lattice.json` is authoritative. When the
//! binary that was invoked from `.lattice/bin` is not that version — a branch
//! switch, a fresh clone, a colleague's bump — it installs the pinned one and
//! hands the invocation over to it, so nothing runs against a build the repo did
//! not ask for.
//!
//! Two properties keep this from being surprising. A binary outside
//! `.lattice/bin` is never replaced, so a `cargo install` build and the dev
//! symlink stay exactly what they are. And the pin is read straight out of the
//! JSON rather than through the config loader: a config written for a newer
//! schema must still be able to say which version can read it.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};

use crate::cli::BIN_VERSION;
use crate::release::{self, ReleaseUrls};

/// Set on the binary being handed to, naming the version it was chosen for. Its
/// presence stops a second hop.
///
/// This one stays an environment variable rather than becoming a flag: the
/// process being handed to is a *different* build of Lattice, and one older than
/// the flag would reject it outright. The environment crosses that gap.
const SWITCH_ENV: &str = "LATTICE_SWITCHED_FROM";

/// What this invocation should do about the pin.
#[derive(Debug, PartialEq, Eq)]
pub enum Drift {
	/// Run as invoked.
	Proceed,
	/// Install `version` if needed, then hand the invocation to it.
	SwitchTo(String),
}

/// Everything the decision depends on, so it can be made without touching the
/// filesystem, the environment, or the network.
pub struct DriftInputs<'a> {
	/// `latticeVersion` from the nearest `lattice.json`.
	pub pinned: Option<&'a str>,
	/// The running binary's version.
	pub bin: &'a str,
	/// Whether the running binary is one Lattice installed under `.lattice/bin`.
	pub managed: bool,
	/// Whether this process was already handed to by another one.
	pub switched: bool,
	/// Whether the check is turned off by flag, env, or `settings.versionCheck`.
	pub suppressed: bool,
}

pub fn decide(inputs: &DriftInputs) -> Drift {
	if inputs.suppressed || inputs.switched || !inputs.managed {
		return Drift::Proceed;
	}
	match inputs.pinned {
		Some(pinned) if pinned != inputs.bin => Drift::SwitchTo(pinned.to_string()),
		_ => Drift::Proceed,
	}
}

/// The pin and the opt-out read leniently from a `lattice.json` that may not be
/// valid to the running binary.
#[derive(Debug, PartialEq, Eq)]
pub struct Pin {
	pub version: Option<String>,
	pub version_check: bool,
}

pub fn parse_pin(text: &str) -> Pin {
	let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
		return Pin {
			version: None,
			version_check: true,
		};
	};
	Pin {
		version: value
			.get("latticeVersion")
			.and_then(|v| v.as_str())
			.map(str::to_string),
		version_check: value
			.get("settings")
			.and_then(|s| s.get("versionCheck"))
			.and_then(|v| v.as_bool())
			.unwrap_or(true),
	}
}

/// Whether `exe` is a binary Lattice installed for this repo. Both paths are
/// resolved first so the `.lattice/bin/lattice` symlink is judged by what it
/// points at: the dev symlink aims at `target/debug`, which is not managed.
fn is_managed(root: &Path, exe: &Path) -> bool {
	match (release::bin_dir(root).canonicalize(), exe.canonicalize()) {
		(Ok(bin_dir), Ok(exe)) => exe.starts_with(bin_dir),
		_ => false,
	}
}

/// Run the pinned version instead of this one when the repo asks for a different
/// one. Returns only when this binary should carry on.
pub fn honor_pin(root: &Path, no_version_check_flag: bool, urls: &ReleaseUrls) -> Result<()> {
	let text = match std::fs::read_to_string(root.join("lattice.json")) {
		Ok(text) => text,
		Err(_) => return Ok(()),
	};
	let pin = parse_pin(&text);
	let exe = std::env::current_exe().unwrap_or_default();

	let inputs = DriftInputs {
		pinned: pin.version.as_deref(),
		bin: BIN_VERSION,
		managed: is_managed(root, &exe),
		switched: std::env::var_os(SWITCH_ENV).is_some(),
		suppressed: no_version_check_flag
			|| std::env::var_os("LATTICE_NO_VERSION_CHECK").is_some()
			|| !pin.version_check,
	};

	let Drift::SwitchTo(version) = decide(&inputs) else {
		return Ok(());
	};

	eprintln!(
		"{}",
		lattice_output::switching_notice(BIN_VERSION, &version)
	);
	let pinned = release::ensure_installed(root, &version, urls, &mut |line| eprintln!("  {line}"))
		.with_context(|| {
			format!(
				"this repo pins lattice {version}. That version is not installed, and \
				 Lattice could not download it.\nRun with --no-version-check to use \
				 lattice {BIN_VERSION} instead"
			)
		})?;
	release::link_stable(root, &version)?;
	hand_over(&pinned, &version)
}

/// Replace this process with `bin`, passing the arguments through untouched.
#[cfg(unix)]
fn hand_over(bin: &Path, version: &str) -> Result<()> {
	use std::os::unix::process::CommandExt;

	let error = Command::new(bin)
		.args(std::env::args_os().skip(1))
		.env(SWITCH_ENV, version)
		.exec();
	Err(anyhow::Error::new(error).context(format!("failed to run {}", bin.display())))
}

/// Windows has no `exec`, so the pinned binary runs as a child and this process
/// exits with its status.
#[cfg(not(unix))]
fn hand_over(bin: &Path, version: &str) -> Result<()> {
	let status = Command::new(bin)
		.args(std::env::args_os().skip(1))
		.env(SWITCH_ENV, version)
		.status()
		.with_context(|| format!("failed to run {}", bin.display()))?;
	std::process::exit(status.code().unwrap_or(1));
}

/// The repo root for this invocation, or `None` when there is no `lattice.json`.
pub fn repo_root() -> Option<PathBuf> {
	let cwd = std::env::current_dir().ok()?;
	lattice_config::find_root(&cwd)
}

#[cfg(test)]
mod tests {
	use super::*;

	fn inputs() -> DriftInputs<'static> {
		DriftInputs {
			pinned: Some("0.2.0"),
			bin: "0.1.0",
			managed: true,
			switched: false,
			suppressed: false,
		}
	}

	#[test]
	fn drift_on_a_managed_binary_switches() {
		assert_eq!(decide(&inputs()), Drift::SwitchTo("0.2.0".to_string()));
	}

	#[test]
	fn matching_or_absent_pin_proceeds() {
		assert_eq!(
			decide(&DriftInputs {
				pinned: Some("0.1.0"),
				..inputs()
			}),
			Drift::Proceed
		);
		assert_eq!(
			decide(&DriftInputs {
				pinned: None,
				..inputs()
			}),
			Drift::Proceed
		);
	}

	#[test]
	fn an_unmanaged_binary_is_left_alone() {
		// A `cargo install` build or the dev symlink: the nag covers these.
		assert_eq!(
			decide(&DriftInputs {
				managed: false,
				..inputs()
			}),
			Drift::Proceed
		);
	}

	#[test]
	fn a_handed_to_process_never_hands_over_again() {
		assert_eq!(
			decide(&DriftInputs {
				switched: true,
				..inputs()
			}),
			Drift::Proceed
		);
	}

	#[test]
	fn suppression_wins() {
		assert_eq!(
			decide(&DriftInputs {
				suppressed: true,
				..inputs()
			}),
			Drift::Proceed
		);
	}

	#[test]
	fn pin_read_without_the_config_loader() {
		let pin =
			parse_pin(r#"{ "latticeVersion": "9.9.9", "settings": { "versionCheck": false } }"#);
		assert_eq!(pin.version.as_deref(), Some("9.9.9"));
		assert!(!pin.version_check);

		// Absent settings default to checking.
		let pin = parse_pin(r#"{ "latticeVersion": "1.0.0" }"#);
		assert_eq!(pin.version.as_deref(), Some("1.0.0"));
		assert!(pin.version_check);
	}

	#[test]
	fn an_unreadable_config_yields_no_pin_rather_than_an_error() {
		// The check runs before the config loader, so it must not turn a malformed
		// or unexpected config into a failure of its own.
		let pin = parse_pin("{ this is not json");
		assert_eq!(pin.version, None);
		assert!(pin.version_check);

		let pin = parse_pin(r#"{ "latticeVersion": 3 }"#);
		assert_eq!(pin.version, None);
	}
}
