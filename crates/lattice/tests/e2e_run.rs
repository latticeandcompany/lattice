//! E2E tests for `lattice run`: caching, filtering, dry-run, stacked and
//! sequential phases, keep-going, an undefined task, and unknown config keys.
//!
//! A task's command goes to the platform shell, so a test that writes its task
//! body as a POSIX shell script only means what it says under `sh`. `cmd` has no
//! `;` separator, `mkdir -p` is not its `mkdir`, and `echo x >> f` appends a
//! trailing space. The handful that rely on any of that are marked `#[cfg(unix)]`
//! rather than rewritten twice over; what they cover is the runner's behavior,
//! which the rest of the suite exercises on both platforms.

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

	// Delete the outputs, then a third run must restore them from cache.
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

// `mkdir -p` in the task body.
#[cfg(unix)]
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

	// The next run must not report a hit: it re-runs the task.
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

/// `base <- mid <- top`, each `dependsOn` the previous, with `build` wired to
/// `^build` so every workspace waits on its dependencies.
fn chain_repo(fx: &Fixture) {
	for dir in ["base", "mid", "top"] {
		fx.mkdir(dir);
	}
	fx.config(
		r#"{
  "latticeVersion": "0.1.0",
  "workspaces": [
    { "name": "base", "path": "base", "auto": false, "scripts": { "build": "echo B > marker.txt" } },
    { "name": "mid",  "path": "mid",  "auto": false, "dependsOn": ["base"],
      "scripts": { "build": "echo M > marker.txt" } },
    { "name": "top",  "path": "top",  "auto": false, "dependsOn": ["mid"],
      "scripts": { "build": "echo T > marker.txt" } }
  ],
  "tasks": { "build": { "dependsOn": ["^build"] } }
}
"#,
	);
}

#[test]
fn filter_pulls_in_transitive_dependencies() {
	let fx = Fixture::new();
	chain_repo(&fx);

	fx.lattice()
		.args(["run", "build", "-f", "top", "-l"])
		.assert()
		.success()
		.stdout(predicate::str::contains("across 3 workspace(s)"))
		.stdout(predicate::str::contains("base:build"))
		.stdout(predicate::str::contains("mid:build"))
		.stdout(predicate::str::contains("top:build"));

	assert!(fx.exists("base/marker.txt"), "a transitive dependency ran");
	assert!(fx.exists("mid/marker.txt"), "a direct dependency ran");
	assert!(fx.exists("top/marker.txt"), "the matched workspace ran");
}

#[test]
fn filter_excludes_workspaces_that_depend_on_the_match() {
	let fx = Fixture::new();
	chain_repo(&fx);

	fx.lattice()
		.args(["run", "build", "-f", "mid", "-l"])
		.assert()
		.success()
		.stdout(predicate::str::contains("top:build").not());

	assert!(fx.exists("base/marker.txt"), "mid's dependency ran");
	assert!(fx.exists("mid/marker.txt"), "the matched workspace ran");
	assert!(
		!fx.exists("top/marker.txt"),
		"a workspace that only depends on the match stays out of the run"
	);
}

#[test]
fn filtered_dry_run_marks_pulled_in_dependencies() {
	let fx = Fixture::new();
	chain_repo(&fx);

	fx.lattice()
		.args(["run", "build", "--dry-run", "-f", "top"])
		.assert()
		.success()
		.stdout(predicate::str::contains("base:build (dependency)"))
		.stdout(predicate::str::contains("mid:build (dependency)"))
		.stdout(predicate::str::contains("top:build (dependency)").not());
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
fn stacked_tasks_run_as_one_graph() {
	let fx = Fixture::new();
	fx.mkdir("a");
	// test dependsOn build; lint is independent. Stacking all three should run
	// build once (before test) and lint too.
	fx.config(
		r#"{
  "latticeVersion": "0.1.0",
  "workspaces": [
    { "name": "app", "path": "a", "auto": false, "scripts": {
      "lint": "echo LINTED > lint.txt",
      "build": "echo BUILT > build.txt",
      "test": "echo TESTED > test.txt"
    } }
  ],
  "tasks": {
    "lint": {},
    "build": {},
    "test": { "dependsOn": ["build"] }
  }
}
"#,
	);

	fx.lattice()
		.args(["run", "lint", "test", "build", "-l"])
		.assert()
		.success()
		.stdout(predicate::str::contains("app:lint: done"))
		.stdout(predicate::str::contains("app:build: done"))
		.stdout(predicate::str::contains("app:test: done"));

	assert!(fx.exists("a/lint.txt"), "lint ran");
	assert!(fx.exists("a/build.txt"), "build ran");
	assert!(fx.exists("a/test.txt"), "test ran");
}

