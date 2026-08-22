use anyhow::Result;
use clap::Args;
use console::style;

use dagger::{dry_run_order, includes_persistent_task, ExecutionGraph};
use lattice_output::{apply_color_policy, banner_line, make_reporter, paint_teal, OutputMode};
use lattice_project::{Plan, Project, RunOptions, RunRequest};

use crate::cli::{detect_output_mode, effective_loquacious, maybe_emit_version_nag, BIN_VERSION};

#[derive(Args, Debug)]
#[command(long_about = "Run one or more tasks across your workspaces.\n\n\
Lattice builds each task's dependency graph from the `tasks` map in lattice.json, \
then runs that graph in dependency order. Naming several tasks at once merges them \
into one graph, so a dependency they share runs once. To run each task's graph to \
completion before the next one starts, pass --sequentially.\n\n\
Examples:\n  \
lattice run build\n  \
lattice run lint test build\n  \
lattice run lint test build --sequentially\n  \
lattice run test --filter api\n  \
lattice run lint --concurrency 4 --continue")]
pub struct RunArgs {
	/// One or more task names, separated by spaces.
	#[arg(required = true, num_args = 1..)]
	pub tasks: Vec<String>,

	/// Run each task's graph to completion in turn, instead of merging them
	/// into one combined graph.
	#[arg(short = 's', long = "sequentially")]
	pub sequentially: bool,

	/// Run in the workspaces whose name contains this pattern, plus what they
	/// depend on.
	#[arg(short, long, value_name = "PATTERN")]
	pub filter: Option<String>,

	/// Cap how many tasks run at once. The default is the number of CPUs.
	#[arg(long, value_name = "N")]
	pub concurrency: Option<usize>,

	/// Keep running independent tasks after a failure instead of stopping.
	#[arg(long = "continue")]
	pub keep_going: bool,

	/// Neither read nor write the cache. Lattice re-runs every task and stores nothing.
	#[arg(long)]
	pub no_cache: bool,

	/// Re-run every task and write fresh cache entries, replacing any already stored.
	#[arg(long)]
	pub force: bool,

	/// List the tasks that would run, then exit without running them.
	#[arg(long)]
	pub dry_run: bool,
}

impl RunArgs {
	pub async fn execute(&self, flag_loq: bool, no_version_check: bool) -> Result<()> {
		let cwd = std::env::current_dir()?;
		let project = Project::open(&cwd)?;
		project.require_known_tasks(&self.tasks)?;

		let effective_loq = effective_loquacious(flag_loq, project.config.settings.loquacious);
		let mut mode = detect_output_mode(effective_loq);

		// Persistent tasks (dev servers, watchers) stream output indefinitely,
		// which the live TUI can't render. Default such runs to raw, CI-style
		// line output so that streaming output stays visible.
		if mode == OutputMode::Interactive {
			let task_refs: Vec<&str> = self.tasks.iter().map(|t| t.as_str()).collect();
			if includes_persistent_task(&task_refs, &project.config) {
				mode = OutputMode::Raw;
			}
		}

		// The mode is final here, and nothing has printed yet.
		apply_color_policy(mode);

		let request = self.request();

		if self.dry_run {
			return self.print_plan(&project, &request);
		}

		match project.plan(&request.plan)? {
			Plan::NoWorkspaces => {
				// A freshly-scaffolded repo has an empty `workspaces` array; exit 0
				// rather than erroring.
				println!(
					"lattice: no workspaces declared. Add one to the \
                     `workspaces` array in lattice.json, then run `{}`.",
					self.tasks.join(" ")
				);
				return Ok(());
			}
			Plan::NoMatch { filter } => {
				// A filtered no-op is not a failure; report and exit cleanly.
				println!("lattice: no workspaces matched filter '{}'.", filter);
				return Ok(());
			}
			Plan::Phases(_) => {}
		}

		// Advisory version-drift nag, before the run and only in interactive mode.
		maybe_emit_version_nag(mode, &project.config, no_version_check);

		let reporter = make_reporter(mode, effective_loq);

		let outcome = lattice_project::run(RunOptions {
			project: &project,
			request: &request,
			reporter: reporter.as_ref(),
			lattice_version: BIN_VERSION,
			// A fresh ctrl-c future per phase: the runner consumes it to tear down
			// still-running persistent tasks after its graph drains.
			shutdown: Some(Box::new(|| {
				Box::pin(async {
					let _ = tokio::signal::ctrl_c().await;
				})
			})),
			// The terminal's own signal is the cancel here, and the runner already
			// watches for it.
			cancel: None,
		})
		.await?;

		match outcome.exit_code() {
			0 => Ok(()),
			// The runner already printed the run summary, including for a
			// keep-going failure; just carry the status out.
			code => std::process::exit(code),
		}
	}

	fn request(&self) -> RunRequest {
		RunRequest {
			plan: lattice_project::PlanRequest {
				tasks: self.tasks.clone(),
				filter: self.filter.clone(),
				sequentially: self.sequentially,
			},
			no_cache: self.no_cache,
			force: self.force,
			concurrency: self.concurrency,
			keep_going: self.keep_going,
		}
	}

	fn print_plan(&self, project: &Project, request: &RunRequest) -> Result<()> {
		match project.plan(&request.plan)? {
			Plan::NoWorkspaces => {
				println!(
					"lattice: no workspaces declared. Add one to the \
                     `workspaces` array in lattice.json, then run `{}`.",
					self.tasks.join(" ")
				);
			}
			Plan::NoMatch { filter } => {
				println!("lattice: no workspaces matched filter '{}'.", filter);
			}
			Plan::Phases(phases) => {
				if self.sequentially {
					// Each phase is its own graph; list them in order.
					for (task, graph) in self.tasks.iter().zip(&phases) {
						print_dry_run(&format!("dry run · {} (phase)", task), graph);
					}
				} else {
					for graph in &phases {
						print_dry_run(&format!("dry run · {}", self.tasks.join(" ")), graph);
					}
				}
			}
		}
		Ok(())
	}
}

/// Print a task graph's topological order under a banner (used by `--dry-run`).
fn print_dry_run(banner: &str, graph: &ExecutionGraph) {
	println!("{}", banner_line(banner));
	for node in dry_run_order(graph) {
		// Tag the nodes a --filter did not match, so it is clear which lines are
		// there because something matched depends on them.
		let tag = if node.pulled_in {
			format!(" {}", style("(dependency)").dim())
		} else {
			String::new()
		};
		println!(
			"  {} {}{}  {}",
			paint_teal("→"),
			style(format!("{}:{}", node.workspace_name, node.task_name)).bold(),
			tag,
			style(&node.command).dim()
		);
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn the_cli_flags_reach_the_request_unchanged() {
		let args = RunArgs {
			tasks: vec!["build".to_string(), "test".to_string()],
			sequentially: true,
			filter: Some("api".to_string()),
			concurrency: Some(4),
			keep_going: true,
			no_cache: false,
			force: true,
			dry_run: false,
		};
		let request = args.request();
		assert_eq!(request.plan.tasks, vec!["build", "test"]);
		assert_eq!(request.plan.filter.as_deref(), Some("api"));
		assert!(request.plan.sequentially);
		assert_eq!(request.concurrency, Some(4));
		assert!(request.keep_going);
		// --force re-runs and refreshes; it does not stop the run from storing.
		assert_eq!(request.cache_flags(), (true, false));
	}
}
