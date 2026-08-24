//! Its own test binary on purpose. Raising a signal is process-wide and every
//! `execute_tasks` in the process watches for one, so a test that raises
//! `SIGTERM` alongside the rest of the suite would cancel whatever else was
//! running at the time.

#![cfg(unix)]

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use dagger::build_execution_graph;
use lattice_config::{LatticeConfig, PipelineTask};
use lattice_events::{Reporter, TaskEvent};
use lattice_runner::{execute_tasks, ExecuteOptions, RunInterrupted};
use lattice_testkit as sh;
use lattice_workspace::Workspace;

struct StartWatcher {
	started: Arc<AtomicBool>,
}

impl Reporter for StartWatcher {
	fn run_start(&self, _task: &str, _workspaces: usize) {}
	fn event(&self, ev: TaskEvent) {
		if matches!(ev, TaskEvent::Started { .. }) {
			self.started.store(true, Ordering::SeqCst);
		}
	}
	fn surface_failure(&self, _workspace: &str, _task: &str, _captured: &[(bool, String)]) {}
	fn run_summary(&self, _total: usize, _cached: usize, _failed: usize, _elapsed_ms: u64) {}
	fn note(&self, _msg: &str) {}
	fn warn(&self, _msg: &str) {}
	fn finish(&self) {}
}

fn workspace(name: &str, root: &std::path::Path, task: &str, command: &str) -> Workspace {
	let path = root.join(name);
	std::fs::create_dir_all(&path).unwrap();
	let mut commands = indexmap::IndexMap::new();
	commands.insert(task.to_string(), command.to_string());
	Workspace {
		name: name.to_string(),
		path,
		auto: false,
		depends_on: Vec::new(),
		engines: lattice_config::EngineMap::new(),
		driver: None,
		commands,
	}
}

/// A cancelled CI job arrives as `SIGTERM`, not as Ctrl-C. The runner's own
/// watcher always covered both, but the wait that a persistent task holds open
/// consulted neither it nor the flags it sets — and the shutdown future the CLI
/// supplies watched `SIGINT` alone. A cancelled job sat there streaming dev
/// server output until the runner was force-killed.
#[tokio::test(flavor = "multi_thread")]
async fn sigterm_ends_a_run_that_persistent_tasks_are_holding_open() {
	// Registering before anything raises is what keeps the signal from falling
	// through to the default disposition and killing the test binary.
	let _installed = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
		.expect("SIGTERM is catchable");

	let tmp = tempfile::tempdir().unwrap();
	let root = tmp.path();
	let workspaces = vec![workspace("app", root, "dev", &sh::sleep(120))];
	let mut config = LatticeConfig::default();
	config.tasks.insert(
		"dev".to_string(),
		PipelineTask {
			persistent: Some(true),
			..Default::default()
		},
	);
	let graph = build_execution_graph(&workspaces, "dev", &config).unwrap();

	let started = Arc::new(AtomicBool::new(false));
	let reporter = StartWatcher {
		started: started.clone(),
	};

	// Held until the child is up, then repeated: tokio only delivers a signal to
	// the streams that already exist when it arrives, and the runner creates its
	// own as the run starts.
	let raiser = tokio::spawn(async move {
		while !started.load(Ordering::SeqCst) {
			tokio::time::sleep(Duration::from_millis(20)).await;
		}
		loop {
			tokio::time::sleep(Duration::from_millis(250)).await;
			// SAFETY: `raise(3)` sends the signal to this process, which has a
			// handler installed for it above.
			unsafe { libc::raise(libc::SIGTERM) };
		}
	});

	// The same future the CLI hands over.
	let shutdown: Pin<Box<dyn Future<Output = ()> + Send>> =
		Box::pin(lattice_runner::interrupt_signal());

	let run = execute_tasks(ExecuteOptions {
		graph: &graph,
		workspaces: &workspaces,
		config: &config,
		root,
		no_cache: true,
		no_store: true,
		concurrency: None,
		keep_going: false,
		reporter: &reporter,
		lattice_version: "0.1.0-test",
		shutdown: Some(shutdown),
		cancel: None,
	});

	let err = tokio::time::timeout(Duration::from_secs(30), run)
		.await
		.expect("a SIGTERM has to end the run the way a cancelled CI job means it to")
		.unwrap_err();

	raiser.abort();
	assert!(
		err.downcast_ref::<RunInterrupted>().is_some(),
		"an interrupted run reports the interruption: {err:#}"
	);
}