#[test]
fn stacked_dry_run_lists_all_tasks() {
	let fx = Fixture::new();
	fx.mkdir("a");
	fx.config(
		r#"{
  "latticeVersion": "0.1.0",
  "workspaces": [
    { "name": "app", "path": "a", "auto": false, "scripts": {
      "lint": "echo L", "build": "echo B" } }
  ],
  "tasks": { "lint": {}, "build": {} }
}
"#,
	);

	fx.lattice()
		.args(["run", "lint", "build", "--dry-run"])
		.assert()
		.success()
		.stdout(predicate::str::contains("dry run · lint build"))
		.stdout(predicate::str::contains("app:lint"))
		.stdout(predicate::str::contains("app:build"));
}

// `echo x >> f` appends a trailing space under `cmd`, which the order assertion
// would then read as a different word.
#[cfg(unix)]
#[test]
fn sequentially_runs_each_task_phase_in_order() {
	let fx = Fixture::new();
	fx.mkdir("a");
	// Each task appends a marker; --sequentially must run them strictly in the
	// listed order (lint, then test — which drags in its build dep — then build).
	fx.config(
		r#"{
  "latticeVersion": "0.1.0",
  "workspaces": [
    { "name": "app", "path": "a", "auto": false, "scripts": {
      "lint": "echo lint >> order.txt",
      "build": "echo build >> order.txt",
      "test": "echo test >> order.txt"
    } }
  ],
  "tasks": {
    "lint": {},
    "build": {},
    "test": { "dependsOn": ["build"] }
  }
}
"#,
	);

	fx.lattice()
		.args(["run", "lint", "test", "-s", "-l"])
		.assert()
		.success();

	// Phase order: lint (phase 1), then build→test (phase 2). build appears
	// before test because test dependsOn build.
	let order = std::fs::read_to_string(fx.join("a/order.txt")).unwrap();
	let lines: Vec<&str> = order.lines().collect();
	assert_eq!(lines, vec!["lint", "build", "test"]);
}

#[test]
fn sequentially_stops_at_first_failed_phase() {
	let fx = Fixture::new();
	fx.mkdir("a");
	// lint fails; --sequentially fail-fast must not reach the build phase.
	fx.config(
		r#"{
  "latticeVersion": "0.1.0",
  "workspaces": [
    { "name": "app", "path": "a", "auto": false, "scripts": {
      "lint": "exit 1",
      "build": "echo built > build.txt"
    } }
  ],
  "tasks": { "lint": {}, "build": {} }
}
"#,
	);

	fx.lattice()
		.args(["run", "lint", "build", "-s", "-l"])
		.assert()
		.failure();

	assert!(
		!fx.exists("a/build.txt"),
		"a failed earlier phase must stop the run before the build phase"
	);
}

#[test]
fn sequentially_dry_run_lists_each_phase() {
	let fx = Fixture::new();
	fx.mkdir("a");
	fx.config(
		r#"{
  "latticeVersion": "0.1.0",
  "workspaces": [
    { "name": "app", "path": "a", "auto": false, "scripts": {
      "lint": "echo L", "build": "echo B" } }
  ],
  "tasks": { "lint": {}, "build": {} }
}
"#,
	);

	fx.lattice()
		.args(["run", "lint", "build", "--sequentially", "--dry-run"])
		.assert()
		.success()
		.stdout(predicate::str::contains("dry run · lint (phase)"))
		.stdout(predicate::str::contains("dry run · build (phase)"));
}

// POSIX shell in the task bodies.
#[cfg(unix)]
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

