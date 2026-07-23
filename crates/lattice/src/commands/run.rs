use anyhow::{bail, Result};
use clap::Args;
use console::style;

use lattice_config::find_root;
use lattice_dag::build_execution_graph;
use lattice_output::OutputManager;
use lattice_runner::execute_tasks;
use lattice_workspace::discover_workspaces;

#[derive(Args, Debug)]
#[command(long_about = "Run a pipeline task across your workspaces.\n\n\
Lattice resolves the task's dependency graph from the 'pipeline' in lattice.json \
and executes it in parallel, honoring each task's dependsOn edges. Scope the run \
to specific workspaces with --filter, tune parallelism with --concurrency, and \
choose whether a failure stops the whole run (default) or lets independent tasks \
finish with --continue.\n\n\
Examples:\n  \
lattice run build\n  \
lattice run test --filter api\n  \
lattice run lint --concurrency 4 --continue")]
pub struct RunArgs {
    #[arg(help = "Pipeline task to run across workspaces (e.g. build, test, lint)")]
    pub task: String,

    #[arg(
        short,
        long,
        value_name = "PATTERN",
        help = "Only run in workspaces whose name contains this pattern"
    )]
    pub filter: Option<String>,

    #[arg(
        long,
        value_name = "N",
        help = "Cap how many tasks run at once (default: number of CPUs)"
    )]
    pub concurrency: Option<usize>,

    #[arg(
        long = "continue",
        help = "Keep running independent tasks after a failure instead of stopping"
    )]
    pub keep_going: bool,

    #[arg(long, help = "Ignore the cache and re-run every task")]
    pub no_cache: bool,

    #[arg(long, help = "Ignore the cache for this run (alias for --no-cache)")]
    pub force: bool,

    #[arg(
        long,
        help = "List the tasks that would run, then exit without running them"
    )]
    pub dry_run: bool,
}

impl RunArgs {
    pub async fn execute(&self, loquacious: bool) -> Result<()> {
        let cwd = std::env::current_dir()?;
        let root = find_root(&cwd).ok_or_else(|| {
            anyhow::anyhow!("No lattice.json found in this directory or any parent directory.")
        })?;

        let config = lattice_config::load_config(&root)?;
        let output = OutputManager::new(loquacious);

        if !config.pipeline.contains_key(self.task.as_str()) {
            bail!(
                "Task '{}' is not defined in the pipeline. \
                Add it to the 'pipeline' section of lattice.json.",
                self.task
            );
        }

        let mut workspaces = discover_workspaces(&root, &config)?;

        if workspaces.is_empty() {
            output.warn(
                "No workspaces found. Declare them in the 'workspaces' array of lattice.json.",
            );
            return Ok(());
        }

        if let Some(filter) = &self.filter {
            workspaces.retain(|ws| ws.name.contains(filter.as_str()));
            if workspaces.is_empty() {
                output.warn(&format!("No workspaces matched filter '{}'", filter));
                return Ok(());
            }
        }

        output.header(&format!(
            "\n{} Lattice — running {} across {} workspaces\n",
            style("◆").cyan().bold(),
            style(&self.task).bold(),
            workspaces.len()
        ));

        if loquacious {
            output.detail(&format!("root: {}", root.display()));
            output.detail(&format!(
                "workspaces: {}",
                workspaces
                    .iter()
                    .map(|w| format!("{} ({})", w.name, w.language.name()))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }

        let graph = build_execution_graph(&workspaces, &self.task, &config)?;

        if self.dry_run {
            println!("{} Dry run — tasks that would execute:", style("◇").dim());
            for &node_idx in &graph.topo_order {
                let node = &graph.graph[node_idx];
                println!(
                    "  {} {}  {}",
                    style("→").dim(),
                    style(format!("{}:{}", node.workspace_name, node.task_name)).bold(),
                    style(&node.command).dim()
                );
            }
            return Ok(());
        }

        // `--force` is a Turbo-familiar alias for `--no-cache`; either one bypasses
        // the cache for this run.
        let no_cache = self.no_cache || self.force;

        let result = match execute_tasks(
            &graph,
            &workspaces,
            &config,
            &root,
            no_cache,
            self.concurrency,
            self.keep_going,
            &output,
        )
        .await
        {
            Ok(result) => result,
            Err(err) => {
                // In keep-going mode a run with failures returns a RunFailure that
                // still carries the full tally, so print an accurate summary line
                // before propagating the non-zero exit.
                if let Some(failure) = err.downcast_ref::<lattice_runner::RunFailure>() {
                    let r = failure.result;
                    output.summary(r.total, r.cached, r.failed, r.elapsed_ms);
                }
                return Err(err);
            }
        };

        output.summary(
            result.total,
            result.cached,
            result.failed,
            result.elapsed_ms,
        );

        Ok(())
    }
}
