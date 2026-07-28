use anyhow::{bail, Result};
use clap::Args;
use console::style;

use dagger::{build_execution_graph_multi, dry_run_order, includes_persistent_task, ExecutionGraph};
use lattice_config::find_root;
use lattice_output::{banner_line, make_reporter, paint_teal, OutputMode};
use lattice_runner::{execute_tasks, ExecuteOptions, RunFailure};
use lattice_workspace::discover_workspaces;

use crate::cli::{detect_output_mode, effective_loquacious, maybe_emit_version_nag, BIN_VERSION};

#[derive(Args, Debug)]
#[command(long_about = "Run one or more tasks across your workspaces.\n\n\
Lattice resolves each task's dependency graph from the `tasks` map in lattice.json \
and runs it in dependency order across your workspaces. Stack tasks to run them as \
one combined graph — a shared dependency runs once and independent tasks parallelize, \
or pass --sequentially to run each task's graph to completion before the next. \
Scope with --filter, tune parallelism with --concurrency, and choose whether a failure \
stops the run (default) or lets independent tasks finish with --continue.\n\n\
Examples:\n  \
lattice run build\n  \
lattice run lint test build\n  \
lattice run lint test build --sequentially\n  \
lattice run test --filter api\n  \
lattice run lint --concurrency 4 --continue")]
pub struct RunArgs {
    /// One or more tasks to run across workspaces (e.g. lint test build).
    #[arg(required = true, num_args = 1..)]
    pub tasks: Vec<String>,

    /// Run the given tasks one at a time, each graph to completion, in order —
    /// instead of merging them into one combined graph.
    #[arg(short = 's', long = "sequentially")]
    pub sequentially: bool,

    /// Only run in workspaces whose name contains this pattern.
    #[arg(short, long, value_name = "PATTERN")]
    pub filter: Option<String>,

    /// Cap how many tasks run at once (default: number of CPUs).
    #[arg(long, value_name = "N")]
    pub concurrency: Option<usize>,

    /// Keep running independent tasks after a failure instead of stopping.
    #[arg(long = "continue")]
    pub keep_going: bool,

    /// Ignore the cache and re-run every task.
    #[arg(long)]
    pub no_cache: bool,

    /// Ignore the cache for this run (alias for --no-cache).
    #[arg(long)]
    pub force: bool,

    /// List the tasks that would run, then exit without running them.
    #[arg(long)]
    pub dry_run: bool,
}

