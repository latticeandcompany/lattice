//! Build the task execution DAG from resolved workspaces, and derive a
//! petgraph-independent [`Schedule`] the runner drives.
//!
//! Each workspace carries its own resolved tasks, so the graph is built from
//! those: a node is a task some workspace can run, and its `dependsOn` — as
//! that workspace resolved it — draws the edges.

use std::collections::{HashMap, HashSet};

use anyhow::{bail, Result};
use petgraph::algo::toposort;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::EdgeRef;
use petgraph::Direction;

use serde::Serialize;

use lattice_config::closest_field;
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

/// Every `dependsOn` any workspace gives `task`, `^` kept.
fn deps_of<'a>(task: &str, workspaces: &'a [Workspace]) -> Vec<&'a String> {
	let mut deps: Vec<&String> = Vec::new();
	for ws in workspaces {
		if let Some(t) = ws.task(task) {
			for dep in t.config.depends_on.iter().flatten() {
				if !deps.contains(&dep) {
					deps.push(dep);
				}
			}
		}
	}
	deps
}

/// The transitive closure of `root_tasks` over `dependsOn`, across every
/// workspace. Stacked roots share this one set, so a dependency common to
/// several roots is collected once.
fn collect_task_set(root_tasks: &[&str], workspaces: &[Workspace]) -> Vec<String> {
	let mut visited = HashSet::new();
	let mut ordered = Vec::new();
	let mut stack: Vec<String> = root_tasks.iter().map(|t| t.to_string()).collect();

	while let Some(task) = stack.pop() {
		if !visited.insert(task.clone()) {
			continue;
		}
		for dep in deps_of(&task, workspaces) {
			let bare = dep.strip_prefix('^').unwrap_or(dep).to_string();
			if !visited.contains(&bare) {
				stack.push(bare);
			}
		}
		ordered.push(task);
	}

	ordered
}

/// Whether any task in the transitive closure of `root_tasks` is persistent in
/// any workspace, so the caller can pick an output mode that streams.
pub fn includes_persistent_task(root_tasks: &[&str], workspaces: &[Workspace]) -> bool {
	collect_task_set(root_tasks, workspaces).iter().any(|task| {
		workspaces
			.iter()
			.any(|ws| ws.task(task).is_some_and(|t| t.config.is_persistent()))
	})
}

/// Every task name any workspace can run, sorted.
pub fn known_tasks(workspaces: &[Workspace]) -> Vec<String> {
	let mut names: Vec<String> = workspaces
		.iter()
		.flat_map(|ws| ws.tasks.keys().cloned())
		.collect();
	names.sort();
	names.dedup();
	names
}

/// Fail on a task nobody can run, naming what is there instead. Asked of the
/// tasks a run requests and of every `dependsOn` they pull in, since a
/// misspelled prerequisite would otherwise drop its ordering without a word.
fn require_somewhere(
	task: &str,
	needed_by: Option<(&str, &str)>,
	workspaces: &[Workspace],
) -> Result<()> {
	if workspaces.iter().any(|ws| ws.tasks.contains_key(task)) {
		return Ok(());
	}
	let known = known_tasks(workspaces);
	let mut message = match needed_by {
		Some((owner, dep)) => {
			format!("task '{owner}' depends on '{dep}', but no workspace has a '{task}' task")
		}
		None => format!("no workspace has a '{task}' task"),
	};
	if let Some(near) = closest_field(task, &known) {
		message.push_str(&format!("\nDid you mean `{near}`?"));
	}
	if known.is_empty() {
		message.push_str("\nLattice found no tasks in any workspace");
	} else {
		message.push_str(&format!("\nTasks: {}", known.join(", ")));
	}
	bail!(message)
}

/// Build one combined graph for `root_tasks` across every workspace.
pub fn build_execution_graph(
	workspaces: &[Workspace],
	root_tasks: &[&str],
) -> Result<ExecutionGraph> {
	build_execution_graph_selected(workspaces, root_tasks, None)
}

