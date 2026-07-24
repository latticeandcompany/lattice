use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use console::style;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use petgraph::graph::NodeIndex;
use petgraph::Direction;

use lattice_cache::{CacheManager, CacheMeta};
use lattice_config::{LatticeConfig, PipelineTask};
use lattice_dag::ExecutionGraph;
use lattice_output::OutputManager;
use lattice_workspace::Workspace;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunResult {
    pub total: usize,
    pub cached: usize,
    pub failed: usize,
    pub elapsed_ms: u64,
}

/// A plain, `Send`-able description of one task to execute. Extracted from the
/// (non-`Send`) `ExecutionGraph` so it can be moved into spawned tasks.
#[derive(Clone)]
struct TaskSpec {
    workspace_name: String,
    task_name: String,
    command: String,
    is_persistent: bool,
    ws_path: PathBuf,
    pipeline_task: PipelineTask,
}

/// The scheduling shape of the DAG, decoupled from petgraph so it can be unit
/// tested in isolation.
///
/// * `prerequisites[i]` = the set of node indices that must complete before node
///   `i` may start (its incoming-edge sources).
/// * `dependents[i]` = the nodes that become closer to ready once `i` finishes
///   (its outgoing-edge targets).
/// * `indegree[i]` = number of outstanding prerequisites for node `i`.
struct Schedule {
    /// Prerequisite sets, retained for clarity and asserted on in tests; the
    /// live scheduler drives readiness off `indegree` (a running count of the
    /// same information) for O(1) unblocking.
    #[cfg_attr(not(test), allow(dead_code))]
    prerequisites: Vec<HashSet<usize>>,
    dependents: Vec<Vec<usize>>,
    indegree: Vec<usize>,
}

impl Schedule {
    /// The initially-ready nodes: those with no prerequisites.
    fn initial_ready(&self) -> Vec<usize> {
        (0..self.indegree.len())
            .filter(|&i| self.indegree[i] == 0)
            .collect()
    }
}

/// Build the scheduling structure from the DAG. An edge `from -> to` means
/// `from` is a dependency of `to`, so `from` is a prerequisite of `to` and `to`
/// is a dependent of `from`.
fn build_schedule(graph: &ExecutionGraph) -> (Vec<NodeIndex>, Schedule) {
    let node_indices: Vec<NodeIndex> = graph.graph.node_indices().collect();
    let n = node_indices.len();

    // Map NodeIndex -> dense 0..n position.
    let mut pos: HashMap<NodeIndex, usize> = HashMap::with_capacity(n);
    for (i, &ni) in node_indices.iter().enumerate() {
        pos.insert(ni, i);
    }

    let mut prerequisites = vec![HashSet::new(); n];
    let mut dependents = vec![Vec::new(); n];
    let mut indegree = vec![0usize; n];

    for (i, &ni) in node_indices.iter().enumerate() {
        for src in graph.graph.neighbors_directed(ni, Direction::Incoming) {
            let j = pos[&src];
            if prerequisites[i].insert(j) {
                indegree[i] += 1;
                dependents[j].push(i);
            }
        }
    }

    (
        node_indices,
        Schedule {
            prerequisites,
            dependents,
            indegree,
        },
    )
}

/// The outcome of executing a single node.
enum TaskOutcome {
    /// Restored from cache (counts toward `cached`).
    Cached,
    /// Ran successfully.
    Ran,
    /// Failed: carries the workspace/task and captured child output for surfacing.
    Failed {
        workspace: String,
        task: String,
        captured: Vec<(bool, String)>,
    },
}

