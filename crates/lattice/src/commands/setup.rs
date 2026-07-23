use anyhow::Result;
use clap::Args;
use console::style;
use tokio::io::{AsyncBufReadExt, BufReader};

use lattice_config::find_root;
use lattice_output::OutputManager;
use lattice_workspace::discover_workspaces;

#[derive(Args, Debug)]
pub struct SetupArgs {
    #[arg(help = "Only set up specific workspaces (by name)")]
    pub workspaces: Vec<String>,

    #[arg(long, help = "Force reinstall even if lockfile hasn't changed")]
    pub force: bool,
}

impl SetupArgs {
    pub async fn execute(&self, loquacious: bool) -> Result<()> {
        let cwd = std::env::current_dir()?;
        let root = find_root(&cwd).ok_or_else(|| {
            anyhow::anyhow!("No lattice.json found in this directory or any parent directory.")
        })?;

        let config = lattice_config::load_config(&root)?;
        let output = OutputManager::new(loquacious);

        let mut workspaces = discover_workspaces(&root, &config)?;

        if !self.workspaces.is_empty() {
            workspaces.retain(|ws| self.workspaces.contains(&ws.name));
        }

        if workspaces.is_empty() {
            output.warn("No workspaces found.");
            return Ok(());
        }

        output.header(&format!(
            "\n{} Lattice setup — {} workspaces\n",
            style("◆").cyan().bold(),
            workspaces.len()
        ));

        let mut any_failed = false;

        for ws in &workspaces {
            let setup_cmd = match ws.language.setup_command(&ws.path) {
                Some(cmd) => cmd,
                None => {
                    output.log_skipped(&ws.name, "setup");
                    continue;
                }
            };

            if !self.force && !lockfile_changed(&ws.path) {
                println!(
                    "{} {} {}",
                    style("●").green().bold(),
                    style(&ws.name).bold(),
                    style("dependencies up to date").dim()
                );
                continue;
            }

            let bin = setup_cmd.split_whitespace().next().unwrap_or("");
            if !bin.is_empty() && bin != "sh" {
                let available = tokio::process::Command::new("which")
                    .arg(bin)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status()
                    .await
                    .map(|s| s.success())
                    .unwrap_or(false);

                if !available {
                    output.warn(&format!(
                        "{}: '{}' not found in PATH — skipping setup",
                        ws.name, bin
                    ));
                    continue;
                }
            }

            output.log_start(&ws.name, "setup");
            output.detail(&format!("running: {}", setup_cmd));

            let start = std::time::Instant::now();
            let result = run_setup_command(&setup_cmd, &ws.path, loquacious).await;
            let duration_ms = start.elapsed().as_millis() as u64;

            match result {
                Ok(true) => {
                    output.log_success(&ws.name, "setup", duration_ms);
                    let _ = touch_marker(&ws.path);
                }
                Ok(false) => {
                    output.log_failure(&ws.name, "setup");
                    any_failed = true;
                }
                Err(e) => {
                    output.log_failure(&ws.name, "setup");
                    eprintln!("  error: {}", e);
                    any_failed = true;
                }
            }
        }

        if any_failed {
            anyhow::bail!("One or more workspaces failed setup.");
        }

        println!();
        println!(
            "{} All workspaces set up successfully.",
            style("◆").cyan().bold()
        );
        Ok(())
    }
}

fn lockfile_changed(workspace_path: &std::path::Path) -> bool {
    let marker = workspace_path.join(".lattice-setup-marker");
    if !marker.exists() {
        return true;
    }

    let lockfiles = [
        "package-lock.json",
        "yarn.lock",
        "pnpm-lock.yaml",
        "bun.lockb",
        "bun.lock",
        "Cargo.lock",
        "go.sum",
        "poetry.lock",
        "uv.lock",
    ];

    let marker_time = std::fs::metadata(&marker).and_then(|m| m.modified()).ok();

    for lf in &lockfiles {
        let lf_path = workspace_path.join(lf);
        if lf_path.exists() {
            if let (Some(marker_t), Ok(lf_meta)) = (marker_time, std::fs::metadata(&lf_path)) {
                if let Ok(lf_t) = lf_meta.modified() {
                    if lf_t > marker_t {
                        return true;
                    }
                }
            }
        }
    }

    false
}

fn touch_marker(workspace_path: &std::path::Path) -> std::io::Result<()> {
    let marker = workspace_path.join(".lattice-setup-marker");
    std::fs::write(marker, "")
}

async fn run_setup_command(command: &str, cwd: &std::path::Path, loquacious: bool) -> Result<bool> {
    let mut child = tokio::process::Command::new("sh")
        .args(["-c", command])
        .current_dir(cwd)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let show = loquacious;

    let stdout_task = tokio::spawn(async move {
        let mut reader = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            if show {
                println!("    {}", line);
            }
        }
    });

    let stderr_task = tokio::spawn(async move {
        let mut reader = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            eprintln!("    {}", line);
        }
    });

    let status = child.wait().await?;
    let _ = stdout_task.await;
    let _ = stderr_task.await;

    Ok(status.success())
}
