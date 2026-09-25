use anyhow::Result;
use clap::Args;
use console::style;

use dagger::{dry_run_order, includes_persistent_task, ExecutionGraph};
use lattice_output::{apply_color_policy, banner_line, make_reporter, paint_teal, OutputMode};
use lattice_project::{Plan, Project, RunOptions, RunRequest};

use crate::cli::{detect_output_mode, effective_loquacious, maybe_emit_version_nag, BIN_VERSION};

#[derive(Args, Debug)]
#[command(long_about = "Run one or more tasks across your workspaces.\n\n\
A task runs in every workspace that has it: a script in package.json, a recipe in a \
justfile, or a verb its tool has, such as `cargo clippy` for lint. Workspaces run in \
dependency order, and naming several tasks at once merges them into one graph, so a \
dependency they share runs once. Run from inside a workspace to run it and what it \
depends on.\n\n\
Examples:\n  \
lattice run build\n  \
lattice run lint test build\n  \
lattice run test --filter api\n  \
lattice run test -- --nocapture\n  \
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

	/// Arguments after `--` are passed to the tasks you named.
	#[arg(last = true, value_name = "ARGS")]
	pub args: Vec<String>,
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
			if includes_persistent_task(&task_refs, &project.workspaces) {
				mode = OutputMode::Raw;
			}
		}

		// The mode is final here, and nothing has printed yet.
		apply_color_policy(mode);

		let request = self.request(project.cwd_workspace.as_deref());

		if self.dry_run {
			// A dry run has no runner to report through, so it gets a reporter of
			// its own for the one thing there is to say before the plan.
			let reporter = make_reporter(mode, effective_loq);
			project.report_skipped_tasks(&self.tasks, reporter.as_ref());
			reporter.finish();
			return self.print_plan(&project, &request);
		}

		match project.plan(&request.plan)? {
			Plan::NoWorkspaces => {
				// A freshly-scaffolded repo has an empty `workspaces` array; exit 0
				// rather than erroring.
				println!("{NO_WORKSPACES}");
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
			// A fresh interrupt future per phase: the runner consumes it to tear down
			// still-running persistent tasks after its graph drains. It has to watch
			// the same signals the runner does — a bare ctrl_c left a CI job
			// cancelled with SIGTERM hanging until the runner force-killed it.
			shutdown: Some(Box::new(|| Box::pin(lattice_runner::interrupt_signal()))),
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

	/// `cwd_workspace` scopes the run to the workspace it was started inside,
	/// unless `--filter` picks workspaces itself.
	fn request(&self, cwd_workspace: Option<&str>) -> RunRequest {
		RunRequest {
			plan: lattice_project::PlanRequest {
				tasks: self.tasks.clone(),
				filter: self.filter.clone(),
				workspaces: cwd_workspace
					.filter(|_| self.filter.is_none())
					.map(|ws| vec![ws.to_string()]),
				args: self.args.clone(),
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
				println!("{NO_WORKSPACES}");
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

const NO_WORKSPACES: &str = "lattice: found no workspaces. The repo root names none in a \
workspace file and holds no project Lattice recognizes. List workspace directories under \
`workspaces` in lattice.json, like \"apps/*\".";

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
	use clap::Parser;

	#[derive(Parser)]
	struct Harness {
		#[command(flatten)]
		run: RunArgs,
	}

	fn parse(argv: &[&str]) -> RunArgs {
		Harness::try_parse_from(argv).unwrap().run
	}

	#[test]
	fn a_run_from_inside_a_workspace_is_scoped_to_it() {
		let args = parse(&["run", "build"]);
		let request = args.request(Some("web"));
		assert_eq!(request.plan.workspaces, Some(vec!["web".to_string()]));
	}

	#[test]
	fn arguments_after_a_double_dash_are_passed_through() {
		let args = parse(&["run", "test", "--", "--nocapture", "-q"]);
		assert_eq!(args.tasks, ["test"]);
		assert_eq!(args.args, ["--nocapture", "-q"]);
	}

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
			args: vec!["-x".to_string()],
		};
		let request = args.request(Some("web"));
		assert_eq!(request.plan.tasks, vec!["build", "test"]);
		assert_eq!(request.plan.filter.as_deref(), Some("api"));
		assert_eq!(
			request.plan.workspaces, None,
			"a filter wins over the directory"
		);
		assert_eq!(request.plan.args, ["-x"]);
		assert!(request.plan.sequentially);
		assert_eq!(request.concurrency, Some(4));
		assert!(request.keep_going);
		// --force re-runs and refreshes; it does not stop the run from storing.
		assert_eq!(request.cache_flags(), (true, false));
	}
}