/// Execute the DAG.
///
/// * `no_cache` — bypass the cache and force re-execution of every task.
/// * `concurrency` — cap on the number of tasks running at once. `None` keeps the
///   default of `std::thread::available_parallelism()`.
/// * `keep_going` — when `true`, do NOT fail-fast: independent tasks keep running
///   after a failure, tasks whose prerequisites failed are skipped, and the run
///   ends by reporting how many tasks failed (a non-`Ok` result). When `false`,
///   the first failure aborts scheduling of new work (fail-fast).
#[allow(clippy::too_many_arguments)]
pub async fn execute_tasks(
    graph: &ExecutionGraph,
    workspaces: &[Workspace],
    config: &LatticeConfig,
    root: &Path,
    no_cache: bool,
    concurrency: Option<usize>,
    keep_going: bool,
    output: &OutputManager,
) -> Result<RunResult> {
    let ws_map: HashMap<&str, &Workspace> =
        workspaces.iter().map(|w| (w.name.as_str(), w)).collect();

    let (node_indices, schedule) = build_schedule(graph);
    let n = node_indices.len();

    // Precompute an owned, Send-able spec for every node so spawned tasks don't
    // borrow the graph. Nodes whose workspace is unknown are `None` and treated
    // as immediately-complete no-ops (matching the old `continue`).
    let mut specs: Vec<Option<TaskSpec>> = Vec::with_capacity(n);
    for &ni in &node_indices {
        let node = &graph.graph[ni];
        let spec = ws_map.get(node.workspace_name.as_str()).map(|ws| {
            let pipeline_task = config
                .pipeline
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
            }
        });
        specs.push(spec);
    }
    let specs = Arc::new(specs);

    // Cap parallelism at the requested override, else the machine's default.
    // Guard against a nonsensical `Some(0)` so the semaphore always has permits.
    let concurrency = concurrency.filter(|&c| c > 0).unwrap_or_else(|| {
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
    });

    let cache_manager = Arc::new(CacheManager::new(root));
    let semaphore = Arc::new(Semaphore::new(concurrency));
    let abort = Arc::new(AtomicBool::new(false));

    let total = Arc::new(AtomicUsize::new(0));
    let cached = Arc::new(AtomicUsize::new(0));

    let global_start = std::time::Instant::now();

    // Interactive TUI rendering surface (only used when interactive).
    let interactive = output.is_interactive();
    let multi = if interactive {
        Some(Arc::new(MultiProgress::new()))
    } else {
        None
    };

    // In-degree scheduler: track outstanding prerequisites, spawn nodes as they
    // become ready, and unblock successors as each completes.
    let mut remaining_indegree = schedule.indegree.clone();
    let mut completed = vec![false; n];
    let mut in_flight = 0usize;
    let mut join_set: JoinSet<(usize, TaskOutcome)> = JoinSet::new();

    // The first failure to report (fail-fast). Preserved across the drain phase.
    // In keep-going mode this is left `None`; we summarize via `failed` instead.
    let first_failure: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

    // Count of tasks that failed. In keep-going mode this drives the final
    // summary/exit; in fail-fast mode the run aborts on the first failure so the
    // count is effectively 1.
    let mut failed = 0usize;
    // Nodes whose prerequisites failed and were therefore skipped (keep-going).
    let mut skipped = 0usize;

    // Queue of ready-but-not-yet-spawned node positions.
    let mut ready: Vec<usize> = schedule.initial_ready();

    loop {
        // Spawn every ready node (bounded by the semaphore inside each task).
        // Stop spawning new work once an abort has been signalled (fail-fast).
        if !abort.load(Ordering::SeqCst) {
            while let Some(pos) = ready.pop() {
                in_flight += 1;
                let spec = specs.clone();
                let cache_manager = cache_manager.clone();
                let semaphore = semaphore.clone();
                let abort = abort.clone();
                let total = total.clone();
                let cached = cached.clone();
                let multi = multi.clone();
                let loquacious = output.loquacious;
                let is_ci = output.is_ci();

                join_set.spawn(async move {
                    let outcome = run_node(
                        pos,
                        &spec,
                        &cache_manager,
                        &semaphore,
                        &abort,
                        &total,
                        &cached,
                        no_cache,
                        multi.as_deref(),
                        loquacious,
                        is_ci,
                    )
                    .await;
                    (pos, outcome)
                });
            }
        }

        if in_flight == 0 {
            break;
        }

        // Wait for the next task to finish. `in_flight > 0` here, so the JoinSet
        // is non-empty and `None` is unreachable — guard it defensively.
        let (pos, outcome) = match join_set.join_next().await {
            Some(Ok(res)) => res,
            Some(Err(e)) => {
                // A spawned task panicked. This is a runner-internal fault (not a
                // task exit code), so treat it as fatal even in keep-going mode.
                abort.store(true, Ordering::SeqCst);
                let mut ff = first_failure.lock().unwrap();
                if ff.is_none() {
                    *ff = Some(format!("Task runner panicked: {}", e));
                }
                drop(ff);
                failed += 1;
                in_flight -= 1;
                continue;
            }
            None => break,
        };
        in_flight -= 1;
        completed[pos] = true;

        match outcome {
            TaskOutcome::Cached | TaskOutcome::Ran => {
                // Unblock dependents: decrement their in-degree; those reaching
                // zero become ready (unless we're aborting).
                for &dep in &schedule.dependents[pos] {
                    if remaining_indegree[dep] > 0 {
                        remaining_indegree[dep] -= 1;
                        if remaining_indegree[dep] == 0 && !completed[dep] {
                            ready.push(dep);
                        }
                    }
                }
            }
            TaskOutcome::Failed {
                workspace,
                task,
                captured,
            } => {
                failed += 1;
                // Surface the failed task's captured output so the user can see
                // what broke. In interactive mode this is collapsed by default.
                surface_failure(multi.as_deref(), interactive, &workspace, &task, &captured);

                if keep_going {
                    // Keep-going: don't abort. Transitively skip every dependent
                    // of the failed node (its prerequisites can never succeed),
                    // so independent branches of the DAG keep running.
                    skipped += skip_dependents(pos, &schedule, &mut completed);
                } else {
                    // Fail-fast: signal abort and remember the first failure so
                    // scheduling of new work stops. Downstream tasks won't run.
                    abort.store(true, Ordering::SeqCst);
                    let mut ff = first_failure.lock().unwrap();
                    if ff.is_none() {
                        *ff = Some(format!(
                            "Task '{}:{}' failed. Stopping pipeline.",
                            workspace, task
                        ));
                    }
                }
            }
        }
    }

    // Clear the progress surface before printing the summary so it doesn't get
    // clobbered by the MultiProgress redraw.
    if let Some(m) = &multi {
        let _ = m.clear();
    }

    let elapsed_ms = global_start.elapsed().as_millis() as u64;

    // Fail-fast: return the first failure verbatim (also covers a runner panic).
    if let Some(msg) = first_failure.lock().unwrap().take() {
        return Err(anyhow::anyhow!(msg));
    }

    // Keep-going: everything that could run has run. If anything failed, report a
    // summary error so the process exits non-zero; `RunResult.failed` carries the
    // real count for the caller's summary line.
    if keep_going && failed > 0 {
        let result = RunResult {
            total: total.load(Ordering::SeqCst),
            cached: cached.load(Ordering::SeqCst),
            failed,
            elapsed_ms,
        };
        return Err(RunFailure { result, skipped }.into());
    }

    Ok(RunResult {
        total: total.load(Ordering::SeqCst),
        cached: cached.load(Ordering::SeqCst),
        failed,
        elapsed_ms,
    })
}

