use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;
use tokio::io::{AsyncBufReadExt, BufReader};

use lattice_cache::{CacheManager, CacheMeta};
use lattice_config::LatticeConfig;
use lattice_dag::ExecutionGraph;
use lattice_output::OutputManager;
use lattice_workspace::Workspace;

pub struct RunResult {
    pub total: usize,
    pub cached: usize,
    pub failed: usize,
    pub elapsed_ms: u64,
}

pub async fn execute_tasks(
    graph: &ExecutionGraph,
    workspaces: &[Workspace],
    config: &LatticeConfig,
    root: &Path,
    no_cache: bool,
    output: &OutputManager,
) -> Result<RunResult> {
    let ws_map: HashMap<String, &Workspace> =
        workspaces.iter().map(|w| (w.name.clone(), w)).collect();

    let cache_manager = CacheManager::new(root);
    let mut total = 0usize;
    let mut cached = 0usize;
    let global_start = std::time::Instant::now();
    #[allow(unused_assignments)]
    let mut _failed = 0usize;
    let order = graph.topo_order.clone();

    for &node_idx in &order {
        let node = &graph.graph[node_idx];
        let ws = match ws_map.get(&node.workspace_name) {
            Some(w) => *w,
            None => continue,
        };

        let task_name = &node.task_name;
        let pipeline_task = config
            .pipeline
            .get(task_name.as_str())
            .cloned()
            .unwrap_or_default();

        total += 1;

        let hash = cache_manager.compute_hash(&ws.path, task_name, &node.command, &pipeline_task)?;

        output.detail(&format!("hash: {}", hash));

        if !no_cache && !node.is_persistent && cache_manager.is_cached(&hash) {
            output.log_cache_hit(&node.workspace_name, task_name, &hash);
            if let Some(outputs) = &pipeline_task.outputs {
                if !outputs.is_empty() {
                    if let Err(e) = cache_manager.restore_outputs(&hash, &ws.path) {
                        output.warn(&format!("Failed to restore cached outputs: {}", e));
                    }
                }
            }
            cached += 1;
            continue;
        }

        output.log_start(&node.workspace_name, task_name);
        output.detail(&format!("running: {}", node.command));

        let start = std::time::Instant::now();
        let result = run_command(&node.command, &ws.path, output).await;
        let duration_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok(true) => {
                output.log_success(&node.workspace_name, task_name, duration_ms);

                if !node.is_persistent && !no_cache {
                    let env: HashMap<String, String> = pipeline_task
                        .env
                        .as_deref()
                        .unwrap_or(&[])
                        .iter()
                        .filter_map(|var| std::env::var(var).ok().map(|val| (var.clone(), val)))
                        .collect();

                    let meta = CacheMeta {
                        hash: hash.clone(),
                        task: task_name.clone(),
                        workspace: node.workspace_name.clone(),
                        duration_ms,
                        last_used: chrono::Utc::now(),
                        env,
                    };

                    if let Err(e) = cache_manager.store_meta(&meta) {
                        output.warn(&format!("Failed to write cache metadata: {}", e));
                    }

                    if let Some(outputs) = &pipeline_task.outputs {
                        if !outputs.is_empty() {
                            if let Err(e) =
                                cache_manager.store_outputs(&hash, &ws.path, outputs)
                            {
                                output.warn(&format!("Failed to cache outputs: {}", e));
                            }
                        }
                    }
                }
            }
            Ok(false) => {
                output.log_failure(&node.workspace_name, task_name);
                return Err(anyhow::anyhow!(
                    "Task '{}:{}' failed. Stopping pipeline.",
                    node.workspace_name,
                    task_name
                ));
            }
            Err(e) => {
                output.log_failure(&node.workspace_name, task_name);
                return Err(e);
            }
        }
    }

    let elapsed_ms = global_start.elapsed().as_millis() as u64;
    Ok(RunResult {
        total,
        cached,
        failed: 0,
        elapsed_ms,
    })
}

async fn run_command(command: &str, cwd: &Path, output: &OutputManager) -> Result<bool> {
    let mut parts = shlex::split(command).unwrap_or_else(|| {
        vec![
            "sh".to_string(),
            "-c".to_string(),
            command.to_string(),
        ]
    });

    if parts.is_empty() {
        return Ok(true);
    }

    let (program, args) = if command.contains("&&")
        || command.contains("||")
        || command.contains("|")
        || command.contains(">")
        || command.contains("<")
        || command.starts_with("./")
    {
        ("sh".to_string(), vec!["-c".to_string(), command.to_string()])
    } else {
        let program = parts.remove(0);
        (program, parts)
    };

    let mut child = tokio::process::Command::new(&program)
        .args(&args)
        .current_dir(cwd)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                anyhow::anyhow!(
                    "Toolchain '{}' not found. Make sure it is installed and available in PATH.",
                    program
                )
            } else {
                anyhow::Error::from(e)
            }
        })?;

    let stdout = child.stdout.take().expect("stdout piped");
    let stderr = child.stderr.take().expect("stderr piped");

    let loquacious = output.loquacious;
    let is_ci = output.is_ci();
    let show_output = loquacious || is_ci;

    let stdout_handle = tokio::spawn(async move {
        let mut reader = BufReader::new(stdout).lines();
        let mut lines = Vec::new();
        while let Ok(Some(line)) = reader.next_line().await {
            if show_output {
                println!("    {}", line);
            }
            lines.push(line);
        }
        lines
    });

    let stderr_handle = tokio::spawn(async move {
        let mut reader = BufReader::new(stderr).lines();
        let mut lines = Vec::new();
        while let Ok(Some(line)) = reader.next_line().await {
            if show_output {
                eprintln!("    {}", line);
            }
            lines.push(line);
        }
        lines
    });

    let status = child.wait().await?;
    let _ = stdout_handle.await;
    let _ = stderr_handle.await;

    Ok(status.success())
}
