//! E2E tests for `lattice run`: caching, filtering, dry-run, stacked and
//! sequential phases, keep-going, an undefined task, and unknown config keys.
//!
//! Task bodies come from `lattice_testkit`, which spells them for whichever shell
//! will run them, so every case here covers the same behavior on both platforms.

mod common;

use common::Fixture;
use lattice_testkit as sh;
use predicates::prelude::*;

/// A single workspace whose written-out `build` writes an output file, with a
/// task that declares inputs and outputs so the run is cacheable.
fn single_ws_repo(fx: &Fixture) {
	fx.write("app/src/f.txt", "hello\n");
	fx.config_from(
		r#"{
		    "latticeVersion": "0.1.0",
		    "workspaces": ["app"],
		    "tasks": { "build": { "inputs": ["src/**/*"], "outputs": ["dist/**/*"] } },
		    "overrides": { "app": { "tasks": { "build": @build@ } } }
		}"#,
		&[(
			"build",
			sh::all([sh::mkdirs("dist"), sh::write("dist/out.txt", "hi")]),
		)],
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

/// `ui` has no `build`, so `web:build` has no prerequisite in the graph to carry
/// an edit to `ui` into its key.
#[test]
fn an_edit_in_a_dependency_without_the_task_still_misses() {
	let fx = Fixture::new();
	fx.write("ui/src/button.txt", "v1\n");
	fx.write("web/src/app.txt", "app\n");
	fx.config_from(
		r#"{
		    "latticeVersion": "0.1.0",
		    "workspaces": ["ui", "web"],
		    "tasks": { "build": { "outputs": ["out.txt"] } },
		    "overrides": {
		      "ui": { "tasks": { "lint": @lint@ } },
		      "web": { "dependsOn": ["ui"], "tasks": { "build": @build@ } }
		    }
		}"#,
		&[
			("build", sh::write("out.txt", "built")),
			("lint", sh::write("lint.txt", "ok")),
		],
	);

	fx.lattice().args(["run", "build", "-l"]).assert().success();
	fx.lattice()
		.args(["run", "build", "-l"])
		.assert()
		.success()
		.stdout(predicate::str::contains("web:build: cache hit ["));

	fx.write("ui/src/button.txt", "v2\n");
	fx.lattice()
		.args(["run", "build", "-l"])
		.assert()
		.success()
		.stdout(predicate::str::contains("web:build: cache hit [").not())
		.stdout(predicate::str::contains("0 cached"));
}

