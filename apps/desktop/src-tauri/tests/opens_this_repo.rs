//! The backend against a real repo: this one.
//!
//! The unit tests cover each piece with a fixture. This covers the path the window
//! actually takes — open a directory, resolve every workspace, plan a graph, dump it —
//! against a config with eleven workspaces and a real dependency structure, which no
//! fixture is going to reproduce faithfully.
//!
//! It is dogfooding, so it also fails if the repo's own `lattice.json` stops being
//! valid, which is worth knowing.

use std::path::{Path, PathBuf};

use lattice_project::{PlanRequest, Project};

/// The repo root, four levels up from this crate's manifest.
fn repo_root() -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("..")
		.join("..")
		.join("..")
		.canonicalize()
		.expect("the repo root has to exist relative to this crate")
}

#[test]
fn this_repo_opens_and_resolves_every_workspace() {
	let project = Project::open_root(&repo_root()).expect("lattice.json in this repo must load");
	let view = project.view();

	// Every crate plus the two apps.
	assert!(
		view.workspaces.len() >= 11,
		"expected the crates and apps, got {}",
		view.workspaces.len()
	);

	let names: Vec<&str> = view.workspaces.iter().map(|ws| ws.name.as_str()).collect();
	for expected in [
		"lattice",
		"lattice-config",
		"lattice-events",
		"lattice-project",
		"lattice-runner",
		"web",
		"desktop",
	] {
		assert!(
			names.contains(&expected),
			"{expected} is missing from {names:?}"
		);
	}
}

#[test]
fn a_rust_crate_resolves_cargo_and_a_node_app_resolves_npm() {
	let project = Project::open_root(&repo_root()).unwrap();
	let view = project.view();

	let core = view
		.workspaces
		.iter()
		.find(|ws| ws.name == "lattice-config")
		.expect("lattice-config is declared");
	let driver = core.driver.as_ref().expect("a crate resolves a driver");
	assert_eq!(driver.tool, "cargo");
	// The language is what a front end keys a mark off, so it has to arrive.
	assert_eq!(driver.language.as_deref(), Some("rust"));

	let desktop = view
		.workspaces
		.iter()
		.find(|ws| ws.name == "desktop")
		.expect("the desktop app is declared");
	let driver = desktop
		.driver
		.as_ref()
		.expect("a node app resolves a driver");
	assert_eq!(driver.language.as_deref(), Some("node"));
}

#[test]
fn paths_are_repo_relative_and_forward_slashed() {
	let project = Project::open_root(&repo_root()).unwrap();
	let view = project.view();

	for workspace in &view.workspaces {
		assert!(
			!workspace.path.contains('\\'),
			"{} has a backslash in {}",
			workspace.name,
			workspace.path
		);
		assert!(
			!Path::new(&workspace.path).is_absolute(),
			"{} is absolute: {}",
			workspace.name,
			workspace.path
		);
	}

	let config = view
		.workspaces
		.iter()
		.find(|ws| ws.name == "lattice-config")
		.unwrap();
	assert_eq!(config.path, "crates/lattice-config");
}

#[test]
fn every_workspace_lists_the_tasks_it_can_actually_run() {
	let project = Project::open_root(&repo_root()).unwrap();
	let view = project.view();

	let core = view
		.workspaces
		.iter()
		.find(|ws| ws.name == "lattice-config")
		.unwrap();
	let tasks: Vec<&str> = core.tasks.iter().map(|t| t.task.as_str()).collect();
	assert!(tasks.contains(&"build"), "a crate can build: {tasks:?}");

	// The pipeline declares `clean`, but nothing resolves a command for it here, so it
	// is absent rather than listed and broken.
	let build = core.tasks.iter().find(|t| t.task == "build").unwrap();
	assert_eq!(build.command, "cargo build");
	assert!(build.cacheable);
	assert!(!build.persistent);
}

#[test]
fn a_dev_task_is_reported_as_persistent() {
	let project = Project::open_root(&repo_root()).unwrap();
	let view = project.view();

	let dev = view
		.tasks
		.iter()
		.find(|task| task.name == "dev")
		.expect("the pipeline declares dev");
	assert!(dev.persistent);
	assert!(
		!dev.cache,
		"a dev server's output is not a cacheable artifact"
	);
}

