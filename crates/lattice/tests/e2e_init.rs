//! E2E tests for `lattice init`: scaffolding, the overwrite guard, idempotent
//! `.gitignore` maintenance, and the committed `.lattice/schema.json`.

mod common;

use common::Fixture;
use predicates::prelude::*;
use serde_json::Value;

#[test]
fn init_scaffolds_a_repo_and_runs_cleanly() {
    let fx = Fixture::new();

    fx.lattice().args(["init", "-y"]).assert().success();

    // The three artifacts are written.
    assert!(fx.exists("lattice.json"), "lattice.json written");
    assert!(
        fx.exists(".lattice/schema.json"),
        ".lattice/schema.json written"
    );
    assert!(fx.exists(".gitignore"), ".gitignore written");

    // The skeleton declares zero workspaces, so `run build` has nothing to do:
    // it must exit 0 (a fresh scaffold is not a failure) with a clean, actionable
    // notice and NEVER a panic. (The scaffolded config is valid and loads fine.)
    fx.lattice()
        .args(["run", "build", "-l"])
        .assert()
        .success()
        .stdout(predicate::str::contains("no workspaces declared"))
        .stderr(predicate::str::contains("panicked").not())
        .stderr(predicate::str::contains("RUST_BACKTRACE").not());
}

#[test]
fn init_force_guard() {
    let fx = Fixture::new();

    // First init succeeds.
    fx.lattice().args(["init", "-y"]).assert().success();

    // Second init without --force is rejected.
    fx.lattice()
        .args(["init", "-y"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));

    // With --force it overwrites and succeeds.
    fx.lattice()
        .args(["init", "-y", "--force"])
        .assert()
        .success();
}

#[test]
fn gitignore_lines_are_idempotent_across_reinits() {
    let fx = Fixture::new();

    fx.lattice().args(["init", "-y"]).assert().success();
    fx.lattice()
        .args(["init", "-y", "--force"])
        .assert()
        .success();

    let gi = fx.read(".gitignore");
    assert_eq!(
        gi.matches(".lattice/cache/").count(),
        1,
        "exactly one cache ignore line, got:\n{gi}"
    );
    assert_eq!(
        gi.matches(".lattice/toolchains/").count(),
        1,
        "exactly one toolchains ignore line, got:\n{gi}"
    );
}

#[test]
fn schema_json_is_committed_and_referenced() {
    let fx = Fixture::new();
    fx.lattice().args(["init", "-y"]).assert().success();

    // The committed schema exists and is a JSON Schema (draft key present).
    let schema: Value =
        serde_json::from_str(&fx.read(".lattice/schema.json")).expect("schema.json parses as JSON");
    let draft = schema
        .get("$schema")
        .and_then(Value::as_str)
        .expect("schema.json has a $schema draft key");
    assert!(
        draft.contains("json-schema.org"),
        "unexpected $schema draft: {draft}"
    );

    // The scaffolded config's own `$schema` points at the committed file.
    let config: Value =
        serde_json::from_str(&fx.read("lattice.json")).expect("lattice.json parses");
    assert_eq!(
        config.get("$schema").and_then(Value::as_str),
        Some(".lattice/schema.json"),
    );
}
