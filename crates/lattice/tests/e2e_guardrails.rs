//! E2E coverage for the things that go wrong quietly.
//!
//! Each case here is a defect that produced a plausible-looking run: a build
//! ordered wrongly because a name was misspelled, a cache hit serving an
//! artifact built from a file that has since changed, a prune taking the
//! toolchains with it. None of them announced themselves, so each one is pinned
//! from the outside, at the surface a user actually sees.

mod common;

use common::Fixture;
use lattice_testkit as sh;

/// A repo with two workspaces whose `build` writes its own name, so the ordering
/// and the caching are both observable from the files left behind.
fn two_workspace_repo(fx: &Fixture, config: &str, cmds: &[(&str, String)]) {
	fx.mkdir("lib");
	fx.mkdir("app");
	fx.config_from(config, cmds);
}

#[test]
fn a_workspace_dep_that_names_nothing_is_rejected() {
	let fx = Fixture::new();
	two_workspace_repo(
		&fx,
		r#"{
          "workspaces": [
            { "name": "lib", "path": "lib", "auto": false, "scripts": { "build": @lib@ } },
            { "name": "app", "path": "app", "auto": false, "dependsOn": ["libb"],
              "scripts": { "build": @app@ } }
          ],
          "tasks": { "build": { "dependsOn": ["^build"] } }
        }"#,
		&[("lib", sh::echo("lib")), ("app", sh::echo("app"))],
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
	fx.config_from(
		r#"{
          "workspaces": [{ "name": "app", "path": "app", "auto": false,
                           "scripts": { "build": @build@ } }],
          "tasks": { "build": { "dependsOn": ["codegen"] } }
        }"#,
		&[("build", sh::echo("app"))],
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
	fx.config_from(
		r#"{
          "workspaces": [{ "name": "esc", "path": "../outside", "auto": false,
                           "scripts": { "build": @build@ } }],
          "tasks": { "build": {} }
        }"#,
		&[("build", sh::echo("x"))],
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
	fx.config_from(
		r#"{
          "workspaces": [{ "name": "app", "path": "app", "auto": false,
                           "scripts": { "build": @build@ } }],
          "globalDependencies": ["shared.config.json"],
          "tasks": { "build": { "outputs": ["out.txt"] } }
        }"#,
		&[("build", sh::copy("../shared.config.json", "out.txt"))],
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
	fx.config_from(
		r#"{
          "workspaces": [{ "name": "app", "path": "app", "auto": false,
                           "scripts": { "build": @build@ } }],
          "globalDependencies": ["shared.json"],
          "tasks": { "build": { "outputs": ["out.txt"] } }
        }"#,
		&[("build", sh::write("out.txt", "hi"))],
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
/// outright — here `.lattice`, beside `toolchains/` and `bin/`. Prune reclaims
/// cache entries and the debris of an interrupted store, and no directories at
/// all; it used to remove every neighbour that was not the current cache format,
/// which took the provisioned toolchains and the installed binary too.
#[test]
fn prune_leaves_everything_that_is_not_a_cache_entry() {
	let fx = Fixture::new();
	fx.mkdir("app");
	fx.config_from(
		r#"{
          "workspaces": [{ "name": "app", "path": "app", "auto": false,
                           "scripts": { "build": @build@ } }],
          "tasks": { "build": { "outputs": ["out.txt"] } },
          "settings": { "cacheDir": ".lattice", "maxCacheSize": "1B" }
        }"#,
		&[("build", sh::write("out.txt", "hi"))],
	);
	fx.write(
		".lattice/toolchains/faketool/1.0.0-abcd/bin/faketool",
		"#!/bin/sh\n",
	);
	fx.write(".lattice/bin/lattice-1.0.0", "the binary in use");
	// Debris from an interrupted store, which should still go.
	fx.write(".lattice/deadbeef.tar.gz", "an artifact with no metadata");

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
		!fx.exists(".lattice/deadbeef.tar.gz"),
		"an orphaned artifact is still reclaimed"
	);
}

/// `maxCacheSize` reads as a budget, so it has to be one. Leaving enforcement to
/// `lattice prune` alone meant a repo that set it still grew without limit.
///
/// Measured by whether entries were evicted rather than by total bytes, so it
/// does not turn on producing a file of some particular size. The unbudgeted run
/// is the control: without it, a cache that simply stayed small would pass.
#[test]
fn max_cache_size_is_held_without_running_prune() {
	/// Three runs over three different inputs, leaving three distinct keys.
	fn three_runs(fx: &Fixture, settings: &str) {
		fx.mkdir("app");
		fx.config_from(
			&format!(
				r#"{{
          "workspaces": [{{ "name": "app", "path": "app", "auto": false,
                           "scripts": {{ "build": @build@ }} }}],
          "tasks": {{ "build": {{ "outputs": ["out.txt"] }} }},
          "settings": {{ {settings} }}
        }}"#
			),
			&[("build", sh::write("out.txt", "built"))],
		);
		for seed in 0..3 {
			fx.write("app/seed.txt", &format!("seed{seed}"));
			fx.lattice().args(["run", "build"]).assert().success();
		}
	}

	/// Stored entries, counted by their metadata files.
	fn entries(fx: &Fixture) -> usize {
		fx.files_under(".lattice/cache")
			.iter()
			.filter(|p| p.to_string_lossy().ends_with(".meta.json"))
			.count()
	}

	let unbounded = Fixture::new();
	three_runs(&unbounded, r#""cacheDir": ".lattice/cache""#);
	assert_eq!(
		entries(&unbounded),
		3,
		"without a budget nothing should be evicted"
	);

	let bounded = Fixture::new();
	three_runs(&bounded, r#""maxCacheSize": "1B""#);
	assert_eq!(
		entries(&bounded),
		0,
		"a run must hold the cache to settings.maxCacheSize without calling prune"
	);
}

/// A task with no limit that never exits hangs the run, in CI as much as
/// anywhere else.
#[test]
fn a_task_that_overruns_its_timeout_is_stopped() {
	let fx = Fixture::new();
	fx.mkdir("app");
	fx.config_from(
		r#"{
          "workspaces": [{ "name": "app", "path": "app", "auto": false,
                           "scripts": { "build": @build@ } }],
          "tasks": { "build": { "timeout": "1s" } }
        }"#,
		&[(
			"build",
			sh::then([sh::sleep(60), sh::touch("finished.txt")]),
		)],
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
	fx.config_from(
		r#"{
          "workspaces": [{ "name": "app", "path": "app", "auto": false,
                           "scripts": { "build": @build@ } }],
          "tasks": { "build": { "timeout": "5m", "outputs": ["out.txt"] } }
        }"#,
		&[("build", sh::write("out.txt", "hi"))],
	);

	fx.lattice().args(["run", "build"]).assert().success();
	assert_eq!(fx.read("app/out.txt").trim(), "hi");
}
