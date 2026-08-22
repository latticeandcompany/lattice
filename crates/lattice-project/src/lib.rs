//! One opened repo, and the pipeline both front ends run against it.
//!
//! Opening a project is four steps that always happen together — find the root,
//! keep the schema present, load the config, resolve the workspaces — and running
//! a task is five more. They used to live inside the CLI's `run` subcommand,
//! which meant the only way to reach them was to be a terminal.
//!
//! Nothing here renders anything or ends the process. A run reports through
//! [`lattice_events::Reporter`] and returns a [`RunOutcome`]; the caller decides
//! whether that is an exit status or a window.

pub mod scaffold;
pub mod view;

use std::collections::HashSet;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

use dagger::{build_execution_graph_selected, ExecutionGraph};
use lattice_config::LatticeConfig;
use lattice_events::Reporter;
use lattice_runner::{execute_tasks, ExecuteOptions, RunFailure, RunInterrupted, RunResult};
use lattice_workspace::{discover_workspaces, Workspace};

/// A repo Lattice has read: where it is, what it declares, and what that resolves
/// to on this machine.
#[derive(Debug)]
pub struct Project {
	pub root: PathBuf,
	pub config: LatticeConfig,
	pub workspaces: Vec<Workspace>,
}

impl Project {
	/// Walk up from `start` for a `lattice.json` and resolve everything it names.
	pub fn open(start: &Path) -> Result<Self> {
		let root = lattice_config::find_root(start).ok_or_else(|| {
			anyhow::anyhow!(
				"no lattice.json found in this directory or any parent. \
				 Run `lattice init` to create one"
			)
		})?;
		Self::open_root(&root)
	}

	/// Open a directory already known to be the repo root.
	pub fn open_root(root: &Path) -> Result<Self> {
		lattice_config::schema::ensure_schema(root);
		let config = lattice_config::load_config(root)?;
		let workspaces = discover_workspaces(root, &config)?;
		Ok(Self {
			root: root.to_path_buf(),
			config,
			workspaces,
		})
	}

	/// Re-read the config and re-resolve the workspaces in place.
	pub fn reload(&mut self) -> Result<()> {
		let fresh = Self::open_root(&self.root)?;
		self.config = fresh.config;
		self.workspaces = fresh.workspaces;
		Ok(())
	}

	/// Fail if any of `tasks` is not defined in the pipeline, naming the ones that
	/// are. A run is refused before anything is provisioned or spawned.
	pub fn require_known_tasks(&self, tasks: &[String]) -> Result<()> {
		for task in tasks {
			if !self.config.tasks.contains_key(task.as_str()) {
				let mut available: Vec<&str> =
					self.config.tasks.keys().map(|s| s.as_str()).collect();
				available.sort_unstable();
				let listed = if available.is_empty() {
					"lattice.json defines no tasks".to_string()
				} else {
					format!("Defined tasks: {}", available.join(", "))
				};
				bail!(
					"task '{}' is not defined in the `tasks` map in lattice.json. {}",
					task,
					listed
				);
			}
		}
		Ok(())
	}

	/// The workspace names a `--filter` selects, or `None` for no filter.
	///
	/// The filter picks the workspaces a run is *for*; the graph builder pulls in
	/// whatever they depend on. An empty match is [`Plan::NoMatch`], not an error.
	pub fn select(&self, filter: Option<&str>) -> Option<HashSet<String>> {
		filter.map(|pattern| {
			self.workspaces
				.iter()
				.filter(|ws| ws.name.contains(pattern))
				.map(|ws| ws.name.clone())
				.collect()
		})
	}

