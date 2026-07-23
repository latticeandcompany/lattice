use console::{style, Term};

/// The two output modes Lattice supports (PRD §6.1/§6.2).
///
/// `Interactive` is the default for humans attached to a terminal: it is the
/// mode under which a live TUI should be rendered. `Raw` is the plain,
/// sequential, line-by-line, ANSI-free stream used for CI, pipes, redirects, or
/// when the user explicitly asks for it via `--loquacious`/`-l`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    /// Full interactive terminal experience (TUI). Renders ONLY in this mode.
    Interactive,
    /// Plain, deterministic, greppable, line-by-line output. No TUI, no ANSI.
    Raw,
}

/// Pure, unit-testable mode detection (PRD §6.2).
///
/// Returns [`OutputMode::Raw`] whenever the output is not attached to an
/// interactive terminal (`!stdout_is_tty`), a CI environment is detected
/// (`ci_env`), or the user explicitly requested raw output (`loquacious`).
/// Otherwise returns [`OutputMode::Interactive`].
///
/// The two non-interactive triggers produce the same stream; `loquacious`
/// simply forces `Raw` on a machine that would otherwise render the TUI.
pub fn detect_mode(stdout_is_tty: bool, loquacious: bool, ci_env: bool) -> OutputMode {
    if !stdout_is_tty || ci_env || loquacious {
        OutputMode::Raw
    } else {
        OutputMode::Interactive
    }
}

/// Pure, unit-testable rule for whether ANSI color should be enabled.
///
/// Color is disabled when `NO_COLOR` is set (see <https://no-color.org/>) or
/// whenever we are in [`OutputMode::Raw`] (CI, pipe, redirect, or loquacious).
/// It is only enabled for a genuine [`OutputMode::Interactive`] session with no
/// `NO_COLOR` override.
pub fn should_enable_color(mode: OutputMode, no_color_set: bool) -> bool {
    !no_color_set && mode == OutputMode::Interactive
}

pub struct OutputManager {
    /// Whether the user explicitly requested raw output via `--loquacious`/`-l`.
    pub loquacious: bool,
    mode: OutputMode,
}

impl OutputManager {
    /// Construct an `OutputManager`, auto-detecting the terminal mode.
    ///
    /// The constructor signature is intentionally stable: real TTY/CI detection
    /// is performed here and, as a side effect, ANSI color is disabled globally
    /// (via `console::set_colors_enabled(false)`) whenever we are not in a
    /// genuine interactive session or `NO_COLOR` is set. This guarantees that
    /// `console::style(...)` emits no escape codes in Raw/CI mode.
    pub fn new(loquacious: bool) -> Self {
        let stdout_is_tty = Term::stdout().is_term() && console::user_attended();
        let ci_env = std::env::var("CI").is_ok();
        let mode = detect_mode(stdout_is_tty, loquacious, ci_env);

        let no_color_set = std::env::var_os("NO_COLOR").is_some();
        if !should_enable_color(mode, no_color_set) {
            console::set_colors_enabled(false);
        }

        Self { loquacious, mode }
    }

    /// The detected output mode. A TUI must render ONLY when this is
    /// [`OutputMode::Interactive`].
    pub fn mode(&self) -> OutputMode {
        self.mode
    }

    /// `true` when running in a genuine interactive terminal session (i.e. a
    /// TUI may be rendered).
    pub fn is_interactive(&self) -> bool {
        self.mode == OutputMode::Interactive
    }

    /// `true` when output is in raw/CI mode (no TTY, CI env, or loquacious).
    ///
    /// Retained as a method so existing readers of the former `is_ci` field
    /// continue to work. Note this is now `true` for the loquacious case too,
    /// since both triggers produce the same non-interactive stream.
    pub fn is_ci(&self) -> bool {
        self.mode == OutputMode::Raw
    }

    pub fn header(&self, msg: &str) {
        if self.loquacious || self.is_ci() {
            println!("{}", msg);
        } else {
            println!("{}", style(msg).bold());
        }
    }

    pub fn log_start(&self, workspace: &str, task: &str) {
        println!(
            "{} {} {}",
            style("▶").cyan().bold(),
            style(format!("{}:{}", workspace, task)).bold(),
            style("starting...").dim()
        );
    }

