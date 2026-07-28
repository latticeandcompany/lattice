//! `lattice upgrade` and the pinned-version handover, end to end.
//!
//! Every test publishes a fake release to a temp directory and points the CLI at
//! it with `LATTICE_RELEASE_BASE_URL`, so the real download, checksum, extract,
//! install and link path runs with no network.

mod common;

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
		.env("LATTICE_RELEASE_BASE_URL", release.base_url())
		.args(["upgrade", "9.9.9"])
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
		.env("LATTICE_RELEASE_BASE_URL", release.base_url())
		.args(["upgrade", "v9.9.9"])
		.assert()
		.success();

	// Second time: already pinned and already on disk, so it reports that and
	// does not need the release at all.
	release.unpublish("9.9.9");
	fx.lattice()
		.env("LATTICE_RELEASE_BASE_URL", release.base_url())
		.args(["upgrade", "9.9.9"])
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
		.env("LATTICE_RELEASE_BASE_URL", release.base_url())
		.env("LATTICE_RELEASE_LATEST_URL", release.latest_url("9.9.9"))
		.args(["upgrade", "latest"])
		.assert()
		.success()
		.stdout(predicates::str::contains("9.9.9"));

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
		.env("LATTICE_RELEASE_BASE_URL", release.base_url())
		.args(["upgrade", "9.9.9"])
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
		.env("LATTICE_RELEASE_BASE_URL", release.base_url())
		.args(["run", "greet"])
		.assert()
		.success()
		// The pinned build ran, with the arguments passed through.
		.stdout(predicates::str::contains("pinned-binary run greet"))
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
		.env("LATTICE_RELEASE_BASE_URL", release.base_url())
		.args(["run", "greet"])
		.assert()
		.success();

	// Point the stable link back at the older binary, as a branch switch would,
	// and take the release away entirely.
	fx.install_managed(&bin);
	release.unpublish("9.9.9");

	fx.managed_lattice(&bin)
		.env("LATTICE_RELEASE_BASE_URL", release.base_url())
		.args(["run", "greet"])
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
		.env("LATTICE_RELEASE_BASE_URL", release.base_url())
		.args(["run", "greet"])
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
