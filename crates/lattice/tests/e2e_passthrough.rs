//! E2E tests for the passthrough pattern: a nested repo that already owns its
//! own task runner, wired into the Lattice graph as one manual workspace whose
//! scripts shell out to that runner. Lattice has no knowledge of the inner
//! graph; the nested repo is a single node it schedules and caches as one unit.
//!
//! The inner runner here is a stub executable named `turbo` dropped on `PATH`:
//! a POSIX shell script that fans `turbo run <task>` out over the nested repo's
//! packages, so the suite stays hermetic (no node, no network). It also leaves a
//! nondeterministic file in its own cache dir on every invocation, which is what
//! the `ignore` patterns below have to keep out of Lattice's cache key.

#![cfg(unix)]

mod common;

use assert_cmd::Command;
use common::Fixture;
use predicates::prelude::*;

/// Stand-in for the nested repo's runner: bundles every package and records a
/// per-invocation marker in its own cache dir.
const TURBO_STUB: &str = r#"#!/bin/sh
set -e
[ "$1" = "run" ] || { echo "turbo-stub: expected 'run', got '$*'" >&2; exit 2; }
task="$2"
mkdir -p .turbo
echo "$$ $(date +%s)" > .turbo/last-run
for pkg in packages/*; do
  mkdir -p "$pkg/dist"
  cat "$pkg/src/index.js" > "$pkg/dist/bundle.js"
  echo "$(basename "$pkg"):$task: done"
done
echo "turbo-stub: $task complete"
"#;

/// The `ignore` set a passthrough workspace needs when its `inputs` are broad:
/// dependency trees, the inner runner's own cache, and the outputs themselves
/// (an output inside the input globs would invalidate the key it just wrote).
const IGNORE: &str = r#"["**/node_modules/**", "**/.turbo/**", "**/dist/**"]"#;

/// A repo with a nested runner-owned workspace (`frontend`) and an ordinary
/// workspace (`api`) that consumes its build output, so the passthrough node has
/// to be scheduled first. `ignore` is a parameter so a test can show what
/// happens when a pattern is missing.
fn nested_repo(fx: &Fixture, ignore: &str) {
	fx.write_exec("bin/turbo", TURBO_STUB);

	// The nested repo: its own manifest, its own task config, its own packages.
	fx.write(
		"frontend/package.json",
		r#"{ "name": "frontend", "private": true, "workspaces": ["packages/*"] }
"#,
	);
	fx.write(
		"frontend/turbo.json",
		r#"{ "tasks": { "build": { "dependsOn": ["^build"], "outputs": ["dist/**"] } } }
"#,
	);
	fx.write(
		"frontend/packages/ui/src/index.js",
		"export const ui = 1;\n",
	);
	fx.write(
		"frontend/packages/site/src/index.js",
		"export const site = 1;\n",
	);

	fx.write("api/src/main.txt", "api v1\n");

	fx.config(&format!(
        r#"{{
  "latticeVersion": "0.1.0",
  "workspaces": [
    {{ "name": "frontend", "path": "frontend", "auto": false,
      "scripts": {{ "build": "turbo run build" }} }},
    {{ "name": "api", "path": "api", "auto": false, "dependsOn": ["frontend"],
      "scripts": {{ "build": "mkdir -p dist && cp ../frontend/packages/site/dist/bundle.js dist/site.js && echo api-built" }} }}
  ],
  "tasks": {{
    "build": {{
      "dependsOn": ["^build"],
      "inputs": ["**/*"],
      "ignore": {ignore},
      "outputs": ["dist/**", "packages/*/dist/**"]
    }}
  }}
}}
"#
    ))
}

/// `lattice` with the stub runner's dir prepended to `PATH` — the same way a
/// nested repo's runner is resolved in a real checkout (a global install, or
/// `node_modules/.bin`).
fn lattice(fx: &Fixture) -> Command {
	let mut cmd = fx.lattice();
	let host = std::env::var("PATH").unwrap_or_default();
	cmd.env("PATH", format!("{}:{host}", fx.join("bin").display()));
	cmd
}

