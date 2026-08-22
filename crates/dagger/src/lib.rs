//! Build the task execution DAG from resolved workspaces + the root task graph,
//! and derive a petgraph-independent [`Schedule`] the runner drives.

use std::collections::{HashMap, HashSet};

use anyhow::{bail, Result};
use petgraph::algo::toposort;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::EdgeRef;
use petgraph::Direction;

use serde::Serialize;

use lattice_config::LatticeConfig;
use lattice_workspace::Workspace;

#[derive(Debug, Clone)]
pub struct TaskNode {
	pub workspace_name: String,
	pub task_name: String,
	pub command: String,
	pub is_persistent: bool,
	/// False when the node's workspace was outside the run's workspace selection
	/// and the node is only here because something selected depends on it.
	pub pulled_in: bool,
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
	build_execution_graph_selected(workspaces, root_tasks, config, None)
}

/// Build the graph for `root_tasks` narrowed to `selected_workspaces` plus
/// everything those workspaces depend on, transitively.
///
/// `selected_workspaces` holds workspace names — the ones a `--filter` matched.
/// They are the roots of the run: every node they reach backwards through the
/// graph comes along, so a `^build` edge into an unselected workspace still
/// resolves to that workspace's task. Nodes outside the closure are dropped, and
/// the ones pulled in only as prerequisites are marked [`TaskNode::pulled_in`].
/// `None` selects every workspace, which is [`build_execution_graph_multi`].
pub fn build_execution_graph_selected(
	workspaces: &[Workspace],
	root_tasks: &[&str],
	config: &LatticeConfig,
	selected_workspaces: Option<&HashSet<String>>,
) -> Result<ExecutionGraph> {
	for root_task in root_tasks {
		if !config.tasks.contains_key(*root_task) {
			bail!(
				"task '{}' is not defined in the `tasks` map in lattice.json",
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
			let selected = selected_workspaces.is_none_or(|sel| sel.contains(&ws.name));
			let command = match ws.command_for(task_name) {
				Some(cmd) => cmd.to_string(),
				None => {
					// A manual workspace that declares no command for the
					// requested root task halts — unless the workspace is outside
					// the selection, where the task was never asked for. Auto
					// workspaces silently skip tasks that don't apply to their
					// toolchain.
					if !ws.auto && selected && root_tasks.contains(&task_name.as_str()) {
						bail!(
                            "workspace '{}' has \"auto\": false and declares no command for \
                             task '{}'. Add the command under this workspace's \"scripts\" \
                             map in lattice.json",
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
				pulled_in: !selected,
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

	if selected_workspaces.is_some() {
		graph = dependency_closure(&graph);
	}

	// Persistent tasks must be leaves: nothing may depend on them.
	for idx in graph.node_indices() {
		if graph[idx].is_persistent
			&& graph
				.neighbors_directed(idx, Direction::Outgoing)
				.next()
				.is_some()
		{
			bail!(
				"task '{}' in workspace '{}' is persistent, so no other task may depend on it",
				graph[idx].task_name,
				graph[idx].workspace_name
			);
		}
	}

	let topo_order = toposort(&graph, None)
		.map_err(|_| anyhow::anyhow!("the task graph has a cycle"))?;

	Ok(ExecutionGraph { graph, topo_order })
}

/// The selected nodes plus everything they depend on, transitively, as a new
/// graph. Nodes outside that closure are dropped; a node is reached only through
/// incoming edges, so a dependent of a selected node does not come along.
fn dependency_closure(graph: &DiGraph<TaskNode, ()>) -> DiGraph<TaskNode, ()> {
	let mut keep: HashSet<NodeIndex> = HashSet::new();
	let mut stack: Vec<NodeIndex> = graph
		.node_indices()
		.filter(|&idx| !graph[idx].pulled_in)
		.collect();
	while let Some(idx) = stack.pop() {
		if !keep.insert(idx) {
			continue;
		}
		for src in graph.neighbors_directed(idx, Direction::Incoming) {
			if !keep.contains(&src) {
				stack.push(src);
			}
		}
	}

	let mut pruned: DiGraph<TaskNode, ()> = DiGraph::new();
	let mut remap: HashMap<NodeIndex, NodeIndex> = HashMap::with_capacity(keep.len());
	for idx in graph.node_indices() {
		if keep.contains(&idx) {
			remap.insert(idx, pruned.add_node(graph[idx].clone()));
		}
	}
	// Index order, not map order: the pruned graph's node and edge insertion order
	// decides the topological order the dry run prints.
	for old_to in graph.node_indices() {
		let Some(&new_to) = remap.get(&old_to) else {
			continue;
		};
		for src in graph.neighbors_directed(old_to, Direction::Incoming) {
			if let Some(&new_from) = remap.get(&src) {
				pruned.add_edge(new_from, new_to, ());
			}
		}
	}

	pruned
}

pub fn dry_run_order(graph: &ExecutionGraph) -> Vec<&TaskNode> {
	graph
		.topo_order
		.iter()
		.map(|&idx| &graph.graph[idx])
		.collect()
}

/// How a node is named outside the graph. Positions are meaningless to anything
/// that did not build the graph, so a dump identifies nodes the way the rest of
/// Lattice does — the label a run reports and a filter matches.
pub fn node_id(workspace: &str, task: &str) -> String {
	format!("{workspace}:{task}")
}

/// One node of a [`GraphDump`].
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphNode {
	pub id: String,
	pub workspace: String,
	pub task: String,
	pub command: String,
	pub persistent: bool,
	pub pulled_in: bool,
}

/// A dependency: `from` must finish before `to` may start.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphEdge {
	pub from: String,
	pub to: String,
	/// A `^task` dependency, satisfied by another workspace rather than this one.
	pub cross_workspace: bool,
}

/// A graph in a form something outside this crate can read.
///
/// `nodes` follows the topological order, so reading it front to back is a valid
/// execution order and a layered drawing can assign depth without sorting first.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphDump {
	pub nodes: Vec<GraphNode>,
	pub edges: Vec<GraphEdge>,
}

pub fn dump_graph(graph: &ExecutionGraph) -> GraphDump {
	let nodes: Vec<GraphNode> = graph
		.topo_order
		.iter()
		.map(|&idx| {
			let node = &graph.graph[idx];
			GraphNode {
				id: node_id(&node.workspace_name, &node.task_name),
				workspace: node.workspace_name.clone(),
				task: node.task_name.clone(),
				command: node.command.clone(),
				persistent: node.is_persistent,
				pulled_in: node.pulled_in,
			}
		})
		.collect();

	let mut edges: Vec<GraphEdge> = graph
		.graph
		.edge_references()
		.map(|edge| {
			let from = &graph.graph[edge.source()];
			let to = &graph.graph[edge.target()];
			GraphEdge {
				from: node_id(&from.workspace_name, &from.task_name),
				to: node_id(&to.workspace_name, &to.task_name),
				cross_workspace: from.workspace_name != to.workspace_name,
			}
		})
		.collect();
	// Petgraph's edge order is an insertion detail. Sorting makes two dumps of
	// the same graph compare equal, which is what lets a caller diff them.
	edges.sort_unstable_by(|a, b| (&a.from, &a.to).cmp(&(&b.from, &b.to)));

	GraphDump { nodes, edges }
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

	fn selection(names: &[&str]) -> HashSet<String> {
		names.iter().map(|n| (*n).to_string()).collect()
	}

	/// `base <- mid <- top`, each depending on the previous, all with a `build`.
	fn chain() -> Vec<Workspace> {
		vec![
			ws("base", &[], &[("build", "build base")]),
			ws("mid", &["base"], &[("build", "build mid")]),
			ws("top", &["mid"], &[("build", "build top")]),
		]
	}

	#[test]
	fn selected_workspace_pulls_in_transitive_deps() {
		let cfg = config(&[("build", task(&["^build"]))]);
		let g =
			build_execution_graph_selected(&chain(), &["build"], &cfg, Some(&selection(&["top"])))
				.unwrap();
		let order = dry_run_order(&g);

		let names: Vec<&str> = order.iter().map(|n| n.workspace_name.as_str()).collect();
		assert_eq!(names, vec!["base", "mid", "top"]);
		// Only the match is a root of the run; the rest came along as prerequisites.
		assert!(!order.last().unwrap().pulled_in);
		assert!(order[0].pulled_in && order[1].pulled_in);
	}

	#[test]
	fn selection_excludes_workspaces_that_depend_on_the_match() {
		let cfg = config(&[("build", task(&["^build"]))]);
		let g =
			build_execution_graph_selected(&chain(), &["build"], &cfg, Some(&selection(&["mid"])))
				.unwrap();
		let names: Vec<&str> = dry_run_order(&g)
			.iter()
			.map(|n| n.workspace_name.as_str())
			.collect();
		assert_eq!(names, vec!["base", "mid"]);
	}

	#[test]
	fn selection_dedupes_a_diamond() {
		let workspaces = vec![
			ws("base", &[], &[("build", "build base")]),
			ws("left", &["base"], &[("build", "build left")]),
			ws("right", &["base"], &[("build", "build right")]),
			ws("app", &["left", "right"], &[("build", "build app")]),
		];
		let cfg = config(&[("build", task(&["^build"]))]);
		let g = build_execution_graph_selected(
			&workspaces,
			&["build"],
			&cfg,
			Some(&selection(&["app"])),
		)
		.unwrap();
		let order = dry_run_order(&g);

		assert_eq!(order.len(), 4, "the shared dependency is collected once");
		let pos = |name: &str| {
			order
				.iter()
				.position(|n| n.workspace_name == name)
				.unwrap_or_else(|| panic!("{name} is missing from the graph"))
		};
		assert!(pos("base") < pos("left"));
		assert!(pos("base") < pos("right"));
		assert!(pos("left") < pos("app"));
		assert!(pos("right") < pos("app"));
	}

	#[test]
	fn selecting_a_leaf_pulls_in_nothing() {
		let cfg = config(&[("build", task(&["^build"]))]);
		let g =
			build_execution_graph_selected(&chain(), &["build"], &cfg, Some(&selection(&["base"])))
				.unwrap();
		let order = dry_run_order(&g);

		assert_eq!(order.len(), 1);
		assert_eq!(order[0].workspace_name, "base");
		assert!(!order[0].pulled_in);
	}

	#[test]
	fn selecting_nothing_yields_an_empty_graph() {
		let cfg = config(&[("build", task(&["^build"]))]);
		let g = build_execution_graph_selected(&chain(), &["build"], &cfg, Some(&HashSet::new()))
			.unwrap();
		assert!(dry_run_order(&g).is_empty());
	}

	#[test]
	fn same_workspace_deps_survive_a_selection() {
		// test depends on build in its own workspace, and build on ^build.
		let mut workspaces = chain();
		for w in &mut workspaces {
			w.commands.insert("test".to_string(), "test".to_string());
		}
		let cfg = config(&[("build", task(&["^build"])), ("test", task(&["build"]))]);
		let g = build_execution_graph_selected(
			&workspaces,
			&["test"],
			&cfg,
			Some(&selection(&["top"])),
		)
		.unwrap();
		let order = dry_run_order(&g);

		let labels: Vec<String> = order
			.iter()
			.map(|n| format!("{}:{}", n.workspace_name, n.task_name))
			.collect();
		assert_eq!(
			labels,
			vec!["base:build", "mid:build", "top:build", "top:test"],
			"only the selected workspace's test runs, on top of every build it needs"
		);
	}

	#[test]
	fn a_manual_workspace_outside_the_selection_may_lack_the_task() {
		let mut deps_only = ws("base", &[], &[("build", "build base")]);
		deps_only.auto = false;
		let mut app = ws(
			"app",
			&["base"],
			&[("build", "build app"), ("serve", "serve")],
		);
		app.auto = false;
		let workspaces = vec![deps_only, app];
		let cfg = config(&[("build", task(&["^build"])), ("serve", task(&["build"]))]);

		// Unfiltered, `serve` halts: base is manual and declares no serve command.
		let err = build_execution_graph_multi(&workspaces, &["serve"], &cfg).unwrap_err();
		assert!(format!("{err}").contains("base"));

		// Selecting app asks base only for the build it depends on, which it has.
		let g = build_execution_graph_selected(
			&workspaces,
			&["serve"],
			&cfg,
			Some(&selection(&["app"])),
		)
		.unwrap();
		let labels: Vec<String> = dry_run_order(&g)
			.iter()
			.map(|n| format!("{}:{}", n.workspace_name, n.task_name))
			.collect();
		assert_eq!(labels, vec!["base:build", "app:build", "app:serve"]);
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
	#[test]
	fn a_dump_lists_nodes_in_topological_order() {
		let workspaces = vec![ws(
			"api",
			&[],
			&[("build", "make api"), ("codegen", "make codegen")],
		)];
		let build = PipelineTask {
			depends_on: Some(vec!["codegen".to_string()]),
			..Default::default()
		};
		let codegen = PipelineTask::default();
		let cfg = config(&[("build", build), ("codegen", codegen)]);
		let graph = build_execution_graph(&workspaces, "build", &cfg).unwrap();

		let dump = dump_graph(&graph);
		let order: Vec<&str> = dump.nodes.iter().map(|n| n.id.as_str()).collect();
		assert_eq!(order, vec!["api:codegen", "api:build"]);
	}

	#[test]
	fn a_dump_keeps_every_edge_and_points_it_at_the_dependent() {
		let workspaces = vec![
			ws("core", &[], &[("build", "make core")]),
			ws("web", &["core"], &[("build", "make web")]),
		];
		let build = PipelineTask {
			depends_on: Some(vec!["^build".to_string()]),
			..Default::default()
		};
		let cfg = config(&[("build", build)]);
		let graph = build_execution_graph(&workspaces, "build", &cfg).unwrap();

		let dump = dump_graph(&graph);
		assert_eq!(
			dump.edges.len(),
			graph.graph.edge_count(),
			"every edge in the graph has to survive the dump"
		);
		let edge = &dump.edges[0];
		assert_eq!(edge.from, "core:build");
		assert_eq!(edge.to, "web:build");
		assert!(edge.cross_workspace);
	}

	#[test]
	fn a_same_workspace_edge_is_not_marked_cross_workspace() {
		let workspaces = vec![ws("api", &[], &[("build", "b"), ("codegen", "c")])];
		let build = PipelineTask {
			depends_on: Some(vec!["codegen".to_string()]),
			..Default::default()
		};
		let cfg = config(&[("build", build), ("codegen", PipelineTask::default())]);
		let graph = build_execution_graph(&workspaces, "build", &cfg).unwrap();

		let dump = dump_graph(&graph);
		assert_eq!(dump.edges.len(), 1);
		assert!(!dump.edges[0].cross_workspace);
	}

	#[test]
	fn a_diamond_dumps_all_four_of_its_edges_in_a_stable_order() {
		let workspaces = vec![
			ws("base", &[], &[("build", "b")]),
			ws("left", &["base"], &[("build", "b")]),
			ws("right", &["base"], &[("build", "b")]),
			ws("top", &["left", "right"], &[("build", "b")]),
		];
		let build = PipelineTask {
			depends_on: Some(vec!["^build".to_string()]),
			..Default::default()
		};
		let cfg = config(&[("build", build)]);
		let graph = build_execution_graph(&workspaces, "build", &cfg).unwrap();

		let dump = dump_graph(&graph);
		let pairs: Vec<(&str, &str)> = dump
			.edges
			.iter()
			.map(|e| (e.from.as_str(), e.to.as_str()))
			.collect();
		assert_eq!(
			pairs,
			vec![
				("base:build", "left:build"),
				("base:build", "right:build"),
				("left:build", "top:build"),
				("right:build", "top:build"),
			]
		);
	}

	#[test]
	fn a_node_dump_uses_camel_case_on_the_wire() {
		let workspaces = vec![ws("web", &[], &[("dev", "vite")])];
		let dev = PipelineTask {
			persistent: Some(true),
			..Default::default()
		};
		let cfg = config(&[("dev", dev)]);
		let graph = build_execution_graph(&workspaces, "dev", &cfg).unwrap();

		let value = serde_json::to_value(dump_graph(&graph)).unwrap();
		assert_eq!(
			value["nodes"][0],
			serde_json::json!({
				"id": "web:dev",
				"workspace": "web",
				"task": "dev",
				"command": "vite",
				"persistent": true,
				"pulledIn": false
			})
		);
	}
}
