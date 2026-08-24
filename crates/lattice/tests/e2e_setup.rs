//! E2E tests for `lattice setup`: which workspaces it accepts, when it decides
//! dependencies are stale, and what the installer it spawns can see and say.
//!
//! No real package manager is involved. Each test puts a stand-in on `PATH` under
//! the name the detected driver installs with, so the whole path runs offline.

mod common;

use std::time::{Duration, SystemTime};

use common::Fixture;
use predicates::prelude::*;

/// Where `setup` records a successful install: one file per workspace under
/// `.lattice/setup`, named for the workspace's path with `/` written `%2F`.
const WEB_MARKER: &str = ".lattice/setup/apps%2Fweb.marker";

/// The path older versions used, still honoured so an upgrade does not reinstall
/// everything.
const LEGACY_MARKER: &str = ".lattice-setup-marker";

/// A repo whose only lockfile is at the root, which is where npm, pnpm and yarn
/// put it for a workspace tree. The workspace itself holds a manifest naming its
/// package manager, and nothing else.
fn hoisted_repo() -> Fixture {
	let fx = Fixture::new();
	fx.write(
		"apps/web/package.json",
		r#"{ "name": "web", "packageManager": "npm@10.0.0" }"#,
	);
	fx.write("package-lock.json", "{}");
	fx.config(
		r#"{
  "latticeVersion": "0.1.0",
  "workspaces": [ { "name": "web", "path": "apps/web", "auto": false } ],
  "tasks": {}
}
"#,
	);
	fx.install_stub_bin("release-stub", "npm");
	fx
}

/// A `lattice setup` for `fx` with the stand-in installer on `PATH`.
fn setup(fx: &Fixture) -> assert_cmd::Command {
	let mut cmd = fx.lattice();
	cmd.env("PATH", fx.path_with_stub_bin()).arg("setup");
	cmd
}

/// Whether setup decided this workspace needed installing, which it says on the
/// line it prints before spawning the installer.
fn install_attempted(stdout: &str) -> bool {
	stdout
		.lines()
		.any(|l| l.contains("web") && l.contains("npm install"))
}

/// Whether the installer's own output reached the user: the stand-in prints the
/// name it was invoked as and its arguments, indented under setup's line.
fn installer_output_shown(stdout: &str) -> bool {
	stdout.lines().any(|l| l.trim() == "npm install")
}

fn stdout_of(assert: assert_cmd::assert::Assert) -> String {
	String::from_utf8_lossy(&assert.get_output().stdout).into_owned()
}

/// The repo-relative names of the files directly under a directory, sorted.
fn file_names_under(fx: &Fixture, rel: &str) -> Vec<String> {
	let mut names: Vec<String> = fx
		.files_under(rel)
		.iter()
		.map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
		.collect();
	names.sort();
	names
}

fn set_mtime(path: &std::path::Path, when: SystemTime) {
	std::fs::File::options()
		.write(true)
		.open(path)
		.expect("open for its mtime")
		.set_modified(when)
		.expect("set mtime");
}

#[test]
fn a_lockfile_hoisted_to_the_repo_root_reinstalls() {
	let fx = hoisted_repo();

	let first = stdout_of(setup(&fx).assert().success());
	assert!(
		install_attempted(&first),
		"the first setup installs:\n{first}"
	);
	assert!(fx.exists(WEB_MARKER));

	// Nothing has moved, so the second run is the skip.
	let second = stdout_of(setup(&fx).assert().success());
	assert!(
		second.contains("dependencies up to date"),
		"an unchanged repo should skip:\n{second}"
	);

	// What a `git pull` that adds a dependency leaves behind. The lockfile is
	// above the workspace, which is the only place a hoisting package manager
	// keeps one.
	set_mtime(
		&fx.join("package-lock.json"),
		SystemTime::now() + Duration::from_secs(60),
	);

	let third = stdout_of(setup(&fx).assert().success());
	assert!(
		!third.contains("dependencies up to date"),
		"a changed root lockfile must not be reported as up to date:\n{third}"
	);
	assert!(
		install_attempted(&third),
		"it has to install again:\n{third}"
	);
}

#[test]
fn an_unknown_workspace_name_is_refused_rather_than_selecting_nothing() {
	let fx = hoisted_repo();

	setup(&fx)
		.arg("no-such-workspace")
		.assert()
		.failure()
		.stderr(predicate::str::contains("no-such-workspace"))
		.stderr(predicate::str::contains("Declared workspaces: web"))
		.stdout(predicate::str::contains("setup complete").not());

	assert!(!fx.exists(WEB_MARKER), "nothing should have been installed");
}

#[test]
fn a_declared_workspace_name_is_accepted() {
	let fx = hoisted_repo();
	let out = stdout_of(setup(&fx).arg("web").assert().success());
	assert!(install_attempted(&out), "{out}");
}

