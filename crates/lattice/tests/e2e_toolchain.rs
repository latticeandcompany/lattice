//! E2E tests for the toolchain-manager half: provisioning via `setup` and
//! per-task `PATH` injection during `run`. These use an object-form engine whose
//! `installCmd` drops a stand-in tool, so no real toolchain or network is needed.
//!
//! The `installCmd` comes from `lattice_testkit`, which writes a shell script on
//! unix and a `.cmd` on Windows — so this covers provisioning on the platform
//! where the shell and `PATH` handling differ, which is the half most likely to
//! be written unix-only by accident.

mod common;

use common::Fixture;
use lattice_testkit as sh;
use predicates::prelude::*;

/// An object-form engine whose `installCmd` provisions a stand-in `faketool`
/// that prints a version satisfying the constraint.
fn faketool_engine() -> String {
	format!(
		r#"{{
  "version": ">=1.2.0",
  "versionCmd": "faketool --version",
  "installCmd": {},
  "bin": "bin"
}}"#,
		sh::json(&sh::install_fake_tool("faketool", "1.2.3"))
	)
}

#[test]
fn setup_provisions_toolchain_only_repo() {
	let fx = Fixture::new();
	// No workspaces, no tasks: a pure toolchain-manager repo.
	fx.config(&format!(
		r#"{{
  "latticeVersion": "0.1.0",
  "engines": {{ "faketool": {} }}
}}
"#,
		faketool_engine()
	));

	fx.lattice()
		.args(["setup", "-l"])
		.assert()
		.success()
		.stdout(predicate::str::contains("setup complete"));

	// The provisioned tree carries the stand-in tool and its pin record.
	let files = fx.files_under(".lattice/toolchains/faketool");
	let tool = sh::fake_tool_file("faketool");
	assert!(
		files
			.iter()
			.any(|p| p.file_name().is_some_and(|n| n == tool.as_str())
				&& p.parent().is_some_and(|d| d.ends_with("bin"))),
		"provisioned {tool} is missing from bin/; found {files:?}"
	);
	assert!(
		files.iter().any(|p| p.ends_with("pins.json")),
		"pins.json is missing; found {files:?}"
	);
	// It lands under a `<version>-<hash>` dir (1.2.3 from the stand-in versionCmd).
	assert!(
		files.iter().any(|p| p
			.components()
			.any(|c| c.as_os_str().to_string_lossy().starts_with("1.2.3-"))),
		"expected a 1.2.3-<hash> pin dir; found {files:?}"
	);
}

#[test]
fn path_injection_makes_provisioned_tool_available_during_run() {
	let fx = Fixture::new();
	fx.mkdir("app");
	// `faketool` is not on the host PATH; the build command is the bare tool
	// name, which only resolves because its provisioned bin dir is prepended to
	// PATH for the task.
	fx.config(&format!(
		r#"{{
  "latticeVersion": "0.1.0",
  "workspaces": [
    {{ "name": "app", "path": "app", "auto": false,
      "engines": {{ "faketool": {} }},
      "scripts": {{ "build": "faketool --version" }} }}
  ],
  "tasks": {{ "build": {{}} }}
}}
"#,
		faketool_engine()
	));

	fx.lattice()
		.args(["run", "build", "-l"])
		.assert()
		.success()
		.stdout(predicate::str::contains("faketool 1.2.3"))
		.stdout(predicate::str::contains("0 failed"));
}