/// Transitively mark every dependent of a failed/skipped node as completed
/// (skipped), so keep-going mode never schedules a task whose prerequisites can
/// no longer succeed. Returns the number of nodes newly skipped.
fn skip_dependents(pos: usize, schedule: &Schedule, completed: &mut [bool]) -> usize {
    let mut skipped = 0usize;
    let mut stack = vec![pos];
    while let Some(cur) = stack.pop() {
        for &dep in &schedule.dependents[cur] {
            if !completed[dep] {
                completed[dep] = true;
                skipped += 1;
                stack.push(dep);
            }
        }
    }
    skipped
}

/// Error returned by keep-going mode when one or more tasks failed. Carries the
/// full `RunResult` so callers can still print an accurate summary line.
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

/// Execute a single node: acquire a concurrency permit, handle caching, run the
/// command, and store cache artifacts on success.
#[allow(clippy::too_many_arguments)]
async fn run_node(
    _pos: usize,
    specs: &[Option<TaskSpec>],
    cache_manager: &CacheManager,
    semaphore: &Semaphore,
    abort: &AtomicBool,
    total: &AtomicUsize,
    cached: &AtomicUsize,
    no_cache: bool,
    multi: Option<&MultiProgress>,
    loquacious: bool,
    is_ci: bool,
) -> TaskOutcome {
    let spec = match &specs[_pos] {
        Some(s) => s,
        // Unknown workspace: no-op, treated as a completed dependency edge.
        None => return TaskOutcome::Ran,
    };

    // Bound concurrency. If the pipeline has already aborted, skip fast.
    let _permit = semaphore.acquire().await.expect("semaphore not closed");
    if abort.load(Ordering::SeqCst) {
        // Fail-fast: don't start new work; report as a benign no-op so it does
        // not increment counters or unblock dependents beyond what already ran.
        return TaskOutcome::Ran;
    }

    total.fetch_add(1, Ordering::SeqCst);

    let interactive = multi.is_some();

    let hash = match cache_manager.compute_hash(
        &spec.ws_path,
        &spec.task_name,
        &spec.command,
        &spec.pipeline_task,
    ) {
        Ok(h) => h,
        Err(e) => {
            return TaskOutcome::Failed {
                workspace: spec.workspace_name.clone(),
                task: spec.task_name.clone(),
                captured: vec![(true, format!("failed to compute cache hash: {}", e))],
            };
        }
    };

    // Raw + loquacious: mirror the original detail trace. Skipped in interactive
    // mode where the TUI owns the render surface.
    if !interactive && loquacious {
        println!("  {}", style(format!("hash: {}", hash)).dim());
    }

    // Cache hit path.
    if !no_cache && !spec.is_persistent && cache_manager.is_cached(&hash) {
        if interactive {
            let pb = new_spinner(multi.unwrap(), &spec.workspace_name, &spec.task_name);
            if let Some(outputs) = &spec.pipeline_task.outputs {
                if !outputs.is_empty() {
                    let _ = cache_manager.restore_outputs(&hash, &spec.ws_path);
                }
            }
            finish_bar(
                &pb,
                &format!(
                    "{} {} {} {}",
                    style("●").green().bold(),
                    style(format!("{}:{}", spec.workspace_name, spec.task_name)).bold(),
                    style("cache hit").green(),
                    style(format!("[{}]", hash)).dim()
                ),
            );
        } else {
            // Raw mode: mirror the old plain println behavior.
            println!(
                "{} {} {} {}",
                style("●").green().bold(),
                style(format!("{}:{}", spec.workspace_name, spec.task_name)).bold(),
                style("cache hit").green(),
                style(format!("[{}]", hash)).dim()
            );
            if let Some(outputs) = &spec.pipeline_task.outputs {
                if !outputs.is_empty() {
                    if let Err(e) = cache_manager.restore_outputs(&hash, &spec.ws_path) {
                        eprintln!(
                            "{} Failed to restore cached outputs: {}",
                            style("warn").yellow().bold(),
                            e
                        );
                    }
                }
            }
        }
        cached.fetch_add(1, Ordering::SeqCst);
        return TaskOutcome::Cached;
    }

    // Run path.
    let pb = if interactive {
        let pb = new_spinner(multi.unwrap(), &spec.workspace_name, &spec.task_name);
        pb.set_message(format!(
            "{} {}",
            style(format!("{}:{}", spec.workspace_name, spec.task_name)).bold(),
            style("running...").cyan()
        ));
        Some(pb)
    } else {
        // Raw mode: plain start line, matching the old log_start / detail output.
        println!(
            "{} {} {}",
            style("▶").cyan().bold(),
            style(format!("{}:{}", spec.workspace_name, spec.task_name)).bold(),
            style("starting...").dim()
        );
        if loquacious {
            println!("  {}", style(format!("running: {}", spec.command)).dim());
        }
        None
    };

    let start = std::time::Instant::now();
    let result = run_command(&spec.command, &spec.ws_path, interactive, loquacious, is_ci).await;
    let duration_ms = start.elapsed().as_millis() as u64;

    match result {
        Ok((true, _captured)) => {
            // Store cache artifacts on success.
            if !spec.is_persistent && !no_cache {
                let env: HashMap<String, String> = spec
                    .pipeline_task
                    .env
                    .as_deref()
                    .unwrap_or(&[])
                    .iter()
                    .filter_map(|var| std::env::var(var).ok().map(|val| (var.clone(), val)))
                    .collect();

                let meta = CacheMeta {
                    hash: hash.clone(),
                    task: spec.task_name.clone(),
                    workspace: spec.workspace_name.clone(),
                    duration_ms,
                    last_used: chrono::Utc::now(),
                    env,
                };

                if let Err(e) = cache_manager.store_meta(&meta) {
                    warn_line(
                        pb.as_ref(),
                        interactive,
                        &format!("Failed to write cache metadata: {}", e),
                    );
                }

                if let Some(outputs) = &spec.pipeline_task.outputs {
                    if !outputs.is_empty() {
                        if let Err(e) = cache_manager.store_outputs(&hash, &spec.ws_path, outputs) {
                            warn_line(
                                pb.as_ref(),
                                interactive,
                                &format!("Failed to cache outputs: {}", e),
                            );
                        }
                    }
                }
            }

            if let Some(pb) = pb {
                finish_bar(
                    &pb,
                    &format!(
                        "{} {} {} {}",
                        style("✓").green().bold(),
                        style(format!("{}:{}", spec.workspace_name, spec.task_name)).bold(),
                        style(format!("{:.2}s", duration_ms as f64 / 1000.0)).dim(),
                        style("done").green()
                    ),
                );
            } else {
                println!(
                    "{} {} {} {}",
                    style("✓").green().bold(),
                    style(format!("{}:{}", spec.workspace_name, spec.task_name)).bold(),
                    style(format!("{:.2}s", duration_ms as f64 / 1000.0)).dim(),
                    style("done").green()
                );
            }
            TaskOutcome::Ran
        }
        Ok((false, captured)) => {
            if let Some(pb) = pb {
                finish_bar(
                    &pb,
                    &format!(
                        "{} {} {}",
                        style("✗").red().bold(),
                        style(format!("{}:{}", spec.workspace_name, spec.task_name)).bold(),
                        style("FAILED").red().bold()
                    ),
                );
            } else {
                eprintln!(
                    "{} {} {}",
                    style("✗").red().bold(),
                    style(format!("{}:{}", spec.workspace_name, spec.task_name)).bold(),
                    style("FAILED").red().bold()
                );
            }
            TaskOutcome::Failed {
                workspace: spec.workspace_name.clone(),
                task: spec.task_name.clone(),
                captured,
            }
        }
        Err(e) => {
            if let Some(pb) = pb {
                finish_bar(
                    &pb,
                    &format!(
                        "{} {} {}",
                        style("✗").red().bold(),
                        style(format!("{}:{}", spec.workspace_name, spec.task_name)).bold(),
                        style("FAILED").red().bold()
                    ),
                );
            } else {
                eprintln!(
                    "{} {} {}",
                    style("✗").red().bold(),
                    style(format!("{}:{}", spec.workspace_name, spec.task_name)).bold(),
                    style("FAILED").red().bold()
                );
            }
            TaskOutcome::Failed {
                workspace: spec.workspace_name.clone(),
                task: spec.task_name.clone(),
                captured: vec![(true, e.to_string())],
            }
        }
    }
}

