use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Result};
use clap::Args;
use console::style;
use tokio::io::{AsyncBufReadExt, BufReader};

use lattice_config::{find_root, resolve_engines};
use lattice_output::{banner_line, make_reporter, paint_teal, ROSETTE};
use lattice_workspace::toolchain;
use lattice_workspace::{discover_workspaces, Workspace};

use crate::cli::{detect_output_mode, effective_loquacious, maybe_emit_version_nag};

#[derive(Args, Debug)]
#[command(
	long_about = "Provision pinned toolchains and install native dependencies.\n\n\
Toolchains declared under `engines` are provisioned first (into .lattice/toolchains), so \
dependency installers see the pinned PATH. Then each workspace's package manager installs \
its dependencies. A repo with no workspaces still has its `engines` provisioned."
)]
pub struct SetupArgs {
	/// Only set up specific workspaces (by name).
	pub workspaces: Vec<String>,

	/// Reinstall dependencies even if the lockfile has not changed.
	#[arg(long)]
	pub force: bool,
}

impl SetupArgs {
	pub async fn execute(&self, flag_loq: bool, no_version_check: bool) -> Result<()> {
		let cwd = std::env::current_dir()?;
		let root = find_root(&cwd).ok_or_else(|| {
			anyhow::anyhow!(
				"no lattice.json found in this directory or any parent; \
                 run `lattice init` to create one"
			)
		})?;

		crate::schema::ensure_schema(&root);
		let config = lattice_config::load_config(&root)?;
		let effective_loq = effective_loquacious(flag_loq, config.settings.loquacious);
		let mode = detect_output_mode(effective_loq);
		let reporter = make_reporter(mode, effective_loq);

		maybe_emit_version_nag(mode, &config, no_version_check);

		println!("{}", banner_line("setup"));

		// Root engines, so dependency installers see the pinned PATH. Memoize
		// by the merged engine spec so identical toolchains install once.
		let mut memo: HashMap<String, Vec<PathBuf>> = HashMap::new();
		reporter.note("provisioning root toolchains");
		let root_resolved =
			toolchain::provision_and_resolve(&root, &config.engines, &mut |m| reporter.note(m))?;
		memo.insert(format!("{:?}", config.engines), root_resolved.path_prepend);

		let workspaces = discover_workspaces(&root, &config)?;
		let selected: Vec<&Workspace> = if self.workspaces.is_empty() {
			workspaces.iter().collect()
		} else {
			workspaces
				.iter()
				.filter(|w| self.workspaces.contains(&w.name))
				.collect()
		};

		let mut ws_prepend: HashMap<String, Vec<PathBuf>> = HashMap::new();
		for ws in &selected {
			let merged = resolve_engines(&config.engines, &ws.engines);
			let key = format!("{merged:?}");
			let pp = if let Some(hit) = memo.get(&key) {
				hit.clone()
			} else {
				reporter.note(&format!("provisioning toolchains for '{}'", ws.name));
				let resolved =
					toolchain::provision_and_resolve(&root, &merged, &mut |m| reporter.note(m))?;
				memo.insert(key, resolved.path_prepend.clone());
				resolved.path_prepend
			};
			ws_prepend.insert(ws.name.clone(), pp);
		}

		let mut any_failed = false;
		let mut installed_any = false;

		for ws in &selected {
			// No driver and no engines → nothing to do, skip quietly.
			let Some(driver) = &ws.driver else {
				if ws.engines.is_empty() {
					continue;
				}
				reporter.note(&format!(
					"{}: toolchains ready (no package manager to install)",
					ws.name
				));
				continue;
			};

			let has_wrapper = match driver.tool.as_str() {
				"gradle" => ws.path.join("gradlew").exists(),
				"maven" => ws.path.join("mvnw").exists(),
				_ => false,
			};

			let install_cmd = match install_command_for(&driver.tool, has_wrapper) {
				Some(cmd) => cmd,
				None => {
					reporter.note(&format!(
						"{}: no known dependency installer for '{}'; skipping",
						ws.name, driver.tool
					));
					continue;
				}
			};

			// Keep the lockfile-mtime "up to date" skip unless --force.
			if !self.force && !lockfile_changed(&ws.path) {
				println!(
					"{} {} {}",
					paint_teal("●"),
					style(&ws.name).bold(),
					style("dependencies up to date").dim()
				);
				continue;
			}

			installed_any = true;
			println!(
				"{} {} {}",
				paint_teal("●"),
				style(&ws.name).bold(),
				style(&install_cmd).dim()
			);

			let prepend = ws_prepend.get(&ws.name).cloned().unwrap_or_default();
			match run_install(&install_cmd, &ws.path, &prepend, effective_loq).await {
				Ok(true) => {
					let _ = touch_marker(&ws.path);
				}
				Ok(false) => {
					reporter.warn(&format!("{}: `{}` failed", ws.name, install_cmd));
					any_failed = true;
				}
				Err(e) => {
					reporter.warn(&format!("{}: {}", ws.name, e));
					any_failed = true;
				}
			}
		}

		if any_failed {
			bail!("one or more workspaces failed setup");
		}

		let _ = installed_any;
		println!("{} {}", paint_teal(ROSETTE), style("setup complete").bold());
		Ok(())
	}
}