/// Build the graph for `root_tasks` narrowed to `selected_workspaces` plus
/// everything those workspaces depend on, transitively.
///
/// The roots are merged into a single DAG, so a dependency shared by several
/// roots runs once and independent roots parallelize where the graph allows.
/// `^task` connects a workspace's task to that task in each workspace it depends
/// on; a bare name connects tasks within one workspace. A persistent task may
/// not be depended on (it never completes), and cycles are rejected.
///
/// `selected_workspaces` holds the names a filter or the working directory
/// picked. They are the roots of the run: every node they reach backwards
/// comes along, marked [`TaskNode::pulled_in`]. `None` selects every workspace.
pub fn build_execution_graph_selected(
	workspaces: &[Workspace],
	root_tasks: &[&str],
	selected_workspaces: Option<&HashSet<String>>,
) -> Result<ExecutionGraph> {
	for root_task in root_tasks {
		require_somewhere(root_task, None, workspaces)?;
	}

	let task_set = collect_task_set(root_tasks, workspaces);
	for task in &task_set {
		for dep in deps_of(task, workspaces) {
			let bare = dep.strip_prefix('^').unwrap_or(dep);
			require_somewhere(bare, Some((task, dep)), workspaces)?;
		}
	}

	let mut graph: DiGraph<TaskNode, ()> = DiGraph::new();
	let mut node_map: HashMap<(String, String), NodeIndex> = HashMap::new();

	for task_name in &task_set {
		for ws in workspaces {
			let Some(task) = ws.task(task_name) else {
				continue;
			};
			let selected = selected_workspaces.is_none_or(|sel| sel.contains(&ws.name));
			let idx = graph.add_node(TaskNode {
				workspace_name: ws.name.clone(),
				task_name: task_name.clone(),
				command: task.command.clone(),
				is_persistent: task.config.is_persistent(),
				pulled_in: !selected,
			});
			node_map.insert((ws.name.clone(), task_name.clone()), idx);
		}
	}

	for ws in workspaces {
		for task_name in &task_set {
			let Some(task) = ws.task(task_name) else {
				continue;
			};
			let to_idx = node_map[&(ws.name.clone(), task_name.clone())];
			for dep in task.config.depends_on.iter().flatten() {
				if let Some(dep_task) = dep.strip_prefix('^') {
					for dep_ws in &ws.depends_on {
						if let Some(&from_idx) =
							node_map.get(&(dep_ws.clone(), dep_task.to_string()))
						{
							graph.add_edge(from_idx, to_idx, ());
						}
					}
				} else if let Some(&from_idx) = node_map.get(&(ws.name.clone(), dep.clone())) {
					graph.add_edge(from_idx, to_idx, ());
				}
			}
		}
	}

	if selected_workspaces.is_some() {
		graph = dependency_closure(&graph);
	}

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

	let topo_order = toposort(&graph, None).map_err(|cycle| {
		let node = &graph[cycle.node_id()];
		anyhow::anyhow!(
			"the task graph has a cycle through {}:{}. Workspace dependencies come from each \
			 manifest and from `dependsOn` in `overrides`; one of them points back the way it came",
			node.workspace_name,
			node.task_name
		)
	})?;

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
	use lattice_config::PipelineTask;
	use lattice_workspace::WorkspaceTask;

	type Tasks = Vec<(String, PipelineTask)>;

	fn ws(name: &str, deps: &[&str], commands: &[(&str, &str)]) -> Workspace {
		let mut w = Workspace::new(name, name).with_deps(deps);
		for (task, command) in commands {
			w = w.with_task(task, command, PipelineTask::default());
		}
		w
	}

	fn config(tasks: &[(&str, PipelineTask)]) -> Tasks {
		tasks
			.iter()
			.map(|(n, t)| ((*n).to_string(), t.clone()))
			.collect()
	}

	/// Every workspace's copy of each task in `cfg` given `cfg`'s settings, the
	/// way discovery layers the root `tasks` map onto what it finds.
	fn apply(workspaces: &[Workspace], cfg: &Tasks) -> Vec<Workspace> {
		let mut out = workspaces.to_vec();
		for w in &mut out {
			for (name, settings) in cfg {
				if let Some(task) = w.tasks.get_mut(name) {
					task.config = settings.clone();
				}
			}
		}
		out
	}

	fn graph(workspaces: &[Workspace], root: &str, cfg: &Tasks) -> Result<ExecutionGraph> {
		build_execution_graph(&apply(workspaces, cfg), &[root])
	}

	fn graph_multi(
		workspaces: &[Workspace],
		roots: &[&str],
		cfg: &Tasks,
	) -> Result<ExecutionGraph> {
		build_execution_graph(&apply(workspaces, cfg), roots)
	}

	fn graph_selected(
		workspaces: &[Workspace],
		roots: &[&str],
		cfg: &Tasks,
		selected: Option<&HashSet<String>>,
	) -> Result<ExecutionGraph> {
		build_execution_graph_selected(&apply(workspaces, cfg), roots, selected)
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
		let g = graph(&workspaces, "test", &cfg).unwrap();
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
		let g = graph_multi(&workspaces, &["lint", "test", "build"], &cfg).unwrap();
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
		let err = graph_multi(&workspaces, &["build", "nope"], &cfg).unwrap_err();
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
		let g = graph(&workspaces, "build", &cfg).unwrap();
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
		let g = graph_selected(&chain(), &["build"], &cfg, Some(&selection(&["top"]))).unwrap();
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
		let g = graph_selected(&chain(), &["build"], &cfg, Some(&selection(&["mid"]))).unwrap();
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
		let g = graph_selected(&workspaces, &["build"], &cfg, Some(&selection(&["app"]))).unwrap();
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
		let g = graph_selected(&chain(), &["build"], &cfg, Some(&selection(&["base"]))).unwrap();
		let order = dry_run_order(&g);

		assert_eq!(order.len(), 1);
		assert_eq!(order[0].workspace_name, "base");
		assert!(!order[0].pulled_in);
	}

	#[test]
	fn selecting_nothing_yields_an_empty_graph() {
		let cfg = config(&[("build", task(&["^build"]))]);
		let g = graph_selected(&chain(), &["build"], &cfg, Some(&HashSet::new())).unwrap();
		assert!(dry_run_order(&g).is_empty());
	}

	#[test]
	fn same_workspace_deps_survive_a_selection() {
		// test depends on build in its own workspace, and build on ^build.
		let mut workspaces = chain();
		for w in &mut workspaces {
			w.tasks.insert(
				"test".to_string(),
				WorkspaceTask::new("test", PipelineTask::default()),
			);
		}
		let cfg = config(&[("build", task(&["^build"])), ("test", task(&["build"]))]);
		let g = graph_selected(&workspaces, &["test"], &cfg, Some(&selection(&["top"]))).unwrap();
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
	fn a_prerequisite_no_workspace_has_is_named() {
		let workspaces = vec![ws("app", &[], &[("build", "b")])];
		let cfg = config(&[("build", task(&["codegne"]))]);
		let err = graph(&workspaces, "build", &cfg).unwrap_err().to_string();
		assert!(err.contains("task 'build' depends on 'codegne'"), "{err}");
	}

	#[test]
	fn a_task_only_some_workspaces_have_runs_in_those() {
		let workspaces = vec![
			ws("web", &[], &[("lint", "eslint")]),
			ws("api", &[], &[("build", "go build")]),
		];
		let g = graph(&workspaces, "lint", &config(&[])).unwrap();
		let labels: Vec<&str> = dry_run_order(&g)
			.iter()
			.map(|n| n.workspace_name.as_str())
			.collect();
		assert_eq!(labels, vec!["web"]);
	}

	#[test]
	fn cycle_is_rejected() {
		let workspaces = vec![ws("app", &[], &[("a", "x"), ("b", "y")])];
		let cfg = config(&[("a", task(&["b"])), ("b", task(&["a"]))]);
		let err = graph(&workspaces, "a", &cfg).unwrap_err();
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
		let err = graph(&workspaces, "build", &cfg).unwrap_err();
		assert!(format!("{err}").contains("persistent"));
	}

	#[test]
	fn includes_persistent_task_detects_persistent_roots_and_deps() {
		let dev = PipelineTask {
			persistent: Some(true),
			..Default::default()
		};
		let workspaces = apply(
			&[ws(
				"app",
				&[],
				&[("dev", "vite"), ("build", "tsc"), ("start", "node .")],
			)],
			&config(&[("dev", dev), ("start", task(&["dev"]))]),
		);
		assert!(includes_persistent_task(&["dev"], &workspaces));
		assert!(includes_persistent_task(&["start"], &workspaces));
		assert!(!includes_persistent_task(&["build"], &workspaces));
	}

	#[test]
	fn schedule_two_node_chain() {
		// a -> b (b depends on a).
		let workspaces = vec![ws("app", &[], &[("a", "x"), ("b", "y")])];
		let cfg = config(&[("a", PipelineTask::default()), ("b", task(&["a"]))]);
		let g = graph(&workspaces, "b", &cfg).unwrap();
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
		let graph = graph(&workspaces, "build", &cfg).unwrap();

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
		let graph = graph(&workspaces, "build", &cfg).unwrap();

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
		let graph = graph(&workspaces, "build", &cfg).unwrap();

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
		let graph = graph(&workspaces, "build", &cfg).unwrap();

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
		let graph = graph(&workspaces, "dev", &cfg).unwrap();

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