#[test]
fn an_installers_output_is_visible_without_a_verbosity_flag() {
	// An installer that asks for a passphrase, prints a warning, or explains what
	// it is waiting for says so on stdout. Hiding that behind `-v` is what makes
	// a stuck install look like a hung command.
	let fx = hoisted_repo();
	let out = stdout_of(setup(&fx).assert().success());
	assert!(
		installer_output_shown(&out),
		"the installer's own output belongs in a plain `lattice setup`:\n{out}"
	);
}

/// Unix only: the installer here is a script, because no compiled stand-in reads
/// stdin, and `/dev/stdin` is the only portable-enough name for "what this
/// process was given as input". `cmd` can reach the console but not a redirected
/// handle, so the Windows half of this asserts nothing.
#[cfg(unix)]
#[test]
fn an_installer_cannot_read_the_terminals_stdin() {
	let fx = hoisted_repo();
	let captured = fx.join("captured.txt");
	fx.install_script_bin("npm", &lattice_testkit::copy("/dev/stdin", &captured));

	setup(&fx).write_stdin("hunter2\n").assert().success();

	assert_eq!(
		std::fs::read_to_string(&captured).expect("the installer ran"),
		"",
		"the installer must not be handed the terminal's stdin"
	);
}

#[test]
fn init_ignores_where_setup_writes_its_markers() {
	let fx = Fixture::new();
	fx.write(
		"apps/web/package.json",
		r#"{ "name": "web", "packageManager": "npm@10.0.0" }"#,
	);
	fx.write("apps/web/package-lock.json", "{}");
	fx.install_stub_bin("release-stub", "npm");

	fx.lattice().args(["init", "-y"]).assert().success();
	setup(&fx).assert().success();

	assert!(
		fx.exists(WEB_MARKER),
		"{WEB_MARKER} is where setup records the install"
	);
	let ignored = fx.read(".gitignore");
	assert!(
		ignored.lines().any(|l| l.trim() == ".lattice/setup/"),
		"a scaffolded repo must not be left with untracked setup markers:\n{ignored}"
	);
	assert!(
		ignored.lines().any(|l| l.trim() == LEGACY_MARKER),
		"a marker from before the move has to stay ignored too:\n{ignored}"
	);
	assert!(
		!ignored.lines().any(|l| l.trim() == ".lattice/schema.json"),
		"the committed schema copy must stay tracked:\n{ignored}"
	);
}

#[test]
fn nothing_machine_local_is_written_into_the_workspace_directory() {
	let fx = hoisted_repo();

	let before = file_names_under(&fx, "apps/web");
	setup(&fx).assert().success();

	assert!(fx.exists(WEB_MARKER));
	assert_eq!(
		file_names_under(&fx, "apps/web"),
		before,
		"a task with no declared `inputs` hashes the workspace directory, so setup \
		 must not add a machine-local file to it"
	);
}

/// Two workspaces whose paths flatten to the same string. Sharing one marker
/// would mean installing either marks both up to date.
#[test]
fn workspaces_whose_paths_would_flatten_together_get_distinct_markers() {
	let fx = Fixture::new();
	for path in ["apps/web", "apps-web"] {
		fx.write(
			&format!("{path}/package.json"),
			r#"{ "name": "w", "packageManager": "npm@10.0.0" }"#,
		);
	}
	fx.write("package-lock.json", "{}");
	fx.config(
		r#"{
  "latticeVersion": "0.1.0",
  "workspaces": [
    { "name": "nested", "path": "apps/web", "auto": false },
    { "name": "sibling", "path": "apps-web", "auto": false }
  ],
  "tasks": {}
}
"#,
	);
	fx.install_stub_bin("release-stub", "npm");

	setup(&fx).assert().success();

	assert!(fx.exists(WEB_MARKER));
	assert!(fx.exists(".lattice/setup/apps-web.marker"));
	assert_eq!(
		file_names_under(&fx, ".lattice/setup").len(),
		2,
		"one marker per workspace"
	);
}

/// What an upgrade looks like: the marker from the old version is in the
/// workspace, and nothing has changed since. Reinstalling every workspace in the
/// repo is not an acceptable cost for moving the file.
#[test]
fn a_marker_left_by_an_older_version_still_counts_as_installed() {
	let fx = hoisted_repo();
	fx.write(&format!("apps/web/{LEGACY_MARKER}"), "");

	let out = stdout_of(setup(&fx).assert().success());

	assert!(
		out.contains("dependencies up to date"),
		"an upgrade must not reinstall what is already installed:\n{out}"
	);
	assert!(!install_attempted(&out), "{out}");
}

/// Installing again writes the new marker and takes the old one with it, so the
/// migration finishes on its own rather than leaving a file behind forever.
#[test]
fn a_marker_left_by_an_older_version_is_replaced_on_the_next_install() {
	let fx = hoisted_repo();
	fx.write(&format!("apps/web/{LEGACY_MARKER}"), "");

	let out = stdout_of(setup(&fx).arg("--force").assert().success());
	assert!(install_attempted(&out), "{out}");

	assert!(fx.exists(WEB_MARKER));
	assert!(!fx.exists(&format!("apps/web/{LEGACY_MARKER}")));
}
