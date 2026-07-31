//! The Lattice task runner: an in-degree scheduler over the execution DAG that
//! runs each task through the platform shell, wires the content-addressed cache,
//! and keeps long-running (`persistent`) tasks from blocking the graph while
//! still watching them for an exit.
//!
//! The runner is deliberately I/O-only with respect to presentation: it emits
//! typed [`lattice_output::TaskEvent`]s and calls the [`lattice_output::Reporter`]
//! hooks. It never touches `console`, `indicatif`, or `println!` for task status.

use std::collections::{BTreeSet, HashMap};
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use anyhow::Result;
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::sync::mpsc::{self, UnboundedSender};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use dagger::{build_schedule, ExecutionGraph, Schedule};
use lattice_cache::{compute_key, CacheMeta, CacheStore, HashInputs, LocalStore};
use lattice_config::{resolve_engines, LatticeConfig, PipelineTask};
use lattice_output::{Reporter, TaskEvent};
use lattice_workspace::toolchain;
use lattice_workspace::Workspace;

/// Cap on how many child-output lines a single failing task retains for the
/// expand-on-failure surface. Beyond this, lines are still streamed live but not
/// buffered, bounding memory for pathological tasks.
const MAX_CAPTURED_LINES: usize = 5000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunResult {
	pub total: usize,
	pub cached: usize,
	pub failed: usize,
	pub elapsed_ms: u64,
}

/// Error returned by keep-going mode when one or more tasks failed. Carries the
/// full [`RunResult`] so callers can still print an accurate summary line.
#[derive(Debug)]
pub struct RunFailure {
	pub result: RunResult,
	/// Downstream tasks skipped because a prerequisite failed.
	pub skipped: usize,
}

impl std::fmt::Display for RunFailure {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let n = self.result.failed;
		write!(
			f,
			"{} task{} failed (kept going)",
			n,
			if n == 1 { "" } else { "s" }
		)?;
		if self.skipped > 0 {
			write!(
				f,
				"; {} downstream task{} skipped",
				self.skipped,
				if self.skipped == 1 { "" } else { "s" }
			)?;
		}
		Ok(())
	}
}

impl std::error::Error for RunFailure {}

/// Arguments for [`execute_tasks`].
pub struct ExecuteOptions<'a> {
	pub graph: &'a ExecutionGraph,
	pub workspaces: &'a [Workspace],
	pub config: &'a LatticeConfig,
	pub root: &'a std::path::Path,
	pub no_cache: bool,
	pub concurrency: Option<usize>,
	pub keep_going: bool,
	pub reporter: &'a dyn Reporter,
	pub lattice_version: &'a str,
	/// Fires to tear down still-running persistent tasks after the graph drains.
	/// The CLI passes `ctrl_c()`; tests pass a short timer. `None` => if
	/// persistent tasks remain, wait on `ctrl_c` internally.
	pub shutdown: Option<Pin<Box<dyn Future<Output = ()> + Send>>>,
}

/// A plain, `Send`-able description of one task to execute. Extracted from the
/// (non-`Send`) [`ExecutionGraph`] so it can be moved into spawned tasks.
#[derive(Clone)]
struct TaskSpec {
	workspace_name: String,
	task_name: String,
	command: String,
	is_persistent: bool,
	ws_path: PathBuf,
	pipeline_task: PipelineTask,
	/// This workspace's resolved toolchain `PATH`-prepend dirs.
	path_prepend: Vec<PathBuf>,
	/// This workspace's resolved toolchain identity (part of the cache key).
	toolchain_identity: String,
}

/// Everything one spawned task needs. The shared handles are cheap to clone
/// (all `Arc`/channel senders); the [`TaskSpec`] is per-task.
struct TaskRunContext {
	spec: TaskSpec,
	store: Arc<dyn CacheStore>,
	semaphore: Arc<Semaphore>,
	abort: Arc<AtomicBool>,
	no_cache: bool,
	lattice_version: Arc<str>,
	tx: UnboundedSender<RunnerMsg>,
	persistent_watches: Arc<Mutex<Vec<PersistentWatch>>>,
	shutting_down: Arc<AtomicBool>,
}

/// A persistent child process, owned by the reaper task that waits on it.
struct PersistentChild {
	child: tokio::process::Child,
	#[cfg(unix)]
	pgid: Option<i32>,
}

impl PersistentChild {
	/// On unix, signal the child's whole process group so a server launched
	/// through a shell dies along with the shell. Elsewhere, nothing: only the
	/// direct child can be killed.
	fn kill_group(&self) {
		#[cfg(unix)]
		if let Some(pgid) = self.pgid {
			// SAFETY: `kill(2)` with a negative pid signals the process group;
			// an invalid/already-dead group is a harmless ESRCH.
			unsafe {
				libc::kill(-pgid, libc::SIGKILL);
			}
		}
	}

	/// Kill the child and its group, then reap it.
	async fn terminate(mut self) {
		self.kill_group();
		let _ = self.child.start_kill();
		let _ = self.child.wait().await;
	}
}

/// The scheduler's half of a watched persistent child: a way to stop it once the
/// run is over, and the reaper task to wait on while it does.
struct PersistentWatch {
	kill: tokio::sync::oneshot::Sender<()>,
	reaper: tokio::task::JoinHandle<()>,
}

/// Everything [`reap_persistent`] needs to watch one persistent child.
struct ReapContext {
	child: PersistentChild,
	kill: tokio::sync::oneshot::Receiver<()>,
	tx: UnboundedSender<RunnerMsg>,
	shutting_down: Arc<AtomicBool>,
	workspace: String,
	task: String,
	start: Instant,
}

/// Reporter interactions originating inside spawned tasks. Forwarded to the
/// (borrowed) reporter by the main scheduler loop, so spawned tasks stay
/// `'static` and never touch the reporter directly.
enum RunnerMsg {
	Event(TaskEvent),
	SurfaceFailure {
		workspace: String,
		task: String,
		captured: Vec<(bool, String)>,
	},
	/// A warning about one task, label kept separate so the reporter can color it.
	TaskWarn {
		workspace: String,
		task: String,
		msg: String,
	},
	/// A loquacious-only trace line about one task, labeled the same way.
	TaskNote {
		workspace: String,
		task: String,
		msg: String,
	},
	/// A persistent child ended on its own. Sent by [`reap_persistent`]; the
	/// scheduler counts it, so it never goes straight to the reporter.
	PersistentExited {
		workspace: String,
		task: String,
		code: Option<i32>,
		duration_ms: u64,
	},
}