/// Rust ignores SIGPIPE, so a write to a pipe whose reader left returns an error,
/// and `println!` turns that into a panic that ends the run with its tasks alive.
#[test]
fn a_reader_that_goes_away_does_not_crash_the_run() {
	let fx = Fixture::new();
	fx.write("app/src.txt", "x");
	fx.config_from(
		r#"{
		    "latticeVersion": "0.1.0",
		    "workspaces": ["app"],
		    "overrides": { "app": { "tasks": { "build": @build@ } } }
		}"#,
		&[(
			"build",
			sh::all([sh::sleep(1), sh::echo("one"), sh::echo("two")]),
		)],
	);

	let mut child = std::process::Command::new(assert_cmd::cargo::cargo_bin("lattice"))
		.args(["run", "build"])
		.current_dir(fx.root())
		.env("CI", "1")
		.env("NO_COLOR", "1")
		.stdout(std::process::Stdio::piped())
		.stderr(std::process::Stdio::piped())
		.spawn()
		.expect("spawn lattice");
	drop(child.stdout.take());
	let out = child.wait_with_output().expect("wait for lattice");
	let stderr = String::from_utf8_lossy(&out.stderr);
	assert!(out.status.success(), "{:?}: {stderr}", out.status);
	assert!(!stderr.contains("panicked"), "{stderr}");
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
	fx.config_from(
		r#"{
		    "latticeVersion": "0.1.0",
		    "workspaces": ["a", "b"],
		    "tasks": { "build": {} },
		    "overrides": {
		      "./a": { "name": "alpha", "tasks": { "build": @a@ } },
		      "./b": { "name": "beta", "tasks": { "build": @b@ } }
		    }
		}"#,
		&[
			("a", sh::write("marker.txt", "A")),
			("b", sh::write("marker.txt", "B")),
		],
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
	fx.config_from(
		r#"{
		    "latticeVersion": "0.1.0",
		    "workspaces": ["base", "mid", "top"],
		    "tasks": { "build": { "dependsOn": ["^build"] } },
		    "overrides": {
		      "base": { "tasks": { "build": @base@ } },
		      "mid": { "dependsOn": ["base"], "tasks": { "build": @mid@ } },
		      "top": { "dependsOn": ["mid"], "tasks": { "build": @top@ } }
		    }
		}"#,
		&[
			("base", sh::write("marker.txt", "B")),
			("mid", sh::write("marker.txt", "M")),
			("top", sh::write("marker.txt", "T")),
		],
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
		.stdout(predicate::str::contains("across 3 workspaces"))
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
fn a_run_from_inside_a_workspace_without_the_task_fails() {
	let fx = Fixture::new();
	chain_repo(&fx);
	fx.mkdir("mid/src");

	fx.lattice()
		.current_dir(fx.join("mid/src"))
		.args(["run", "lint"])
		.assert()
		.failure()
		.stderr(predicate::str::contains("no workspace has a 'lint' task"));

	let fx = Fixture::new();
	fx.mkdir("docs");
	fx.mkdir("app");
	fx.config_from(
		r#"{
		    "latticeVersion": "0.1.0",
		    "workspaces": ["docs", "app"],
		    "overrides": {
		      "docs": { "tasks": { "build": @ok@ } },
		      "app": { "tasks": { "test": @ok@ } }
		    }
		}"#,
		&[("ok", sh::succeed())],
	);
	fx.lattice()
		.current_dir(fx.join("docs"))
		.args(["run", "test"])
		.assert()
		.failure()
		.stderr(predicate::str::contains(
			"docs, the workspace you are in, has no 'test' task",
		));
}

/// A run started inside a workspace is for that workspace, and still builds
/// what it depends on, the way cargo scopes itself to the member it is run in.
#[test]
fn a_run_from_inside_a_workspace_is_scoped_to_it() {
	let fx = Fixture::new();
	chain_repo(&fx);
	fx.mkdir("mid/src");

	fx.lattice()
		.current_dir(fx.join("mid/src"))
		.args(["run", "build"])
		.assert()
		.success();

	assert!(fx.exists("base/marker.txt"), "mid's dependency ran");
	assert!(
		fx.exists("mid/marker.txt"),
		"the workspace the run started in ran"
	);
	assert!(
		!fx.exists("top/marker.txt"),
		"a workspace outside the one the run started in stays out"
	);
}

/// Arguments after `--` belong to the task that was asked for, not to the
/// builds it pulls in on the way.
#[test]
fn arguments_after_a_double_dash_reach_only_the_requested_task() {
	let fx = Fixture::new();
	for dir in ["lib", "app"] {
		fx.mkdir(dir);
	}
	fx.config_from(
		r#"{
		    "workspaces": ["lib", "app"],
		    "tasks": { "test": { "dependsOn": ["build"] } },
		    "overrides": {
		      "lib": { "tasks": { "build": @build@ } },
		      "app": { "dependsOn": ["lib"], "tasks": { "build": @build@, "test": @test@ } }
		    }
		}"#,
		&[("build", sh::succeed()), ("test", sh::echo("tested"))],
	);

	let out = fx
		.lattice()
		.args(["run", "test", "--dry-run", "--", "--only", "fast"])
		.assert()
		.success();
	let stdout = String::from_utf8_lossy(&out.get_output().stdout).to_string();
	let line = |label: &str| {
		stdout
			.lines()
			.find(|l| l.contains(label))
			.unwrap_or_else(|| panic!("no {label} line:\n{stdout}"))
			.to_string()
	};
	assert!(line("app:test").ends_with("--only fast"), "{stdout}");
	assert!(!line("app:build").contains("--only"), "{stdout}");
	assert!(!line("lib:build").contains("--only"), "{stdout}");
}