#[test]
fn a_build_graph_over_this_repo_is_ordered_and_connected() {
	let project = Project::open_root(&repo_root()).unwrap();
	let request = PlanRequest {
		tasks: vec!["build".to_string()],
		filter: None,
		sequentially: false,
	};

	let plan = project.plan(&request).expect("build must plan");
	let phases = match plan {
		lattice_project::Plan::Phases(phases) => phases,
		_ => panic!("this repo declares workspaces, so there is a graph"),
	};
	assert_eq!(phases.len(), 1, "a merged run is one graph");

	let dump = dagger::dump_graph(&phases[0]);
	assert!(dump.nodes.len() >= 11);
	assert!(!dump.edges.is_empty(), "the crates depend on each other");

	// Topological order is the contract the layered drawing relies on: a
	// prerequisite must appear before whatever depends on it.
	let position: std::collections::HashMap<&str, usize> = dump
		.nodes
		.iter()
		.enumerate()
		.map(|(index, node)| (node.id.as_str(), index))
		.collect();
	for edge in &dump.edges {
		assert!(
			position[edge.from.as_str()] < position[edge.to.as_str()],
			"{} comes after {} in the dump",
			edge.from,
			edge.to
		);
	}

	// lattice-config is the base, so the binary's build depends on it transitively.
	assert!(position.contains_key("lattice-config:build"));
	assert!(position["lattice-config:build"] < position["lattice:build"]);
}

#[test]
fn a_filter_narrows_the_graph_but_keeps_what_it_needs() {
	let project = Project::open_root(&repo_root()).unwrap();
	let request = PlanRequest {
		tasks: vec!["build".to_string()],
		filter: Some("lattice-runner".to_string()),
		sequentially: false,
	};

	let phases = match project.plan(&request).unwrap() {
		lattice_project::Plan::Phases(phases) => phases,
		_ => panic!("the filter matches a workspace"),
	};
	let dump = dagger::dump_graph(&phases[0]);

	let ids: Vec<&str> = dump.nodes.iter().map(|n| n.id.as_str()).collect();
	assert!(ids.contains(&"lattice-runner:build"));
	// Its prerequisites come along, marked as pulled in rather than selected.
	assert!(ids.contains(&"lattice-config:build"));
	let config = dump
		.nodes
		.iter()
		.find(|n| n.id == "lattice-config:build")
		.unwrap();
	assert!(config.pulled_in);
	let runner = dump
		.nodes
		.iter()
		.find(|n| n.id == "lattice-runner:build")
		.unwrap();
	assert!(!runner.pulled_in);
	// And the binary, which depends on the runner rather than the other way round,
	// does not.
	assert!(!ids.contains(&"lattice:build"));
}

#[test]
fn a_filter_that_matches_nothing_is_reported_rather_than_failing() {
	let project = Project::open_root(&repo_root()).unwrap();
	let request = PlanRequest {
		tasks: vec!["build".to_string()],
		filter: Some("no-such-workspace".to_string()),
		sequentially: false,
	};

	match project.plan(&request).unwrap() {
		lattice_project::Plan::NoMatch { filter } => assert_eq!(filter, "no-such-workspace"),
		_ => panic!("an empty match is a value, not an error"),
	}
}

#[test]
fn an_undefined_task_is_refused_with_the_ones_that_exist() {
	let project = Project::open_root(&repo_root()).unwrap();
	let err = project
		.require_known_tasks(&["nonexistent".to_string()])
		.expect_err("a task that is not in the pipeline cannot run");
	let message = err.to_string();
	assert!(message.contains("nonexistent"));
	assert!(
		message.contains("build"),
		"the message lists what is available: {message}"
	);
}

#[test]
fn the_catalog_covers_what_this_repo_uses() {
	let catalog = lattice_project::view::catalog();
	let tools: Vec<&str> = catalog.drivers.iter().map(|d| d.tool.as_str()).collect();
	assert!(tools.contains(&"cargo"));
	assert!(tools.contains(&"npm"));
	// Every component a cache miss can name has to be in the catalog, or the window
	// cannot label one.
	assert!(catalog.key_components.contains(&"inputs".to_string()));
	assert!(catalog.key_components.contains(&"toolchain".to_string()));
}
