//! E2E coverage for the things that go wrong quietly.
//!
//! Each case here is a defect that produced a plausible-looking run: a build
//! ordered wrongly because a name was misspelled, a cache hit serving an
//! artifact built from a file that has since changed, a prune taking the
//! toolchains with it. None of them announced themselves, so each one is pinned
//! from the outside, at the surface a user actually sees.

mod common;

use common::Fixture;

/// A repo with two workspaces whose `build` writes its own name, so the ordering
/// and the caching are both observable from the files left behind.
fn two_workspace_repo(fx: &Fixture, config: &str) {
	fx.mkdir("lib");
	fx.mkdir("app");
	fx.config(config);
}

#[test]
fn a_workspace_dep_that_names_nothing_is_rejected() {
	let fx = Fixture::new();
	two_workspace_repo(
		&fx,
		r#"{
          "workspaces": [
            { "name": "lib", "path": "lib", "auto": false, "scripts": { "build": "echo lib" } },
            { "name": "app", "path": "app", "auto": false, "dependsOn": ["libb"],
              "scripts": { "build": "echo app" } }
          ],
          "tasks": { "build": { "dependsOn": ["^build"] } }
        }"#,
	);

	// A dry run is enough: the config never had the edge it was written to have,
	// so the graph is already wrong before anything executes.
	let out = fx
		.lattice()
		.args(["run", "build", "--dry-run"])
		.assert()
		.failure();
	let stderr = String::from_utf8_lossy(&out.get_output().stderr).to_string();
	assert!(stderr.contains("libb"), "must name the typo: {stderr}");
	assert!(
		stderr.contains("Did you mean `lib`?"),
		"must offer the near miss: {stderr}"
	);
}

#[test]
fn a_task_dep_that_names_nothing_is_rejected() {
	let fx = Fixture::new();
	fx.mkdir("app");
	fx.config(
		r#"{
          "workspaces": [{ "name": "app", "path": "app", "auto": false,
                           "scripts": { "build": "echo app" } }],
          "tasks": { "build": { "dependsOn": ["codegen"] } }
        }"#,
	);

	let out = fx
		.lattice()
		.args(["run", "build", "--dry-run"])
		.assert()
		.failure();
	let stderr = String::from_utf8_lossy(&out.get_output().stderr).to_string();
	assert!(stderr.contains("codegen"), "{stderr}");
	assert!(stderr.contains("Defined tasks: build"), "{stderr}");
}

#[test]
fn a_workspace_path_outside_the_repo_is_rejected() {
	let fx = Fixture::new();
	fx.config(
		r#"{
          "workspaces": [{ "name": "esc", "path": "../outside", "auto": false,
                           "scripts": { "build": "echo x" } }],
          "tasks": { "build": {} }
        }"#,
	);

	let out = fx
		.lattice()
		.args(["run", "build", "--dry-run"])
		.assert()
		.failure();
	let stderr = String::from_utf8_lossy(&out.get_output().stderr).to_string();
	assert!(stderr.contains("outside the repo root"), "{stderr}");
}

