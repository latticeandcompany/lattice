use console::style;

/// How Lattice renders progress and task output.
///
/// `Interactive` = a real, attended TTY that is not CI and not in loquacious
/// mode; this is the only mode in which a live TUI (progress bars / spinners)
/// should be drawn. `Raw` = everything else (CI, piped/redirected output, or
/// `--loquacious`); it emits plain, sequential text with no cursor/ANSI control
/// sequences so `lattice run build | cat` stays readable and stable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    Interactive,
    Raw,
}

/// Decide the output mode from the raw environment signals.
///
/// Pure and deterministic so it can be unit-tested without touching the real
/// terminal or process environment.
pub fn detect_mode(stdout_is_tty: bool, loquacious: bool, ci_env: bool) -> OutputMode {
    if stdout_is_tty && !loquacious && !ci_env {
        OutputMode::Interactive
    } else {
        OutputMode::Raw
    }
}

/// Whether ANSI color/styling should be emitted at all. Color is only ever used
/// in interactive mode, and never when `NO_COLOR` is set.
pub fn should_enable_color(mode: OutputMode, no_color_set: bool) -> bool {
    mode == OutputMode::Interactive && !no_color_set
}

pub struct OutputManager {
    pub loquacious: bool,
    /// True whenever we are NOT drawing an interactive TUI (CI, piped output, or
    /// loquacious). Retained as a public field for backward compatibility.
    pub is_ci: bool,
    mode: OutputMode,
}

impl OutputManager {
    pub fn new(loquacious: bool) -> Self {
        let stdout_is_tty = console::user_attended();
        let ci_env = std::env::var("CI").is_ok();
        let mode = detect_mode(stdout_is_tty, loquacious, ci_env);
        // Disable colorized styling whenever we are not interactive, or when the
        // user asked for no color. `console` reads these globally.
        let no_color = std::env::var("NO_COLOR").is_ok();
        console::set_colors_enabled(should_enable_color(mode, no_color));
        Self {
            loquacious,
            is_ci: mode == OutputMode::Raw,
            mode,
        }
    }

    /// The resolved output mode.
    pub fn mode(&self) -> OutputMode {
        self.mode
    }

    /// THE GATE: render the interactive TUI only when this is true.
    pub fn is_interactive(&self) -> bool {
        self.mode == OutputMode::Interactive
    }

    /// True in Raw mode (CI / piped / loquacious).
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
    fn interactive_only_when_tty_not_ci_not_loquacious() {
        assert_eq!(detect_mode(true, false, false), OutputMode::Interactive);
        assert_eq!(detect_mode(true, true, false), OutputMode::Raw); // loquacious
        assert_eq!(detect_mode(true, false, true), OutputMode::Raw); // CI
        assert_eq!(detect_mode(false, false, false), OutputMode::Raw); // piped
    }

    #[test]
    fn color_gated_on_interactive_and_no_color() {
        assert!(should_enable_color(OutputMode::Interactive, false));
        assert!(!should_enable_color(OutputMode::Interactive, true));
        assert!(!should_enable_color(OutputMode::Raw, false));
    }
}
