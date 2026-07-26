use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use console::{style, Style, Term};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

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

// ---------------------------------------------------------------------------
// Brand
// ---------------------------------------------------------------------------

/// The teal accent for **Lattice Build** (`teal-500`, BRAND.md §2). This is the
/// ~5% accent — it rides on the rosette, spinners, and the summary glyph only.
pub const TEAL: (u8, u8, u8) = (0x1B, 0x99, 0x8B);

/// Map an RGB triple to the nearest xterm-256 color-cube index.
///
/// `console` 0.15 does not expose 24-bit truecolor via [`Style`], so we snap the
/// brand RGB to the closest entry of the 6×6×6 color cube. `TEAL` → index 30.
fn rgb_to_ansi256(r: u8, g: u8, b: u8) -> u8 {
    const LEVELS: [i32; 6] = [0, 95, 135, 175, 215, 255];
    fn nearest(v: u8) -> i32 {
        let v = v as i32;
        let mut best = 0usize;
        let mut best_d = i32::MAX;
        for (i, &lvl) in LEVELS.iter().enumerate() {
            let d = (v - lvl).abs();
            if d < best_d {
                best_d = d;
                best = i;
            }
        }
        best as i32
    }
    (16 + 36 * nearest(r) + 6 * nearest(g) + nearest(b)) as u8
}

/// The teal accent as a [`console::Style`]. Used for the ~5% brand accent only.
///
/// Emits no escapes when color is globally disabled (NO_COLOR / Raw), because
/// `console` respects [`console::set_colors_enabled`].
pub fn teal() -> Style {
    Style::new().color256(rgb_to_ansi256(TEAL.0, TEAL.1, TEAL.2))
}

/// A quiet, branded banner/header line: rosette + lowercase `lattice` wordmark
/// (per BRAND.md the wordmark stays ink/paper; only the rosette carries teal).
pub fn banner_line(subtitle: &str) -> String {
    format!("{} lattice {}", teal().apply_to("◆"), style(subtitle).dim())
}