/// A task no workspace defines is refused before anything runs, and a near
/// miss is named.
#[test]
fn a_task_no_workspace_has_suggests_the_one_it_nearly_matches() {
	let fx = Fixture::new();
	chain_repo(&fx);
	fx.lattice()
		.args(["run", "biuld"])
		.assert()
		.failure()
		.stderr(predicate::str::contains("no workspace has a 'biuld' task"))
		.stderr(predicate::str::contains("Did you mean 'build'?"));
	assert!(!fx.exists("base/marker.txt"));
}

/// An override for a workspace that does not exist would change nothing and
/// say nothing, which is what a typo in a key looks like.
#[test]
fn an_override_that_matches_no_workspace_is_refused() {
	let fx = Fixture::new();
	chain_repo(&fx);
	let config = fx.read("lattice.json").replacen(
		r#""overrides": {"#,
		r#""overrides": { "tpo": { "dependsOn": ["base"] },"#,
		1,
	);
	fx.config(&config);

	fx.lattice()
		.args(["run", "build", "--dry-run"])
		.assert()
		.failure()
		.stderr(predicate::str::contains("'tpo' names no workspace"))
		.stderr(predicate::str::contains("Did you mean `top`?"));
}

#[test]
fn dry_run_lists_tasks_without_executing() {
	let fx = Fixture::new();
	fx.mkdir("a");
	fx.config_from(
		r#"{
		    "latticeVersion": "0.1.0",
		    "workspaces": ["a"],
		    "tasks": { "build": {} },
		    "overrides": { "./a": { "name": "alpha", "tasks": { "build": @a@ } } }
		}"#,
		&[("a", sh::write("marker.txt", "A"))],
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
	fx.config_from(
		r#"{
		    "latticeVersion": "0.1.0",
		    "workspaces": ["a"],
		    "tasks": { "lint": {}, "build": {}, "test": { "dependsOn": ["build"] } },
		    "overrides": {
		      "./a": {
		        "name": "app",
		        "tasks": { "lint": @lint@, "build": @build@, "test": @test@ }
		      }
		    }
		}"#,
		&[
			("lint", sh::write("lint.txt", "LINTED")),
			("build", sh::write("build.txt", "BUILT")),
			("test", sh::write("test.txt", "TESTED")),
		],
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
	fx.config_from(
		r#"{
		    "latticeVersion": "0.1.0",
		    "workspaces": ["a"],
		    "tasks": { "lint": {}, "build": {} },
		    "overrides": { "./a": { "name": "app", "tasks": { "lint": @lint@, "build": @build@ } } }
		}"#,
		&[("lint", sh::echo("L")), ("build", sh::echo("B"))],
	);

	fx.lattice()
		.args(["run", "lint", "build", "--dry-run"])
		.assert()
		.success()
		.stdout(predicate::str::contains("dry run · lint build"))
		.stdout(predicate::str::contains("app:lint"))
		.stdout(predicate::str::contains("app:build"));
}

