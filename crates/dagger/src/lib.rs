//! Build the task execution DAG from resolved workspaces + the root task graph,
//! and derive a petgraph-independent [`Schedule`] the runner drives.

use std::collections::{HashMap, HashSet};

use anyhow::{bail, Result};
use petgraph::algo::toposort;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::Direction;

use lattice_config::LatticeConfig;
use lattice_workspace::Workspace;

#[derive(Debug, Clone)]
pub struct TaskNode {
	pub workspace_name: String,
	pub task_name: String,
	pub command: String,
	pub is_persistent: bool,
}

#[derive(Debug)]
pub struct ExecutionGraph {
	pub graph: DiGraph<TaskNode, ()>,
	pub topo_order: Vec<NodeIndex>,
}

/// The transitive closure of `root_tasks` over `dependsOn` (both same-workspace
/// and `^`-prefixed cross-workspace edges refer to task names). Stacked roots
/// share this one set, so a dependency common to several roots is collected once.
fn collect_task_set(root_tasks: &[&str], config: &LatticeConfig) -> Vec<String> {
	let mut visited = HashSet::new();
	let mut ordered = Vec::new();
	let mut stack: Vec<String> = root_tasks.iter().map(|t| t.to_string()).collect();

	while let Some(task) = stack.pop() {
		if visited.contains(&task) {
			continue;
		}
		visited.insert(task.clone());

		if let Some(task_cfg) = config.tasks.get(&task) {
			if let Some(deps) = &task_cfg.depends_on {
				for dep in deps {
					let dep_task = dep.strip_prefix('^').unwrap_or(dep).to_string();
					if !visited.contains(&dep_task) {
						stack.push(dep_task);
					}
				}
			}
		}
		ordered.push(task);
	}

	ordered
}

/// Whether any task in the transitive closure of `root_tasks` (over `dependsOn`)
/// is persistent. A persistent task (dev server, watcher) streams output
/// indefinitely, so the caller uses this to pick a streaming-friendly output
/// mode instead of the live TUI.
pub fn includes_persistent_task(root_tasks: &[&str], config: &LatticeConfig) -> bool {
	collect_task_set(root_tasks, config).iter().any(|task| {
		config
			.tasks
			.get(task)
			.map(|cfg| cfg.is_persistent())
			.unwrap_or(false)
	})
}

/// Build the cross-workspace execution graph for a single `root_task`.
///
/// Convenience wrapper over [`build_execution_graph_multi`] for the common
/// single-task case.
pub fn build_execution_graph(
	workspaces: &[Workspace],
	root_task: &str,
	config: &LatticeConfig,
) -> Result<ExecutionGraph> {
	build_execution_graph_multi(workspaces, &[root_task], config)
}

/// Build one combined cross-workspace execution graph for several stacked
/// `root_tasks` (e.g. `lattice run lint test build`).
///
/// The roots are merged into a single DAG, so a dependency shared by several
/// roots runs once and independent roots parallelize where the graph allows.
///
/// Edges: `^task` deps connect a workspace's task to the same task in each of
/// its `dependsOn` workspaces; bare deps connect tasks within one workspace.
/// A persistent task may not be depended on (it never completes); cycles are
/// rejected.
pub fn build_execution_graph_multi(
	workspaces: &[Workspace],
	root_tasks: &[&str],
	config: &LatticeConfig,
) -> Result<ExecutionGraph> {
	for root_task in root_tasks {
		if !config.tasks.contains_key(*root_task) {
			bail!(
				"task '{}' is not defined in the tasks section of lattice.json",
				root_task
			);
		}
	}

	let task_set = collect_task_set(root_tasks, config);
	let mut graph: DiGraph<TaskNode, ()> = DiGraph::new();
	let mut node_map: HashMap<(String, String), NodeIndex> = HashMap::new();

	// Inter-workspace dependency edges come from each workspace's `dependsOn`.
	let mut ws_deps: HashMap<String, Vec<String>> = HashMap::new();
	for ws in workspaces {
		if !ws.depends_on.is_empty() {
			ws_deps.insert(ws.name.clone(), ws.depends_on.clone());
		}
	}

	for task_name in &task_set {
		let task_cfg = config
			.tasks
			.get(task_name.as_str())
			.cloned()
			.unwrap_or_default();
		let is_persistent = task_cfg.is_persistent();

		for ws in workspaces {
			let command = match ws.command_for(task_name) {
				Some(cmd) => cmd.to_string(),
				None => {
					// A manual workspace that declares no command for the
					// requested root task halts. Auto workspaces silently skip
					// tasks that don't apply to their toolchain.
					if !ws.auto && root_tasks.contains(&task_name.as_str()) {
						bail!(
                            "workspace '{}' is \"auto\": false but declares no command for \
                             task '{}'; add it under this workspace's \"scripts\" map in lattice.json",
                            ws.name,
                            task_name
                        );
					}
					continue;
				}
			};

			let idx = graph.add_node(TaskNode {
				workspace_name: ws.name.clone(),
				task_name: task_name.clone(),
				command,
				is_persistent,
			});
			node_map.insert((ws.name.clone(), task_name.clone()), idx);
		}
	}

	for task_name in &task_set {
		let task_cfg = config
			.tasks
			.get(task_name.as_str())
			.cloned()
			.unwrap_or_default();
		if let Some(depends_on) = &task_cfg.depends_on {
			for dep in depends_on {
				if let Some(dep_task) = dep.strip_prefix('^') {
					// Cross-workspace: link each ws's task to that task in its deps.
					for ws in workspaces {
						if let Some(&to_idx) = node_map.get(&(ws.name.clone(), task_name.clone())) {
							let deps = ws_deps.get(&ws.name).cloned().unwrap_or_default();
							for dep_ws_name in &deps {
								if let Some(&from_idx) =
									node_map.get(&(dep_ws_name.clone(), dep_task.to_string()))
								{
									graph.add_edge(from_idx, to_idx, ());
								}
							}
						}
					}
				} else {
					// Same-workspace edge.
					for ws in workspaces {
						if let Some(&to_idx) = node_map.get(&(ws.name.clone(), task_name.clone())) {
							if let Some(&from_idx) = node_map.get(&(ws.name.clone(), dep.clone())) {
								graph.add_edge(from_idx, to_idx, ());
							}
						}
					}
				}
			}
		}
	}

	// Persistent tasks must be leaves: nothing may depend on them.
	for (key, &idx) in &node_map {
		if graph[idx].is_persistent {
			let outgoing = graph
				.neighbors_directed(idx, Direction::Outgoing)
				.next()
				.is_some();
			if outgoing {
				bail!(
					"persistent task '{}' in workspace '{}' cannot be depended on by other tasks",
					key.1,
					key.0
				);
			}
		}
	}

	let topo_order = toposort(&graph, None)
		.map_err(|_| anyhow::anyhow!("cycle detected in task dependency graph"))?;

	Ok(ExecutionGraph { graph, topo_order })
}

