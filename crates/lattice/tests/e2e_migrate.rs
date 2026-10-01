//! E2E tests for moving a repo off a 1.x `lattice.json`: the hint a 1.x config
//! gets, and `lattice migrate` rewriting it into one that runs the same graph.

mod common;

use common::Fixture;
use lattice_testkit as sh;
use predicates::prelude::*;

/// Two directories with no manifest, whose builds record the order they ran in.
fn v1_repo() -> Fixture {
	let fx = Fixture::new();
	fx.mkdir("lib");
	fx.mkdir("app");
	fx.config_from(
		r#"{
          "latticeVersion": "0.1.0",
          "workspaces": [
            { "name": "lib", "path": "lib", "auto": false, "scripts": { "build": @lib@ } },
            { "name": "app", "path": "app", "auto": false, "dependsOn": ["lib"],
              "scripts": { "build": @app@ } }
          ],
          "tasks": { "build": { "dependsOn": ["^build"] } }
        }"#,
		&[
			("lib", sh::append("../order.txt", "lib")),
			("app", sh::append("../order.txt", "app")),
		],
	);
	fx
}

#[test]
fn a_v1_config_is_pointed_at_migrate() {
	let fx = v1_repo();
	fx.lattice()
		.args(["run", "build"])
		.assert()
		.failure()
		.stderr(predicate::str::contains("Lattice 1.x"))
		.stderr(predicate::str::contains("lattice migrate"));
	assert!(
		!fx.exists("order.txt"),
		"nothing may run on a config it cannot read"
	);
}

#[test]
fn migrate_rewrites_the_config_and_the_graph_still_runs_in_order() {
	let fx = v1_repo();
	let before = fx.read("lattice.json");

	fx.lattice()
		.arg("migrate")
		.assert()
		.success()
		.stdout(predicate::str::contains("rewrote lattice.json"));

	let after = fx.read("lattice.json");
	assert_ne!(before, after);
	let config: serde_json::Value = serde_json::from_str(&after).expect("the new config parses");
	assert_eq!(config["workspaces"], serde_json::json!(["lib", "app"]));
	assert_eq!(
		config["overrides"]["app"]["dependsOn"],
		serde_json::json!(["lib"])
	);

	fx.lattice().args(["run", "build"]).assert().success();
	let order: Vec<String> = fx
		.read("order.txt")
		.lines()
		.map(|l| l.trim().to_string())
		.collect();
	assert_eq!(order, ["lib", "app"]);
}

#[test]
fn migrate_dry_run_prints_the_config_and_writes_nothing() {
	let fx = v1_repo();
	let before = fx.read("lattice.json");

	fx.lattice()
		.args(["migrate", "--dry-run"])
		.assert()
		.success()
		.stdout(predicate::str::contains(r#""workspaces": ["lib", "app"]"#));

	assert_eq!(fx.read("lattice.json"), before);
}

/// What 1.x `init` wrote in an empty directory reads as this version's shape,
/// but its pin still names 1.x.
#[test]
fn an_empty_v1_config_gets_its_pin_moved() {
	let fx = Fixture::new();
	fx.config(r#"{ "latticeVersion": "1.1.1", "workspaces": [] }"#);
	fx.lattice()
		.arg("migrate")
		.assert()
		.success()
		.stdout(predicate::str::contains("pinned latticeVersion to"))
		.stdout(predicate::str::contains("(was 1.1.1)"));
	assert!(!fx.read("lattice.json").contains("1.1.1"));
	fx.lattice()
		.arg("migrate")
		.assert()
		.success()
		.stdout(predicate::str::contains("already written for this version"));
}

#[test]
fn migrating_a_current_config_changes_nothing() {
	let fx = v1_repo();
	fx.lattice().arg("migrate").assert().success();
	let migrated = fx.read("lattice.json");

	fx.lattice()
		.arg("migrate")
		.assert()
		.success()
		.stdout(predicate::str::contains("already written for this version"));
	assert_eq!(fx.read("lattice.json"), migrated);
}
