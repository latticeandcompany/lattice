use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};

use lattice_config::LatticeConfig;
use lattice_output::{OutputMode, Theme};

use crate::commands::{
	completions::CompletionsArgs, init::InitArgs, prune::PruneArgs, run::RunArgs, setup::SetupArgs,
	stats::StatsArgs, upgrade::UpgradeArgs, version::VersionArgs,
};
use crate::release::ReleaseUrls;

/// The compiled-in binary version.
pub const BIN_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Parser, Debug)]
#[command(
    name = "lattice",
    about = "A high-performance, local toolchain for managing monorepos.",
    version = BIN_VERSION,
    long_about = "Lattice runs the tasks you declare in lattice.json across the workspaces \
of your repo, in dependency order. A task whose inputs have not changed comes back from the \
cache instead of running again. Lattice also pins the tool versions the repo needs and \
provisions them into .lattice.\n\n\
To start, run `lattice init`. Then run `lattice run <task>`.",
    help_template = "\u{2756} {name} {version}\n{about}\n\n\
{usage-heading} {usage}\n\n{all-args}{after-help}"
)]
pub struct Cli {
	/// Print raw `workspace:task:` lines instead of the live display.
	#[arg(short, long, global = true)]
	pub verbose: bool,

	/// Hidden alias for --verbose.
	#[arg(short, long, global = true, hide = true)]
	pub loquacious: bool,

	/// Run this binary even when the repo pins another version.
	#[arg(long, global = true)]
	pub no_version_check: bool,

	/// Shade the logo for a light or dark terminal.
	#[arg(long, global = true, value_name = "THEME")]
	pub theme: Option<ThemeArg>,

	/// Base URL to download release archives from. A `file://` base works offline.
	#[arg(long, global = true, value_name = "URL")]
	pub release_base_url: Option<String>,

	#[command(subcommand)]
	pub command: Option<Commands>,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeArg {
	Light,
	Dark,
}

impl From<ThemeArg> for Theme {
	fn from(arg: ThemeArg) -> Self {
		match arg {
			ThemeArg::Light => Theme::Light,
			ThemeArg::Dark => Theme::Dark,
		}
	}
}

#[derive(Subcommand, Debug)]
pub enum Commands {
	/// Run one or more tasks across your workspaces, in dependency order.
	Run(RunArgs),

	/// Provision pinned toolchains, then install each workspace's dependencies.
	Setup(SetupArgs),

	/// Create a lattice.json and a .lattice/schema.json in the current directory.
	Init(InitArgs),

	/// Evict cache artifacts until the cache is under a size limit.
	Prune(PruneArgs),

	/// Report the task time this repo's cache has saved.
	Stats(StatsArgs),

	/// Move this repo to another version of Lattice and pin it.
	Upgrade(UpgradeArgs),

	/// Print a shell completion script to stdout.
	Completions(CompletionsArgs),

	/// Print version information.
	Version(VersionArgs),
}

impl Cli {
	/// The effective loquacious flag from CLI flags alone (`-v` or hidden `-l`).
	pub fn flag_loquacious(&self) -> bool {
		self.verbose || self.loquacious
	}

	/// Whether the pinned-version handover is skipped for this command.
	///
	/// `upgrade` is how the pin changes, so it has to run as invoked. The other
	/// two answer questions about this binary and this shell: a completion script
	/// must be the only thing on stdout, and `version` reporting anything but
	/// what the user just ran would make drift harder to diagnose, not easier.
	fn skips_pin_handover(&self) -> bool {
		matches!(
			self.command,
			Some(Commands::Upgrade(_))
				| Some(Commands::Completions(_))
				| Some(Commands::Version(_))
		)
	}

	/// The theme for this invocation: `--theme` if given, else the environment.
	pub fn effective_theme(&self) -> Theme {
		self.theme
			.map(Theme::from)
			.unwrap_or_else(lattice_output::detect_theme)
	}