enum TaskOutcome {
	/// Ran to success (counts toward `total`).
	Ran,
	/// Restored from cache (counts toward `total` and `cached`).
	Cached,
	/// A persistent child was started and detached (counts toward `total`).
	Persistent,
	/// Failed: carries workspace/task for scheduling decisions. The captured
	/// output was already surfaced through the channel by `run_one`.
	Failed { workspace: String, task: String },
	/// A no-op (unknown workspace, or skipped after an abort). Not counted.
	Noop,
}

/// Hand one message to the reporter, keeping the two counters a persistent
/// child's exit moves: it stops holding the run open, and any exit that is not a
/// clean zero is a failure the summary has to show.
fn forward(
	reporter: &dyn Reporter,
	msg: RunnerMsg,
	live_persistent: &mut usize,
	failed: &mut usize,
) {
	match msg {
		RunnerMsg::PersistentExited {
			workspace,
			task,
			code,
			duration_ms,
		} => {
			*live_persistent = live_persistent.saturating_sub(1);
			if code != Some(0) {
				*failed += 1;
			}
			reporter.event(TaskEvent::PersistentExited {
				workspace,
				task,
				code,
				duration_ms,
			});
		}
		RunnerMsg::Event(ev) => reporter.event(ev),
		RunnerMsg::SurfaceFailure {
			workspace,
			task,
			captured,
		} => reporter.surface_failure(&workspace, &task, &captured),
		RunnerMsg::TaskWarn {
			workspace,
			task,
			msg,
		} => reporter.task_warn(&workspace, &task, &msg),
		RunnerMsg::TaskNote {
			workspace,
			task,
			msg,
		} => reporter.task_note(&workspace, &task, &msg),
	}
}

/// Decrement the in-degree of each dependent of `pos`; any reaching zero (and
/// not already completed) become ready.
fn unblock(
	pos: usize,
	schedule: &Schedule,
	remaining_indegree: &mut [usize],
	completed: &[bool],
	ready: &mut Vec<usize>,
) {
	for &dep in &schedule.dependents[pos] {
		if remaining_indegree[dep] > 0 {
			remaining_indegree[dep] -= 1;
			if remaining_indegree[dep] == 0 && !completed[dep] {
				ready.push(dep);
			}
		}
	}
}

/// Transitively mark every dependent of a failed node as completed (skipped),
/// emitting a `Skipped` event for each, so keep-going mode never schedules a
/// task whose prerequisites can no longer succeed. Returns the count skipped.
fn skip_dependents(
	pos: usize,
	schedule: &Schedule,
	completed: &mut [bool],
	specs: &[Option<TaskSpec>],
	reporter: &dyn Reporter,
) -> usize {
	let mut skipped = 0usize;
	let mut stack = vec![pos];
	while let Some(cur) = stack.pop() {
		for &dep in &schedule.dependents[cur] {
			if !completed[dep] {
				completed[dep] = true;
				skipped += 1;
				stack.push(dep);
				if let Some(s) = &specs[dep] {
					reporter.event(TaskEvent::Skipped {
						workspace: s.workspace_name.clone(),
						task: s.task_name.clone(),
						reason: "dependency failed".to_string(),
					});
				}
			}
		}
	}
	skipped
}

