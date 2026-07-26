use anyhow::{bail, Result};
use clap::Args;
use console::style;

use lattice_config::find_root;
use lattice_dag::{build_execution_graph, dry_run_order};
use lattice_output::{banner_line, make_reporter, teal};
use lattice_runner::{execute_tasks, ExecuteOptions, RunFailure};
use lattice_workspace::discover_workspaces;

use crate::cli::{detect_output_mode, effective_loquacious, maybe_emit_version_nag, BIN_VERSION};

#[derive(Args, Debug)]
#[command(long_about = "Run a task across your workspaces.\n\n\
Lattice resolves the task's dependency graph from the `tasks` map in lattice.json \
and runs it in dependency order across your workspaces. Scope with --filter, tune \
parallelism with --concurrency, and choose whether a failure stops the run (default) \
or lets independent tasks finish with --continue.\n\n\
Examples:\n  \
lattice run build\n  \
lattice run test --filter api\n  \
lattice run lint --concurrency 4 --continue")]
pub struct RunArgs {
    /// Task to run across workspaces (e.g. build, test, lint).
    pub task: String,

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

        let config = lattice_config::load_config(&root)?;

        if !config.tasks.contains_key(self.task.as_str()) {
            let mut available: Vec<&str> = config.tasks.keys().map(|s| s.as_str()).collect();
            available.sort_unstable();
            let listed = if available.is_empty() {
                "(none defined)".to_string()
            } else {
                available.join(", ")
            };
            bail!(
                "Task '{}' is not defined in lattice.json. Available tasks: {}",
                self.task,
                listed
            );
        }

        let effective_loq = effective_loquacious(flag_loq, config.settings.loquacious);
        let mode = detect_output_mode(effective_loq);

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
                self.task
            );
            return Ok(());
        }

        let graph = build_execution_graph(&workspaces, &self.task, &config)?;

        if self.dry_run {
            println!("{}", banner_line(&format!("dry run · {}", self.task)));
            for node in dry_run_order(&graph) {
                println!(
                    "  {} {}  {}",
                    teal().apply_to("→"),
                    style(format!("{}:{}", node.workspace_name, node.task_name)).bold(),
                    style(&node.command).dim()
                );
            }
            return Ok(());
        }

        // Advisory version-drift nag, before the run and only in interactive mode.
        maybe_emit_version_nag(mode, &config, no_version_check);

        let reporter = make_reporter(mode, effective_loq);
        let no_cache = self.no_cache || self.force;

        let shutdown = Some(Box::pin(async {
            let _ = tokio::signal::ctrl_c().await;
        })
            as std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>);

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
            shutdown,
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