	pub async fn execute(self) -> Result<()> {
		let flag_loq = self.flag_loquacious();
		let no_version_check = self.no_version_check;
		let theme = self.effective_theme();
		let base_url = self.release_base_url.clone();

		if !self.skips_pin_handover() {
			if let Some(root) = crate::drift::repo_root() {
				let urls = ReleaseUrls {
					base: base_url.clone(),
					..ReleaseUrls::default()
				};
				crate::drift::honor_pin(&root, no_version_check, &urls)?;
			}
		}

		match self.command {
			Some(Commands::Run(args)) => args.execute(flag_loq, no_version_check).await,
			Some(Commands::Setup(args)) => args.execute(flag_loq, no_version_check).await,
			Some(Commands::Init(args)) => args.execute(theme).await,
			Some(Commands::Prune(args)) => args.execute().await,
			Some(Commands::Stats(args)) => args.execute().await,
			Some(Commands::Upgrade(args)) => args.execute(base_url.as_deref()).await,
			Some(Commands::Completions(args)) => args.execute(),
			Some(Commands::Version(args)) => args.execute(theme).await,
			// Bare `lattice`: show the branded splash and point at `--help`
			// instead of clap's terse "missing subcommand" error.
			None => {
				println!("{}", lattice_output::splash(BIN_VERSION, theme));
				println!();
				println!(
					"Run {} to see available commands.",
					console::style("lattice --help").bold()
				);
				Ok(())
			}
		}
	}
}

/// Effective loquacious: a CLI flag or the `settings.loquacious` config setting.
/// Precedence does not matter for a boolean disjunction, but the CLI honors
/// flag > env > setting > default overall.
pub fn effective_loquacious(flag_loq: bool, setting_loq: bool) -> bool {
	flag_loq || setting_loq
}

/// Detect the output mode for this invocation, consulting the real terminal and
/// `CI` env. `effective_loq` forces [`OutputMode::Raw`].
pub fn detect_output_mode(effective_loq: bool) -> OutputMode {
	let tty = console::user_attended() && console::Term::stdout().is_term();
	let ci = std::env::var("CI").is_ok();
	lattice_output::detect_mode(tty, effective_loq, ci)
}

/// Gating logic for the one-line version-drift nag.
///
/// The nag shows only in an interactive session, when the repo opts into the
/// check, no suppression flag/env is set, and the pinned version differs from
/// the running binary. It is advisory and never an error.
///
/// By the time a command gets this far, a binary under `.lattice/bin` has already
/// been handed over to the pinned one (see [`crate::drift`]), so in practice this
/// is what a binary Lattice did not install gets instead.
pub fn should_nag(
	mode: OutputMode,
	version_check_setting: bool,
	no_version_check_flag: bool,
	env_no_version_check: bool,
	pinned: Option<&str>,
	bin: &str,
) -> bool {
	mode == OutputMode::Interactive
		&& version_check_setting
		&& !no_version_check_flag
		&& !env_no_version_check
		&& matches!(pinned, Some(p) if p != bin)
}

/// Emit the version nag to stderr when [`should_nag`] allows it.
pub fn maybe_emit_version_nag(
	mode: OutputMode,
	config: &LatticeConfig,
	no_version_check_flag: bool,
) {
	let pinned = config.lattice_version.as_deref();
	let env_no = std::env::var("LATTICE_NO_VERSION_CHECK").is_ok();
	if should_nag(
		mode,
		config.settings.version_check,
		no_version_check_flag,
		env_no,
		pinned,
		BIN_VERSION,
	) {
		// `pinned` is guaranteed `Some` here by `should_nag`.
		eprintln!(
			"{}",
			lattice_output::version_nag(BIN_VERSION, pinned.unwrap())
		);
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn effective_loquacious_is_or_of_flag_and_setting() {
		assert!(effective_loquacious(true, false));
		assert!(effective_loquacious(false, true));
		assert!(effective_loquacious(true, true));
		assert!(!effective_loquacious(false, false));
	}

	/// Parsed from argv rather than constructed, so the flag's spelling and its
	/// reach past a subcommand are part of what this pins down.
	#[test]
	fn the_theme_flag_wins_over_detection() {
		let cli = Cli::try_parse_from(["lattice", "version", "--theme", "light"]).unwrap();
		assert_eq!(cli.effective_theme(), Theme::Light);
		let cli = Cli::try_parse_from(["lattice", "--theme", "dark", "version"]).unwrap();
		assert_eq!(cli.effective_theme(), Theme::Dark);
	}

	#[test]
	fn theme_rejects_a_shade_it_does_not_have() {
		assert!(Cli::try_parse_from(["lattice", "--theme", "teal"]).is_err());
	}

	#[test]
	fn the_release_base_url_flag_reaches_every_subcommand() {
		// It has to: the handover that downloads a pinned version can happen under
		// any command, not just `upgrade`.
		for argv in [
			vec!["lattice", "run", "build", "--release-base-url", "file:///r"],
			vec!["lattice", "--release-base-url", "file:///r", "setup"],
			vec![
				"lattice",
				"upgrade",
				"latest",
				"--release-base-url",
				"file:///r",
			],
		] {
			let cli = Cli::try_parse_from(&argv).unwrap();
			assert_eq!(
				cli.release_base_url.as_deref(),
				Some("file:///r"),
				"{argv:?}"
			);
		}
	}

	#[test]
	fn nag_only_in_interactive_with_drift() {
		// Happy path: interactive, opted in, no suppression, pinned differs.
		assert!(should_nag(
			OutputMode::Interactive,
			true,
			false,
			false,
			Some("0.2.0"),
			"0.1.0"
		));
	}

	#[test]
	fn nag_suppressed_in_raw_mode() {
		assert!(!should_nag(
			OutputMode::Raw,
			true,
			false,
			false,
			Some("0.2.0"),
			"0.1.0"
		));
	}

	#[test]
	fn nag_suppressed_by_flag_or_env() {
		assert!(!should_nag(
			OutputMode::Interactive,
			true,
			true,
			false,
			Some("0.2.0"),
			"0.1.0"
		));
		assert!(!should_nag(
			OutputMode::Interactive,
			true,
			false,
			true,
			Some("0.2.0"),
			"0.1.0"
		));
	}

	#[test]
	fn nag_suppressed_when_setting_off() {
		assert!(!should_nag(
			OutputMode::Interactive,
			false,
			false,
			false,
			Some("0.2.0"),
			"0.1.0"
		));
	}

	#[test]
	fn no_nag_when_versions_match_or_unpinned() {
		// Matching versions → no nag.
		assert!(!should_nag(
			OutputMode::Interactive,
			true,
			false,
			false,
			Some("0.1.0"),
			"0.1.0"
		));
		// No pin → no nag.
		assert!(!should_nag(
			OutputMode::Interactive,
			true,
			false,
			false,
			None,
			"0.1.0"
		));
	}
}