/// Create a steady-tick spinner attached to the MultiProgress.
fn new_spinner(multi: &MultiProgress, workspace: &str, task: &str) -> ProgressBar {
    let pb = multi.add(ProgressBar::new_spinner());
    pb.set_style(
        ProgressStyle::with_template("{spinner:.cyan} {msg}")
            .unwrap()
            .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏ "),
    );
    pb.enable_steady_tick(Duration::from_millis(80));
    pb.set_message(format!(
        "{} {}",
        style(format!("{}:{}", workspace, task)).bold(),
        style("queued...").dim()
    ));
    pb
}

/// Finish a spinner, replacing it with a final static status line.
fn finish_bar(pb: &ProgressBar, msg: &str) {
    pb.set_style(ProgressStyle::with_template("{msg}").unwrap());
    pb.finish_with_message(msg.to_string());
}

/// Emit a warning either above the progress bars (interactive) or plainly.
fn warn_line(pb: Option<&ProgressBar>, interactive: bool, msg: &str) {
    let line = format!("{} {}", style("warn").yellow().bold(), msg);
    if interactive {
        if let Some(pb) = pb {
            pb.println(line);
            return;
        }
    }
    eprintln!("{}", line);
}

/// Surface a failed task's captured child output so the user can see the error.
/// In interactive mode we "expand on failure" using `MultiProgress::suspend`
/// (or `println` fallback); in raw mode the output was already streamed, so we
/// only re-print if it wasn't (i.e. not loquacious/CI is impossible here since
/// raw already streamed — but we keep captured for the interactive path).
fn surface_failure(
    multi: Option<&MultiProgress>,
    interactive: bool,
    workspace: &str,
    task: &str,
    captured: &[(bool, String)],
) {
    if !interactive || captured.is_empty() {
        return;
    }
    let header = format!(
        "{} output from {}:",
        style("✗").red().bold(),
        style(format!("{}:{}", workspace, task)).bold()
    );
    if let Some(m) = multi {
        m.suspend(|| {
            eprintln!("{}", header);
            for (is_stderr, line) in captured {
                if *is_stderr {
                    eprintln!("    {}", style(line).dim());
                } else {
                    eprintln!("    {}", line);
                }
            }
        });
    } else {
        eprintln!("{}", header);
        for (_is_stderr, line) in captured {
            eprintln!("    {}", line);
        }
    }
}

