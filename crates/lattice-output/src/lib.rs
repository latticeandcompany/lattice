use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use console::{style, Style, Term};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

/// The two output modes Lattice supports.
///
/// `Interactive` is the default for humans attached to a terminal: it is the
/// mode under which a live TUI is rendered. `Raw` is the line-by-line,
/// ANSI-free stream used for CI, pipes, redirects, or when the user asks for it
/// via `--loquacious`/`-l`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
	/// Full interactive terminal experience (TUI). The only mode that renders it.
	Interactive,
	/// Line-by-line output with no TUI and no ANSI.
	Raw,
}

/// Returns [`OutputMode::Raw`] whenever the output is not attached to an
/// interactive terminal (`!stdout_is_tty`), a CI environment is detected
/// (`ci_env`), or the user explicitly requested raw output (`loquacious`).
/// Otherwise returns [`OutputMode::Interactive`].
///
/// The two non-interactive triggers produce the same stream; `loquacious`
/// forces `Raw` on a machine that would otherwise render the TUI.
pub fn detect_mode(stdout_is_tty: bool, loquacious: bool, ci_env: bool) -> OutputMode {
	if !stdout_is_tty || ci_env || loquacious {
		OutputMode::Raw
	} else {
		OutputMode::Interactive
	}
}

/// Color is disabled when `NO_COLOR` is set (see <https://no-color.org/>) or
/// whenever we are in [`OutputMode::Raw`] (CI, pipe, redirect, or loquacious).
/// It is only enabled for a genuine [`OutputMode::Interactive`] session with no
/// `NO_COLOR` override.
pub fn should_enable_color(mode: OutputMode, no_color_set: bool) -> bool {
	!no_color_set && mode == OutputMode::Interactive
}

/// The teal accent for **Lattice Build** (`teal-500`). It is used on the rosette
/// mark, spinners, and product words only.
pub const TEAL: (u8, u8, u8) = (0x1B, 0x99, 0x8B);
/// The teal tint (`teal-300`) used for the rosette art fill on dark terminals.
pub const TEAL_300: (u8, u8, u8) = (0x63, 0xC4, 0xB8);

/// Which terminal background the splash art should be tuned for. The mark keeps
/// its teal identity but swaps shade so it does not wash out: the lighter
/// `teal-300` on a dark background, the deeper `teal-500` on a light one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
	Light,
	Dark,
}

/// An explicit `LATTICE_THEME=light|dark` override always wins. Otherwise we
/// consult `COLORFGBG` (set by many terminals as `fg;bg`, occasionally
/// `fg;default;bg`) and read the trailing background field: ANSI `7`/`15`
/// (white / bright white) means a light background, anything else is dark.
/// With no signal we default to [`Theme::Dark`] — most terminals are dark, and
/// this preserves the historical splash color.
pub fn theme_from_env(theme_override: Option<&str>, colorfgbg: Option<&str>) -> Theme {
	if let Some(v) = theme_override {
		match v.trim().to_ascii_lowercase().as_str() {
			"light" => return Theme::Light,
			"dark" => return Theme::Dark,
			_ => {}
		}
	}
	if let Some(spec) = colorfgbg {
		if let Some(bg) = spec.split(';').next_back() {
			if let Ok(n) = bg.trim().parse::<u8>() {
				return if n == 7 || n == 15 {
					Theme::Light
				} else {
					Theme::Dark
				};
			}
		}
	}
	Theme::Dark
}

pub fn detect_theme() -> Theme {
	theme_from_env(
		std::env::var("LATTICE_THEME").ok().as_deref(),
		std::env::var("COLORFGBG").ok().as_deref(),
	)
}

/// The rosette (woven-sphere) mark — the Lattice logo — as compact ASCII.
/// Rendered in teal for the `version`/splash surface.
pub const ROSETTE_ART: &str = include_str!("../assets/rosette.txt");

/// The inline rosette glyph — a four-petal node that echoes the woven mark.
/// Used as the teal accent on headers, summaries, and cache hits.
pub const ROSETTE: &str = "\u{2756}"; // ❖

/// Map an RGB triple to the nearest xterm-256 color-cube index.
///
/// Used only for the indicatif spinner template token (which takes a 256 index).
/// `TEAL` → index 30.
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

/// The teal accent as a [`console::Style`] (256-color fallback), kept for the
/// indicatif spinner token. Prefer [`paint_teal`] for static text — it emits
/// true 24-bit color that matches the brand hex exactly.
pub fn teal() -> Style {
	Style::new().color256(rgb_to_ansi256(TEAL.0, TEAL.1, TEAL.2))
}