/// A task's `inputs` are workspace-relative, so a file shared above the
/// workspace could not be named there. Nothing covered it: editing the shared
/// file left the task hitting cache and restoring an artifact built from the
/// version before the edit.
#[test]
fn a_global_dependency_change_invalidates_the_cache() {
	let fx = Fixture::new();
	fx.mkdir("app");
	fx.write("shared.config.json", r#"{"mode":"one"}"#);
	fx.config(
		r#"{
          "workspaces": [{ "name": "app", "path": "app", "auto": false,
                           "scripts": { "build": "cat ../shared.config.json > out.txt" } }],
          "globalDependencies": ["shared.config.json"],
          "tasks": { "build": { "outputs": ["out.txt"] } }
        }"#,
	);

	fx.lattice().args(["run", "build"]).assert().success();
	assert_eq!(fx.read("app/out.txt"), r#"{"mode":"one"}"#);

	// Untouched: a hit, as it should be.
	let warm = fx.lattice().args(["run", "build"]).assert().success();
	assert!(String::from_utf8_lossy(&warm.get_output().stdout).contains("cache"));

	fx.write("shared.config.json", r#"{"mode":"TWO"}"#);
	fx.lattice().args(["run", "build"]).assert().success();
	assert_eq!(
		fx.read("app/out.txt"),
		r#"{"mode":"TWO"}"#,
		"a shared root file must reach the key of every task that reads it"
	);
}

/// A key is one hash: on its own it can say a task missed, never what moved.
#[test]
fn a_miss_says_which_part_of_the_key_moved() {
	let fx = Fixture::new();
	fx.mkdir("app");
	fx.write("shared.json", "one");
	fx.config(
		r#"{
          "workspaces": [{ "name": "app", "path": "app", "auto": false,
                           "scripts": { "build": "echo hi > out.txt" } }],
          "globalDependencies": ["shared.json"],
          "tasks": { "build": { "outputs": ["out.txt"] } }
        }"#,
	);

	fx.lattice().args(["run", "build"]).assert().success();
	fx.write("shared.json", "two");

	let out = fx.lattice().args(["-l", "run", "build"]).assert().success();
	let stdout = String::from_utf8_lossy(&out.get_output().stdout).to_string();
	assert!(
		stdout.contains("globalDependencies changed"),
		"a miss should name the part that moved: {stdout}"
	);
}

/// `cacheDir` can legitimately point at a directory Lattice does not own
/// outright. Prune used to remove every neighbour that was not the current cache
/// format, which took the provisioned toolchains and the installed binary too.
#[test]
fn prune_leaves_everything_that_is_not_a_cache_format() {
	let fx = Fixture::new();
	fx.mkdir("app");
	fx.config(
		r#"{
          "workspaces": [{ "name": "app", "path": "app", "auto": false,
                           "scripts": { "build": "echo hi > out.txt" } }],
          "tasks": { "build": { "outputs": ["out.txt"] } },
          "settings": { "cacheDir": ".lattice", "maxCacheSize": "1B" }
        }"#,
	);
	fx.write(
		".lattice/toolchains/faketool/1.0.0-abcd/bin/faketool",
		"#!/bin/sh\n",
	);
	fx.write(".lattice/bin/lattice-1.0.0", "the binary in use");
	// A directory from a genuinely older cache format, which should still go.
	fx.write(".lattice/v1/dead.meta.json", "{}");

	fx.lattice().args(["run", "build"]).assert().success();
	fx.lattice().arg("prune").assert().success();

	assert!(
		fx.exists(".lattice/toolchains/faketool/1.0.0-abcd/bin/faketool"),
		"prune must not take the provisioned toolchains"
	);
	assert!(
		fx.exists(".lattice/bin/lattice-1.0.0"),
		"prune must not take the installed binary"
	);
	assert!(
		!fx.exists(".lattice/v1"),
		"an earlier cache format is still reclaimed"
	);
}

/// `maxCacheSize` reads as a budget, so it has to be one. Leaving enforcement to
/// `lattice prune` alone meant a repo that set it still grew without limit.
#[test]
fn max_cache_size_is_held_without_running_prune() {
	let fx = Fixture::new();
	fx.mkdir("app");
	fx.config(
		r#"{
          "workspaces": [{ "name": "app", "path": "app", "auto": false,
                           "scripts": { "build": "head -c 200000 /dev/zero | tr '\\0' 'x' > big.bin" } }],
          "tasks": { "build": { "outputs": ["big.bin"] } },
          "settings": { "maxCacheSize": "1KB" }
        }"#,
	);

	for seed in 0..3 {
		fx.write("app/seed.txt", &format!("seed{seed}"));
		fx.lattice().args(["run", "build"]).assert().success();
	}

	// The budget governs cached artifacts. The key breakdowns kept beside them
	// are a few hundred bytes per task and exist to explain a miss, so evicting
	// one would cost the explanation and free nothing worth freeing.
	let total: u64 = fx
		.files_under(".lattice/cache")
		.iter()
		.filter(|p| !p.components().any(|c| c.as_os_str() == "fingerprints"))
		.filter_map(|p| std::fs::metadata(p).ok())
		.map(|m| m.len())
		.sum();
	assert!(
		total <= 1024,
		"the cache must be held to its declared budget, found {total} bytes"
	);
}

/// A task with no limit that never exits hangs the run, in CI as much as
/// anywhere else.
#[cfg(unix)]
#[test]
fn a_task_that_overruns_its_timeout_is_stopped() {
	let fx = Fixture::new();
	fx.mkdir("app");
	fx.config(
		r#"{
          "workspaces": [{ "name": "app", "path": "app", "auto": false,
                           "scripts": { "build": "sleep 60; touch finished.txt" } }],
          "tasks": { "build": { "timeout": "1s" } }
        }"#,
	);

	let out = fx
		.lattice()
		.timeout(std::time::Duration::from_secs(45))
		.args(["run", "build"])
		.assert()
		.failure();

	let combined = format!(
		"{}{}",
		String::from_utf8_lossy(&out.get_output().stdout),
		String::from_utf8_lossy(&out.get_output().stderr)
	);
	assert!(combined.contains("timed out"), "{combined}");
	assert!(
		!fx.exists("app/finished.txt"),
		"the task must not have run to completion"
	);
}

#[test]
fn a_timeout_a_task_stays_inside_is_not_enforced() {
	let fx = Fixture::new();
	fx.mkdir("app");
	fx.config(
		r#"{
          "workspaces": [{ "name": "app", "path": "app", "auto": false,
                           "scripts": { "build": "echo hi > out.txt" } }],
          "tasks": { "build": { "timeout": "5m", "outputs": ["out.txt"] } }
        }"#,
	);

	fx.lattice().args(["run", "build"]).assert().success();
	assert_eq!(fx.read("app/out.txt").trim(), "hi");
}
