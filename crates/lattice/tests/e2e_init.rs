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
	fx.write(
		"apps/web/package.json",
		r#"{ "name": "web", "scripts": { "build": "tsc" } }"#,
	);
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
	fx.write(
		"apps/web/package.json",
		r#"{ "name": "web", "scripts": { "build": "tsc" } }"#,
	);
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

/// A declared workspace whose manifest plainly runs scripts, and does not have
/// the one being asked for, is the shape a typo takes — so a run says so, once,
/// and points at the name it nearly matches. A manifest that declares no scripts
/// at all is a complete config and stays out of it.
#[test]
fn a_run_names_a_task_the_manifest_meant_to_declare_and_did_not() {
	let fx = Fixture::new();
	fx.write(
		"apps/web/package.json",
		r#"{ "name": "web", "scripts": { "biuld": "tsc" } }"#,
	);
	fx.write("apps/web/pnpm-lock.yaml", "");
	// No `scripts` at all: a types-only package, and nothing to report about it.
	fx.write("packages/types/package.json", r#"{ "name": "types" }"#);
	fx.write("packages/types/package-lock.json", "{}");

	// Declared rather than scanned: init imports the script names a manifest
	// actually has, so a repo whose only script is the typo gets a `biuld` task
	// and never reaches this. The case here is the other one — a pipeline that
	// declares `build` because the rest of the repo runs it, and one workspace
	// that spelled it wrong.
	fx.config(
		r#"{
  "latticeVersion": "0.1.0",
  "workspaces": [
    { "name": "web", "path": "apps/web" },
    { "name": "types", "path": "packages/types" }
  ],
  "tasks": { "build": { "dependsOn": ["^build"] } }
}
"#,
	);

	for args in [
		vec!["run", "build"],
		vec!["run", "build", "--dry-run"],
		vec!["run", "build", "-l"],
	] {
		let out = fx.lattice().args(&args).assert().success();
		let stderr = String::from_utf8_lossy(&out.get_output().stderr).to_string();
		assert_eq!(
			stderr.matches("declares scripts but no").count(),
			1,
			"`lattice {}` said it {} times:\n{stderr}",
			args.join(" "),
			stderr.matches("declares scripts but no").count()
		);
		assert!(
			stderr.contains(r#"web declares scripts but no "build""#),
			"`lattice {}`:\n{stderr}",
			args.join(" ")
		);
		assert!(
			stderr.contains(r#"Did you mean "biuld"?"#),
			"`lattice {}`:\n{stderr}",
			args.join(" ")
		);
		assert!(
			!stderr.contains("types"),
			"a manifest with no scripts is a complete config:\n{stderr}"
		);
	}
}

/// The whole of a repo's task runner config, not the one task Lattice used to
/// assume. A repo that declares `lint`, `test` and `typecheck` and gets a config
/// with only `build` has ten commands that no longer run, and nothing said so.
///
/// The stand-in runner lives in `node_modules/.bin`, where a package manager
/// puts a dependency it installed, and the fixture's own `bin/` is deliberately
/// left off `PATH`: finding it is the second half of what this covers.
#[test]
fn init_imports_the_pipeline_the_repo_already_runs() {
	let fx = Fixture::new();
	fx.install_stub_bin_in("node_modules/.bin", "turbo-stub", "turbo");
	fx.write(
		"package.json",
		r#"{ "name": "demo", "private": true, "workspaces": ["packages/*"] }"#,
	);
	fx.write("package-lock.json", "{}");
	fx.write("tsconfig.base.json", "{}");
	fx.write(
		"turbo.json",
		r#"{
			"globalDependencies": ["tsconfig.base.json"],
			"tasks": {
				// turbo.json is read as JSONC, because turbo reads it that way.
				"build": { "dependsOn": ["^build"], "outputs": ["packages/*/dist/**"] },
				"lint": {},
				"test": { "dependsOn": ["build"] },
				"dev": { "persistent": true, "cache": false }
			}
		}"#,
	);
	// Packages for the stand-in runner to fan out over. No manifests, so the
	// root stays the only workspace the scan proposes.
	fx.write("packages/ui/src/index.js", "export const ui = 1;\n");

	fx.lattice().args(["init", "-y"]).assert().success();

	let config: Value =
		serde_json::from_str(&fx.read("lattice.json")).expect("lattice.json parses");
	let tasks = config["tasks"].as_object().expect("tasks is an object");
	let mut names: Vec<&str> = tasks.keys().map(String::as_str).collect();
	names.sort_unstable();
	assert_eq!(names, vec!["build", "dev", "lint", "test"]);
	assert_eq!(tasks["dev"]["persistent"], serde_json::json!(true));
	assert_eq!(tasks["test"]["dependsOn"], serde_json::json!(["build"]));
	assert_eq!(
		config["globalDependencies"],
		serde_json::json!(["tsconfig.base.json"])
	);

	// Every imported task runs, through a runner only the project installed.
	for task in ["build", "lint", "test"] {
		fx.lattice().args(["run", task, "-l"]).assert().success();
	}
	assert!(
		fx.exists("packages/ui/dist/bundle.js"),
		"the project's own runner produced the build"
	);
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