/// Paint text in true 24-bit brand teal (`#1B998B`). Emits no escapes when color
/// is globally disabled (NO_COLOR / Raw / piped), honoring [`console::colors_enabled`].
pub fn paint_teal(s: &str) -> String {
	paint_rgb(s, TEAL)
}

fn paint_rgb(s: &str, (r, g, b): (u8, u8, u8)) -> String {
	if console::colors_enabled() {
		format!("\u{1b}[38;2;{r};{g};{b}m{s}\u{1b}[39m")
	} else {
		s.to_string()
	}
}

/// The lowercase `lattice` wordmark, bold. The wordmark stays ink/paper; only
/// the rosette and product words carry the teal accent.
pub fn wordmark() -> String {
	style("lattice").bold().to_string()
}

/// The rosette logo painted in true teal, ready to print as a splash block.
/// The teal shade adapts to the terminal background (see [`Theme`]).
pub fn logo() -> String {
	logo_for(detect_theme())
}

/// The rosette logo painted for a specific [`Theme`], so callers can pick the
/// shade without consulting the environment.
pub fn logo_for(theme: Theme) -> String {
	let fill = match theme {
		Theme::Light => TEAL,
		Theme::Dark => TEAL_300,
	};
	ROSETTE_ART
		.lines()
		.map(|l| paint_rgb(l, fill))
		.collect::<Vec<_>>()
		.join("\n")
}

/// The full branded splash: the teal rosette mark, the `lattice <version>
/// (arch)` lockup, and the tagline. Shared by `version`, `init`, and the
/// bare `lattice` invocation. The mark's teal shade adapts to the terminal
/// background.
pub fn splash(version: &str) -> String {
	format!(
		"{}\n{} {}  {}  {}\n{}",
		logo(),
		paint_teal(ROSETTE),
		wordmark(),
		style(version).bold(),
		style(format!("({})", std::env::consts::ARCH)).dim(),
		style("A high-performance, local toolchain for managing monorepos.").dim(),
	)
}

/// A quiet, branded one-line header: teal rosette glyph + bold `lattice`
/// wordmark + a dim subtitle.
pub fn banner_line(subtitle: &str) -> String {
	format!(
		"{} {}  {}",
		paint_teal(ROSETTE),
		wordmark(),
		style(subtitle).dim()
	)
}

/// The one-line, advisory version nag (interactive only). Branded and quiet.
pub fn version_nag(binary_version: &str, pinned_version: &str) -> String {
	format!(
		"{} {} {} · this repo pins {} · run {}",
		paint_teal(ROSETTE),
		wordmark(),
		binary_version,
		style(pinned_version).bold(),
		style("`lattice upgrade`").dim()
	)
}

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
		/// From a persistent task (dev server/watcher). Streamed live even
		/// outside loquacious mode.
		persistent: bool,
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

/// Under a minute, seconds with two decimals (`1.23s`). Past that, clock form —
/// `4:07` for four minutes, `1:12:30` once it crosses an hour — because
/// `4350.00s` is not a number anyone can read.
fn fmt_secs(ms: u64) -> String {
	let total = (ms + 500) / 1000;
	if total < 60 {
		return format!("{:.2}s", ms as f64 / 1000.0);
	}
	let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
	if h > 0 {
		format!("{h}:{m:02}:{s:02}")
	} else {
		format!("{m}:{s:02}")
	}
}