	/// Build the graphs a request would run, without running them.
	pub fn plan(&self, request: &PlanRequest) -> Result<Plan> {
		if self.workspaces.is_empty() {
			return Ok(Plan::NoWorkspaces);
		}
		let selected = self.select(request.filter.as_deref());
		if let Some(matched) = &selected {
			if matched.is_empty() {
				return Ok(Plan::NoMatch {
					filter: request.filter.clone().unwrap_or_default(),
				});
			}
		}
		let selected = selected.as_ref();

		let tasks: Vec<&str> = request.tasks.iter().map(|t| t.as_str()).collect();
		let phases = if request.sequentially {
			// Each task is its own graph, run to completion in order.
			tasks
				.iter()
				.map(|task| {
					build_execution_graph_selected(
						&self.workspaces,
						&[task],
						&self.config,
						selected,
					)
				})
				.collect::<Result<Vec<_>>>()?
		} else {
			vec![build_execution_graph_selected(
				&self.workspaces,
				&tasks,
				&self.config,
				selected,
			)?]
		};
		Ok(Plan::Phases(phases))
	}
}

/// What to run, and how to narrow it.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PlanRequest {
	pub tasks: Vec<String>,
	/// Substring matched against workspace names.
	pub filter: Option<String>,
	/// Run each task's graph to completion before starting the next, instead of
	/// merging them into one graph.
	pub sequentially: bool,
}

/// The graphs a request resolved to, or the reason there are none.
///
/// Neither empty case is a failure: a freshly scaffolded repo declares no
/// workspaces, and a filter that matches nothing was still a valid thing to ask.
pub enum Plan {
	NoWorkspaces,
	NoMatch { filter: String },
	Phases(Vec<ExecutionGraph>),
}

/// A run request: a plan plus how to treat the cache and how hard to push.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RunRequest {
	#[serde(flatten)]
	pub plan: PlanRequest,
	/// Neither read nor write the cache.
	pub no_cache: bool,
	/// Re-run every task and write fresh entries over what is there.
	pub force: bool,
	pub concurrency: Option<usize>,
	/// Keep running independent tasks after a failure.
	pub keep_going: bool,
}

impl RunRequest {
	/// `(no_cache, no_store)` for the runner.
	///
	/// Both skip lookups; only `no_cache` also skips writing. `force` exists to
	/// replace a bad entry, which it cannot do if it never stores one.
	pub fn cache_flags(&self) -> (bool, bool) {
		(self.no_cache || self.force, self.no_cache)
	}
}

/// How a run ended.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum RunOutcome {
	Completed {
		result: RunResult,
	},
	Failed {
		result: RunResult,
	},
	Interrupted {
		result: RunResult,
	},
	/// There was nothing to run: no workspaces, or no workspace matched.
	Nothing,
}

impl RunOutcome {
	/// The process exit status this outcome corresponds to. 130 is the shell's
	/// convention for a run ended by SIGINT (128 + 2), which lets a CI runner tell
	/// a cancelled job from a failed build.
	pub fn exit_code(&self) -> i32 {
		match self {
			Self::Completed { .. } | Self::Nothing => 0,
			Self::Failed { .. } => 1,
			Self::Interrupted { .. } => 130,
		}
	}

	pub fn result(&self) -> Option<RunResult> {
		match self {
			Self::Completed { result } | Self::Failed { result } | Self::Interrupted { result } => {
				Some(*result)
			}
			Self::Nothing => None,
		}
	}
}

/// A future the runner waits on, built fresh per phase.
///
/// A factory rather than one future because a sequential run executes several
/// graphs, and each needs its own: a future is consumed by the phase that awaits
/// it, so phase two would otherwise have nothing to wait on.
///
/// `Send + Sync` because a caller may be driving the run from a task that itself
/// has to be `Send` — a GUI's IPC handler, for instance.
pub type SignalFactory<'a> =
	Box<dyn Fn() -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync + 'a>;

/// Everything a run needs that is not part of the request.
pub struct RunOptions<'a> {
	pub project: &'a Project,
	pub request: &'a RunRequest,
	pub reporter: &'a dyn Reporter,
	pub lattice_version: &'a str,
	/// Tears down persistent tasks once the graph has drained.
	pub shutdown: Option<SignalFactory<'a>>,
	/// Aborts the run in progress. See [`lattice_runner::ExecuteOptions::cancel`].
	pub cancel: Option<SignalFactory<'a>>,
}