impl RunArgs {
    pub async fn execute(&self, flag_loq: bool, no_version_check: bool) -> Result<()> {
        let cwd = std::env::current_dir()?;
        let root = find_root(&cwd).ok_or_else(|| {
            anyhow::anyhow!(
                "No lattice.json found in this directory or any parent. \
                 Run `lattice init` to create one."
            )
        })?;

        crate::schema::ensure_schema(&root);
        let config = lattice_config::load_config(&root)?;

        for task in &self.tasks {
            if !config.tasks.contains_key(task.as_str()) {
                let mut available: Vec<&str> = config.tasks.keys().map(|s| s.as_str()).collect();
                available.sort_unstable();
                let listed = if available.is_empty() {
                    "(none defined)".to_string()
                } else {
                    available.join(", ")
                };
                bail!(
                    "Task '{}' is not defined in lattice.json. Available tasks: {}",
                    task,
                    listed
                );
            }
        }

        let effective_loq = effective_loquacious(flag_loq, config.settings.loquacious);
        let mut mode = detect_output_mode(effective_loq);

        // Persistent tasks (dev servers, watchers) stream output indefinitely,
        // which the live TUI can't render. Default such runs to raw, CI-style
        // line output so that streaming output stays visible.
        if mode == OutputMode::Interactive {
            let task_refs: Vec<&str> = self.tasks.iter().map(|t| t.as_str()).collect();
            if includes_persistent_task(&task_refs, &config) {
                mode = OutputMode::Raw;
            }
        }

        let mut workspaces = discover_workspaces(&root, &config)?;

        if let Some(filter) = &self.filter {
            workspaces.retain(|ws| ws.name.contains(filter.as_str()));
            if workspaces.is_empty() {
                // Nothing to run is not a failure — report and exit cleanly so a
                // filtered no-op never breaks a pipeline.
                println!("lattice: no workspaces matched filter '{}'.", filter);
                return Ok(());
            }
        }

        if workspaces.is_empty() {
            // A freshly-scaffolded repo (empty `workspaces`) has nothing to run
            // yet — show the shape and exit 0 rather than erroring.
            println!(
                "lattice: no workspaces declared yet — add them to the \
                 `workspaces` array in lattice.json to run `{}`.",
                self.tasks.join(" ")
            );
            return Ok(());
        }

        let no_cache = self.no_cache || self.force;

        if self.dry_run {
            if self.sequentially {
                // Each phase is its own graph; list them in order.
                for task in &self.tasks {
                    let graph = build_execution_graph_multi(&workspaces, &[task.as_str()], &config)?;
                    print_dry_run(&format!("dry run · {} (phase)", task), &graph);
                }
            } else {
                let task_refs: Vec<&str> = self.tasks.iter().map(|t| t.as_str()).collect();
                let graph = build_execution_graph_multi(&workspaces, &task_refs, &config)?;
                print_dry_run(&format!("dry run · {}", self.tasks.join(" ")), &graph);
            }
            return Ok(());
        }

        // Advisory version-drift nag, before the run and only in interactive mode.
        maybe_emit_version_nag(mode, &config, no_version_check);

        let reporter = make_reporter(mode, effective_loq);

        // A fresh ctrl-c future for each run: the runner consumes it to tear down
        // still-running persistent tasks after its graph drains.
        let make_shutdown = || {
            Some(Box::pin(async {
                let _ = tokio::signal::ctrl_c().await;
            })
                as std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>)
        };

        if self.sequentially {
            // Run each task's full graph to completion, in order, before the next.
            let mut failed_any = false;
            for task in &self.tasks {
                let graph = build_execution_graph_multi(&workspaces, &[task.as_str()], &config)?;
                let opts = ExecuteOptions {
                    graph: &graph,
                    workspaces: &workspaces,
                    config: &config,
                    root: &root,
                    no_cache,
                    concurrency: self.concurrency,
                    keep_going: self.keep_going,
                    reporter: reporter.as_ref(),
                    lattice_version: BIN_VERSION,
                    shutdown: make_shutdown(),
                };
                if let Err(err) = execute_tasks(opts).await {
                    if self.keep_going {
                        // A phase failed; carry on through the remaining phases.
                        failed_any = true;
                        continue;
                    }
                    // Fail-fast: stop at the first failed phase.
                    if err.downcast_ref::<RunFailure>().is_some() {
                        std::process::exit(1);
                    }
                    return Err(err);
                }
            }
            if failed_any {
                std::process::exit(1);
            }
            return Ok(());
        }

        // Default: merge every stacked task into one combined graph.
        let task_refs: Vec<&str> = self.tasks.iter().map(|t| t.as_str()).collect();
        let graph = build_execution_graph_multi(&workspaces, &task_refs, &config)?;
        let opts = ExecuteOptions {
            graph: &graph,
            workspaces: &workspaces,
            config: &config,
            root: &root,
            no_cache,
            concurrency: self.concurrency,
            keep_going: self.keep_going,
            reporter: reporter.as_ref(),
            lattice_version: BIN_VERSION,
            shutdown: make_shutdown(),
        };

        match execute_tasks(opts).await {
            Ok(_) => Ok(()),
            Err(err) => {
                // The runner already printed the run summary (including for a
                // keep-going RunFailure); just propagate a non-zero exit.
                if err.downcast_ref::<RunFailure>().is_some() {
                    // Non-zero exit without an extra error line: the reporter
                    // has already surfaced everything.
                    std::process::exit(1);
                }
                Err(err)
            }
        }
    }
}

/// Print a task graph's topological order under a banner (used by `--dry-run`).
fn print_dry_run(banner: &str, graph: &ExecutionGraph) {
    println!("{}", banner_line(banner));
    for node in dry_run_order(graph) {
        println!(
            "  {} {}  {}",
            paint_teal("→"),
            style(format!("{}:{}", node.workspace_name, node.task_name)).bold(),
            style(&node.command).dim()
        );
    }
}