/// Plain line stream: `workspace:task: <message>`, never styled. In loquacious
/// mode it also prints `note()` trace lines and per-task output. This is the
/// reporter used when there is no TTY, or `-l` or `settings.loquacious` is set.
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
				persistent,
			} => {
				// Persistent output (dev servers) always streams; other per-task
				// output only in loquacious mode (else it's surfaced on failure).
				if self.loquacious || persistent {
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

	/// The `workspace:task` label, truncated and right-padded to a fixed column
	/// so status glyphs and durations align down the run.
	fn label(&self, workspace: &str, task: &str) -> String {
		const LABEL_W: usize = 28;
		let raw = format!("{}:{}", workspace, task);
		let width = LABEL_W.min(self.term_width().saturating_sub(16).max(10));
		let shown = if raw.chars().count() > width {
			let mut s: String = raw.chars().take(width.saturating_sub(1)).collect();
			s.push('…');
			s
		} else {
			raw
		};
		format!("{:<width$}", shown, width = width)
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
		let head = format!(
			"{} {}  {}  {}",
			paint_teal(ROSETTE),
			wordmark(),
			paint_teal(task),
			style(format!(
				"· {} workspace{}",
				workspaces,
				if workspaces == 1 { "" } else { "s" }
			))
			.dim()
		);
		let rule = style("─".repeat(self.term_width().min(52)))
			.dim()
			.to_string();
		self.mp.println(format!("\n{head}\n{rule}")).ok();
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
					paint_teal("●"),
					self.label(&workspace, &task),
					paint_teal("cache hit"),
					style(format!("[{}]", short_key(&key))).dim()
				);
				self.settle(&workspace, &task, line);
			}
			TaskEvent::Output { .. } => {
				// Child output is buffered by the runner and surfaced only on
				// failure via `surface_failure`.
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
		let rule = style("─".repeat(self.term_width().min(52)))
			.dim()
			.to_string();
		let line = format!(
			"{}\n{}  {} tasks · {} cached · {} failed  {}",
			rule,
			paint_teal(ROSETTE),
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
		assert_eq!(detect_mode(false, false, false), OutputMode::Raw);
	}

	#[test]
	fn ci_env_is_raw() {
		assert_eq!(detect_mode(true, false, true), OutputMode::Raw);
	}

	#[test]
	fn loquacious_is_raw() {
		assert_eq!(detect_mode(true, true, false), OutputMode::Raw);
	}

	#[test]
	fn interactive_tty_no_ci_not_loquacious_is_interactive() {
		assert_eq!(detect_mode(true, false, false), OutputMode::Interactive);
	}

	#[test]
	fn detect_mode_is_raw_if_any_trigger_set() {
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

	/// A test-only reporter that records every event in order.
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
			persistent: false,
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
	fn fmt_secs_uses_seconds_under_a_minute() {
		assert_eq!(fmt_secs(0), "0.00s");
		assert_eq!(fmt_secs(1234), "1.23s");
		assert_eq!(fmt_secs(59_499), "59.50s");
	}

	#[test]
	fn fmt_secs_uses_clock_form_past_a_minute() {
		assert_eq!(fmt_secs(60_000), "1:00");
		assert_eq!(fmt_secs(247_000), "4:07");
		assert_eq!(fmt_secs(3_599_400), "59:59");
		assert_eq!(fmt_secs(3_600_000), "1:00:00");
		assert_eq!(fmt_secs(4_350_000), "1:12:30");
		assert_eq!(fmt_secs(90_000_000), "25:00:00");
	}

	#[test]
	fn fmt_secs_rounds_without_showing_sixty_seconds() {
		assert_eq!(fmt_secs(119_500), "2:00");
		assert_eq!(fmt_secs(3_599_500), "1:00:00");
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
		let _i = make_reporter(OutputMode::Interactive, false);
		let _c = make_reporter(OutputMode::Raw, true);
	}

	#[test]
	fn short_key_truncates_to_eight() {
		assert_eq!(short_key("deadbeefcafe"), "deadbeef");
		assert_eq!(short_key("abc"), "abc");
	}

	#[test]
	fn theme_override_wins_over_colorfgbg() {
		assert_eq!(theme_from_env(Some("light"), Some("15;0")), Theme::Light);
		assert_eq!(theme_from_env(Some("dark"), Some("0;15")), Theme::Dark);
		// Case-insensitive and whitespace-tolerant.
		assert_eq!(theme_from_env(Some(" LIGHT "), None), Theme::Light);
	}

	#[test]
	fn theme_reads_colorfgbg_background_field() {
		// Trailing field is the background: 15/7 → light, else dark.
		assert_eq!(theme_from_env(None, Some("0;15")), Theme::Light);
		assert_eq!(theme_from_env(None, Some("0;7")), Theme::Light);
		assert_eq!(theme_from_env(None, Some("15;0")), Theme::Dark);
		// The three-field "fg;default;bg" form still reads the last field.
		assert_eq!(theme_from_env(None, Some("15;default;0")), Theme::Dark);
		assert_eq!(theme_from_env(None, Some("0;default;15")), Theme::Light);
	}

	#[test]
	fn theme_defaults_to_dark_without_signal() {
		assert_eq!(theme_from_env(None, None), Theme::Dark);
		// Unparseable / bogus values fall through to the dark default.
		assert_eq!(theme_from_env(Some("teal"), None), Theme::Dark);
		assert_eq!(theme_from_env(None, Some("nonsense")), Theme::Dark);
	}

	#[test]
	fn logo_variants_carry_the_mark() {
		// Both variants render the full rosette; they differ only in shade.
		let light = logo_for(Theme::Light);
		let dark = logo_for(Theme::Dark);
		assert_eq!(light.lines().count(), ROSETTE_ART.lines().count());
		assert_eq!(dark.lines().count(), ROSETTE_ART.lines().count());
	}

	#[test]
	fn splash_contains_wordmark_and_version() {
		let s = splash("9.9.9");
		assert!(s.contains("lattice"));
		assert!(s.contains("9.9.9"));
	}
}