#[test]
fn sequentially_runs_each_task_phase_in_order() {
	let fx = Fixture::new();
	fx.mkdir("a");
	// Each task appends a marker; --sequentially must run them strictly in the
	// listed order (lint, then test — which drags in its build dep — then build).
	fx.config_from(
		r#"{
		    "latticeVersion": "0.1.0",
		    "workspaces": ["a"],
		    "tasks": { "lint": {}, "build": {}, "test": { "dependsOn": ["build"] } },
		    "overrides": {
		      "./a": {
		        "name": "app",
		        "tasks": { "lint": @lint@, "build": @build@, "test": @test@ }
		      }
		    }
		}"#,
		&[
			("lint", sh::append("order.txt", "lint")),
			("build", sh::append("order.txt", "build")),
			("test", sh::append("order.txt", "test")),
		],
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
	fx.config_from(
		r#"{
		    "latticeVersion": "0.1.0",
		    "workspaces": ["a"],
		    "tasks": { "lint": {}, "build": {} },
		    "overrides": { "./a": { "name": "app", "tasks": { "lint": @lint@, "build": @build@ } } }
		}"#,
		&[
			("lint", sh::exit(1)),
			("build", sh::write("build.txt", "built")),
		],
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
	fx.config_from(
		r#"{
		    "latticeVersion": "0.1.0",
		    "workspaces": ["a"],
		    "tasks": { "lint": {}, "build": {} },
		    "overrides": { "./a": { "name": "app", "tasks": { "lint": @lint@, "build": @build@ } } }
		}"#,
		&[("lint", sh::echo("L")), ("build", sh::echo("B"))],
	);

	fx.lattice()
		.args(["run", "lint", "build", "--sequentially", "--dry-run"])
		.assert()
		.success()
		.stdout(predicate::str::contains("dry run · lint (phase)"))
		.stdout(predicate::str::contains("dry run · build (phase)"));
}

#[test]
fn keep_going_runs_independent_and_reports_failure() {
	let fx = Fixture::new();
	fx.mkdir("a");
	fx.mkdir("b");
	fx.config_from(
		r#"{
		    "latticeVersion": "0.1.0",
		    "workspaces": ["a", "b"],
		    "tasks": { "build": {} },
		    "overrides": {
		      "./a": { "name": "good", "tasks": { "build": @good@ } },
		      "./b": { "name": "bad", "tasks": { "build": @bad@ } }
		    }
		}"#,
		&[
			("good", sh::echo("GOOD-RAN")),
			("bad", sh::then([sh::echo_err("boom"), sh::exit(1)])),
		],
	);

	// --no-cache keeps `good` running (so its output is emitted) on every run.
	fx.lattice()
		.args(["run", "build", "--continue", "--no-cache", "-l"])
		.assert()
		.failure()
		.stdout(predicate::str::contains("good:build: GOOD-RAN"))
		.stdout(predicate::str::contains("1 failed"))
		.stderr(predicate::str::contains(
			"bad:build: FAILED (code 1) after ",
		));
}

/// The typo case: `persistent: true` on a command that exits straight away. The
/// run has to end on its own — no signal is ever sent here — and report it.
#[test]
fn persistent_task_that_exits_immediately_is_reported() {
	let fx = Fixture::new();
	fx.mkdir("a");
	fx.config_from(
		r#"{
		    "latticeVersion": "0.1.0",
		    "workspaces": ["a"],
		    "tasks": { "dev": { "persistent": true } },
		    "overrides": { "./a": { "name": "app", "tasks": { "dev": @dev@ } } }
		}"#,
		&[(
			"dev",
			sh::then([sh::echo_err("port already in use"), sh::exit(1)]),
		)],
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
	fx.config_from(
		r#"{
		    "latticeVersion": "0.1.0",
		    "workspaces": ["a"],
		    "tasks": { "dev": { "persistent": true } },
		    "overrides": { "./a": { "name": "app", "tasks": { "dev": @dev@ } } }
		}"#,
		&[("dev", sh::succeed())],
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
	fx.config_from(
		r#"{
		    "latticeVersion": "0.1.0",
		    "workspaces": ["a"],
		    "tasks": { "build": {} },
		    "overrides": { "./a": { "name": "alpha", "tasks": { "build": @build@ } } }
		}"#,
		&[("build", sh::succeed())],
	);

	fx.lattice()
		.args(["run", "nope"])
		.assert()
		.failure()
		.stderr(predicate::str::contains("no workspace has a 'nope' task"))
		.stderr(predicate::str::contains("Tasks: build"))
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
	fx.config_from(
		r#"{
		    "latticeVersion": "0.1.0",
		    "projects": {},
		    "workspaces": ["a"],
		    "tasks": { "build": {} },
		    "overrides": { "./a": { "name": "alpha", "tasks": { "build": @build@ } } }
		}"#,
		&[("build", sh::succeed())],
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
	fx.config_from(
		r#"{
		    "latticeVersion": "0.1.0",
		    "workspaces": ["a"],
		    "tasks": { "build": { "output": ["dist/**"] } },
		    "overrides": { "./a": { "name": "alpha", "tasks": { "build": @build@ } } }
		}"#,
		&[("build", sh::succeed())],
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

