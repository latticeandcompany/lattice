//! E2E tests for the build-tool half and the standalone maintenance commands:
//! a build-only repo needing no toolchain machinery, cache `prune`, and shell
//! `completions`.

mod common;

use common::Fixture;
use predicates::prelude::*;

#[test]
fn build_only_repo_needs_no_toolchain_machinery() {
    let fx = Fixture::new();
    fx.mkdir("app");
    // Workspaces + tasks, but no `engines`: runs on the host PATH.
    fx.config(
        r#"{
  "latticeVersion": "0.1.0",
  "workspaces": [
    { "name": "app", "path": "app", "auto": false, "scripts": { "build": "echo built" } }
  ],
  "tasks": { "build": {} }
}
"#,
    );

    fx.lattice()
        .args(["run", "build", "-l"])
        .assert()
        .success()
        .stdout(predicate::str::contains("app:build: built"))
        .stdout(predicate::str::contains("0 failed"));

    assert!(
        !fx.exists(".lattice/toolchains"),
        "a repo with no engines must not create a toolchains dir"
    );
}

#[test]
fn prune_evicts_cached_artifacts() {
    let fx = Fixture::new();
    fx.write("app/src/f.txt", "hello\n");
    fx.config(
        r#"{
  "latticeVersion": "0.1.0",
  "workspaces": [
    { "name": "app", "path": "app", "auto": false,
      "scripts": { "build": "mkdir -p dist && echo hi > dist/out.txt" } }
  ],
  "tasks": { "build": { "inputs": ["src/**/*"], "outputs": ["dist/**/*"] } }
}
"#,
    );

    // Populate the cache.
    fx.lattice().args(["run", "build", "-l"]).assert().success();
    assert!(
        !fx.cache_tarballs().is_empty(),
        "a cached artifact should exist before prune"
    );

    // Prune to nothing: reports removal and empties the cache.
    fx.lattice()
        .args(["prune", "--max-size", "0B"])
        .assert()
        .success()
        .stdout(predicate::str::contains("removed"));

    assert!(
        fx.cache_tarballs().is_empty(),
        "prune --max-size 0B must remove every artifact"
    );
}

#[test]
fn completions_emit_nonempty_scripts() {
    let fx = Fixture::new();

    for shell in ["bash", "zsh"] {
        fx.lattice()
            .args(["completions", shell])
            .assert()
            .success()
            .stdout(predicate::str::contains("lattice"))
            .stdout(predicate::str::is_empty().not());
    }
}