/// Execute the DAG.
///
/// See [`ExecuteOptions`] for the knobs. Returns [`RunResult`] on success; a
/// fail-fast run returns its first failure verbatim, and any other run that
/// counted a failure — keep-going, or a persistent child that exited on its own —
/// returns `Err(RunFailure)` carrying the summary.
pub async fn execute_tasks(opts: ExecuteOptions<'_>) -> Result<RunResult> {
	let ExecuteOptions {
		graph,
		workspaces,
		config,
		root,
		no_cache,
		concurrency,
		keep_going,
		reporter,
		lattice_version,
		shutdown,
	} = opts;

	let global_start = Instant::now();

	// Only the workspaces the graph actually has nodes for take part in this run:
	// a filtered run is handed the whole set so it can resolve the dependencies it
	// pulled in, and provisioning toolchains for the rest would be wasted work.
	let in_run: BTreeSet<&str> = graph
		.graph
		.node_weights()
		.map(|node| node.workspace_name.as_str())
		.collect();

	// For each workspace, merge root + workspace engines and provision/resolve
	// them into a PATH-prepend + identity. Memoize by the merged spec so
	// identical toolchains are only resolved (and installed) once.
	let mut tc_cache: HashMap<String, (Vec<PathBuf>, String)> = HashMap::new();
	let mut ws_prepend: HashMap<String, Vec<PathBuf>> = HashMap::new();
	let mut ws_identity: HashMap<String, String> = HashMap::new();
	for ws in workspaces
		.iter()
		.filter(|ws| in_run.contains(ws.name.as_str()))
	{
		let merged = resolve_engines(&config.engines, &ws.engines);
		let memo_key = format!("{merged:?}");
		let (pp, id) = if let Some(hit) = tc_cache.get(&memo_key) {
			hit.clone()
		} else {
			let resolved =
				toolchain::provision_and_resolve(root, &merged, &mut |m| reporter.note(m))?;
			let v = (resolved.path_prepend, resolved.identity);
			tc_cache.insert(memo_key, v.clone());
			v
		};
		ws_prepend.insert(ws.name.clone(), pp);
		ws_identity.insert(ws.name.clone(), id);
	}

	let ws_map: HashMap<&str, &Workspace> =
		workspaces.iter().map(|w| (w.name.as_str(), w)).collect();

	let (node_indices, schedule) = build_schedule(graph);
	let n = node_indices.len();

	let mut specs: Vec<Option<TaskSpec>> = Vec::with_capacity(n);
	let mut task_names: BTreeSet<String> = BTreeSet::new();
	for &ni in &node_indices {
		let node = &graph.graph[ni];
		task_names.insert(node.task_name.clone());
		let spec = ws_map.get(node.workspace_name.as_str()).map(|ws| {
			let pipeline_task = config
				.tasks
				.get(node.task_name.as_str())
				.cloned()
				.unwrap_or_default();
			TaskSpec {
				workspace_name: node.workspace_name.clone(),
				task_name: node.task_name.clone(),
				command: node.command.clone(),
				is_persistent: node.is_persistent,
				ws_path: ws.path.clone(),
				pipeline_task,
				path_prepend: ws_prepend.get(&ws.name).cloned().unwrap_or_default(),
				toolchain_identity: ws_identity.get(&ws.name).cloned().unwrap_or_default(),
			}
		});
		specs.push(spec);
	}

	let run_label = task_names.iter().cloned().collect::<Vec<_>>().join("+");
	reporter.run_start(&run_label, in_run.len());

	// Cap parallelism at the requested override, else the machine default.
	// Guard `Some(0)` so the semaphore always has permits.
	let concurrency = concurrency.filter(|&c| c > 0).unwrap_or_else(|| {
		std::thread::available_parallelism()
			.map(|n| n.get())
			.unwrap_or(4)
	});

	let store: Arc<dyn CacheStore> =
		Arc::new(LocalStore::new(root.join(config.settings.cache_dir())));
	let semaphore = Arc::new(Semaphore::new(concurrency));
	let abort = Arc::new(AtomicBool::new(false));
	let lattice_version: Arc<str> = Arc::from(lattice_version);
	let persistent_watches: Arc<Mutex<Vec<PersistentWatch>>> = Arc::new(Mutex::new(Vec::new()));
	let shutting_down = Arc::new(AtomicBool::new(false));

	let (tx, mut rx) = mpsc::unbounded_channel::<RunnerMsg>();

	let mut remaining_indegree = schedule.indegree.clone();
	let mut completed = vec![false; n];
	let mut in_flight = 0usize;
	let mut join_set: JoinSet<(usize, TaskOutcome)> = JoinSet::new();

	let mut total = 0usize;
	let mut cached = 0usize;
	let mut failed = 0usize;
	let mut skipped = 0usize;
	let mut first_failure: Option<String> = None;
	// Persistent children started and not yet reported as exited.
	let mut live_persistent = 0usize;

	let mut ready: Vec<usize> = schedule.initial_ready();

	loop {
		// Stop spawning new work once an abort has been signalled (fail-fast).
		if !abort.load(Ordering::SeqCst) {
			while let Some(pos) = ready.pop() {
				match specs[pos].clone() {
					None => {
						// Unknown workspace: treat as an immediately-complete
						// no-op dependency edge.
						completed[pos] = true;
						unblock(
							pos,
							&schedule,
							&mut remaining_indegree,
							&completed,
							&mut ready,
						);
					}
					Some(spec) => {
						in_flight += 1;
						let ctx = TaskRunContext {
							spec,
							store: store.clone(),
							semaphore: semaphore.clone(),
							abort: abort.clone(),
							no_cache,
							lattice_version: lattice_version.clone(),
							tx: tx.clone(),
							persistent_watches: persistent_watches.clone(),
							shutting_down: shutting_down.clone(),
						};
						join_set.spawn(async move { (pos, run_one(ctx).await) });
					}
				}
			}
		}

		if in_flight == 0 {
			break;
		}

		tokio::select! {
			joined = join_set.join_next() => {
				let (pos, outcome) = match joined {
					Some(Ok(res)) => res,
					Some(Err(e)) => {
						// A spawned task panicked: a runner-internal fault. Treat
						// it as fatal even in keep-going mode.
						abort.store(true, Ordering::SeqCst);
						if first_failure.is_none() {
							first_failure = Some(format!("task runner panicked: {e}"));
						}
						failed += 1;
						in_flight -= 1;
						continue;
					}
					None => break,
				};
				in_flight -= 1;
				completed[pos] = true;

				match outcome {
					TaskOutcome::Noop => {
						unblock(pos, &schedule, &mut remaining_indegree, &completed, &mut ready);
					}
					TaskOutcome::Ran => {
						total += 1;
						unblock(pos, &schedule, &mut remaining_indegree, &completed, &mut ready);
					}
					TaskOutcome::Persistent => {
						total += 1;
						live_persistent += 1;
						unblock(pos, &schedule, &mut remaining_indegree, &completed, &mut ready);
					}
					TaskOutcome::Cached => {
						total += 1;
						cached += 1;
						unblock(pos, &schedule, &mut remaining_indegree, &completed, &mut ready);
					}
					TaskOutcome::Failed { workspace, task } => {
						total += 1;
						failed += 1;
						if keep_going {
							skipped += skip_dependents(
								pos, &schedule, &mut completed, &specs, reporter,
							);
						} else {
							abort.store(true, Ordering::SeqCst);
							if first_failure.is_none() {
								first_failure = Some(format!(
									"task '{workspace}:{task}' failed, stopping pipeline"
								));
							}
						}
					}
				}
			}
			Some(msg) = rx.recv() => {
				forward(reporter, msg, &mut live_persistent, &mut failed);
			}
		}
	}

	// While any persistent child is still running, wait for the shutdown signal,
	// streaming their output and reporting any that ends on its own. Once none is
	// left there is nothing to hold the run open, so it finishes.
	if live_persistent > 0 {
		let mut shutdown_fut: Pin<Box<dyn Future<Output = ()> + Send>> = match shutdown {
			Some(f) => f,
			None => Box::pin(async {
				let _ = tokio::signal::ctrl_c().await;
			}),
		};
		while live_persistent > 0 {
			tokio::select! {
				_ = &mut shutdown_fut => break,
				Some(msg) = rx.recv() => forward(reporter, msg, &mut live_persistent, &mut failed),
			}
		}
	}

	// Tear down whatever is left. The flag goes up first: past this point a child
	// is dying because we asked, which is not a task failure.
	shutting_down.store(true, Ordering::SeqCst);
	let watches: Vec<PersistentWatch> = persistent_watches.lock().unwrap().drain(..).collect();
	for w in watches {
		let _ = w.kill.send(());
		let _ = w.reaper.await;
	}

	// Drop our sender so the channel closes once every task/streamer sender is
	// gone, then drain any remaining buffered messages.
	drop(tx);
	while let Some(msg) = rx.recv().await {
		forward(reporter, msg, &mut live_persistent, &mut failed);
	}

	let elapsed_ms = global_start.elapsed().as_millis() as u64;
	reporter.run_summary(total, cached, failed, elapsed_ms);
	reporter.finish();

	let result = RunResult {
		total,
		cached,
		failed,
		elapsed_ms,
	};

	// Fail-fast: return the first failure verbatim (also covers a runner panic).
	if let Some(msg) = first_failure {
		return Err(anyhow::anyhow!(msg));
	}
	// Failures the run continued past — kept going, or a persistent child that
	// exited after the graph had already drained. Report a summary error so the
	// process exits non-zero.
	if failed > 0 {
		return Err(RunFailure { result, skipped }.into());
	}

	Ok(result)
}

