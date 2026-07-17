use anyhow::{bail, Result};
use petgraph::algo::toposort;
use petgraph::graph::{DiGraph, NodeIndex};
use std::collections::HashMap;

use lattice_config::LatticeConfig;
use lattice_workspace::Workspace;

#[derive(Debug, Clone)]
pub struct TaskNode {
    pub workspace_name: String,
    pub task_name: String,
    pub command: String,
    pub is_persistent: bool,
}

pub struct ExecutionGraph {
    pub graph: DiGraph<TaskNode, ()>,
    pub topo_order: Vec<NodeIndex>,
}

fn collect_task_set(root_task: &str, config: &LatticeConfig) -> Vec<String> {
    let mut visited = std::collections::HashSet::new();
    let mut ordered = Vec::new();
    let mut stack = vec![root_task.to_string()];

    while let Some(task) = stack.pop() {
        if visited.contains(&task) {
            continue;
        }
        visited.insert(task.clone());

        if let Some(pipeline_task) = config.pipeline.get(&task) {
            if let Some(deps) = &pipeline_task.depends_on {
                for dep in deps {
                    let dep_task = if dep.starts_with('^') {
                        dep[1..].to_string()
                    } else {
                        dep.clone()
                    };
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

pub fn build_execution_graph(
    workspaces: &[Workspace],
    root_task: &str,
    config: &LatticeConfig,
) -> Result<ExecutionGraph> {
    if !config.pipeline.contains_key(root_task) {
        bail!(
            "Task '{}' is not defined in the pipeline section of lattice.json",
            root_task
        );
    }

    let task_set = collect_task_set(root_task, config);
    let mut graph: DiGraph<TaskNode, ()> = DiGraph::new();
    let mut node_map: HashMap<(String, String), NodeIndex> = HashMap::new();

    let mut ws_deps: HashMap<String, Vec<String>> = HashMap::new();
    if let Some(projects) = &config.projects {
        for (ws_name, proj) in projects {
            if let Some(deps) = &proj.depends_on {
                ws_deps.insert(ws_name.clone(), deps.clone());
            }
        }
    }

    for task_name in &task_set {
        let pipeline_task = config.pipeline.get(task_name.as_str()).cloned().unwrap_or_default();
        let is_persistent = pipeline_task.persistent.unwrap_or(false);

        for ws in workspaces {
            let command = match ws.tasks.get(task_name) {
                Some(cmd) => cmd.clone(),
                None => {
                    if ws.language == lattice_workspace::Language::Unknown {
                        bail!(
                            "No command defined for task '{}' in workspace '{}'. \
                            Add a command under 'projects.{}.tasks.{}' in lattice.json, \
                            or add a recognized language manifest (package.json, Cargo.toml, etc.) to the workspace.",
                            task_name, ws.name, ws.name, task_name
                        );
                    }
                    continue;
                }
            };

            let node = TaskNode {
                workspace_name: ws.name.clone(),
                task_name: task_name.clone(),
                command,
                is_persistent,
            };

            let idx = graph.add_node(node);
            node_map.insert((ws.name.clone(), task_name.clone()), idx);
        }
    }

    for task_name in &task_set {
        let pipeline_task = config.pipeline.get(task_name.as_str()).cloned().unwrap_or_default();

        if let Some(depends_on) = &pipeline_task.depends_on {
            for dep in depends_on {
                if dep.starts_with('^') {
                    let dep_task = &dep[1..];
                    for ws in workspaces {
                        if let Some(&to_idx) = node_map.get(&(ws.name.clone(), task_name.clone())) {
                            let deps = ws_deps.get(&ws.name).cloned().unwrap_or_default();
                            for dep_ws_name in &deps {
                                if let Some(&from_idx) = node_map.get(&(dep_ws_name.clone(), dep_task.to_string())) {
                                    graph.add_edge(from_idx, to_idx, ());
                                }
                            }
                        }
                    }
                } else {
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

    for (key, &idx) in &node_map {
        let node = &graph[idx];
        if node.is_persistent {
            let outgoing: Vec<_> = graph
                .neighbors_directed(idx, petgraph::Direction::Outgoing)
                .collect();
            if !outgoing.is_empty() {
                bail!(
                    "Persistent task '{}' in workspace '{}' cannot be depended on by other tasks.",
                    key.1,
                    key.0
                );
            }
        }
    }

    let topo_order = toposort(&graph, None)
        .map_err(|_| anyhow::anyhow!("Cycle detected in task dependency graph"))?;

    Ok(ExecutionGraph { graph, topo_order })
}