#[test]
fn nested_runner_runs_in_graph_order_with_no_engine_machinery() {
	let fx = Fixture::new();
	nested_repo(&fx, IGNORE);

	lattice(&fx)
		.args(["run", "build", "-l"])
		.assert()
		.success()
		// The inner runner ran, and fanned out over the nested repo's packages.
		.stdout(predicate::str::contains("frontend:build: ui:build: done"))
		.stdout(predicate::str::contains("frontend:build: site:build: done"))
		.stdout(predicate::str::contains(
			"frontend:build: turbo-stub: build complete",
		))
		// `api` copies a frontend artifact, so it could only succeed downstream.
		.stdout(predicate::str::contains("api:build: api-built"))
		.stdout(predicate::str::contains("0 failed"));

	assert!(
		fx.exists("frontend/packages/ui/dist/bundle.js")
			&& fx.exists("frontend/packages/site/dist/bundle.js"),
		"the inner runner's artifacts should be on disk"
	);
	assert_eq!(
		fx.read("api/dist/site.js"),
		"export const site = 1;\n",
		"api must consume the artifact the nested repo produced"
	);
	assert!(
		!fx.exists(".lattice/toolchains"),
		"a passthrough repo declares no engines and must provision nothing"
	);
}

#[test]
fn nested_repo_caches_as_one_unit() {
	let fx = Fixture::new();
	nested_repo(&fx, IGNORE);

	lattice(&fx)
		.args(["run", "build", "-l"])
		.assert()
		.success()
		.stdout(predicate::str::contains("0 cached"));

	// Unchanged inputs: the whole nested repo is one hit, and its runner is
	// never invoked.
	lattice(&fx)
		.args(["run", "build", "-l"])
		.assert()
		.success()
		.stdout(predicate::str::contains("frontend:build: cache hit ["))
		.stdout(predicate::str::contains("2 cached"))
		.stdout(predicate::str::contains("turbo-stub").not());

	// The hit restores every artifact the inner runner had produced.
	std::fs::remove_dir_all(fx.join("frontend/packages/ui/dist")).expect("rm dist");
	std::fs::remove_dir_all(fx.join("frontend/packages/site/dist")).expect("rm dist");
	lattice(&fx)
		.args(["run", "build", "-l"])
		.assert()
		.success()
		.stdout(predicate::str::contains("frontend:build: cache hit ["));
	assert!(
		fx.exists("frontend/packages/ui/dist/bundle.js")
			&& fx.exists("frontend/packages/site/dist/bundle.js"),
		"a hit on the passthrough workspace must restore the inner artifacts"
	);

	// A source edit anywhere inside the nested repo busts its key and re-runs the
	// inner runner. `api` consumes what that runner produces, so it has to re-run
	// too: `frontend` is one opaque node, and nothing here can tell which of its
	// outputs moved. A dependent that hits cache after its dependency rebuilt is
	// how a stale artifact ships.
	fx.write(
		"frontend/packages/ui/src/index.js",
		"export const ui = 2;\n",
	);
	lattice(&fx)
		.args(["run", "build", "-l"])
		.assert()
		.success()
		.stdout(predicate::str::contains("frontend:build: cache hit").not())
		.stdout(predicate::str::contains(
			"frontend:build: turbo-stub: build complete",
		))
		.stdout(predicate::str::contains("api:build: cache hit").not())
		.stdout(predicate::str::contains("api:build: api-built"));

	// Nothing touched: both hit again, so the invalidation above was caused by the
	// edit and not by a key that simply never settles.
	lattice(&fx)
		.args(["run", "build", "-l"])
		.assert()
		.success()
		.stdout(predicate::str::contains("2 cached"));
}

#[test]
fn unignored_inner_runner_cache_dir_defeats_the_lattice_cache() {
	let fx = Fixture::new();
	// Same repo, but `.turbo` is no longer ignored: the inner runner's
	// per-invocation marker lands in the key, so no run can ever hit.
	nested_repo(&fx, r#"["**/node_modules/**", "**/dist/**"]"#);

	lattice(&fx).args(["run", "build", "-l"]).assert().success();
	lattice(&fx)
		.args(["run", "build", "-l"])
		.assert()
		.success()
		.stdout(predicate::str::contains("frontend:build: cache hit").not())
		.stdout(predicate::str::contains(
			"frontend:build: turbo-stub: build complete",
		));
}
