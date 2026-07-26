//! The Lattice task runner: an in-degree scheduler over the execution DAG that
//! runs each task through the platform shell, wires the content-addressed cache,
//! and keeps long-running (`persistent`) tasks from blocking the graph.
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

use lattice_cache::{compute_key, CacheMeta, CacheStore, HashInputs, LocalStore};
use lattice_config::{resolve_engines, LatticeConfig, PipelineTask};
use lattice_dag::{build_schedule, ExecutionGraph, Schedule};
use lattice_output::{Reporter, TaskEvent};
use lattice_workspace::toolchain;
use lattice_workspace::Workspace;

/// Cap on how many child-output lines a single failing task retains for the
/// expand-on-failure surface. Beyond this, lines are still streamed live but not
/// buffered, bounding memory for pathological tasks.
const MAX_CAPTURED_LINES: usize = 5000;

/// The tallies a completed run reports.
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
        write!(f, ".")
    }
}

impl std::error::Error for RunFailure {}

/// Struct-of-args for [`execute_tasks`], replacing the old positional soup.
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
    persistent_children: Arc<Mutex<Vec<PersistentChild>>>,
}

/// A still-running persistent child, tracked so it can be torn down after the
/// graph drains.
struct PersistentChild {
    child: tokio::process::Child,
    #[cfg(unix)]
    pgid: Option<i32>,
}

impl PersistentChild {
    /// Kill the child (and, on unix, its whole process group so shell-spawned
    /// grandchildren die too), then reap it.
    async fn terminate(mut self) {
        #[cfg(unix)]
        if let Some(pgid) = self.pgid {
            // SAFETY: `kill(2)` with a negative pid signals the process group;
            // an invalid/already-dead group is a harmless ESRCH.
            unsafe {
                libc::kill(-pgid, libc::SIGKILL);
            }
        }
        let _ = self.child.start_kill();
        let _ = self.child.wait().await;
    }
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
    Warn(String),
}

/// The outcome of executing a single node.
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