/// Execute a single node: acquire a concurrency permit, handle caching, run the
/// command through the platform shell (with per-task PATH injection), and store
/// cache artifacts on success. Persistent tasks are started and detached without
/// holding the permit.
async fn run_one(ctx: TaskRunContext) -> TaskOutcome {
	let _permit = ctx.semaphore.acquire().await.expect("semaphore not closed");
	if ctx.abort.load(Ordering::SeqCst) {
		return TaskOutcome::Noop;
	}

	let spec = &ctx.spec;
	let ws = spec.workspace_name.clone();
	let task = spec.task_name.clone();
	let pt = &spec.pipeline_task;

	// Resolved env values for this task's declared `env` names, sorted by name.
	let mut env_values: Vec<(String, String)> = pt
		.env
		.as_deref()
		.unwrap_or(&[])
		.iter()
		.filter_map(|name| std::env::var(name).ok().map(|val| (name.clone(), val)))
		.collect();
	env_values.sort_by(|a, b| a.0.cmp(&b.0));

	let key = match compute_key(&HashInputs {
		task: &task,
		command: &spec.command,
		workspace_path: &spec.ws_path,
		pipeline_task: pt,
		env_values: &env_values,
		toolchain_identity: &spec.toolchain_identity,
		lattice_version: ctx.lattice_version.as_ref(),
	}) {
		Ok(k) => k,
		Err(e) => {
			let captured = vec![(true, format!("failed to compute cache key: {e}"))];
			emit_failure(&ctx.tx, &ws, &task, captured);
			return TaskOutcome::Failed {
				workspace: ws,
				task,
			};
		}
	};

	// Loquacious trace: the computed cache identity for this task.
	let _ = ctx.tx.send(RunnerMsg::TaskNote {
		workspace: ws.clone(),
		task: task.clone(),
		msg: format!("hash {}", &key[..key.len().min(16)]),
	});

	if !ctx.no_cache && pt.is_cacheable() {
		match ctx.store.lookup(&key) {
			Ok(Some(entry)) => {
				match ctx.store.restore(&entry, &spec.ws_path) {
					Ok(()) => {
						let _ = ctx.store.touch(&key);
						let _ = ctx.tx.send(RunnerMsg::Event(TaskEvent::CacheHit {
							workspace: ws.clone(),
							task: task.clone(),
							key,
						}));
						return TaskOutcome::Cached;
					}
					Err(e) => {
						let _ = ctx.tx.send(RunnerMsg::TaskWarn {
							workspace: ws.clone(),
							task: task.clone(),
							msg: format!("failed to restore cached outputs: {e}"),
						});
						// Fall through and re-run.
					}
				}
			}
			Ok(None) => {
				let _ = ctx.tx.send(RunnerMsg::TaskNote {
					workspace: ws.clone(),
					task: task.clone(),
					msg: "cache miss".to_string(),
				});
			}
			Err(e) => {
				let _ = ctx.tx.send(RunnerMsg::TaskWarn {
					workspace: ws.clone(),
					task: task.clone(),
					msg: format!("cache lookup failed: {e}"),
				});
			}
		}
	}

	let _ = ctx.tx.send(RunnerMsg::Event(TaskEvent::Started {
		workspace: ws.clone(),
		task: task.clone(),
	}));

	let start = Instant::now();
	let mut cmd = build_shell_command(&spec.command);
	cmd.current_dir(&spec.ws_path);
	apply_path_prepend(&mut cmd, &spec.path_prepend);
	for (k, v) in &env_values {
		cmd.env(k, v);
	}
	cmd.stdout(std::process::Stdio::piped())
		.stderr(std::process::Stdio::piped());
	#[cfg(unix)]
	cmd.process_group(0);

	let mut child = match cmd.spawn() {
		Ok(c) => c,
		Err(e) => {
			let msg = if e.kind() == std::io::ErrorKind::NotFound {
				format!(
					"failed to spawn task shell (is `{}` available?): {e}",
					shell_program()
				)
			} else {
				format!("failed to spawn task: {e}")
			};
			emit_failure(&ctx.tx, &ws, &task, vec![(true, msg)]);
			return TaskOutcome::Failed {
				workspace: ws,
				task,
			};
		}
	};

	if spec.is_persistent {
		let stdout = child.stdout.take();
		let stderr = child.stderr.take();
		let tx = ctx.tx.clone();
		let ws2 = ws.clone();
		let task2 = task.clone();
		tokio::spawn(async move {
			let _ = tokio::join!(
				drain_pipe(stdout, false, &tx, &ws2, &task2, false, true),
				drain_pipe(stderr, true, &tx, &ws2, &task2, false, true),
			);
		});

		#[cfg(unix)]
		let pc = PersistentChild {
			pgid: child.id().map(|id| id as i32),
			child,
		};
		#[cfg(not(unix))]
		let pc = PersistentChild { child };

		let (kill_tx, kill_rx) = tokio::sync::oneshot::channel();
		let reaper = tokio::spawn(reap_persistent(ReapContext {
			child: pc,
			kill: kill_rx,
			tx: ctx.tx.clone(),
			shutting_down: ctx.shutting_down.clone(),
			workspace: ws,
			task,
			start,
		}));
		ctx.persistent_watches
			.lock()
			.unwrap()
			.push(PersistentWatch {
				kill: kill_tx,
				reaper,
			});

		return TaskOutcome::Persistent;
	}

	let stdout = child.stdout.take();
	let stderr = child.stderr.take();
	let (out_lines, err_lines, status) = tokio::join!(
		drain_pipe(stdout, false, &ctx.tx, &ws, &task, true, false),
		drain_pipe(stderr, true, &ctx.tx, &ws, &task, true, false),
		child.wait(),
	);

	let mut captured = out_lines;
	captured.extend(err_lines);
	let duration_ms = start.elapsed().as_millis() as u64;

	let success = match status {
		Ok(s) => s.success(),
		Err(e) => {
			captured.push((true, format!("failed to wait for task: {e}")));
			false
		}
	};

	if success {
		// Store cache artifacts on success (never for persistent / cache:false).
		if pt.is_cacheable() && !ctx.no_cache {
			let meta = CacheMeta {
				key: key.clone(),
				task: task.clone(),
				workspace: ws.clone(),
				duration_ms,
				last_used: chrono::Utc::now(),
				env: env_values.iter().cloned().collect(),
				output_digest: String::new(),
			};
			if let Err(e) = ctx.store.store(
				&key,
				&spec.ws_path,
				pt.outputs.as_deref().unwrap_or(&[]),
				meta,
			) {
				let _ = ctx.tx.send(RunnerMsg::TaskWarn {
					workspace: ws.clone(),
					task: task.clone(),
					msg: format!("failed to cache outputs: {e}"),
				});
			}
		}
		let _ = ctx.tx.send(RunnerMsg::Event(TaskEvent::Finished {
			workspace: ws.clone(),
			task: task.clone(),
			duration_ms,
		}));
		TaskOutcome::Ran
	} else {
		emit_failure(&ctx.tx, &ws, &task, captured);
		TaskOutcome::Failed {
			workspace: ws,
			task,
		}
	}
}