pub fn dry_run_order(graph: &ExecutionGraph) -> Vec<&TaskNode> {
	graph
		.topo_order
		.iter()
		.map(|&idx| &graph.graph[idx])
		.collect()
}

/// The scheduling shape of the DAG, decoupled from petgraph so it can be unit
/// tested in isolation.
///
/// * `prerequisites[i]` = node indices that must complete before node `i` may start;
/// * `dependents[i]` = nodes that become closer to ready once `i` finishes;
/// * `indegree[i]` = number of outstanding prerequisites for node `i`.
pub struct Schedule {
	pub prerequisites: Vec<HashSet<usize>>,
	pub dependents: Vec<Vec<usize>>,
	pub indegree: Vec<usize>,
}

impl Schedule {
	pub fn initial_ready(&self) -> Vec<usize> {
		(0..self.indegree.len())
			.filter(|&i| self.indegree[i] == 0)
			.collect()
	}
}

/// Build the [`Schedule`] from the DAG. An edge `from -> to` means `from` is a
/// prerequisite of `to`, and `to` is a dependent of `from`. Returns the dense
/// `NodeIndex` ordering the positions refer to.
pub fn build_schedule(graph: &ExecutionGraph) -> (Vec<NodeIndex>, Schedule) {
	let node_indices: Vec<NodeIndex> = graph.graph.node_indices().collect();
	let n = node_indices.len();

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

#[cfg(test)]
mod tests {
	use super::*;
	use indexmap::IndexMap;
	use lattice_config::{EngineMap, PipelineTask};

	fn ws(name: &str, deps: &[&str], commands: &[(&str, &str)]) -> Workspace {
		let mut map: IndexMap<String, String> = IndexMap::new();
		for (k, v) in commands {
			map.insert((*k).to_string(), (*v).to_string());
		}
		Workspace {
			name: name.to_string(),
			path: std::path::PathBuf::from(name),
			auto: true,
			depends_on: deps.iter().map(|s| s.to_string()).collect(),
			engines: EngineMap::new(),
			driver: None,
			commands: map,
		}
	}

	fn config(tasks: &[(&str, PipelineTask)]) -> LatticeConfig {
		let mut c = LatticeConfig::default();
		for (name, t) in tasks {
			c.tasks.insert((*name).to_string(), t.clone());
		}
		c
	}

	fn task(depends_on: &[&str]) -> PipelineTask {
		PipelineTask {
			depends_on: Some(depends_on.iter().map(|s| s.to_string()).collect()),
			..Default::default()
		}
	}

	#[test]
	fn toposort_orders_same_workspace_deps() {
		let workspaces = vec![ws(
			"app",
			&[],
			&[("build", "cargo build"), ("test", "cargo test")],
		)];
		let cfg = config(&[
			("build", PipelineTask::default()),
			("test", task(&["build"])),
		]);
		let g = build_execution_graph(&workspaces, "test", &cfg).unwrap();
		let order = dry_run_order(&g);
		let build_pos = order.iter().position(|n| n.task_name == "build").unwrap();
		let test_pos = order.iter().position(|n| n.task_name == "test").unwrap();
		assert!(build_pos < test_pos);
	}

	#[test]
	fn stacked_roots_merge_into_one_graph() {
		let workspaces = vec![ws(
			"app",
			&[],
			&[("lint", "eslint"), ("build", "tsc"), ("test", "vitest")],
		)];
		// test depends on build; lint is independent.
		let cfg = config(&[
			("lint", PipelineTask::default()),
			("build", PipelineTask::default()),
			("test", task(&["build"])),
		]);
		let g = build_execution_graph_multi(&workspaces, &["lint", "test", "build"], &cfg).unwrap();
		let order = dry_run_order(&g);

		let build_nodes = order.iter().filter(|n| n.task_name == "build").count();
		assert_eq!(build_nodes, 1, "shared dependency must not be duplicated");
		let build_pos = order.iter().position(|n| n.task_name == "build").unwrap();
		let test_pos = order.iter().position(|n| n.task_name == "test").unwrap();
		assert!(build_pos < test_pos);
		assert!(order.iter().any(|n| n.task_name == "lint"));
	}

	#[test]
	fn stacked_roots_reject_unknown_task() {
		let workspaces = vec![ws("app", &[], &[("build", "tsc")])];
		let cfg = config(&[("build", PipelineTask::default())]);
		let err = build_execution_graph_multi(&workspaces, &["build", "nope"], &cfg).unwrap_err();
		assert!(format!("{err}").contains("nope"));
	}

	#[test]
	fn cross_workspace_caret_edges() {
		let workspaces = vec![
			ws("lib", &[], &[("build", "cargo build -p lib")]),
			ws("app", &["lib"], &[("build", "cargo build -p app")]),
		];
		// app:build depends on ^build → lib:build.
		let cfg = config(&[("build", task(&["^build"]))]);
		let g = build_execution_graph(&workspaces, "build", &cfg).unwrap();
		let (indices, sched) = build_schedule(&g);

		let pos_of = |wsn: &str| {
			indices
				.iter()
				.position(|&ni| g.graph[ni].workspace_name == wsn)
				.unwrap()
		};
		let lib = pos_of("lib");
		let app = pos_of("app");
		assert_eq!(sched.indegree[lib], 0);
		assert_eq!(sched.indegree[app], 1);
		assert!(sched.prerequisites[app].contains(&lib));
		assert_eq!(sched.dependents[lib], vec![app]);
	}

	#[test]
	fn cycle_is_rejected() {
		let workspaces = vec![ws("app", &[], &[("a", "x"), ("b", "y")])];
		let cfg = config(&[("a", task(&["b"])), ("b", task(&["a"]))]);
		let err = build_execution_graph(&workspaces, "a", &cfg).unwrap_err();
		assert!(format!("{err}").contains("cycle"));
	}

	#[test]
	fn persistent_leaf_rejection() {
		let workspaces = vec![ws("app", &[], &[("dev", "vite"), ("build", "vite build")])];
		// build depends on dev, but dev is persistent → rejected.
		let dev = PipelineTask {
			persistent: Some(true),
			..Default::default()
		};
		let cfg = config(&[("dev", dev), ("build", task(&["dev"]))]);
		let err = build_execution_graph(&workspaces, "build", &cfg).unwrap_err();
		assert!(format!("{err}").contains("persistent"));
	}

	#[test]
	fn includes_persistent_task_detects_persistent_roots_and_deps() {
		let dev = PipelineTask {
			persistent: Some(true),
			..Default::default()
		};
		let cfg = config(&[
			("dev", dev),
			("build", PipelineTask::default()),
			// start depends on dev (a persistent dependency).
			("start", task(&["dev"])),
		]);

		// A persistent root is detected directly.
		assert!(includes_persistent_task(&["dev"], &cfg));
		// A persistent dependency pulled in transitively is detected.
		assert!(includes_persistent_task(&["start"], &cfg));
		// A run with no persistent task anywhere in its closure is not.
		assert!(!includes_persistent_task(&["build"], &cfg));
	}

	#[test]
	fn schedule_two_node_chain() {
		// a -> b (b depends on a).
		let workspaces = vec![ws("app", &[], &[("a", "x"), ("b", "y")])];
		let cfg = config(&[("a", PipelineTask::default()), ("b", task(&["a"]))]);
		let g = build_execution_graph(&workspaces, "b", &cfg).unwrap();
		let (indices, sched) = build_schedule(&g);

		let pos = |t: &str| {
			indices
				.iter()
				.position(|&ni| g.graph[ni].task_name == t)
				.unwrap()
		};
		let a = pos("a");
		let b = pos("b");
		assert_eq!(sched.indegree[a], 0);
		assert_eq!(sched.indegree[b], 1);
		assert!(sched.prerequisites[b].contains(&a));
		assert!(sched.prerequisites[a].is_empty());
		assert_eq!(sched.dependents[a], vec![b]);
		assert!(sched.dependents[b].is_empty());
		assert_eq!(sched.initial_ready(), vec![a]);
	}
}
