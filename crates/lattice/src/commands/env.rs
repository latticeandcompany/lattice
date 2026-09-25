use std::path::PathBuf;

use anyhow::{bail, Result};
use clap::{Args, ValueEnum};

use lattice_project::Project;
use lattice_workspace::{dependency_bin_dirs, toolchain};

#[derive(Args, Debug)]
#[command(long_about = "Print the PATH a task sees, for your own shell.\n\n\
Lattice puts each pinned tool, and the tools a workspace's package manager installed, \
on the PATH of every task it runs. `lattice env` prints that PATH as a command your \
shell can evaluate, so `node`, `java` or `cargo` in your terminal are the versions the \
repo pins. Run inside a workspace for that workspace's PATH.\n\n\
Examples:\n  \
eval \"$(lattice env)\"\n  \
lattice env --shell fish | source\n  \
lattice env --workspace api")]
pub struct EnvArgs {
	/// The workspace whose PATH to print. Defaults to the one you are in.
	#[arg(short, long, value_name = "NAME")]
	pub workspace: Option<String>,

	/// The shell to print for. Defaults to the one $SHELL names.
	#[arg(long, value_name = "SHELL")]
	pub shell: Option<Shell>,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shell {
	Posix,
	Fish,
	Powershell,
	Cmd,
}

impl EnvArgs {
	pub async fn execute(&self) -> Result<()> {
		let cwd = std::env::current_dir()?;
		let project = Project::open(&cwd)?;
		let chosen = self.workspace.clone().or(project.cwd_workspace.clone());

		let (engines, ws_path) = match &chosen {
			Some(name) => {
				let Some(ws) = project.workspaces.iter().find(|w| &w.name == name) else {
					let names: Vec<&str> =
						project.workspaces.iter().map(|w| w.name.as_str()).collect();
					bail!(
						"there is no workspace named '{name}'. Workspaces: {}",
						names.join(", ")
					);
				};
				(ws.engines.clone(), Some(ws.path.clone()))
			}
			None => (project.config.engines.clone(), None),
		};

		let resolved = toolchain::provision_and_resolve_in(
			&project.root,
			&engines,
			ws_path.as_deref(),
			&mut |m| eprintln!("{m}"),
		)?;
		let mut dirs: Vec<PathBuf> = resolved.path_prepend;
		if let Some(path) = &ws_path {
			dirs.extend(dependency_bin_dirs(&project.root, path));
		}

		if dirs.is_empty() {
			eprintln!(
				"lattice: nothing to add to PATH. No engine here has an installCmd, and no \
				 package manager has installed tools for this workspace yet"
			);
			return Ok(());
		}
		let shell = self.shell.unwrap_or_else(detect_shell);
		print!("{}", render(shell, &dirs));
		Ok(())
	}
}

fn detect_shell() -> Shell {
	if cfg!(windows) && std::env::var_os("PSModulePath").is_some() {
		return Shell::Powershell;
	}
	match std::env::var("SHELL") {
		Ok(shell) if shell.ends_with("fish") => Shell::Fish,
		_ if cfg!(windows) => Shell::Cmd,
		_ => Shell::Posix,
	}
}

fn render(shell: Shell, dirs: &[PathBuf]) -> String {
	if dirs.is_empty() {
		return String::new();
	}
	let shown: Vec<String> = dirs.iter().map(|d| d.display().to_string()).collect();
	match shell {
		Shell::Posix => format!(
			"export PATH=\"{}:$PATH\"\n",
			shown
				.iter()
				.map(|d| d.replace('"', "\\\""))
				.collect::<Vec<_>>()
				.join(":")
		),
		Shell::Fish => format!(
			"set -gx PATH {} $PATH\n",
			shown
				.iter()
				.map(|d| format!("'{}'", d.replace('\'', "\\'")))
				.collect::<Vec<_>>()
				.join(" ")
		),
		Shell::Powershell => format!(
			"$env:PATH = \"{};$env:PATH\"\n",
			shown.join(";").replace('"', "`\"")
		),
		Shell::Cmd => format!("set \"PATH={};%PATH%\"\n", shown.join(";")),
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn each_shell_prepends_in_its_own_grammar() {
		let dirs = vec![
			PathBuf::from("/r/.lattice/toolchains/node/bin"),
			PathBuf::from("/r/node_modules/.bin"),
		];
		assert_eq!(
			render(Shell::Posix, &dirs),
			"export PATH=\"/r/.lattice/toolchains/node/bin:/r/node_modules/.bin:$PATH\"\n"
		);
		assert_eq!(
			render(Shell::Fish, &dirs),
			"set -gx PATH '/r/.lattice/toolchains/node/bin' '/r/node_modules/.bin' $PATH\n"
		);
		assert!(render(Shell::Powershell, &dirs).starts_with("$env:PATH = \""));
	}

	#[test]
	fn nothing_to_prepend_prints_nothing() {
		assert_eq!(render(Shell::Posix, &[]), "");
	}
}