/// Wait on one persistent child until either it ends on its own — reported back
/// to the scheduler, non-zero counted as a failure — or the run asks for it to
/// stop, which is silent.
///
/// The kill request and the child's own exit can become ready in the same poll,
/// so which `select!` arm wins does not decide what gets reported: the
/// shutdown flag does.
async fn reap_persistent(ctx: ReapContext) {
	let ReapContext {
		mut child,
		kill,
		tx,
		shutting_down,
		workspace,
		task,
		start,
	} = ctx;

	let status = tokio::select! {
		status = child.child.wait() => status,
		_ = kill => {
			child.terminate().await;
			return;
		}
	};

	// The shell is gone, but anything it backgrounded is not. Kill the rest of
	// the group so a half-dead dev server does not keep a port (or this task's
	// output pipes) held open for the remainder of the run.
	child.kill_group();

	if shutting_down.load(Ordering::SeqCst) {
		return;
	}

	let code = match status {
		Ok(s) => s.code(),
		Err(e) => {
			let _ = tx.send(RunnerMsg::TaskWarn {
				workspace: workspace.clone(),
				task: task.clone(),
				msg: format!("failed to wait for persistent task: {e}"),
			});
			None
		}
	};
	let _ = tx.send(RunnerMsg::PersistentExited {
		workspace,
		task,
		code,
		duration_ms: start.elapsed().as_millis() as u64,
	});
}

fn emit_failure(
	tx: &UnboundedSender<RunnerMsg>,
	ws: &str,
	task: &str,
	captured: Vec<(bool, String)>,
) {
	let _ = tx.send(RunnerMsg::Event(TaskEvent::Failed {
		workspace: ws.to_string(),
		task: task.to_string(),
	}));
	let _ = tx.send(RunnerMsg::SurfaceFailure {
		workspace: ws.to_string(),
		task: task.to_string(),
		captured,
	});
}

fn shell_program() -> &'static str {
	if cfg!(windows) {
		"cmd"
	} else {
		"sh"
	}
}

/// Build a [`tokio::process::Command`] that runs `command` through the platform
/// shell: `sh -c "<command>"` on unix, `cmd /C "<command>"` on windows.
fn build_shell_command(command: &str) -> tokio::process::Command {
	if cfg!(windows) {
		let mut c = tokio::process::Command::new("cmd");
		c.arg("/C").arg(command);
		c
	} else {
		let mut c = tokio::process::Command::new("sh");
		c.arg("-c").arg(command);
		c
	}
}

/// Prepend `prepend` dirs to the child's `PATH`, affecting that child only.
fn apply_path_prepend(cmd: &mut tokio::process::Command, prepend: &[PathBuf]) {
	if prepend.is_empty() {
		return;
	}
	let existing = std::env::var_os("PATH").unwrap_or_default();
	let mut paths: Vec<PathBuf> = prepend.to_vec();
	paths.extend(std::env::split_paths(&existing));
	if let Ok(joined) = std::env::join_paths(paths) {
		cmd.env("PATH", joined);
	}
}

/// Read a child pipe line by line, emitting an `Output` event per line and
/// (when `capture`) buffering up to [`MAX_CAPTURED_LINES`] for failure surfacing.
async fn drain_pipe<R: AsyncRead + Unpin>(
	pipe: Option<R>,
	stderr: bool,
	tx: &UnboundedSender<RunnerMsg>,
	ws: &str,
	task: &str,
	capture: bool,
	persistent: bool,
) -> Vec<(bool, String)> {
	let mut captured = Vec::new();
	if let Some(pipe) = pipe {
		let mut lines = BufReader::new(pipe).lines();
		while let Ok(Some(line)) = lines.next_line().await {
			let _ = tx.send(RunnerMsg::Event(TaskEvent::Output {
				workspace: ws.to_string(),
				task: task.to_string(),
				line: line.clone(),
				stderr,
				persistent,
			}));
			if capture && captured.len() < MAX_CAPTURED_LINES {
				captured.push((stderr, line));
			}
		}
	}
	captured
}

#[cfg(test)]
mod tests {
	use super::*;
	use dagger::build_execution_graph;
	use lattice_config::{EngineMap, PipelineTask};
	use lattice_output::TaskEvent;
	use std::sync::Mutex;
	use std::time::Duration;

	#[derive(Default)]
	struct RecordingReporter {
		events: Mutex<Vec<TaskEvent>>,
		summaries: Mutex<Vec<(usize, usize, usize, u64)>>,
		surfaced: Mutex<Vec<(String, String)>>,
	}

	impl RecordingReporter {
		fn new() -> Self {
			Self::default()
		}

		/// Compact "kind:ws:task" labels for the recorded events, in order.
		fn labels(&self) -> Vec<String> {
			self.events
				.lock()
				.unwrap()
				.iter()
				.map(|ev| match ev {
					TaskEvent::Started { workspace, task } => format!("started:{workspace}:{task}"),
					TaskEvent::CacheHit {
						workspace, task, ..
					} => {
						format!("cachehit:{workspace}:{task}")
					}
					TaskEvent::Output {
						workspace, task, ..
					} => {
						format!("output:{workspace}:{task}")
					}
					TaskEvent::Finished {
						workspace, task, ..
					} => {
						format!("finished:{workspace}:{task}")
					}
					TaskEvent::Failed { workspace, task } => format!("failed:{workspace}:{task}"),
					TaskEvent::PersistentExited {
						workspace,
						task,
						code,
						..
					} => {
						let c = code.map(|c| c.to_string()).unwrap_or("signal".to_string());
						format!("exited:{workspace}:{task}:{c}")
					}
					TaskEvent::Skipped {
						workspace, task, ..
					} => {
						format!("skipped:{workspace}:{task}")
					}
				})
				.collect()
		}

		fn has(&self, label: &str) -> bool {
			self.labels().iter().any(|l| l == label)
		}

		fn index_of(&self, label: &str) -> Option<usize> {
			self.labels().iter().position(|l| l == label)
		}
	}