/// Map a detected driver tool to its native dependency-install command.
/// `has_wrapper` selects the wrapper form for gradle/maven.
pub fn install_command_for(tool: &str, has_wrapper: bool) -> Option<String> {
	let cmd = match tool {
		"pnpm" => "pnpm install",
		"yarn" => "yarn install",
		"npm" => "npm install",
		"bun" => "bun install",
		"deno" => "deno cache .",
		"cargo" => "cargo fetch",
		"go" => "go mod download",
		"poetry" => "poetry install",
		"uv" => "uv sync",
		"bundler" => "bundle install",
		"pip" => "pip install -r requirements.txt",
		"gradle" => {
			return Some(
				if has_wrapper {
					"./gradlew dependencies"
				} else {
					"gradle dependencies"
				}
				.to_string(),
			)
		}
		"maven" => {
			return Some(
				if has_wrapper {
					"./mvnw dependency:resolve"
				} else {
					"mvn dependency:resolve"
				}
				.to_string(),
			)
		}
		_ => return None,
	};
	Some(cmd.to_string())
}

/// Lockfiles whose mtime is compared against the setup marker to decide whether
/// dependencies need reinstalling.
const LOCKFILES: &[&str] = &[
	"package-lock.json",
	"yarn.lock",
	"pnpm-lock.yaml",
	"bun.lockb",
	"bun.lock",
	"Cargo.lock",
	"go.sum",
	"poetry.lock",
	"uv.lock",
	"Gemfile.lock",
];

fn lockfile_changed(workspace_path: &Path) -> bool {
	let marker = workspace_path.join(".lattice-setup-marker");
	if !marker.exists() {
		return true;
	}
	let marker_time = std::fs::metadata(&marker).and_then(|m| m.modified()).ok();
	for lf in LOCKFILES {
		let lf_path = workspace_path.join(lf);
		if let (Some(marker_t), Ok(lf_meta)) = (marker_time, std::fs::metadata(&lf_path)) {
			if let Ok(lf_t) = lf_meta.modified() {
				if lf_t > marker_t {
					return true;
				}
			}
		}
	}
	false
}

fn touch_marker(workspace_path: &Path) -> std::io::Result<()> {
	std::fs::write(workspace_path.join(".lattice-setup-marker"), "")
}

/// Run an install command via the platform shell in `cwd`, prepending
/// `path_prepend` to `PATH` (like the runner does). Streams output only in
/// loquacious mode; stderr is always shown.
async fn run_install(
	command: &str,
	cwd: &Path,
	path_prepend: &[PathBuf],
	loquacious: bool,
) -> Result<bool> {
	let mut cmd = if cfg!(windows) {
		let mut c = tokio::process::Command::new("cmd");
		c.arg("/C").arg(command);
		c
	} else {
		let mut c = tokio::process::Command::new("sh");
		c.arg("-c").arg(command);
		c
	};
	cmd.current_dir(cwd);

	if !path_prepend.is_empty() {
		let existing = std::env::var_os("PATH").unwrap_or_default();
		let mut paths: Vec<PathBuf> = path_prepend.to_vec();
		paths.extend(std::env::split_paths(&existing));
		if let Ok(joined) = std::env::join_paths(paths) {
			cmd.env("PATH", joined);
		}
	}

	cmd.stdout(std::process::Stdio::piped())
		.stderr(std::process::Stdio::piped());

	let mut child = cmd.spawn()?;
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

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn install_command_mapping_covers_known_tools() {
		assert_eq!(
			install_command_for("pnpm", false).as_deref(),
			Some("pnpm install")
		);
		assert_eq!(
			install_command_for("yarn", false).as_deref(),
			Some("yarn install")
		);
		assert_eq!(
			install_command_for("npm", false).as_deref(),
			Some("npm install")
		);
		assert_eq!(
			install_command_for("bun", false).as_deref(),
			Some("bun install")
		);
		assert_eq!(
			install_command_for("deno", false).as_deref(),
			Some("deno cache .")
		);
		assert_eq!(
			install_command_for("cargo", false).as_deref(),
			Some("cargo fetch")
		);
		assert_eq!(
			install_command_for("go", false).as_deref(),
			Some("go mod download")
		);
		assert_eq!(
			install_command_for("poetry", false).as_deref(),
			Some("poetry install")
		);
		assert_eq!(install_command_for("uv", false).as_deref(), Some("uv sync"));
		assert_eq!(
			install_command_for("bundler", false).as_deref(),
			Some("bundle install")
		);
		assert_eq!(
			install_command_for("pip", false).as_deref(),
			Some("pip install -r requirements.txt")
		);
	}

	#[test]
	fn gradle_and_maven_pick_wrapper_form() {
		assert_eq!(
			install_command_for("gradle", true).as_deref(),
			Some("./gradlew dependencies")
		);
		assert_eq!(
			install_command_for("gradle", false).as_deref(),
			Some("gradle dependencies")
		);
		assert_eq!(
			install_command_for("maven", true).as_deref(),
			Some("./mvnw dependency:resolve")
		);
		assert_eq!(
			install_command_for("maven", false).as_deref(),
			Some("mvn dependency:resolve")
		);
	}

	#[test]
	fn unknown_tool_has_no_installer() {
		assert_eq!(install_command_for("rake", false), None);
		assert_eq!(install_command_for("pod", false), None);
		assert_eq!(install_command_for("node", false), None);
	}
}