/// Run a request to completion.
///
/// A failed or interrupted run is an `Ok` carrying that outcome: the caller asked
/// what happened, and "the build failed" is an answer, not an error. `Err` is
/// reserved for a request that could not be attempted — an undefined task, a
/// cycle, a toolchain that would not provision.
pub async fn run(opts: RunOptions<'_>) -> Result<RunOutcome> {
	let RunOptions {
		project,
		request,
		reporter,
		lattice_version,
		shutdown,
		cancel,
	} = opts;

	project.require_known_tasks(&request.plan.tasks)?;

	let phases = match project.plan(&request.plan)? {
		Plan::NoWorkspaces | Plan::NoMatch { .. } => return Ok(RunOutcome::Nothing),
		Plan::Phases(phases) => phases,
	};

	let (no_cache, no_store) = request.cache_flags();
	let mut worst: Option<RunOutcome> = None;

	for graph in &phases {
		let execute = ExecuteOptions {
			graph,
			workspaces: &project.workspaces,
			config: &project.config,
			root: &project.root,
			no_cache,
			no_store,
			concurrency: request.concurrency,
			keep_going: request.keep_going,
			reporter,
			lattice_version,
			shutdown: shutdown.as_ref().map(|make| make()),
			cancel: cancel.as_ref().map(|make| make()),
		};

		match execute_tasks(execute).await {
			Ok(result) => {
				worst = Some(match worst {
					Some(RunOutcome::Failed { .. }) => RunOutcome::Failed { result },
					_ => RunOutcome::Completed { result },
				});
			}
			Err(err) => {
				// An interrupt ends the whole run, not just this phase: whoever
				// asked for it did not mean "skip to the next one".
				if let Some(interrupted) = err.downcast_ref::<RunInterrupted>() {
					return Ok(RunOutcome::Interrupted {
						result: interrupted.result,
					});
				}
				if let Some(failure) = err.downcast_ref::<RunFailure>() {
					if request.keep_going {
						worst = Some(RunOutcome::Failed {
							result: failure.result,
						});
						continue;
					}
					return Ok(RunOutcome::Failed {
						result: failure.result,
					});
				}
				return Err(err);
			}
		}
	}

	Ok(worst.unwrap_or(RunOutcome::Nothing))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn force_refreshes_the_entry_and_no_cache_writes_nothing() {
		// The two flags overlap, and the overlap is the point: --force has to
		// store, or it could never replace the entry it re-ran to replace.
		let normal = RunRequest::default();
		assert_eq!(normal.cache_flags(), (false, false));

		let forced = RunRequest {
			force: true,
			..Default::default()
		};
		assert_eq!(forced.cache_flags(), (true, false));

		let uncached = RunRequest {
			no_cache: true,
			..Default::default()
		};
		assert_eq!(uncached.cache_flags(), (true, true));

		let both = RunRequest {
			no_cache: true,
			force: true,
			..Default::default()
		};
		assert_eq!(both.cache_flags(), (true, true));
	}

	#[test]
	fn an_outcome_maps_to_the_status_the_shell_expects() {
		let result = RunResult {
			total: 1,
			cached: 0,
			failed: 0,
			elapsed_ms: 1,
		};
		assert_eq!(RunOutcome::Completed { result }.exit_code(), 0);
		assert_eq!(RunOutcome::Nothing.exit_code(), 0);
		assert_eq!(RunOutcome::Failed { result }.exit_code(), 1);
		assert_eq!(RunOutcome::Interrupted { result }.exit_code(), 130);
	}

	#[test]
	fn nothing_carries_no_result() {
		assert!(RunOutcome::Nothing.result().is_none());
	}
}
