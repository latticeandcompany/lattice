//! `lattice upgrade` and the pinned-version handover, end to end.
//!
//! Every test publishes a fake release to a temp directory and points the CLI at
//! it with `--release-base-url`, so the real download, checksum, extract,
//! install and link path runs with no network.

mod common;

use predicates::prelude::PredicateBooleanExt;

use common::{curl_supports_file, FakeRelease, Fixture};

const CONFIG: &str = r#"{
  "latticeVersion": "0.1.0",
  "workspaces": [
    { "name": "app", "path": "app", "auto": false, "scripts": { "greet": "echo hello" } }
  ],
  "tasks": { "greet": {} }
}
"#;

fn fixture() -> Fixture {
	let fx = Fixture::new();
	fx.config(CONFIG);
	fx.mkdir("app");
	fx
}

/// The version the binary under test reports, which is what a pin has to differ
/// from for anything interesting to happen.
fn bin_version() -> String {
	let out = Fixture::new()
		.lattice()
		.args(["version", "--json"])
		.output()
		.expect("run version --json");
	let text = String::from_utf8_lossy(&out.stdout).into_owned();
	let start = text.find(r#""version":""#).expect("version field") + 11;
	text[start..].split('"').next().unwrap().to_string()
}

#[test]
fn upgrade_installs_the_release_and_rewrites_the_pin() {
	if !curl_supports_file() {
		return;
	}
	let fx = fixture();
	let release = FakeRelease::new();
	release.publish("9.9.9", "pinned-binary");

	fx.lattice()
		.args([
			"--release-base-url",
			&release.base_url(),
			"upgrade",
			"9.9.9",
		])
		.assert()
		.success()
		.stdout(predicates::str::contains("0.1.0"))
		.stdout(predicates::str::contains("9.9.9"));

	assert!(
		fx.exists(".lattice/bin/lattice-9.9.9"),
		"the version-stamped binary should be installed"
	);
	assert!(
		fx.exists(".lattice/bin/lattice"),
		"the stable path should exist"
	);
	assert!(
		fx.read("lattice.json")
			.contains(r#""latticeVersion": "9.9.9""#),
		"the pin should be rewritten in place: {}",
		fx.read("lattice.json")
	);
	// The rest of the config is untouched, including its formatting.
	assert!(fx.read("lattice.json").contains(r#""name": "app""#));
}

#[test]
fn upgrade_accepts_a_v_prefix_and_is_idempotent() {
	if !curl_supports_file() {
		return;
	}
	let fx = fixture();
	let release = FakeRelease::new();
	release.publish("9.9.9", "pinned-binary");

	fx.lattice()
		.args([
			"--release-base-url",
			&release.base_url(),
			"upgrade",
			"v9.9.9",
		])
		.assert()
		.success();

	// Second time: already pinned and already on disk, so it reports that and
	// does not need the release at all.
	release.unpublish("9.9.9");
	fx.lattice()
		.args([
			"--release-base-url",
			&release.base_url(),
			"upgrade",
			"9.9.9",
		])
		.assert()
		.success()
		.stdout(predicates::str::contains("already on 9.9.9"));
}

#[test]
fn upgrade_latest_resolves_the_newest_release() {
	if !curl_supports_file() {
		return;
	}
	let fx = fixture();
	let release = FakeRelease::new();
	release.publish("9.9.9", "pinned-binary");

	fx.lattice()
		.args([
			"--release-base-url",
			&release.base_url(),
			"upgrade",
			"latest",
			"--release-latest-url",
			&release.latest_url("9.9.9"),
		])
		.assert()
		.success()
		.stdout(predicates::str::contains("9.9.9"));

	assert!(fx
		.read("lattice.json")
		.contains(r#""latticeVersion": "9.9.9""#));
}

/// `/releases/latest` is the newest *stable* release, so it 404s while every
/// release is a pre-release — and `upgrade latest` has to keep going rather than
/// report that there is nothing to install.
#[test]
fn upgrade_latest_falls_back_to_the_newest_pre_release() {
	if !curl_supports_file() {
		return;
	}
	let fx = fixture();
	let release = FakeRelease::new();
	release.publish("9.9.9-beta-2", "beta-binary");

	fx.lattice()
		.args([
			"--release-base-url",
			&release.base_url(),
			"upgrade",
			"latest",
			"--release-latest-url",
			&release.missing_latest_url(),
			"--release-list-url",
			&release.list_url(&[("9.9.9-beta-2", true), ("9.9.9-beta-1", true)]),
		])
		.assert()
		.success()
		.stdout(predicates::str::contains("9.9.9-beta-2"))
		// Nobody who typed "latest" should have to notice it was a beta afterwards.
		.stdout(predicates::str::contains("pre-release"));

	assert!(fx
		.read("lattice.json")
		.contains(r#""latticeVersion": "9.9.9-beta-2""#));
}

/// The other half of that: once a stable release exists, a newer pre-release must
/// not start winning. Only 9.9.9 is published, so picking the beta also fails to
/// download.
#[test]
fn upgrade_latest_prefers_a_stable_release_over_a_newer_pre_release() {
	if !curl_supports_file() {
		return;
	}
	let fx = fixture();
	let release = FakeRelease::new();
	release.publish("9.9.9", "stable-binary");

	fx.lattice()
		.args([
			"--release-base-url",
			&release.base_url(),
			"upgrade",
			"latest",
			"--release-latest-url",
			&release.latest_url("9.9.9"),
			"--release-list-url",
			&release.list_url(&[("9.9.10-beta-1", true), ("9.9.9", false)]),
		])
		.assert()
		.success()
		.stdout(predicates::str::contains("9.9.9"))
		.stdout(predicates::str::contains("9.9.10-beta-1").not())
		.stdout(predicates::str::contains("pre-release").not());

	assert!(fx
		.read("lattice.json")
		.contains(r#""latticeVersion": "9.9.9""#));
}

#[test]
fn upgrade_refuses_an_archive_that_fails_its_checksum() {
	if !curl_supports_file() {
		return;
	}
	let fx = fixture();
	let release = FakeRelease::new();
	release.publish_with_wrong_digest("9.9.9", "tampered");

	fx.lattice()
		.args([
			"--release-base-url",
			&release.base_url(),
			"upgrade",
			"9.9.9",
		])
		.assert()
		.failure()
		.stderr(predicates::str::contains("checksum mismatch"));

	assert!(
		!fx.exists(".lattice/bin/lattice-9.9.9"),
		"a binary that failed verification must not be installed"
	);
	assert!(
		fx.read("lattice.json")
			.contains(r#""latticeVersion": "0.1.0""#),
		"a failed upgrade must not move the pin"
	);
}

/// The environment variables the flags replaced still work, so a CI job that
/// exports one keeps installing from the same place.
#[test]
fn the_release_url_env_vars_still_work_without_a_flag() {
	if !curl_supports_file() {
		return;
	}
	let fx = fixture();
	let release = FakeRelease::new();
	release.publish("9.9.9", "pinned-binary");

	fx.lattice()
		.env("LATTICE_RELEASE_BASE_URL", release.base_url())
		.env("LATTICE_RELEASE_LATEST_URL", release.latest_url("9.9.9"))
		.args(["upgrade", "latest"])
		.assert()
		.success()
		.stdout(predicates::str::contains("9.9.9"));

	assert!(fx.exists(".lattice/bin/lattice-9.9.9"));
}

/// ...and where both are given, the flag is the one that counts.
#[test]
fn a_release_url_flag_beats_the_environment() {
	if !curl_supports_file() {
		return;
	}
	let fx = fixture();
	let release = FakeRelease::new();
	release.publish("9.9.9", "pinned-binary");
	let dead_end = FakeRelease::new();

	fx.lattice()
		// The env points somewhere with no release at all: reaching for it instead
		// of the flag is a failure this asserts cannot happen.
		.env("LATTICE_RELEASE_BASE_URL", dead_end.base_url())
		.args([
			"--release-base-url",
			&release.base_url(),
			"upgrade",
			"9.9.9",
		])
		.assert()
		.success();

	assert!(fx.exists(".lattice/bin/lattice-9.9.9"));
}

/// A blank value is not a value — an inherited `LATTICE_RELEASE_BASE_URL=` must
/// fall through to the default rather than build an empty URL.
#[test]
fn a_blank_release_url_env_var_falls_through() {
	let fx = fixture();
	fx.lattice()
		.env("LATTICE_RELEASE_BASE_URL", "")
		.args(["upgrade", "not-a-version"])
		.assert()
		.failure()
		.stderr(predicates::str::contains("is not a version"));
}

#[test]
fn upgrade_rejects_a_version_that_is_not_one() {
	let fx = fixture();
	fx.lattice()
		.args(["upgrade", "../../etc/passwd"])
		.assert()
		.failure()
		.stderr(predicates::str::contains("is not a version"));
}

#[test]
fn upgrade_needs_a_version() {
	let fx = fixture();
	fx.lattice().arg("upgrade").assert().failure();
}

/// The first acceptance criterion: a managed binary in a repo pinned elsewhere
/// says so and hands the invocation to the pinned build.
#[test]
#[cfg(unix)]
fn a_pinned_repo_switches_to_the_version_it_pins() {
	if !curl_supports_file() {
		return;
	}
	let bin = bin_version();
	let fx = fixture();
	fx.config(&CONFIG.replace("0.1.0", "9.9.9"));
	fx.install_managed(&bin);

	let release = FakeRelease::new();
	release.publish("9.9.9", "pinned-binary");

	fx.managed_lattice(&bin)
		.args(["--release-base-url", &release.base_url(), "run", "greet"])
		.assert()
		.success()
		// The pinned build ran, with the whole command line passed through — the
		// global flag included, so the build being handed to reads it the same way.
		.stdout(predicates::str::contains("pinned-binary"))
		.stdout(predicates::str::contains("--release-base-url"))
		.stdout(predicates::str::contains("run greet"))
		.stderr(predicates::str::contains("this repo pins"))
		.stderr(predicates::str::contains("switching"));

	assert!(fx.exists(".lattice/bin/lattice-9.9.9"));
	assert_eq!(fx.stable_link_target(), "lattice-9.9.9");
}

/// The second acceptance criterion: a version already on disk is a symlink swap.
#[test]
#[cfg(unix)]
fn a_version_already_on_disk_is_not_downloaded_again() {
	if !curl_supports_file() {
		return;
	}
	let bin = bin_version();
	let fx = fixture();
	fx.config(&CONFIG.replace("0.1.0", "9.9.9"));
	fx.install_managed(&bin);

	let release = FakeRelease::new();
	release.publish("9.9.9", "pinned-binary");
	fx.managed_lattice(&bin)
		.args(["--release-base-url", &release.base_url(), "run", "greet"])
		.assert()
		.success();

	// Point the stable link back at the older binary, as a branch switch would,
	// and take the release away entirely.
	fx.install_managed(&bin);
	release.unpublish("9.9.9");

	fx.managed_lattice(&bin)
		.args(["--release-base-url", &release.base_url(), "run", "greet"])
		.assert()
		.success()
		.stdout(predicates::str::contains("pinned-binary"))
		.stderr(predicates::str::contains("switching"));

	assert_eq!(fx.stable_link_target(), "lattice-9.9.9");
}

#[test]
#[cfg(unix)]
fn the_switch_can_be_turned_off_per_invocation() {
	let bin = bin_version();
	let fx = fixture();
	fx.config(&CONFIG.replace("0.1.0", "9.9.9"));
	fx.install_managed(&bin);

	// No release is published, so a switch attempt would fail outright. All three
	// opt-outs have to leave the invoked binary running instead.
	fx.managed_lattice(&bin)
		.args(["--no-version-check", "run", "greet", "--no-cache"])
		.assert()
		.success()
		.stdout(predicates::str::contains("app:greet: done"));

	fx.managed_lattice(&bin)
		.env("LATTICE_NO_VERSION_CHECK", "1")
		.args(["run", "greet", "--no-cache"])
		.assert()
		.success()
		.stdout(predicates::str::contains("app:greet: done"));

	fx.config(&CONFIG.replace("0.1.0", "9.9.9").replace(
		r#""tasks""#,
		r#""settings": { "versionCheck": false }, "tasks""#,
	));
	fx.managed_lattice(&bin)
		.args(["run", "greet", "--no-cache"])
		.assert()
		.success()
		.stdout(predicates::str::contains("app:greet: done"));
}

#[test]
#[cfg(unix)]
fn a_missing_pinned_version_fails_loudly_rather_than_running_the_wrong_one() {
	if !curl_supports_file() {
		return;
	}
	let bin = bin_version();
	let fx = fixture();
	fx.config(&CONFIG.replace("0.1.0", "9.9.9"));
	fx.install_managed(&bin);

	let release = FakeRelease::new();
	fx.managed_lattice(&bin)
		.args(["--release-base-url", &release.base_url(), "run", "greet"])
		.assert()
		.failure()
		.stderr(predicates::str::contains("9.9.9"))
		.stderr(predicates::str::contains("--no-version-check"));
}

#[test]
fn a_binary_lattice_did_not_install_is_never_swapped() {
	let fx = fixture();
	fx.config(&CONFIG.replace("0.1.0", "9.9.9"));

	// The compiled binary outside .lattice/bin stands in for a `cargo install`
	// build: the pin is honored by advice, not by replacing someone's binary.
	fx.lattice()
		.args(["run", "greet"])
		.assert()
		.success()
		.stdout(predicates::str::contains("app:greet: done"));
	assert!(
		!fx.exists(".lattice/bin"),
		"nothing should have been installed"
	);
}

#[test]
fn upgrade_outside_a_repo_says_so() {
	let fx = Fixture::new();
	fx.lattice()
		.args(["upgrade", "9.9.9"])
		.assert()
		.failure()
		.stderr(predicates::str::contains("no lattice.json"));
}
