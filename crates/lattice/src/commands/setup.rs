use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use clap::Args;
use console::style;
use tokio::io::{AsyncBufReadExt, BufReader};

use lattice_config::{find_root, resolve_engines, LOCKFILES};
use lattice_output::{apply_color_policy, banner_line, make_reporter, paint_teal, ROSETTE};
use lattice_project::scaffold::{LEGACY_SETUP_MARKER, SETUP_MARKER_DIR};
use lattice_workspace::toolchain;
use lattice_workspace::{discover_workspaces, Workspace};

use crate::cli::{detect_output_mode, effective_loquacious, maybe_emit_version_nag};

#[derive(Args, Debug)]
#[command(
	long_about = "Provision pinned toolchains, then install each workspace's dependencies.\n\n\
Lattice provisions the toolchains declared under `engines` first, into .lattice/toolchains, \
so every dependency installer runs with the pinned PATH. Each workspace's package manager \
then installs that workspace's dependencies. A repo that declares no workspaces still gets \
its `engines` provisioned."
)]
pub struct SetupArgs {
	/// Set up only the workspaces named here.
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
				"no lattice.json found in this directory or any parent. \
                 Run `lattice init` to create one"
			)
		})?;

		lattice_config::schema::ensure_schema(&root);
		let config = lattice_config::load_config(&root)?;
		let declared: Vec<&str> = config.workspaces.iter().map(|w| w.name.as_str()).collect();
		require_known_workspaces(&self.workspaces, &declared)?;

		let effective_loq = effective_loquacious(flag_loq, config.settings.loquacious);
		let mode = detect_output_mode(effective_loq);
		apply_color_policy(mode);
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

		for ws in &selected {
			// No driver and no engines → nothing to do, skip quietly.
			let Some(driver) = &ws.driver else {
				if ws.engines.is_empty() {
					continue;
				}
				reporter.note(&format!(
					"{}: toolchains ready. This workspace has no package manager to install",
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
						"{}: no dependency installer known for '{}'. Skipping this workspace",
						ws.name, driver.tool
					));
					continue;
				}
			};

			// Keep the lockfile-mtime "up to date" skip unless --force.
			if !self.force && !lockfile_changed(&root, &ws.path) {
				println!(
					"{} {} {}",
					paint_teal("●"),
					style(&ws.name).bold(),
					style("dependencies up to date").dim()
				);
				continue;
			}

			println!(
				"{} {} {}",
				paint_teal("●"),
				style(&ws.name).bold(),
				style(&install_cmd).dim()
			);

			let prepend = ws_prepend.get(&ws.name).cloned().unwrap_or_default();
			match run_install(&install_cmd, &ws.path, &prepend).await {
				Ok(true) => {
					if let Err(e) = touch_marker(&root, &ws.path) {
						reporter.warn(&format!(
							"{}: dependencies installed, but {} could not be written, so the \
							 next `lattice setup` will install again: {e}",
							ws.name,
							marker_relative(&root, &ws.path)
						));
					}
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
			bail!("setup failed in one or more workspaces");
		}

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
		"pdm" => "pdm install",
		"pipenv" => "pipenv install",
		"pip" => "pip install -r requirements.txt",
		"bundler" => "bundle install",
		"dotnet" => "dotnet restore",
		"nuget" => "nuget restore",
		"pod" => "pod install",
		"swift" => "swift package resolve",
		"composer" => "composer install",
		"mix" => "mix deps.get",
		"dart" => "dart pub get",
		"stack" => "stack build --only-dependencies",
		"cabal" => "cabal build --only-download",
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

/// Fail if any name in `requested` is not one of `declared`, naming the ones that
/// are, the way `run` refuses an undefined task. A name that selects nothing
/// would otherwise install nothing and exit 0, which is what a typo in a CI
/// script looks like.
fn require_known_workspaces(requested: &[String], declared: &[&str]) -> Result<()> {
	for name in requested {
		if declared.contains(&name.as_str()) {
			continue;
		}
		let mut names: Vec<&str> = declared.to_vec();
		names.sort_unstable();
		let listed = if names.is_empty() {
			"lattice.json declares no workspaces".to_string()
		} else {
			format!("Declared workspaces: {}", names.join(", "))
		};
		bail!(
			"workspace '{}' is not declared in the `workspaces` array in lattice.json. {}",
			name,
			listed
		);
	}
	Ok(())
}

/// The directories whose lockfiles govern `workspace_path`: the workspace itself,
/// then every directory above it up to `root`.
///
/// A hoisted layout — one npm, pnpm or yarn workspace tree — keeps its only
/// lockfile at the repo root, so a workspace directory of its own often has none.
fn lockfile_dirs(root: &Path, workspace_path: &Path) -> Vec<PathBuf> {
	let mut dirs = vec![workspace_path.to_path_buf()];
	let mut current = workspace_path;
	while current != root {
		match current.parent() {
			Some(parent) => {
				dirs.push(parent.to_path_buf());
				current = parent;
			}
			None => break,
		}
	}
	dirs
}

/// Where a workspace's install marker lives, relative to the repo root.
///
/// One flat file per workspace under `.lattice/setup`, named for the workspace's
/// path relative to the root with `/` written `%2F` and a literal `%` written
/// `%25`. Encoding the separator rather than replacing it is what keeps
/// `apps/web` and `apps-web` apart — flattening both to `apps-web` is the
/// collision `lattice-workspace`'s scan already had to undo. Keeping the files
/// flat also means no marker can ever be both a file and another marker's parent
/// directory. The repo root as a workspace encodes to nothing, so its marker is
/// the bare `.marker`.
fn marker_relative(root: &Path, workspace_path: &Path) -> String {
	let relative = workspace_path.strip_prefix(root).unwrap_or(workspace_path);
	let encoded: Vec<String> = relative
		.components()
		.map(|c| c.as_os_str().to_string_lossy().replace('%', "%25"))
		.collect();
	format!("{SETUP_MARKER_DIR}/{}.marker", encoded.join("%2F"))
}

fn marker_path(root: &Path, workspace_path: &Path) -> PathBuf {
	root.join(marker_relative(root, workspace_path))
}

/// When this workspace's dependencies were last installed, preferring the marker
/// under `.lattice` and falling back to one an older version left in the
/// workspace itself.
fn marker_time(root: &Path, workspace_path: &Path) -> Option<std::time::SystemTime> {
	let modified = |path: PathBuf| std::fs::metadata(path).and_then(|m| m.modified()).ok();
	modified(marker_path(root, workspace_path))
		.or_else(|| modified(workspace_path.join(LEGACY_SETUP_MARKER)))
}

fn lockfile_changed(root: &Path, workspace_path: &Path) -> bool {
	let Some(marker_time) = marker_time(root, workspace_path) else {
		return true;
	};
	for dir in lockfile_dirs(root, workspace_path) {
		for lf in LOCKFILES {
			let modified = std::fs::metadata(dir.join(lf)).and_then(|m| m.modified());
			if matches!(modified, Ok(lf_time) if lf_time > marker_time) {
				return true;
			}
		}
	}
	false
}

fn touch_marker(root: &Path, workspace_path: &Path) -> std::io::Result<()> {
	let marker = marker_path(root, workspace_path);
	if let Some(parent) = marker.parent() {
		std::fs::create_dir_all(parent)?;
	}
	std::fs::write(&marker, "")?;
	// The workspace now has a marker under `.lattice`, so the one an older
	// version left in the source tree is stale as well as in the way.
	std::fs::remove_file(workspace_path.join(LEGACY_SETUP_MARKER)).ok();
	Ok(())
}

/// `PATH` with the pinned toolchain directories in front of the inherited value.
///
/// Errors rather than falling back to the inherited `PATH`: an installer run
/// without the pinned toolchain runs against whatever version of the tool the
/// machine happens to have, which is the outcome pinning exists to prevent.
fn path_with_prepend(path_prepend: &[PathBuf]) -> Result<std::ffi::OsString> {
	let existing = std::env::var_os("PATH").unwrap_or_default();
	let mut paths: Vec<PathBuf> = path_prepend.to_vec();
	paths.extend(std::env::split_paths(&existing));
	std::env::join_paths(paths).with_context(|| {
		let shown: Vec<String> = path_prepend
			.iter()
			.map(|p| p.display().to_string())
			.collect();
		format!(
			"the pinned toolchain cannot be put on PATH, because a directory in it \
			 contains the character PATH is split on: {}",
			shown.join(", ")
		)
	})
}

/// Run an install command via the platform shell in `cwd`, prepending
/// `path_prepend` to `PATH` (like the runner does). Both of the installer's
/// output streams are shown, and it is given no stdin.
async fn run_install(command: &str, cwd: &Path, path_prepend: &[PathBuf]) -> Result<bool> {
	// `/S /C` with a raw argument, not `.arg(command)`: Rust escapes an embedded
	// quote the way the MSVC runtime parses it, and `cmd` does not read `\"` as an
	// escape, so an install command containing a quote would arrive mangled.
	#[cfg(windows)]
	let mut cmd = {
		use std::os::windows::process::CommandExt;
		let mut c = tokio::process::Command::new("cmd");
		c.as_std_mut().raw_arg(format!("/S /C \"{command}\""));
		c
	};
	#[cfg(not(windows))]
	let mut cmd = {
		let mut c = tokio::process::Command::new("sh");
		c.arg("-c").arg(command);
		c
	};
	cmd.current_dir(cwd);

	if !path_prepend.is_empty() {
		cmd.env("PATH", path_with_prepend(path_prepend)?);
	}

	// No stdin: a dependency install is not an interactive session, and one that
	// asks for a password would otherwise wait on a terminal nobody is watching
	// until the run is killed. Without it the installer gets EOF and fails, and
	// its own output says what it wanted.
	cmd.stdin(std::process::Stdio::null())
		.stdout(std::process::Stdio::piped())
		.stderr(std::process::Stdio::piped());

	let mut child = cmd.spawn()?;
	let stdout = child.stdout.take().unwrap();
	let stderr = child.stderr.take().unwrap();

	let stdout_task = tokio::spawn(async move {
		let mut reader = BufReader::new(stdout).lines();
		while let Ok(Some(line)) = reader.next_line().await {
			println!("    {line}");
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
	use std::time::Duration;

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
	fn every_package_manager_and_build_tool_driver_has_an_installer() {
		// A driver that resolves dependencies must know how; only pure runtimes
		// and task runners (which sit above one) have no install step.
		for spec in lattice_workspace::DriverRegistry::known() {
			let installs = spec.roles.iter().any(|r| {
				matches!(
					r,
					lattice_workspace::Role::PackageManager | lattice_workspace::Role::BuildTool
				)
			});
			assert_eq!(
				install_command_for(spec.tool, false).is_some(),
				installs,
				"install command for '{}' does not match its role",
				spec.tool
			);
		}
	}

	#[test]
	fn unknown_tool_has_no_installer() {
		assert_eq!(install_command_for("rake", false), None);
		assert_eq!(install_command_for("alpes", false), None);
		assert_eq!(install_command_for("node", false), None);
	}

	#[test]
	fn an_undeclared_workspace_name_is_refused() {
		let declared = ["web", "api"];
		assert!(require_known_workspaces(&[], &declared).is_ok());
		assert!(require_known_workspaces(&["web".to_string()], &declared).is_ok());

		let err = require_known_workspaces(&["wbe".to_string()], &declared)
			.unwrap_err()
			.to_string();
		assert!(err.contains("'wbe'"), "{err}");
		assert!(
			err.contains("api, web"),
			"the declared names belong in it: {err}"
		);

		let err = require_known_workspaces(&["web".to_string()], &[])
			.unwrap_err()
			.to_string();
		assert!(err.contains("declares no workspaces"), "{err}");
	}

	#[test]
	fn lockfile_dirs_run_from_the_workspace_up_to_the_root() {
		let root = Path::new("/repo");
		assert_eq!(
			lockfile_dirs(root, &root.join("apps").join("web")),
			vec![
				PathBuf::from("/repo/apps/web"),
				PathBuf::from("/repo/apps"),
				PathBuf::from("/repo"),
			]
		);
		assert_eq!(lockfile_dirs(root, root), vec![PathBuf::from("/repo")]);
	}

	/// The everyday npm/pnpm/yarn layout: one lockfile, at the repo root, and a
	/// workspace directory that holds only a manifest. A `git pull` that changes
	/// that lockfile has to reinstall, or the next build fails on a dependency
	/// that was never installed.
	#[test]
	fn a_lockfile_hoisted_above_the_workspace_invalidates_the_marker() {
		let dir = tempfile::tempdir().unwrap();
		let root = dir.path();
		let ws = root.join("apps").join("web");
		std::fs::create_dir_all(&ws).unwrap();
		std::fs::write(ws.join("package.json"), "{}").unwrap();
		let lockfile = root.join("package-lock.json");
		std::fs::write(&lockfile, "{}").unwrap();

		assert!(lockfile_changed(root, &ws), "there is no marker yet");

		touch_marker(root, &ws).unwrap();
		assert!(!lockfile_changed(root, &ws), "nothing has moved since");

		set_mtime(
			&lockfile,
			std::time::SystemTime::now() + Duration::from_secs(60),
		);
		assert!(lockfile_changed(root, &ws));
	}

	#[test]
	fn a_workspaces_own_lockfile_still_invalidates_the_marker() {
		let dir = tempfile::tempdir().unwrap();
		let root = dir.path();
		let ws = root.join("services").join("api");
		std::fs::create_dir_all(&ws).unwrap();
		let lockfile = ws.join("Cargo.lock");
		std::fs::write(&lockfile, "").unwrap();

		touch_marker(root, &ws).unwrap();
		assert!(!lockfile_changed(root, &ws));

		set_mtime(
			&lockfile,
			std::time::SystemTime::now() + Duration::from_secs(60),
		);
		assert!(lockfile_changed(root, &ws));
	}

	#[test]
	fn the_marker_lives_under_lattice_and_not_in_the_workspace() {
		let dir = tempfile::tempdir().unwrap();
		let root = dir.path();
		let ws = root.join("apps").join("web");
		std::fs::create_dir_all(&ws).unwrap();

		touch_marker(root, &ws).unwrap();

		assert_eq!(
			marker_relative(root, &ws),
			".lattice/setup/apps%2Fweb.marker"
		);
		assert!(root.join(".lattice/setup/apps%2Fweb.marker").is_file());
		assert!(
			!ws.join(LEGACY_SETUP_MARKER).exists(),
			"nothing machine-local belongs in the workspace directory"
		);
	}

	/// The collision `lattice-workspace`'s scan already had to undo: two distinct
	/// paths that flatten to one string. Sharing a marker would mean installing
	/// one workspace marks the other up to date.
	#[test]
	fn paths_that_would_flatten_together_get_distinct_markers() {
		let root = Path::new("/repo");
		let names: Vec<String> = ["apps/web", "apps-web", "a/b-c", "a-b/c", "a%2Fb"]
			.iter()
			.map(|p| marker_relative(root, &root.join(p)))
			.collect();

		let unique: std::collections::HashSet<&String> = names.iter().collect();
		assert_eq!(unique.len(), names.len(), "duplicate marker in {names:?}");
		assert_eq!(names[3], ".lattice/setup/a-b%2Fc.marker");
		assert_eq!(names[4], ".lattice/setup/a%252Fb.marker");
	}

	#[test]
	fn the_root_as_a_workspace_has_a_marker_of_its_own() {
		let root = Path::new("/repo");
		assert_eq!(marker_relative(root, root), ".lattice/setup/.marker");
	}

	/// Upgrading mid-project must not reinstall every workspace, so the marker an
	/// older version left in the workspace still counts.
	#[test]
	fn a_marker_at_the_old_in_workspace_path_is_still_honoured() {
		let dir = tempfile::tempdir().unwrap();
		let root = dir.path();
		let ws = root.join("apps").join("web");
		std::fs::create_dir_all(&ws).unwrap();
		std::fs::write(root.join("package-lock.json"), "{}").unwrap();

		assert!(
			lockfile_changed(root, &ws),
			"there is no marker anywhere yet"
		);

		std::fs::write(ws.join(LEGACY_SETUP_MARKER), "").unwrap();
		assert!(!lockfile_changed(root, &ws));

		// And it is still only a marker: a lockfile newer than it reinstalls.
		set_mtime(
			&root.join("package-lock.json"),
			std::time::SystemTime::now() + Duration::from_secs(60),
		);
		assert!(lockfile_changed(root, &ws));
	}

	#[test]
	fn installing_clears_the_old_in_workspace_marker() {
		let dir = tempfile::tempdir().unwrap();
		let root = dir.path();
		let ws = root.join("apps").join("web");
		std::fs::create_dir_all(&ws).unwrap();
		std::fs::write(ws.join(LEGACY_SETUP_MARKER), "").unwrap();

		touch_marker(root, &ws).unwrap();

		assert!(!ws.join(LEGACY_SETUP_MARKER).exists());
		assert!(root.join(".lattice/setup/apps%2Fweb.marker").is_file());
	}

	/// A repo path or `PATH` entry can contain the separator, and joining then
	/// fails. Falling back to the inherited `PATH` would run the installer against
	/// whatever version of the tool is on the machine.
	#[test]
	fn a_toolchain_dir_that_cannot_go_on_path_is_an_error() {
		let separator = if cfg!(windows) { ';' } else { ':' };
		let unusable = PathBuf::from(format!("toolchains{separator}bin"));
		assert!(path_with_prepend(&[unusable]).is_err());

		let usable = PathBuf::from("toolchains");
		let joined = path_with_prepend(std::slice::from_ref(&usable)).unwrap();
		assert_eq!(std::env::split_paths(&joined).next(), Some(usable));
	}

	fn set_mtime(path: &Path, when: std::time::SystemTime) {
		std::fs::File::options()
			.write(true)
			.open(path)
			.unwrap()
			.set_modified(when)
			.unwrap();
	}
}