/// The one-line, advisory version nag (interactive only). Branded and quiet.
pub fn version_nag(binary_version: &str, pinned_version: &str) -> String {
    format!(
        "lattice {} · this repo pins {} — run `lattice upgrade`",
        binary_version, pinned_version
    )
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

/// Typed events the runner emits. Reporter impls decide how to render them.
#[derive(Clone)]
pub enum TaskEvent {
    Started {
        workspace: String,
        task: String,
    },
    CacheHit {
        workspace: String,
        task: String,
        key: String,
    },
    Output {
        workspace: String,
        task: String,
        line: String,
        stderr: bool,
    },
    Finished {
        workspace: String,
        task: String,
        duration_ms: u64,
    },
    Failed {
        workspace: String,
        task: String,
    },
    Skipped {
        workspace: String,
        task: String,
        reason: String,
    },
}

/// One output abstraction, consumed via events. Must be `Send + Sync`: the
/// runner shares it across concurrently spawned tasks behind an `Arc`, so all
/// state lives behind interior mutability.
pub trait Reporter: Send + Sync {
    fn run_start(&self, task: &str, workspaces: usize);
    fn event(&self, ev: TaskEvent);
    /// A failed task's captured output, surfaced together (expand-on-fail).
    fn surface_failure(&self, workspace: &str, task: &str, captured: &[(bool, String)]);
    fn run_summary(&self, total: usize, cached: usize, failed: usize, elapsed_ms: u64);
    /// Trace/detail line (hashing/cache/toolchain trace) — shown only in loquacious.
    fn note(&self, msg: &str);
    fn warn(&self, msg: &str);
    /// Called once at the end so the interactive impl can clear its progress surface.
    fn finish(&self);
}

/// Pick the reporter from detected mode. `Interactive` → [`InteractiveReporter`],
/// else [`CiReporter`] (carrying `loquacious` so its trace lines turn on).
pub fn make_reporter(mode: OutputMode, loquacious: bool) -> Box<dyn Reporter> {
    match mode {
        OutputMode::Interactive => Box::new(InteractiveReporter::new()),
        OutputMode::Raw => Box::new(CiReporter::new(loquacious)),
    }
}

fn short_key(key: &str) -> &str {
    let n = key.len().min(8);
    &key[..n]
}

fn fmt_secs(ms: u64) -> String {
    format!("{:.2}s", ms as f64 / 1000.0)
}

// ---------------------------------------------------------------------------
// CiReporter — Turborepo-style, ANSI-off, deterministic, greppable
// ---------------------------------------------------------------------------

/// Turborepo-style line stream: `workspace:task: <message>`, ANSI OFF,
/// deterministic, greppable. In loquacious mode it ALSO prints `note()` trace
/// lines and per-task output. This is the CI-mode reporter (no-TTY OR `-l` OR
/// `settings.loquacious`). It never styles output.
pub struct CiReporter {
    pub loquacious: bool,
}

impl CiReporter {
    pub fn new(loquacious: bool) -> Self {
        Self { loquacious }
    }
}

impl Reporter for CiReporter {
    fn run_start(&self, task: &str, workspaces: usize) {
        if self.loquacious {
            println!(
                "lattice: running `{}` across {} workspace(s)",
                task, workspaces
            );
        }
    }

    fn event(&self, ev: TaskEvent) {
        match ev {
            TaskEvent::Started { workspace, task } => {
                println!("{}:{}: running", workspace, task);
            }
            TaskEvent::CacheHit {
                workspace,
                task,
                key,
            } => {
                println!("{}:{}: cache hit [{}]", workspace, task, short_key(&key));
            }
            TaskEvent::Output {
                workspace,
                task,
                line,
                stderr,
            } => {
                if self.loquacious {
                    if stderr {
                        eprintln!("{}:{}: {}", workspace, task, line);
                    } else {
                        println!("{}:{}: {}", workspace, task, line);
                    }
                }
            }
            TaskEvent::Finished {
                workspace,
                task,
                duration_ms,
            } => {
                println!("{}:{}: done ({})", workspace, task, fmt_secs(duration_ms));
            }
            TaskEvent::Failed { workspace, task } => {
                eprintln!("{}:{}: FAILED", workspace, task);
            }
            TaskEvent::Skipped {
                workspace,
                task,
                reason,
            } => {
                if self.loquacious {
                    println!("{}:{}: skipped ({})", workspace, task, reason);
                }
            }
        }
    }

    fn surface_failure(&self, workspace: &str, task: &str, captured: &[(bool, String)]) {
        for (stderr, line) in captured {
            if *stderr {
                eprintln!("{}:{}: {}", workspace, task, line);
            } else {
                println!("{}:{}: {}", workspace, task, line);
            }
        }
    }

    fn run_summary(&self, total: usize, cached: usize, failed: usize, elapsed_ms: u64) {
        println!(
            "lattice: {} tasks, {} cached, {} failed, {}",
            total,
            cached,
            failed,
            fmt_secs(elapsed_ms)
        );
    }

    fn note(&self, msg: &str) {
        if self.loquacious {
            println!("lattice: {}", msg);
        }
    }

    fn warn(&self, msg: &str) {
        eprintln!("lattice: warning: {}", msg);
    }

    fn finish(&self) {}
}

// ---------------------------------------------------------------------------
// InteractiveReporter — branded indicatif MultiProgress
// ---------------------------------------------------------------------------

/// Branded interactive reporter: an [`indicatif::MultiProgress`] with one teal
/// spinner per running task. Child output is collapsed (buffered by the runner)
/// and surfaced only on failure. Finished bars settle into a static line with
/// a `✓`/`✗`/`●`/`○` glyph and a dim duration.
pub struct InteractiveReporter {
    mp: MultiProgress,
    bars: Mutex<HashMap<(String, String), ProgressBar>>,
}

impl Default for InteractiveReporter {
    fn default() -> Self {
        Self::new()
    }
}

impl InteractiveReporter {
    pub fn new() -> Self {
        Self {
            mp: MultiProgress::new(),
            bars: Mutex::new(HashMap::new()),
        }
    }

    fn term_width(&self) -> usize {
        let (_rows, cols) = Term::stdout().size();
        (cols as usize).max(20)
    }

    /// Truncate a display string to fit the terminal, appending an ellipsis.
    fn truncate(&self, s: &str, reserve: usize) -> String {
        let max = self.term_width().saturating_sub(reserve).max(8);
        if s.chars().count() <= max {
            s.to_string()
        } else {
            let keep = max.saturating_sub(1);
            let mut out: String = s.chars().take(keep).collect();
            out.push('…');
            out
        }
    }

    fn label(&self, workspace: &str, task: &str) -> String {
        self.truncate(&format!("{}:{}", workspace, task), 12)
    }

    /// Retire a running bar with a final static line (glyph + label + detail).
    fn settle(&self, workspace: &str, task: &str, line: String) {
        let key = (workspace.to_string(), task.to_string());
        let bar = self.bars.lock().unwrap().remove(&key);
        if let Some(bar) = bar {
            bar.set_style(ProgressStyle::with_template("{msg}").unwrap());
            bar.finish_with_message(line);
        } else {
            self.mp.println(line).ok();
        }
    }
}

impl Reporter for InteractiveReporter {
    fn run_start(&self, task: &str, workspaces: usize) {
        self.mp
            .println(banner_line(&format!(
                "{} · {} workspaces",
                task, workspaces
            )))
            .ok();
    }

    fn event(&self, ev: TaskEvent) {
        match ev {
            TaskEvent::Started { workspace, task } => {
                let idx = rgb_to_ansi256(TEAL.0, TEAL.1, TEAL.2);
                let label = self.label(&workspace, &task);
                let tmpl = format!("{{spinner:.{}}} {} {{wide_msg}}", idx, label);
                let bar = self.mp.add(ProgressBar::new_spinner());
                bar.set_style(
                    ProgressStyle::with_template(&tmpl)
                        .unwrap()
                        .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏ "),
                );
                bar.set_message(style("running…").dim().to_string());
                bar.enable_steady_tick(Duration::from_millis(80));
                self.bars.lock().unwrap().insert((workspace, task), bar);
            }
            TaskEvent::CacheHit {
                workspace,
                task,
                key,
            } => {
                let line = format!(
                    "{} {} {} {}",
                    teal().apply_to("●"),
                    self.label(&workspace, &task),
                    teal().apply_to("cache hit"),
                    style(format!("[{}]", short_key(&key))).dim()
                );
                self.settle(&workspace, &task, line);
            }
            TaskEvent::Output { .. } => {
                // Collapsed: child output is buffered by the runner and surfaced
                // only on failure via `surface_failure`. Do not render live.
            }
            TaskEvent::Finished {
                workspace,
                task,
                duration_ms,
            } => {
                let line = format!(
                    "{} {} {}",
                    style("✓").green().bold(),
                    self.label(&workspace, &task),
                    style(fmt_secs(duration_ms)).dim()
                );
                self.settle(&workspace, &task, line);
            }
            TaskEvent::Failed { workspace, task } => {
                let line = format!(
                    "{} {} {}",
                    style("✗").red().bold(),
                    self.label(&workspace, &task),
                    style("FAILED").red()
                );
                self.settle(&workspace, &task, line);
            }
            TaskEvent::Skipped {
                workspace,
                task,
                reason,
            } => {
                let line = format!(
                    "{} {} {}",
                    style("○").dim(),
                    self.label(&workspace, &task),
                    style(format!("skipped ({})", reason)).dim()
                );
                self.settle(&workspace, &task, line);
            }
        }
    }

    fn surface_failure(&self, workspace: &str, task: &str, captured: &[(bool, String)]) {
        let header = format!(
            "{} {}",
            style("✗").red().bold(),
            style(format!("{}:{} output", workspace, task)).bold()
        );
        self.mp.suspend(|| {
            eprintln!("{}", header);
            for (_stderr, line) in captured {
                eprintln!("    {}", style(line).dim());
            }
        })
    }

    fn run_summary(&self, total: usize, cached: usize, failed: usize, elapsed_ms: u64) {
        let failed_str = if failed > 0 {
            style(failed.to_string()).red().to_string()
        } else {
            style(failed.to_string()).dim().to_string()
        };
        let line = format!(
            "{}  {} tasks, {} cached, {} failed  {}",
            teal().apply_to("◆"),
            style(total).bold(),
            style(cached).green(),
            failed_str,
            style(fmt_secs(elapsed_ms)).dim()
        );
        self.mp.println(line).ok();
    }

    fn note(&self, msg: &str) {
        self.mp.println(style(msg).dim().to_string()).ok();
    }

    fn warn(&self, msg: &str) {
        self.mp
            .println(format!("{} {}", style("warn").yellow().bold(), msg))
            .ok();
    }

    fn finish(&self) {
        self.mp.clear().ok();
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

    // ----- New API tests -----

    /// A test-only reporter that records every event in order. Proves the trait
    /// shape is usable and that impls can be `Send + Sync` via interior mut.
    struct RecordingReporter {
        events: Mutex<Vec<TaskEvent>>,
    }

    impl RecordingReporter {
        fn new() -> Self {
            Self {
                events: Mutex::new(Vec::new()),
            }
        }
    }

    impl Reporter for RecordingReporter {
        fn run_start(&self, _task: &str, _workspaces: usize) {}
        fn event(&self, ev: TaskEvent) {
            self.events.lock().unwrap().push(ev);
        }
        fn surface_failure(&self, _workspace: &str, _task: &str, _captured: &[(bool, String)]) {}
        fn run_summary(&self, _total: usize, _cached: usize, _failed: usize, _elapsed_ms: u64) {}
        fn note(&self, _msg: &str) {}
        fn warn(&self, _msg: &str) {}
        fn finish(&self) {}
    }

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn reporters_are_send_sync() {
        _assert_send_sync::<InteractiveReporter>();
        _assert_send_sync::<CiReporter>();
        _assert_send_sync::<RecordingReporter>();
        _assert_send_sync::<Box<dyn Reporter>>();
    }

    #[test]
    fn recording_reporter_captures_events_in_order() {
        let r = RecordingReporter::new();
        let ws = "app".to_string();
        let task = "build".to_string();

        r.event(TaskEvent::Started {
            workspace: ws.clone(),
            task: task.clone(),
        });
        r.event(TaskEvent::Output {
            workspace: ws.clone(),
            task: task.clone(),
            line: "compiling".to_string(),
            stderr: false,
        });
        r.event(TaskEvent::Finished {
            workspace: ws.clone(),
            task: task.clone(),
            duration_ms: 1234,
        });
        r.event(TaskEvent::CacheHit {
            workspace: ws.clone(),
            task: task.clone(),
            key: "deadbeefcafef00d".to_string(),
        });
        r.event(TaskEvent::Failed {
            workspace: ws.clone(),
            task: task.clone(),
        });

        let events = r.events.lock().unwrap();
        assert_eq!(events.len(), 5);
        assert!(matches!(events[0], TaskEvent::Started { .. }));
        assert!(matches!(events[1], TaskEvent::Output { stderr: false, .. }));
        assert!(matches!(
            events[2],
            TaskEvent::Finished {
                duration_ms: 1234,
                ..
            }
        ));
        assert!(matches!(events[3], TaskEvent::CacheHit { .. }));
        assert!(matches!(events[4], TaskEvent::Failed { .. }));
    }

    #[test]
    fn teal_produces_a_style() {
        // Snapping brand teal to the 256-cube lands on index 30.
        assert_eq!(rgb_to_ansi256(TEAL.0, TEAL.1, TEAL.2), 30);
        let s = teal();
        // Force-enable colors locally so we can assert it emits an escape.
        let rendered = s.force_styling(true).apply_to("x").to_string();
        assert!(rendered.contains("\u{1b}["));
    }

    #[test]
    fn banner_line_contains_wordmark() {
        let b = banner_line("run build");
        assert!(!b.is_empty());
        assert!(b.contains("lattice"));
        assert!(b.contains("run build"));
    }

    #[test]
    fn version_nag_contains_both_versions() {
        let n = version_nag("1.0.0", "1.2.0");
        assert!(!n.is_empty());
        assert!(n.contains("lattice"));
        assert!(n.contains("1.0.0"));
        assert!(n.contains("1.2.0"));
    }

    #[test]
    fn make_reporter_picks_impl_by_mode() {
        // Just exercises construction paths; both are boxed trait objects.
        let _i = make_reporter(OutputMode::Interactive, false);
        let _c = make_reporter(OutputMode::Raw, true);
    }

    #[test]
    fn short_key_truncates_to_eight() {
        assert_eq!(short_key("deadbeefcafe"), "deadbeef");
        assert_eq!(short_key("abc"), "abc");
    }
}