/// An override's key is part of the message, so it points at one of several.
#[test]
fn an_unknown_override_key_names_the_override() {
	let fx = Fixture::new();
	fx.mkdir("a");
	fx.mkdir("b");
	fx.config_from(
		r#"{
		    "latticeVersion": "0.1.0",
		    "workspaces": ["a", "b"],
		    "tasks": { "build": {} },
		    "overrides": {
		      "./a": { "name": "alpha", "tasks": { "build": @build@ } },
		      "b": { "dependOn": ["alpha"] }
		    }
		}"#,
		&[("build", sh::succeed())],
	);

	fx.lattice()
		.args(["run", "build"])
		.assert()
		.failure()
		.stderr(predicate::str::contains(
			"unknown field `dependOn` in overrides.b",
		))
		.stderr(predicate::str::contains("Did you mean `dependsOn`?"));
}

/// A workspace whose `build` takes a measurable second, so the entry it writes
/// records a duration a later hit can report as saved.
fn slow_ws_repo(fx: &Fixture) {
	fx.write("app/src/f.txt", "hello\n");
	fx.config_from(
		r#"{
		    "latticeVersion": "0.1.0",
		    "workspaces": ["app"],
		    "tasks": { "build": { "inputs": ["src/**/*"], "outputs": ["dist/**/*"] } },
		    "overrides": { "app": { "tasks": { "build": @build@ } } }
		}"#,
		&[(
			"build",
			sh::all([
				sh::mkdirs("dist"),
				sh::sleep(1),
				sh::write("dist/out.txt", "hi"),
			]),
		)],
	);
}

#[test]
fn a_fully_cached_run_reports_full_power_and_the_time_it_saved() {
	let fx = Fixture::new();
	slow_ws_repo(&fx);

	// Cold run: nothing to save, and nothing to celebrate.
	fx.lattice()
		.args(["run", "build"])
		.assert()
		.success()
		.stdout(predicate::str::contains("saved").not())
		.stdout(predicate::str::contains("full power").not());

	// Warm run: the hit skipped the second the cold run spent.
	fx.lattice()
		.args(["run", "build"])
		.assert()
		.success()
		.stdout(predicate::str::contains("1 cached"))
		.stdout(predicate::str::contains("saved"))
		.stdout(predicate::str::contains("full power, nothing to run"))
		// The old phrasing described a disk filling up. It must not come back.
		.stdout(predicate::str::contains("full cache").not());
}

#[test]
fn stats_adds_up_the_runs_the_ledger_recorded() {
	let fx = Fixture::new();
	slow_ws_repo(&fx);

	// Nothing has run, so there is nothing to report and that is not an error.
	fx.lattice()
		.args(["stats"])
		.assert()
		.success()
		.stdout(predicate::str::contains("No runs recorded yet"));

	fx.lattice().args(["run", "build"]).assert().success();
	fx.lattice().args(["run", "build"]).assert().success();

	// Two runs, two scheduled tasks, one of them a hit.
	fx.lattice()
		.args(["stats"])
		.assert()
		.success()
		.stdout(predicate::str::contains("saved"))
		.stdout(predicate::str::contains("1 of 2 tasks cached (50%)"))
		.stdout(predicate::str::contains("1 entry"));
}

#[test]
fn a_run_that_stored_nothing_records_nothing() {
	let fx = Fixture::new();
	slow_ws_repo(&fx);

	// --no-cache means this run neither reads nor writes the cache, so it has no
	// business writing a ledger line either.
	fx.lattice()
		.args(["run", "build", "--no-cache"])
		.assert()
		.success();
	fx.lattice()
		.args(["stats"])
		.assert()
		.success()
		.stdout(predicate::str::contains("No runs recorded yet"));
}
