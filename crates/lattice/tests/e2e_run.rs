//! E2E tests for `lattice run`: caching, artifact verification, filtering,
//! dry-run, keep-going, and clean failure on an undefined task.

mod common;

use common::Fixture;
use predicates::prelude::*;

/// A single `auto:false` workspace whose `build` writes an output file, with a
/// task that declares inputs and outputs so the run is cacheable.
fn single_ws_repo(fx: &Fixture) {
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
}

#[test]
fn cold_then_cached_then_restores_outputs() {
    let fx = Fixture::new();
    single_ws_repo(&fx);

    // Cold run: executes, stores, reports 0 cached.
    fx.lattice()
        .args(["run", "build", "-l"])
        .assert()
        .success()
        .stdout(predicate::str::contains("app:build: done"))
        .stdout(predicate::str::contains("0 cached"));
    assert!(
        fx.exists("app/dist/out.txt"),
        "cold run produced the output"
    );

    // Warm run with identical inputs: cache hit, 1 cached, no re-run.
    fx.lattice()
        .args(["run", "build", "-l"])
        .assert()
        .success()
        .stdout(predicate::str::contains("app:build: cache hit ["))
        .stdout(predicate::str::contains("1 cached"))
        .stdout(predicate::str::contains("app:build: running").not());

    // Delete the outputs, then a third run must RESTORE them from cache.
    std::fs::remove_dir_all(fx.join("app/dist")).unwrap();
    assert!(!fx.exists("app/dist/out.txt"));
    fx.lattice()
        .args(["run", "build", "-l"])
        .assert()
        .success()
        .stdout(predicate::str::contains("app:build: cache hit ["))
        .stdout(predicate::str::contains("1 cached"));
    assert!(
        fx.exists("app/dist/out.txt"),
        "cache hit must restore the deleted output"
    );
}

#[test]
fn corrupt_tarball_is_a_miss_not_a_false_hit() {
    let fx = Fixture::new();
    single_ws_repo(&fx);

    // Prime the cache.
    fx.lattice().args(["run", "build", "-l"]).assert().success();

    // Corrupt every stored artifact so its digest no longer matches the meta.
    let tarballs = fx.cache_tarballs();
    assert!(!tarballs.is_empty(), "a cached tarball should exist");
    for t in &tarballs {
        std::fs::write(t, b"garbage").unwrap();
    }

    // The next run must NOT report a hit: it re-runs the task.
    fx.lattice()
        .args(["run", "build", "-l"])
        .assert()
        .success()
        .stdout(predicate::str::contains("app:build: running"))
        .stdout(predicate::str::contains("0 cached"))
        .stdout(predicate::str::contains("cache hit").not());
}

#[test]
fn filter_runs_only_matching_workspace() {
    let fx = Fixture::new();
    fx.mkdir("a");
    fx.mkdir("b");
    fx.config(
        r#"{
  "latticeVersion": "0.1.0",
  "workspaces": [
    { "name": "alpha", "path": "a", "auto": false, "scripts": { "build": "echo A > marker.txt" } },
    { "name": "beta",  "path": "b", "auto": false, "scripts": { "build": "echo B > marker.txt" } }
  ],
  "tasks": { "build": {} }
}
"#,
    );

    fx.lattice()
        .args(["run", "build", "-f", "alpha", "-l"])
        .assert()
        .success()
        .stdout(predicate::str::contains("alpha:build"))
        .stdout(predicate::str::contains("beta:build").not());

    assert!(fx.exists("a/marker.txt"), "filtered-in workspace ran");
    assert!(
        !fx.exists("b/marker.txt"),
        "filtered-out workspace did not run"
    );
}

#[test]
fn dry_run_lists_tasks_without_executing() {
    let fx = Fixture::new();
    fx.mkdir("a");
    fx.config(
        r#"{
  "latticeVersion": "0.1.0",
  "workspaces": [
    { "name": "alpha", "path": "a", "auto": false, "scripts": { "build": "echo A > marker.txt" } }
  ],
  "tasks": { "build": {} }
}
"#,
    );

    fx.lattice()
        .args(["run", "build", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("dry run"))
        .stdout(predicate::str::contains("alpha:build"));

    assert!(
        !fx.exists("a/marker.txt"),
        "dry run must not execute the task"
    );
}

#[test]
fn keep_going_runs_independent_and_reports_failure() {
    let fx = Fixture::new();
    fx.mkdir("a");
    fx.mkdir("b");
    fx.config(
        r#"{
  "latticeVersion": "0.1.0",
  "workspaces": [
    { "name": "good", "path": "a", "auto": false, "scripts": { "build": "echo GOOD-RAN" } },
    { "name": "bad",  "path": "b", "auto": false, "scripts": { "build": "echo boom >&2; exit 1" } }
  ],
  "tasks": { "build": {} }
}
"#,
    );

    // --no-cache keeps `good` running (so its output is emitted) on every run.
    fx.lattice()
        .args(["run", "build", "--continue", "--no-cache", "-l"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("good:build: GOOD-RAN"))
        .stdout(predicate::str::contains("1 failed"))
        .stderr(predicate::str::contains("bad:build: FAILED"));
}

#[test]
fn undefined_task_fails_cleanly() {
    let fx = Fixture::new();
    fx.mkdir("a");
    fx.config(
        r#"{
  "latticeVersion": "0.1.0",
  "workspaces": [
    { "name": "alpha", "path": "a", "auto": false, "scripts": { "build": "true" } }
  ],
  "tasks": { "build": {} }
}
"#,
    );

    fx.lattice()
        .args(["run", "nope"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not defined"))
        .stderr(predicate::str::contains("Available tasks"))
        .stderr(predicate::str::contains("build"))
        // A clean, actionable error — never a panic/backtrace.
        .stderr(predicate::str::contains("panicked").not())
        .stderr(predicate::str::contains("RUST_BACKTRACE").not());
}
