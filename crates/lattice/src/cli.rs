use anyhow::Result;
use clap::{Parser, Subcommand};

use lattice_config::LatticeConfig;
use lattice_output::OutputMode;

use crate::commands::{
    completions::CompletionsArgs, init::InitArgs, prune::PruneArgs, run::RunArgs, setup::SetupArgs,
    version::VersionArgs,
};

/// The compiled-in binary version.
pub const BIN_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Parser, Debug)]
#[command(
    name = "lattice",
    about = "Local-first build tool for polyglot monorepos.",
    version = BIN_VERSION,
    long_about = "Lattice runs tasks across the workspaces of a polyglot monorepo in dependency \
order, with pinned toolchains and a content-addressed cache for reproducible builds.\n\n\
Declare workspaces and tasks in lattice.json, then `lattice run <task>`. Toolchains are \
provisioned locally under .lattice — nothing is installed globally."
)]
pub struct Cli {
    /// Stream the line-by-line log (Turborepo-style) instead of the interactive UI.
    #[arg(short, long, global = true)]
    pub loquacious: bool,

    /// Hidden alias for --loquacious.
    #[arg(short, long, global = true, hide = true)]
    pub verbose: bool,

    /// Suppress the version-drift nag.
    #[arg(long, global = true)]
    pub no_version_check: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Run a task across your workspaces, in dependency order.
    Run(RunArgs),

    /// Provision pinned toolchains and install native dependencies.
    Setup(SetupArgs),

    /// Scaffold a lattice.json (and .lattice/schema.json) in the current directory.
    Init(InitArgs),

    /// Evict cache artifacts until the cache is under a size limit.
    Prune(PruneArgs),

    /// Print a shell completion script to stdout.
    Completions(CompletionsArgs),

    /// Print version information.
    Version(VersionArgs),
}

impl Cli {
    /// The effective loquacious flag from CLI flags alone (`-l` OR hidden `-v`).
    pub fn flag_loquacious(&self) -> bool {
        self.loquacious || self.verbose
    }

    pub async fn execute(self) -> Result<()> {
        let flag_loq = self.flag_loquacious();
        let no_version_check = self.no_version_check;
        match self.command {
            Commands::Run(args) => args.execute(flag_loq, no_version_check).await,
            Commands::Setup(args) => args.execute(flag_loq, no_version_check).await,
            Commands::Init(args) => args.execute().await,
            Commands::Prune(args) => args.execute().await,
            Commands::Completions(args) => args.execute(),
            Commands::Version(args) => args.execute().await,
        }
    }
}

// ---------------------------------------------------------------------------
// Shared, testable helpers
// ---------------------------------------------------------------------------

/// Effective loquacious: a CLI flag OR the `settings.loquacious` config setting.
/// (Precedence is moot for a boolean OR, but keeping it explicit documents the
/// flag > env > setting > default order the CLI honors overall.)
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

/// Pure gating logic for the one-line version-drift nag.
///
/// The nag shows ONLY in an interactive session, when the repo opts into the
/// check, no suppression flag/env is set, and the pinned version differs from
/// the running binary. Advisory only — never an error.
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