/// Run a command, returning `(success, captured_output)`.
///
/// In interactive mode child output is buffered (collapsed) and only surfaced on
/// failure. In raw mode (loquacious / CI) output is streamed live, line by line,
/// exactly as before.
async fn run_command(
    command: &str,
    cwd: &Path,
    interactive: bool,
    loquacious: bool,
    is_ci: bool,
) -> Result<(bool, Vec<(bool, String)>)> {
    let mut parts = shlex::split(command)
        .unwrap_or_else(|| vec!["sh".to_string(), "-c".to_string(), command.to_string()]);

    if parts.is_empty() {
        return Ok((true, Vec::new()));
    }

    let (program, args) = if command.contains("&&")
        || command.contains("||")
        || command.contains('|')
        || command.contains('>')
        || command.contains('<')
        || command.starts_with("./")
    {
        (
            "sh".to_string(),
            vec!["-c".to_string(), command.to_string()],
        )
    } else {
        let program = parts.remove(0);
        (program, parts)
    };

    let mut child = tokio::process::Command::new(&program)
        .args(&args)
        .current_dir(cwd)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                anyhow::anyhow!(
                    "Toolchain '{}' not found. Make sure it is installed and available in PATH.",
                    program
                )
            } else {
                anyhow::Error::from(e)
            }
        })?;

    let stdout = child.stdout.take().expect("stdout piped");
    let stderr = child.stderr.take().expect("stderr piped");

    // Stream live only in raw mode when loquacious/CI (unchanged behavior).
    // In interactive mode never stream: buffer and surface only on failure.
    let stream_live = !interactive && (loquacious || is_ci);

    let stdout_handle = tokio::spawn(async move {
        let mut reader = BufReader::new(stdout).lines();
        let mut lines = Vec::new();
        while let Ok(Some(line)) = reader.next_line().await {
            if stream_live {
                println!("    {}", line);
            }
            lines.push((false, line));
        }
        lines
    });

    let stderr_handle = tokio::spawn(async move {
        let mut reader = BufReader::new(stderr).lines();
        let mut lines = Vec::new();
        while let Ok(Some(line)) = reader.next_line().await {
            if stream_live {
                eprintln!("    {}", line);
            }
            lines.push((true, line));
        }
        lines
    });

    let status = child.wait().await?;
    let mut captured = stdout_handle.await.unwrap_or_default();
    let mut errs = stderr_handle.await.unwrap_or_default();
    captured.append(&mut errs);

    Ok((status.success(), captured))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_config::LatticeConfig;
    use lattice_dag::{build_execution_graph, TaskNode};
    use lattice_workspace::{Language, Workspace};
    use petgraph::graph::DiGraph;

    fn ws(name: &str, root: &Path, tasks: &[(&str, &str)]) -> Workspace {
        let path = root.join(name);
        std::fs::create_dir_all(&path).unwrap();
        Workspace {
            name: name.to_string(),
            path,
            language: Language::Unknown,
            auto: false,
            depends_on: Vec::new(),
            tasks: tasks
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        }
    }

    /// Manually build an ExecutionGraph with an edge a -> b (a is a prerequisite
    /// of b) to test the pure scheduling helper directly.
    fn two_node_graph_a_before_b() -> (ExecutionGraph, NodeIndex, NodeIndex) {
        let mut g: DiGraph<TaskNode, ()> = DiGraph::new();
        let a = g.add_node(TaskNode {
            workspace_name: "a".into(),
            task_name: "build".into(),
            command: "true".into(),
            is_persistent: false,
        });
        let b = g.add_node(TaskNode {
            workspace_name: "b".into(),
            task_name: "build".into(),
            command: "true".into(),
            is_persistent: false,
        });
        g.add_edge(a, b, ());
        let topo = petgraph::algo::toposort(&g, None).unwrap();
        (
            ExecutionGraph {
                graph: g,
                topo_order: topo,
            },
            a,
            b,
        )
    }

    #[test]
    fn schedule_respects_dependency_ordering() {
        let (graph, a, b) = two_node_graph_a_before_b();
        let (node_indices, schedule) = build_schedule(&graph);

        // Find dense positions of a and b.
        let pos_a = node_indices.iter().position(|&n| n == a).unwrap();
        let pos_b = node_indices.iter().position(|&n| n == b).unwrap();

        // a has no prerequisites; b has exactly one (a).
        assert_eq!(schedule.indegree[pos_a], 0);
        assert_eq!(schedule.indegree[pos_b], 1);
        assert!(schedule.prerequisites[pos_b].contains(&pos_a));
        assert!(schedule.prerequisites[pos_a].is_empty());

        // a is a dependency, so b is its dependent.
        assert_eq!(schedule.dependents[pos_a], vec![pos_b]);
        assert!(schedule.dependents[pos_b].is_empty());

        // Only a is initially ready.
        let init = schedule.initial_ready();
        assert_eq!(init, vec![pos_a]);
    }

    fn config_with(tasks: &[(&str, Option<&str>)]) -> LatticeConfig {
        let mut cfg = LatticeConfig::default();
        for (name, dep) in tasks {
            let mut pt = lattice_config::PipelineTask::default();
            if let Some(d) = dep {
                pt.depends_on = Some(vec![d.to_string()]);
            }
            cfg.pipeline.insert(name.to_string(), pt);
        }
        cfg
    }

    #[tokio::test]
    async fn all_tasks_run_with_correct_counts() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // Two independent workspaces each with a trivial build.
        let workspaces = vec![
            ws("wa", root, &[("build", "sh -c \"true\"")]),
            ws("wb", root, &[("build", "sh -c \"true\"")]),
        ];
        let config = config_with(&[("build", None)]);
        let graph = build_execution_graph(&workspaces, "build", &config).unwrap();
        let output = OutputManager::new(true); // loquacious => Raw / non-interactive

        let result = execute_tasks(
            &graph,
            &workspaces,
            &config,
            root,
            true,
            None,
            false,
            &output,
        )
        .await
        .unwrap();

        assert_eq!(result.total, 2);
        assert_eq!(result.cached, 0);
        assert_eq!(result.failed, 0);
    }

    #[tokio::test]
    async fn concurrency_one_runs_everything_serially() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // Three independent workspaces; with --concurrency 1 they run one at a
        // time but all must still complete with correct counts.
        let workspaces = vec![
            ws("wa", root, &[("build", "sh -c \"true\"")]),
            ws("wb", root, &[("build", "sh -c \"true\"")]),
            ws("wc", root, &[("build", "sh -c \"true\"")]),
        ];
        let config = config_with(&[("build", None)]);
        let graph = build_execution_graph(&workspaces, "build", &config).unwrap();
        let output = OutputManager::new(true);

        let result = execute_tasks(
            &graph,
            &workspaces,
            &config,
            root,
            true,
            Some(1),
            false,
            &output,
        )
        .await
        .unwrap();

        assert_eq!(result.total, 3);
        assert_eq!(result.cached, 0);
        assert_eq!(result.failed, 0);
    }

    #[tokio::test]
    async fn keep_going_runs_independent_tasks_and_reports_failed_count() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let marker = root.join("independent-ran.txt");
        let marker_str = marker.display().to_string();

        // wa:build fails. wb:build is independent and touches a marker: in
        // keep-going mode it must still run despite wa's failure.
        let workspaces = vec![
            ws("wa", root, &[("build", "sh -c \"exit 1\"")]),
            ws(
                "wb",
                root,
                &[("build", &format!("sh -c \"touch {}\"", marker_str))],
            ),
        ];
        let config = config_with(&[("build", None)]);
        let graph = build_execution_graph(&workspaces, "build", &config).unwrap();
        let output = OutputManager::new(true);

        // keep_going = true: the run should error, but report exactly one failure
        // and still have executed the independent task.
        let err = execute_tasks(
            &graph,
            &workspaces,
            &config,
            root,
            true,
            None,
            true,
            &output,
        )
        .await
        .unwrap_err();

        let run_failure = err
            .downcast_ref::<RunFailure>()
            .expect("keep-going should yield a RunFailure");
        assert_eq!(run_failure.result.failed, 1, "expected exactly one failure");
        assert!(
            marker.exists(),
            "independent task did not run in keep-going mode"
        );
    }

    #[tokio::test]
    async fn keep_going_skips_downstream_of_failed_prerequisite() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let marker = root.join("downstream-ran.txt");
        let marker_str = marker.display().to_string();

        // build fails; test dependsOn build. Even in keep-going mode, test's
        // prerequisite failed, so test must be skipped (marker never created).
        let workspaces = vec![ws(
            "wa",
            root,
            &[
                ("build", "sh -c \"exit 1\""),
                ("test", &format!("sh -c \"touch {}\"", marker_str)),
            ],
        )];
        let config = config_with(&[("build", None), ("test", Some("build"))]);
        let graph = build_execution_graph(&workspaces, "test", &config).unwrap();
        let output = OutputManager::new(true);

        let err = execute_tasks(
            &graph,
            &workspaces,
            &config,
            root,
            true,
            None,
            true,
            &output,
        )
        .await
        .unwrap_err();
        let run_failure = err.downcast_ref::<RunFailure>().unwrap();
        assert_eq!(run_failure.result.failed, 1);
        assert!(
            !marker.exists(),
            "downstream task ran despite a failed prerequisite in keep-going mode"
        );
    }

    #[tokio::test]
    async fn failure_stops_pipeline_and_blocks_downstream() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // build fails; test depends on build, so test must not run.
        let workspaces = vec![ws(
            "wa",
            root,
            &[("build", "sh -c \"exit 1\""), ("test", "sh -c \"true\"")],
        )];
        // test dependsOn build (same-workspace edge build -> test).
        let config = config_with(&[("build", None), ("test", Some("build"))]);
        let graph = build_execution_graph(&workspaces, "test", &config).unwrap();
        let output = OutputManager::new(true); // Raw / non-interactive

        let err = execute_tasks(
            &graph,
            &workspaces,
            &config,
            root,
            true,
            None,
            false,
            &output,
        )
        .await
        .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("wa:build") && msg.contains("Stopping pipeline"),
            "unexpected error message: {}",
            msg
        );
        // The `downstream_task_does_not_execute_on_failure` test proves the
        // dependent `test` task is actually blocked (via a side-effect marker).
    }

    #[tokio::test]
    async fn downstream_task_does_not_execute_on_failure() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let marker = root.join("marker.txt");
        let marker_str = marker.display().to_string();

        // build fails; test would create marker.txt if it ran.
        let workspaces = vec![ws(
            "wa",
            root,
            &[
                ("build", "sh -c \"exit 1\""),
                ("test", &format!("sh -c \"touch {}\"", marker_str)),
            ],
        )];
        let config = config_with(&[("build", None), ("test", Some("build"))]);
        let graph = build_execution_graph(&workspaces, "test", &config).unwrap();
        let output = OutputManager::new(true);

        let _ = execute_tasks(
            &graph,
            &workspaces,
            &config,
            root,
            true,
            None,
            false,
            &output,
        )
        .await;
        assert!(
            !marker.exists(),
            "downstream task ran despite upstream failure"
        );
    }
}