	impl Reporter for RecordingReporter {
		fn run_start(&self, _task: &str, _workspaces: usize) {}
		fn event(&self, ev: TaskEvent) {
			self.events.lock().unwrap().push(ev);
		}
		fn surface_failure(&self, workspace: &str, task: &str, _captured: &[(bool, String)]) {
			self.surfaced
				.lock()
				.unwrap()
				.push((workspace.to_string(), task.to_string()));
		}
		fn run_summary(&self, total: usize, cached: usize, failed: usize, elapsed_ms: u64) {
			self.summaries
				.lock()
				.unwrap()
				.push((total, cached, failed, elapsed_ms));
		}
		fn note(&self, _msg: &str) {}
		fn warn(&self, _msg: &str) {}
		fn finish(&self) {}
	}

	fn ws(name: &str, root: &std::path::Path, commands: &[(&str, &str)]) -> Workspace {
		let path = root.join(name);
		std::fs::create_dir_all(&path).unwrap();
		let mut map = indexmap::IndexMap::new();
		for (k, v) in commands {
			map.insert((*k).to_string(), (*v).to_string());
		}
		Workspace {
			name: name.to_string(),
			path,
			auto: false,
			depends_on: Vec::new(),
			engines: EngineMap::new(),
			driver: None,
			commands: map,
		}
	}

	fn config_with(tasks: &[(&str, PipelineTask)]) -> LatticeConfig {
		let mut cfg = LatticeConfig::default();
		for (name, pt) in tasks {
			cfg.tasks.insert((*name).to_string(), pt.clone());
		}
		cfg
	}

	fn dep(deps: &[&str]) -> PipelineTask {
		PipelineTask {
			depends_on: Some(deps.iter().map(|s| s.to_string()).collect()),
			..Default::default()
		}
	}

	fn persistent(deps: &[&str]) -> PipelineTask {
		PipelineTask {
			persistent: Some(true),
			..dep(deps)
		}
	}

	/// A stand-in for Ctrl-C, firing after `ms`.
	fn shutdown_after(ms: u64) -> Pin<Box<dyn Future<Output = ()> + Send>> {
		Box::pin(async move { tokio::time::sleep(Duration::from_millis(ms)).await })
	}