/// The typo case: `persistent: true` on a command that exits straight away. The
/// run has to end on its own — no signal is ever sent here — and report it.
// `;` as a command separator, and `>&2`, in the task body.
#[cfg(unix)]
#[test]
fn persistent_task_that_exits_immediately_is_reported() {
	let fx = Fixture::new();
	fx.mkdir("a");
	fx.config(
		r#"{
  "latticeVersion": "0.1.0",
  "workspaces": [
    { "name": "app", "path": "a", "auto": false,
      "scripts": { "dev": "echo port already in use >&2; exit 1" } }
  ],
  "tasks": { "dev": { "persistent": true } }
}
"#,
	);

	fx.lattice()
		.args(["run", "dev"])
		.timeout(std::time::Duration::from_secs(30))
		.assert()
		.failure()
		.stdout(predicate::str::contains("1 failed"))
		.stderr(predicate::str::contains("app:dev: EXITED (code 1)"));
}

#[test]
fn persistent_task_that_finishes_cleanly_says_so() {
	let fx = Fixture::new();
	fx.mkdir("a");
	fx.config(
		r#"{
  "latticeVersion": "0.1.0",
  "workspaces": [
    { "name": "app", "path": "a", "auto": false, "scripts": { "dev": "true" } }
  ],
  "tasks": { "dev": { "persistent": true } }
}
"#,
	);

	fx.lattice()
		.args(["run", "dev"])
		.timeout(std::time::Duration::from_secs(30))
		.assert()
		.success()
		.stdout(predicate::str::contains("app:dev: exited (code 0)"))
		.stdout(predicate::str::contains("0 failed"));
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
		.stderr(predicate::str::contains("available tasks"))
		.stderr(predicate::str::contains("build"))
		// An error, not a panic or backtrace.
		.stderr(predicate::str::contains("panicked").not())
		.stderr(predicate::str::contains("RUST_BACKTRACE").not());
}

/// An unknown key stops the run at load time. A typo'd `outputs` would otherwise
/// decide what the task caches.
#[test]
fn unknown_top_level_key_fails_before_anything_runs() {
	let fx = Fixture::new();
	fx.mkdir("a");
	fx.config(
		r#"{
  "latticeVersion": "0.1.0",
  "projects": {},
  "workspaces": [
    { "name": "alpha", "path": "a", "auto": false, "scripts": { "build": "true" } }
  ],
  "tasks": { "build": {} }
}
"#,
	);

	fx.lattice()
		.args(["run", "build"])
		.assert()
		.failure()
		.stderr(predicate::str::contains("unknown field `projects`"))
		.stderr(predicate::str::contains("at the top level of lattice.json"))
		.stderr(predicate::str::contains("line 3"))
		.stderr(predicate::str::contains("Fields accepted here:"))
		.stderr(predicate::str::contains("panicked").not());
}

/// The message places the key in the task it was written in and names the field
/// it was probably meant to be.
#[test]
fn a_misspelled_outputs_key_names_the_task_and_the_fix() {
	let fx = Fixture::new();
	fx.mkdir("a");
	fx.config(
		r#"{
  "latticeVersion": "0.1.0",
  "workspaces": [
    { "name": "alpha", "path": "a", "auto": false, "scripts": { "build": "true" } }
  ],
  "tasks": { "build": { "output": ["dist/**"] } }
}
"#,
	);

	fx.lattice()
		.args(["run", "build"])
		.assert()
		.failure()
		.stderr(predicate::str::contains(
			"unknown field `output` in tasks.build",
		))
		.stderr(predicate::str::contains("Did you mean `outputs`?"));
}

/// Workspace entries are indexed, so the message points at one of several.
#[test]
fn an_unknown_workspace_key_indexes_the_entry() {
	let fx = Fixture::new();
	fx.mkdir("a");
	fx.mkdir("b");
	fx.config(
		r#"{
  "latticeVersion": "0.1.0",
  "workspaces": [
    { "name": "alpha", "path": "a", "auto": false, "scripts": { "build": "true" } },
    { "name": "beta", "path": "b", "auto": false, "glob": "b/*" }
  ],
  "tasks": { "build": {} }
}
"#,
	);

	fx.lattice()
		.args(["run", "build"])
		.assert()
		.failure()
		.stderr(predicate::str::contains(
			"unknown field `glob` in workspaces[1]",
		));
}
