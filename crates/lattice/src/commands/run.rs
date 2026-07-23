use anyhow::{bail, Result};
use clap::Args;
use console::style;

use lattice_config::find_root;
use lattice_dag::build_execution_graph;
use lattice_output::OutputManager;
use lattice_runner::execute_tasks;
use lattice_workspace::discover_workspaces;

#[derive(Args, Debug)]
pub struct RunArgs {
    #[arg(help = "Task name to run across workspaces")]
    pub task: String,

    #[arg(
        short,
        long,
        help = "Only run tasks in workspaces matching this pattern"
    )]
    pub filter: Option<String>,

    #[arg(long, help = "Skip the cache and force re-execution of all tasks")]
    pub no_cache: bool,

    #[arg(long, help = "List tasks that would run without executing")]
    pub dry_run: bool,
}

impl RunArgs {
    pub async fn execute(&self, loquacious: bool) -> Result<()> {
        let cwd = std::env::current_dir()?;
        let root = find_root(&cwd)
            .ok_or_else(|| anyhow::anyhow!("No lattice.json found in this directory or any parent directory."))?;

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
            output.warn("No workspaces found. Declare them in the 'workspaces' array of lattice.json.");
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

        let result = execute_tasks(&graph, &workspaces, &config, &root, self.no_cache, &output).await?;

        output.summary(result.total, result.cached, result.failed, result.elapsed_ms);

        Ok(())
    }
}