	/// Default options (cache off, unbounded concurrency, fail-fast). Tests that
	/// need other knobs mutate the returned struct's fields directly.
	fn opts<'a>(
		graph: &'a ExecutionGraph,
		workspaces: &'a [Workspace],
		config: &'a LatticeConfig,
		root: &'a std::path::Path,
		reporter: &'a dyn Reporter,
	) -> ExecuteOptions<'a> {
		ExecuteOptions {
			graph,
			workspaces,
			config,
			root,
			no_cache: true,
			concurrency: None,
			keep_going: false,
			reporter,
			lattice_version: "0.1.0-test",
			shutdown: None,
		}
	}

	#[tokio::test]
	async fn all_tasks_run_with_correct_counts() {
		let tmp = tempfile::tempdir().unwrap();
		let root = tmp.path();
		let workspaces = vec![
			ws("wa", root, &[("build", "true")]),
			ws("wb", root, &[("build", "true")]),
		];
		let config = config_with(&[("build", PipelineTask::default())]);
		let graph = build_execution_graph(&workspaces, "build", &config).unwrap();
		let r = RecordingReporter::new();

		let result = execute_tasks(opts(&graph, &workspaces, &config, root, &r))
			.await
			.unwrap();

		assert_eq!(result.total, 2);
		assert_eq!(result.cached, 0);
		assert_eq!(result.failed, 0);
		let s = r.summaries.lock().unwrap();
		assert_eq!(s.len(), 1);
		assert_eq!((s[0].0, s[0].1, s[0].2), (2, 0, 0));
	}

	#[tokio::test]
	async fn concurrency_one_serializes_execution() {
		// Each task writes its letter, sleeps, then writes it again. Under a
		// concurrency of 1 the two tasks cannot interleave, so the shared file
		// is "aabb" or "bbaa" (never "abab").
		let tmp = tempfile::tempdir().unwrap();
		let root = tmp.path();
		let order = root.join("order.txt");
		let a = format!(
			"printf a >> {p}; sleep 0.1; printf a >> {p}",
			p = order.display()
		);
		let b = format!(
			"printf b >> {p}; sleep 0.1; printf b >> {p}",
			p = order.display()
		);
		let workspaces = vec![
			ws("wa", root, &[("build", &a)]),
			ws("wb", root, &[("build", &b)]),
		];
		let config = config_with(&[("build", PipelineTask::default())]);
		let graph = build_execution_graph(&workspaces, "build", &config).unwrap();
		let r = RecordingReporter::new();

		let mut o = opts(&graph, &workspaces, &config, root, &r);
		o.concurrency = Some(1);
		let result = execute_tasks(o).await.unwrap();

		assert_eq!(result.total, 2);
		let content = std::fs::read_to_string(&order).unwrap();
		assert!(
			content == "aabb" || content == "bbaa",
			"expected serialized output, got {content:?}"
		);
	}

	#[tokio::test]
	async fn dependency_ordering_is_honored() {
		// b dependsOn a; a writes "a", b appends "b" → "ab" iff a ran first.
		let tmp = tempfile::tempdir().unwrap();
		let root = tmp.path();
		let order = root.join("order.txt");
		let a = format!("printf a >> {}", order.display());
		let b = format!("printf b >> {}", order.display());
		let workspaces = vec![ws("wa", root, &[("a", &a), ("b", &b)])];
		let config = config_with(&[("a", PipelineTask::default()), ("b", dep(&["a"]))]);
		let graph = build_execution_graph(&workspaces, "b", &config).unwrap();
		let r = RecordingReporter::new();

		execute_tasks(opts(&graph, &workspaces, &config, root, &r))
			.await
			.unwrap();

		assert_eq!(std::fs::read_to_string(&order).unwrap(), "ab");
		assert!(r.index_of("finished:wa:a").unwrap() < r.index_of("started:wa:b").unwrap());
	}

	#[tokio::test]
	async fn fail_fast_stops_pipeline_and_blocks_downstream() {
		let tmp = tempfile::tempdir().unwrap();
		let root = tmp.path();
		let marker = root.join("downstream.txt");
		let test_cmd = format!("touch {}", marker.display());
		let workspaces = vec![ws("wa", root, &[("build", "exit 1"), ("test", &test_cmd)])];
		let config = config_with(&[
			("build", PipelineTask::default()),
			("test", dep(&["build"])),
		]);
		let graph = build_execution_graph(&workspaces, "test", &config).unwrap();
		let r = RecordingReporter::new();

		let err = execute_tasks(opts(&graph, &workspaces, &config, root, &r))
			.await
			.unwrap_err();

		let msg = err.to_string();
		assert!(
			msg.contains("wa:build") && msg.contains("stopping pipeline"),
			"unexpected error: {msg}"
		);
		assert!(!marker.exists(), "downstream task ran despite failure");
		assert!(r.has("started:wa:build"));
		assert!(r.has("failed:wa:build"));
		assert_eq!(r.surfaced.lock().unwrap().len(), 1);
	}

	#[tokio::test]
	async fn keep_going_runs_independent_and_skips_downstream() {
		let tmp = tempfile::tempdir().unwrap();
		let root = tmp.path();
		let indep = root.join("indep.txt");
		let downstream = root.join("down.txt");
		let indep_cmd = format!("touch {}", indep.display());
		let down_cmd = format!("touch {}", downstream.display());
		// wa: build fails, test dependsOn build (skipped). wb: independent build.
		let workspaces = vec![
			ws("wa", root, &[("build", "exit 1"), ("test", &down_cmd)]),
			ws("wb", root, &[("build", &indep_cmd), ("test", "true")]),
		];
		let config = config_with(&[
			("build", PipelineTask::default()),
			("test", dep(&["build"])),
		]);
		let graph = build_execution_graph(&workspaces, "test", &config).unwrap();
		let r = RecordingReporter::new();

		let mut o = opts(&graph, &workspaces, &config, root, &r);
		o.keep_going = true;
		let err = execute_tasks(o).await.unwrap_err();

		let rf = err
			.downcast_ref::<RunFailure>()
			.expect("keep-going yields RunFailure");
		assert_eq!(rf.result.failed, 1);
		assert!(rf.skipped >= 1, "downstream of failure should be skipped");
		assert!(indep.exists(), "independent branch must run in keep-going");
		assert!(
			!downstream.exists(),
			"downstream of failure must be skipped"
		);
		assert!(r.has("skipped:wa:test"));
	}

	#[tokio::test]
	async fn passing_task_emits_started_then_finished() {
		let tmp = tempfile::tempdir().unwrap();
		let root = tmp.path();
		let workspaces = vec![ws("app", root, &[("build", "true")])];
		let config = config_with(&[("build", PipelineTask::default())]);
		let graph = build_execution_graph(&workspaces, "build", &config).unwrap();
		let r = RecordingReporter::new();

		execute_tasks(opts(&graph, &workspaces, &config, root, &r))
			.await
			.unwrap();

		let started = r.index_of("started:app:build").expect("Started emitted");
		let finished = r.index_of("finished:app:build").expect("Finished emitted");
		assert!(started < finished);
	}

	#[tokio::test]
	async fn cache_round_trip_hits_and_restores_then_corruption_misses() {
		let tmp = tempfile::tempdir().unwrap();
		let root = tmp.path();
		// The command writes an output file; the task caches "out.txt".
		let workspaces = vec![ws("app", root, &[("build", "echo hello > out.txt")])];
		let out_file = workspaces[0].path.join("out.txt");
		let build = PipelineTask {
			outputs: Some(vec!["out.txt".to_string()]),
			..Default::default()
		};
		let config = config_with(&[("build", build)]);
		let graph = build_execution_graph(&workspaces, "build", &config).unwrap();

		// First run with caching enabled (no_cache=false), so it stores.
		let r1 = RecordingReporter::new();
		let mut o1 = opts(&graph, &workspaces, &config, root, &r1);
		o1.no_cache = false;
		execute_tasks(o1).await.unwrap();
		assert!(r1.has("finished:app:build"));
		assert!(!r1.has("cachehit:app:build"));
		assert!(out_file.exists());

		// Remove the output so a hit must restore it.
		std::fs::remove_file(&out_file).unwrap();

		// Second run: same inputs → cache hit, restores out.txt.
		let r2 = RecordingReporter::new();
		let mut o2 = opts(&graph, &workspaces, &config, root, &r2);
		o2.no_cache = false;
		let res2 = execute_tasks(o2).await.unwrap();
		assert_eq!(res2.cached, 1);
		assert!(r2.has("cachehit:app:build"));
		assert!(!r2.has("started:app:build"));
		assert!(out_file.exists(), "cache hit must restore outputs");

		// Corrupt the stored tarball: the digest mismatch is a miss, so it re-runs.
		let cache_dir = root.join(config.settings.cache_dir());
		let tarball = std::fs::read_dir(&cache_dir)
			.unwrap()
			.flatten()
			.map(|e| e.path())
			.find(|p| p.extension().map(|x| x == "gz").unwrap_or(false))
			.expect("a cached tarball exists");
		std::fs::write(&tarball, b"garbage").unwrap();

		let r3 = RecordingReporter::new();
		let mut o3 = opts(&graph, &workspaces, &config, root, &r3);
		o3.no_cache = false;
		let res3 = execute_tasks(o3).await.unwrap();
		assert_eq!(res3.cached, 0, "corrupt tarball must be a miss");
		assert!(r3.has("started:app:build"));
		assert!(!r3.has("cachehit:app:build"));
	}

	#[tokio::test]
	async fn stored_meta_records_the_env_the_key_was_computed_from() {
		let tmp = tempfile::tempdir().unwrap();
		let root = tmp.path();
		let workspaces = vec![ws("app", root, &[("build", "echo hello > out.txt")])];
		// PATH is declared so the task has a resolved env value without the test
		// mutating the process environment.
		let build = PipelineTask {
			outputs: Some(vec!["out.txt".to_string()]),
			env: Some(vec!["PATH".to_string()]),
			..Default::default()
		};
		let config = config_with(&[("build", build)]);
		let graph = build_execution_graph(&workspaces, "build", &config).unwrap();

		let reporter = RecordingReporter::new();
		let mut o = opts(&graph, &workspaces, &config, root, &reporter);
		o.no_cache = false;
		execute_tasks(o).await.unwrap();

		let cache_dir = root.join(config.settings.cache_dir());
		let meta_path = std::fs::read_dir(&cache_dir)
			.unwrap()
			.flatten()
			.map(|e| e.path())
			.find(|p| p.to_string_lossy().ends_with(".meta.json"))
			.expect("a cached entry's meta file exists");
		let meta: lattice_cache::CacheMeta =
			serde_json::from_str(&std::fs::read_to_string(meta_path).unwrap()).unwrap();

		assert_eq!(
			meta.env.get("PATH").map(String::as_str),
			Some(std::env::var("PATH").unwrap().as_str()),
			"the entry must record the env values that fed its key"
		);
	}

	#[tokio::test]
	async fn persistent_task_does_not_block_the_graph() {
		let tmp = tempfile::tempdir().unwrap();
		let root = tmp.path();
		let marker = root.join("normal.txt");
		let normal_cmd = format!("touch {}", marker.display());
		// dev (persistent) dependsOn build (normal). dev runs `sleep 30`.
		let workspaces = vec![ws(
			"app",
			root,
			&[("build", &normal_cmd), ("dev", "sleep 30")],
		)];
		let config = config_with(&[
			("build", PipelineTask::default()),
			(
				"dev",
				PipelineTask {
					persistent: Some(true),
					depends_on: Some(vec!["build".to_string()]),
					..Default::default()
				},
			),
		]);
		let graph = build_execution_graph(&workspaces, "dev", &config).unwrap();
		let r = RecordingReporter::new();

		let mut o = opts(&graph, &workspaces, &config, root, &r);
		o.shutdown = Some(shutdown_after(200));

		let result = tokio::time::timeout(Duration::from_secs(5), execute_tasks(o))
			.await
			.expect("execute_tasks must finish well before the 5s timeout")
			.unwrap();

		assert!(marker.exists(), "normal task did not complete");
		assert!(r.has("finished:app:build"));
		assert!(r.has("started:app:dev"));
		assert_eq!(result.failed, 0);
		// The child was still running and we killed it: that is not a failure,
		// and it is not an exit worth reporting either.
		assert!(
			!r.labels().iter().any(|l| l.starts_with("exited:")),
			"a child killed at shutdown must not be reported as exited: {:?}",
			r.labels()
		);
	}

	#[tokio::test]
	async fn persistent_task_that_exits_nonzero_fails_the_run() {
		let tmp = tempfile::tempdir().unwrap();
		let root = tmp.path();
		let workspaces = vec![ws("app", root, &[("dev", "exit 3")])];
		let config = config_with(&[("dev", persistent(&[]))]);
		let graph = build_execution_graph(&workspaces, "dev", &config).unwrap();
		let r = RecordingReporter::new();

		// A shutdown signal far past the timeout: the run has to end because the
		// child is gone, not because it was told to stop.
		let mut o = opts(&graph, &workspaces, &config, root, &r);
		o.shutdown = Some(shutdown_after(60_000));

		let err = tokio::time::timeout(Duration::from_secs(5), execute_tasks(o))
			.await
			.expect("an exited persistent child must end the run without a signal")
			.unwrap_err();

		let rf = err
			.downcast_ref::<RunFailure>()
			.expect("a persistent child's non-zero exit yields RunFailure");
		assert_eq!(rf.result.failed, 1);
		assert!(r.has("exited:app:dev:3"));
		let s = r.summaries.lock().unwrap();
		assert_eq!((s[0].0, s[0].2), (1, 1), "summary must count the failure");
	}

	#[tokio::test]
	async fn persistent_task_that_exits_zero_is_reported() {
		let tmp = tempfile::tempdir().unwrap();
		let root = tmp.path();
		let workspaces = vec![ws("app", root, &[("dev", "true")])];
		let config = config_with(&[("dev", persistent(&[]))]);
		let graph = build_execution_graph(&workspaces, "dev", &config).unwrap();
		let r = RecordingReporter::new();

		let mut o = opts(&graph, &workspaces, &config, root, &r);
		o.shutdown = Some(shutdown_after(60_000));

		let result = tokio::time::timeout(Duration::from_secs(5), execute_tasks(o))
			.await
			.expect("an exited persistent child must end the run without a signal")
			.unwrap()
			.total;

		assert_eq!(result, 1);
		// Nothing failed, but the process the user asked to keep running is gone,
		// so the run says so.
		assert!(r.has("exited:app:dev:0"));
		let s = r.summaries.lock().unwrap();
		assert_eq!(s[0].2, 0, "a clean exit is not a failure");
	}

	#[tokio::test]
	async fn one_persistent_exit_leaves_the_others_running() {
		let tmp = tempfile::tempdir().unwrap();
		let root = tmp.path();
		let workspaces = vec![
			ws("dies", root, &[("dev", "exit 1")]),
			ws("stays", root, &[("dev", "sleep 30")]),
		];
		let config = config_with(&[("dev", persistent(&[]))]);
		let graph = build_execution_graph(&workspaces, "dev", &config).unwrap();
		let r = RecordingReporter::new();

		// The run keeps waiting for `stays`, so only the signal ends it.
		let mut o = opts(&graph, &workspaces, &config, root, &r);
		o.shutdown = Some(shutdown_after(400));

		let err = tokio::time::timeout(Duration::from_secs(5), execute_tasks(o))
			.await
			.expect("execute_tasks must finish well before the 5s timeout")
			.unwrap_err();

		let rf = err.downcast_ref::<RunFailure>().expect("RunFailure");
		assert_eq!(rf.result.failed, 1);
		assert_eq!(rf.result.total, 2);
		assert!(r.has("exited:dies:dev:1"));
		assert!(!r.has("exited:stays:dev:0"));
	}

	#[tokio::test]
	async fn path_injection_makes_provisioned_tool_available() {
		// The workspace command is the bare tool name, which resolves only if
		// the provisioned toolchain bin dir was prepended to PATH for the task.
		let tmp = tempfile::tempdir().unwrap();
		let root = tmp.path();
		let engines: EngineMap = serde_json::from_value(serde_json::json!({
            "faketool": {
                "version": ">=1.0.0",
                "installCmd": "mkdir -p \"$LATTICE_TOOLCHAIN_DIR/bin\" && printf '#!/bin/sh\\necho faketool 1.2.3\\n' > \"$LATTICE_TOOLCHAIN_DIR/bin/faketool\" && chmod +x \"$LATTICE_TOOLCHAIN_DIR/bin/faketool\"",
                "versionCmd": "faketool",
                "bin": "bin"
            }
        }))
        .unwrap();

		let mut workspace = ws(root_ws_name(), root, &[("build", "faketool")]);
		workspace.engines = engines;
		let workspaces = vec![workspace];
		let config = config_with(&[("build", PipelineTask::default())]);
		let graph = build_execution_graph(&workspaces, "build", &config).unwrap();
		let r = RecordingReporter::new();

		let result = execute_tasks(opts(&graph, &workspaces, &config, root, &r))
			.await
			.unwrap();

		assert_eq!(result.failed, 0, "bare `faketool` should resolve via PATH");
		assert_eq!(result.total, 1);
		assert!(r.has("finished:app:build"));
	}

	fn root_ws_name() -> &'static str {
		"app"
	}
}