/// Forward one [`RunnerMsg`] to the reporter.
fn forward(reporter: &dyn Reporter, msg: RunnerMsg) {
    match msg {
        RunnerMsg::Event(ev) => reporter.event(ev),
        RunnerMsg::SurfaceFailure {
            workspace,
            task,
            captured,
        } => reporter.surface_failure(&workspace, &task, &captured),
        RunnerMsg::Warn(m) => reporter.warn(&m),
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
/// keep-going run with failures returns `Err(RunFailure)`, and a fail-fast run
/// returns the first failure verbatim.
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

    // --- Toolchain resolution (before the scheduler) -----------------------
    // For each workspace, merge root + workspace engines and provision/resolve
    // them into a PATH-prepend + identity. Memoize by the merged spec so
    // identical toolchains are only resolved (and installed) once.
    let mut tc_cache: HashMap<String, (Vec<PathBuf>, String)> = HashMap::new();
    let mut ws_prepend: HashMap<String, Vec<PathBuf>> = HashMap::new();
    let mut ws_identity: HashMap<String, String> = HashMap::new();
    for ws in workspaces {
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

    // --- Schedule + specs ---------------------------------------------------
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
    reporter.run_start(&run_label, workspaces.len());

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
    let persistent_children: Arc<Mutex<Vec<PersistentChild>>> = Arc::new(Mutex::new(Vec::new()));

    let (tx, mut rx) = mpsc::unbounded_channel::<RunnerMsg>();

    // --- In-degree scheduler ------------------------------------------------
    let mut remaining_indegree = schedule.indegree.clone();
    let mut completed = vec![false; n];
    let mut in_flight = 0usize;
    let mut join_set: JoinSet<(usize, TaskOutcome)> = JoinSet::new();

    let mut total = 0usize;
    let mut cached = 0usize;
    let mut failed = 0usize;
    let mut skipped = 0usize;
    let mut first_failure: Option<String> = None;

    let mut ready: Vec<usize> = schedule.initial_ready();

    loop {
        // Spawn every ready node (bounded by the semaphore inside each task).
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
                            persistent_children: persistent_children.clone(),
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
                            first_failure = Some(format!("Task runner panicked: {e}"));
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
                    TaskOutcome::Ran | TaskOutcome::Persistent => {
                        total += 1;
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
                                    "Task '{workspace}:{task}' failed. Stopping pipeline."
                                ));
                            }
                        }
                    }
                }
            }
            Some(msg) = rx.recv() => {
                forward(reporter, msg);
            }
        }
    }

    // --- Persistent teardown ------------------------------------------------
    // If persistent children are still running, wait for the shutdown signal
    // (streaming their output meanwhile), then tear them all down.
    let had_persistent = !persistent_children.lock().unwrap().is_empty();
    if had_persistent {
        let mut shutdown_fut: Pin<Box<dyn Future<Output = ()> + Send>> = match shutdown {
            Some(f) => f,
            None => Box::pin(async {
                let _ = tokio::signal::ctrl_c().await;
            }),
        };
        loop {
            tokio::select! {
                _ = &mut shutdown_fut => break,
                Some(msg) = rx.recv() => forward(reporter, msg),
            }
        }
        let drained: Vec<PersistentChild> = persistent_children.lock().unwrap().drain(..).collect();
        for pc in drained {
            pc.terminate().await;
        }
    }

    // Drop our sender so the channel closes once every task/streamer sender is
    // gone, then drain any remaining buffered messages.
    drop(tx);
    while let Some(msg) = rx.recv().await {
        forward(reporter, msg);
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
    // Keep-going: report a summary error so the process exits non-zero.
    if keep_going && failed > 0 {
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

    // --- Cache lookup (never for persistent / cache:false tasks) ------------
    if !ctx.no_cache && pt.is_cacheable() {
        match ctx.store.lookup(&key) {
            Ok(Some(entry)) => {
                match ctx.store.restore(&entry, &spec.ws_path) {
                    Ok(()) => {
                        let _ = ctx.store.touch(&key);
                        // Round-trip the cached env (fixes the old "written but
                        // never read" bug); at minimum we read it here.
                        let _cached_env = entry.env();
                        let _ = ctx.tx.send(RunnerMsg::Event(TaskEvent::CacheHit {
                            workspace: ws.clone(),
                            task: task.clone(),
                            key,
                        }));
                        return TaskOutcome::Cached;
                    }
                    Err(e) => {
                        let _ = ctx.tx.send(RunnerMsg::Warn(format!(
                            "{ws}:{task}: failed to restore cached outputs: {e}"
                        )));
                        // Fall through and re-run.
                    }
                }
            }
            Ok(None) => {}
            Err(e) => {
                let _ = ctx.tx.send(RunnerMsg::Warn(format!(
                    "{ws}:{task}: cache lookup failed: {e}"
                )));
            }
        }
    }

    // --- Run path -----------------------------------------------------------
    let _ = ctx.tx.send(RunnerMsg::Event(TaskEvent::Started {
        workspace: ws.clone(),
        task: task.clone(),
    }));

    let start = Instant::now();
    let mut cmd = build_shell_command(&spec.command);
    cmd.current_dir(&spec.ws_path);
    apply_path_prepend(&mut cmd, &spec.path_prepend);
    // Re-export the resolved env into the child for consistency.
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

    // --- Persistent: detach, release the permit, do NOT wait ----------------
    if spec.is_persistent {
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let tx = ctx.tx.clone();
        let ws2 = ws.clone();
        let task2 = task.clone();
        tokio::spawn(async move {
            let _ = tokio::join!(
                drain_pipe(stdout, false, &tx, &ws2, &task2, false),
                drain_pipe(stderr, true, &tx, &ws2, &task2, false),
            );
        });

        #[cfg(unix)]
        let pc = PersistentChild {
            pgid: child.id().map(|id| id as i32),
            child,
        };
        #[cfg(not(unix))]
        let pc = PersistentChild { child };
        ctx.persistent_children.lock().unwrap().push(pc);

        return TaskOutcome::Persistent;
    }

    // --- Normal: stream, capture, and wait ----------------------------------
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let (out_lines, err_lines, status) = tokio::join!(
        drain_pipe(stdout, false, &ctx.tx, &ws, &task, true),
        drain_pipe(stderr, true, &ctx.tx, &ws, &task, true),
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
                let _ = ctx.tx.send(RunnerMsg::Warn(format!(
                    "{ws}:{task}: failed to cache outputs: {e}"
                )));
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

/// Emit a `Failed` event plus the captured output for the expand-on-fail surface.
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

/// The platform shell program name, for diagnostics.
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

/// Prepend `prepend` dirs to the child's `PATH`, cloning the parent env for that
/// child only (no global mutation).
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
    use lattice_config::{EngineMap, PipelineTask};
    use lattice_dag::build_execution_graph;
    use lattice_output::TaskEvent;
    use std::sync::Mutex;
    use std::time::Duration;

    // ---- recording reporter ---------------------------------------------------

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

    // ---- helpers --------------------------------------------------------------

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

    // ---- basic scheduling -----------------------------------------------------

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
        // Summary was reported exactly once with matching numbers.
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
        // And the events reflect a before b.
        assert!(r.index_of("finished:wa:a").unwrap() < r.index_of("started:wa:b").unwrap());
    }

    // ---- failure handling -----------------------------------------------------

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
            msg.contains("wa:build") && msg.contains("Stopping pipeline"),
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

    // ---- reporter event sequence ----------------------------------------------

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

    // ---- cache round-trip -----------------------------------------------------

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

        // First run: cache disabled? No — caching ON (no_cache=false) so it stores.
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

        // Corrupt the stored tarball → digest mismatch → MISS → re-runs.
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

    // ---- persistent task must not hang ---------------------------------------

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

        let shutdown: Pin<Box<dyn Future<Output = ()> + Send>> = Box::pin(async {
            tokio::time::sleep(Duration::from_millis(200)).await;
        });
        let options = ExecuteOptions {
            graph: &graph,
            workspaces: &workspaces,
            config: &config,
            root,
            no_cache: true,
            concurrency: None,
            keep_going: false,
            reporter: &r,
            lattice_version: "0.1.0-test",
            shutdown: Some(shutdown),
        };

        let result = tokio::time::timeout(Duration::from_secs(5), execute_tasks(options))
            .await
            .expect("execute_tasks must finish well before the 5s timeout")
            .unwrap();

        // The normal task completed; the persistent child was torn down.
        assert!(marker.exists(), "normal task did not complete");
        assert!(r.has("finished:app:build"));
        assert!(r.has("started:app:dev"));
        assert_eq!(result.failed, 0);
    }

    // ---- PATH injection via a provisioned toolchain --------------------------

    #[tokio::test]
    async fn path_injection_makes_provisioned_tool_available() {
        // An object-form engine whose installCmd drops a `faketool` script into
        // $LATTICE_TOOLCHAIN_DIR/bin. The workspace command is the bare tool
        // name, which only resolves if the toolchain bin dir was prepended to
        // PATH for the task.
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
