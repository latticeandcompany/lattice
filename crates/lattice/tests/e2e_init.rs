//! E2E tests for `lattice init`: the repo scan, scaffolding, the overwrite
//! guard, idempotent `.gitignore` maintenance, and the committed
//! `.lattice/schema.json`.

mod common;

use common::Fixture;
use predicates::prelude::*;
use serde_json::Value;

#[test]
fn init_scaffolds_a_repo_and_runs_cleanly() {
	let fx = Fixture::new();

	fx.lattice().args(["init", "-y"]).assert().success();

	assert!(fx.exists("lattice.json"), "lattice.json written");
	assert!(
		fx.exists(".lattice/schema.json"),
		".lattice/schema.json written"
	);
	assert!(fx.exists(".gitignore"), ".gitignore written");

	// The skeleton declares zero workspaces, so `run build` has nothing to do:
	// it must exit 0 with an actionable notice and no panic.
	fx.lattice()
		.args(["run", "build", "-l"])
		.assert()
		.success()
		.stdout(predicate::str::contains("no workspaces declared"))
		.stderr(predicate::str::contains("panicked").not())
		.stderr(predicate::str::contains("RUST_BACKTRACE").not());
}

#[test]
fn init_declares_the_workspaces_it_finds() {
	let fx = Fixture::new();
	fx.write("apps/web/package.json", r#"{ "name": "web" }"#);
	fx.write("apps/web/pnpm-lock.yaml", "");
	fx.write("services/api/Cargo.toml", "[package]\nname = \"api\"\n");
	fx.write("services/api/Cargo.lock", "");
	// Neither of these is a workspace: one is a dependency tree, the other is
	// gitignored.
	fx.write("apps/web/node_modules/dep/package.json", "{}");
	fx.write(".gitignore", "generated/\n");
	fx.write("generated/proto/package.json", "{}");

	fx.lattice().args(["init", "-y"]).assert().success();

	let config: Value =
		serde_json::from_str(&fx.read("lattice.json")).expect("lattice.json parses");
	assert_eq!(
		config["workspaces"],
		serde_json::json!([
			{ "name": "web", "path": "apps/web" },
			{ "name": "api", "path": "services/api" }
		])
	);

	// The scanned config drives a real run: both workspaces resolve a driver.
	fx.lattice()
		.args(["run", "build", "-l", "--dry-run"])
		.assert()
		.success()
		.stdout(predicate::str::contains("web"))
		.stdout(predicate::str::contains("api"));
}

#[test]
fn init_leaves_out_what_it_cannot_drive_and_says_so() {
	let fx = Fixture::new();
	fx.write("apps/web/package.json", r#"{ "name": "web" }"#);
	fx.write("apps/web/package-lock.json", "{}");
	// A bare Cargo.toml with the lock at the repo root is not enough evidence
	// to drive tasks. Declaring it would halt the very next run.
	fx.write("crates/core/Cargo.toml", "[package]\nname = \"core\"\n");

	fx.lattice()
		.args(["init", "-y"])
		.assert()
		.success()
		.stdout(predicate::str::contains("crates/core"))
		.stdout(predicate::str::contains("driver resolved"));

	let config: Value =
		serde_json::from_str(&fx.read("lattice.json")).expect("lattice.json parses");
	assert_eq!(
		config["workspaces"],
		serde_json::json!([{ "name": "web", "path": "apps/web" }]),
		"only the workspace with a resolved driver is declared"
	);

	// The point of holding it back: what init writes actually runs.
	fx.lattice()
		.args(["run", "build", "-l", "--dry-run"])
		.assert()
		.success()
		.stdout(predicate::str::contains("npm run build"));
}

#[test]
fn init_pins_the_tool_versions_the_repo_already_records() {
	let fx = Fixture::new();
	fx.write("apps/web/package.json", r#"{ "name": "web" }"#);
	fx.write("apps/web/pnpm-lock.yaml", "");
	fx.write(".nvmrc", "v22.11.0\n");
	fx.write("rust-toolchain.toml", "[toolchain]\nchannel = \"1.83.0\"\n");

	fx.lattice().args(["init", "-y"]).assert().success();

	let config: Value =
		serde_json::from_str(&fx.read("lattice.json")).expect("lattice.json parses");
	assert_eq!(config["engines"]["node"], serde_json::json!("22.11.0"));
	assert_eq!(config["engines"]["rust"], serde_json::json!("1.83.0"));
}

#[test]
fn init_on_a_bare_directory_still_writes_the_skeleton() {
	// There is no one to prompt under `-y`, so a scan that finds nothing falls
	// back to the skeleton rather than failing the pipeline.
	let fx = Fixture::new();
	fx.lattice().args(["init", "-y"]).assert().success();

	let config: Value =
		serde_json::from_str(&fx.read("lattice.json")).expect("lattice.json parses");
	assert_eq!(config["workspaces"], serde_json::json!([]));
}

#[test]
fn init_force_guard() {
	let fx = Fixture::new();

	fx.lattice().args(["init", "-y"]).assert().success();

	// Second init without --force is rejected.
	fx.lattice()
		.args(["init", "-y"])
		.assert()
		.failure()
		.stderr(predicate::str::contains("already exists"));

	// With --force it overwrites and succeeds.
	fx.lattice()
		.args(["init", "-y", "--force"])
		.assert()
		.success();
}

#[test]
fn gitignore_lines_are_idempotent_across_reinits() {
	let fx = Fixture::new();

	fx.lattice().args(["init", "-y"]).assert().success();
	fx.lattice()
		.args(["init", "-y", "--force"])
		.assert()
		.success();

	let gi = fx.read(".gitignore");
	assert_eq!(
		gi.matches(".lattice/cache/").count(),
		1,
		"exactly one cache ignore line, got:\n{gi}"
	);
	assert_eq!(
		gi.matches(".lattice/toolchains/").count(),
		1,
		"exactly one toolchains ignore line, got:\n{gi}"
	);
}

#[test]
fn schema_json_is_committed_and_referenced() {
	let fx = Fixture::new();
	fx.lattice().args(["init", "-y"]).assert().success();

	// The committed schema exists and is a JSON Schema (draft key present).
	let schema: Value =
		serde_json::from_str(&fx.read(".lattice/schema.json")).expect("schema.json parses as JSON");
	let draft = schema
		.get("$schema")
		.and_then(Value::as_str)
		.expect("schema.json has a $schema draft key");
	assert!(
		draft.contains("json-schema.org"),
		"unexpected $schema draft: {draft}"
	);

	// The scaffolded config's own `$schema` points at the committed file.
	let config: Value =
		serde_json::from_str(&fx.read("lattice.json")).expect("lattice.json parses");
	assert_eq!(
		config.get("$schema").and_then(Value::as_str),
		Some(".lattice/schema.json"),
	);
}
