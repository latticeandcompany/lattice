//! E2E tests for the toolchain-manager half: provisioning via `setup` and
//! per-task `PATH` injection during `run`. These use an object-form engine whose
//! `installCmd` drops a fake `sh` script, so no real toolchain or network is
//! needed. They are unix-only (the installCmd is a `sh` one-liner).

#![cfg(unix)]

mod common;

use common::Fixture;
use predicates::prelude::*;

/// The object-form `faketool` engine: its `installCmd` writes a tiny shell
/// script into `$LATTICE_TOOLCHAIN_DIR/bin` that prints a satisfying version.
const FAKETOOL_ENGINE: &str = r#"{
  "version": ">=1.2.0",
  "versionCmd": "faketool --version",
  "installCmd": "mkdir -p \"$LATTICE_TOOLCHAIN_DIR/bin\" && printf '#!/bin/sh\\necho faketool 1.2.3\\n' > \"$LATTICE_TOOLCHAIN_DIR/bin/faketool\" && chmod +x \"$LATTICE_TOOLCHAIN_DIR/bin/faketool\"",
  "bin": "bin"
}"#;

#[test]
fn setup_provisions_toolchain_only_repo() {
    let fx = Fixture::new();
    // No workspaces, no tasks: a pure toolchain-manager repo.
    fx.config(&format!(
        r#"{{
  "latticeVersion": "0.1.0",
  "engines": {{ "faketool": {FAKETOOL_ENGINE} }}
}}
"#
    ));

    fx.lattice()
        .args(["setup", "-l"])
        .assert()
        .success()
        .stdout(predicate::str::contains("setup complete"));

    // The provisioned tree carries the fake binary and its pin record.
    let files = fx.files_under(".lattice/toolchains/faketool");
    assert!(
        files.iter().any(|p| p.ends_with("bin/faketool")),
        "provisioned faketool binary is missing; found {files:?}"
    );
    assert!(
        files.iter().any(|p| p.ends_with("pins.json")),
        "pins.json is missing; found {files:?}"
    );
    // It lands under a `<version>-<hash>` dir (1.2.3 from the fake versionCmd).
    assert!(
        files
            .iter()
            .any(|p| p.to_string_lossy().contains("/1.2.3-")),
        "expected a 1.2.3-<hash> pin dir; found {files:?}"
    );
}

#[test]
fn path_injection_makes_provisioned_tool_available_during_run() {
    let fx = Fixture::new();
    fx.mkdir("app");
    // `faketool` is NOT on the host PATH; the build command is the bare tool
    // name, which only resolves because its provisioned bin dir is prepended to
    // PATH for the task.
    fx.config(&format!(
        r#"{{
  "latticeVersion": "0.1.0",
  "workspaces": [
    {{ "name": "app", "path": "app", "auto": false,
      "engines": {{ "faketool": {FAKETOOL_ENGINE} }},
      "scripts": {{ "build": "faketool --version" }} }}
  ],
  "tasks": {{ "build": {{}} }}
}}
"#
    ));

    fx.lattice()
        .args(["run", "build", "-l"])
        .assert()
        .success()
        .stdout(predicate::str::contains("faketool 1.2.3"))
        .stdout(predicate::str::contains("0 failed"));
}