    pub fn log_cache_hit(&self, workspace: &str, task: &str, hash: &str) {
        println!(
            "{} {} {} {}",
            style("●").green().bold(),
            style(format!("{}:{}", workspace, task)).bold(),
            style("cache hit").green(),
            style(format!("[{}]", hash)).dim()
        );
    }

    pub fn log_success(&self, workspace: &str, task: &str, duration_ms: u64) {
        println!(
            "{} {} {} {}",
            style("✓").green().bold(),
            style(format!("{}:{}", workspace, task)).bold(),
            style(format!("{:.2}s", duration_ms as f64 / 1000.0)).dim(),
            style("done").green()
        );
    }

    pub fn log_failure(&self, workspace: &str, task: &str) {
        eprintln!(
            "{} {} {}",
            style("✗").red().bold(),
            style(format!("{}:{}", workspace, task)).bold(),
            style("FAILED").red().bold()
        );
    }

    pub fn log_skipped(&self, workspace: &str, task: &str) {
        if self.loquacious {
            println!(
                "{} {} {}",
                style("○").dim(),
                style(format!("{}:{}", workspace, task)).dim(),
                style("skipped (no command)").dim()
            );
        }
    }

    #[allow(dead_code)]
    pub fn info(&self, msg: &str) {
        println!("{} {}", style("info").cyan().bold(), msg);
    }

    pub fn warn(&self, msg: &str) {
        eprintln!("{} {}", style("warn").yellow().bold(), msg);
    }

    pub fn detail(&self, msg: &str) {
        if self.loquacious {
            println!("  {}", style(msg).dim());
        }
    }

    #[allow(dead_code)]
    pub fn task_output(&self, line: &str, is_stderr: bool) {
        if is_stderr {
            eprintln!("    {}", style(line).dim());
        } else {
            println!("    {}", line);
        }
    }

    pub fn summary(&self, total: usize, cached: usize, failed: usize, elapsed_ms: u64) {
        println!();
        println!(
            "{}  {} tasks, {} cached, {} failed  {}",
            style("◆").bold(),
            style(total).bold(),
            style(cached).green(),
            if failed > 0 {
                style(failed.to_string()).red()
            } else {
                style(failed.to_string()).dim()
            },
            style(format!("{:.2}s", elapsed_ms as f64 / 1000.0)).dim()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_a_tty_is_raw() {
        // No interactive terminal (piped/redirected) always forces Raw.
        assert_eq!(detect_mode(false, false, false), OutputMode::Raw);
    }

    #[test]
    fn ci_env_is_raw() {
        // CI detected → Raw, even on a TTY without loquacious.
        assert_eq!(detect_mode(true, false, true), OutputMode::Raw);
    }

    #[test]
    fn loquacious_is_raw() {
        // Explicit --loquacious forces Raw even on an interactive TTY.
        assert_eq!(detect_mode(true, true, false), OutputMode::Raw);
    }

    #[test]
    fn interactive_tty_no_ci_not_loquacious_is_interactive() {
        assert_eq!(detect_mode(true, false, false), OutputMode::Interactive);
    }

    #[test]
    fn detect_mode_is_raw_if_any_trigger_set() {
        // Any single trigger, or any combination, yields Raw.
        for &loq in &[true, false] {
            for &ci in &[true, false] {
                for &tty in &[true, false] {
                    let expected = if tty && !ci && !loq {
                        OutputMode::Interactive
                    } else {
                        OutputMode::Raw
                    };
                    assert_eq!(
                        detect_mode(tty, loq, ci),
                        expected,
                        "tty={tty} loq={loq} ci={ci}"
                    );
                }
            }
        }
    }

    #[test]
    fn color_only_enabled_for_interactive_without_no_color() {
        assert!(should_enable_color(OutputMode::Interactive, false));
    }

    #[test]
    fn no_color_disables_color_in_interactive() {
        assert!(!should_enable_color(OutputMode::Interactive, true));
    }

    #[test]
    fn raw_mode_never_enables_color() {
        assert!(!should_enable_color(OutputMode::Raw, false));
        assert!(!should_enable_color(OutputMode::Raw, true));
    }
}
