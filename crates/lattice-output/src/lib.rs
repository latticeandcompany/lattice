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
/// via `--verbose`/`-v`.
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

/// Color is disabled when `NO_COLOR` is set (see <https://no-color.org/>) and
/// whenever stdout is not a terminal, so a pipe, a redirect, or a CI log never
/// receives an escape it would have to strip. A terminal in
/// [`OutputMode::Raw`] — `-v`, or a persistent task forcing the stream — still
/// gets color: that is where the per-task label colors live.
pub fn should_enable_color(mode: OutputMode, no_color_set: bool, stdout_is_tty: bool) -> bool {
	if no_color_set {
		return false;
	}
	match mode {
		OutputMode::Interactive => true,
		OutputMode::Raw => stdout_is_tty,
	}
}

/// Set the process-wide `console` color gate from [`should_enable_color`],
/// reading `NO_COLOR` and the real terminal. Call this once the output mode is
/// final and before anything prints: every `paint_*` helper and every
/// [`console::style`] call downstream reads the gate this sets.
pub fn apply_color_policy(mode: OutputMode) {
	let on = should_enable_color(
		mode,
		std::env::var_os("NO_COLOR").is_some(),
		Term::stdout().is_term(),
	);
	console::set_colors_enabled(on);
	console::set_colors_enabled_stderr(on);
}

/// The teal accent for **Lattice Build** (`teal-500`). It is used on the rosette
/// mark, spinners, and product words only.
pub const TEAL: (u8, u8, u8) = (0x1B, 0x99, 0x8B);
/// The teal tint (`teal-300`) used for the rosette art fill on dark terminals.
pub const TEAL_300: (u8, u8, u8) = (0x63, 0xC4, 0xB8);
/// The teal shade (`teal-700`), the dark end of the accent ramp.
pub const TEAL_700: (u8, u8, u8) = (0x13, 0x6E, 0x64);

/// The accent ramp, dark to light. The full-cache banner is the only surface
/// that walks it; everywhere else picks a single shade.
const TEAL_RAMP: [(u8, u8, u8); 3] = [TEAL_700, TEAL, TEAL_300];

/// Which terminal background the splash art should be tuned for. The mark keeps
/// its teal identity but swaps shade so it does not wash out: the lighter
/// `teal-300` on a dark background, the deeper `teal-500` on a light one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
	Light,
	Dark,
}

/// An explicit `light`/`dark` override always wins. Otherwise we consult
/// `COLORFGBG` (set by many terminals as `fg;bg`, occasionally `fg;default;bg`)
/// and read the trailing background field: ANSI `7`/`15` (white / bright white)
/// means a light background, anything else is dark. With no signal we default to
/// [`Theme::Dark`] — most terminals are dark, and this preserves the historical
/// splash color.
pub fn resolve_theme(theme_override: Option<&str>, colorfgbg: Option<&str>) -> Theme {
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

/// The theme when `--theme` was not passed: `LATTICE_THEME`, else the terminal's
/// own `COLORFGBG` signal.
pub fn detect_theme() -> Theme {
	resolve_theme(
		std::env::var("LATTICE_THEME").ok().as_deref(),
		std::env::var("COLORFGBG").ok().as_deref(),
	)
}

/// The colors a `workspace:task` label can take in the raw stream: eight hues,
/// one per 45° step around the wheel, all at the same saturation and lightness
/// so no label reads as louder than another. The wheel starts at 25° rather
/// than 0° to keep every entry clear of the red a `FAILED` marker uses.
pub const LABEL_PALETTE: [(u8, u8, u8); 8] = [
	(0xE0, 0x8D, 0x52), // 25°  amber
	(0xC9, 0xE0, 0x52), // 70°  lime
	(0x5E, 0xE0, 0x52), // 115° green
	(0x52, 0xE0, 0xB1), // 160° aqua
	(0x52, 0xA5, 0xE0), // 205° blue
	(0x69, 0x52, 0xE0), // 250° violet
	(0xD4, 0x52, 0xE0), // 295° magenta
	(0xE0, 0x52, 0x81), // 340° rose
];

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

/// The color `t` of the way along `stops`, `t` clamped to `0.0..=1.0`. Segments
/// are equal-width, so a three-stop ramp puts the middle stop exactly at 0.5.
fn ramp_at(stops: &[(u8, u8, u8)], t: f64) -> (u8, u8, u8) {
	let t = t.clamp(0.0, 1.0);
	let last = stops.len() - 1;
	let scaled = t * last as f64;
	let i = (scaled.floor() as usize).min(last.saturating_sub(1));
	let f = scaled - i as f64;
	let (a, b) = (stops[i], stops[i + 1]);
	let mix = |x: u8, y: u8| (x as f64 + (y as f64 - x as f64) * f).round() as u8;
	(mix(a.0, b.0), mix(a.1, b.1), mix(a.2, b.2))
}

/// Paint `s` along `stops`, one step per character. Whitespace is left
/// unpainted so the escapes only wrap glyphs that show the color.
fn paint_gradient(s: &str, stops: &[(u8, u8, u8)]) -> String {
	if !console::colors_enabled() || stops.len() < 2 {
		return s.to_string();
	}
	gradient_escapes(s, stops)
}

fn gradient_escapes(s: &str, stops: &[(u8, u8, u8)]) -> String {
	let n = s.chars().count();
	let span = (n.saturating_sub(1)).max(1) as f64;
	let mut out = String::with_capacity(s.len() + n * 20);
	for (i, ch) in s.chars().enumerate() {
		if ch.is_whitespace() {
			out.push(ch);
			continue;
		}
		let (r, g, b) = ramp_at(stops, i as f64 / span);
		out.push_str(&format!("\u{1b}[38;2;{r};{g};{b}m{ch}"));
	}
	out.push_str("\u{1b}[39m");
	out
}

/// `workspace:task`, painted in `color`. Kept separate from [`LabelColors::paint`]
/// so the escape sequence is testable without touching the global color gate.
fn label_escapes(label: &str, color: (u8, u8, u8)) -> String {
	let (r, g, b) = color;
	format!("\u{1b}[38;2;{r};{g};{b}m{label}\u{1b}[39m")
}

/// Hands each `workspace:task` in a run its own color from [`LABEL_PALETTE`],
/// assigned in the order labels are first seen so that no two share one until
/// the ninth label wraps the palette. The raw stream interleaves lines from
/// every task at once, and the color is what lets the eye follow one of them.
///
/// The runner reports from concurrently spawned tasks, so the assignment table
/// lives behind a lock.
pub struct LabelColors {
	assigned: Mutex<HashMap<String, usize>>,
}

impl Default for LabelColors {
	fn default() -> Self {
		Self::new()
	}
}

impl LabelColors {
	pub fn new() -> Self {
		Self {
			assigned: Mutex::new(HashMap::new()),
		}
	}

	/// This label's color, assigning one on first sight. Stable for the rest of
	/// the run.
	pub fn color(&self, workspace: &str, task: &str) -> (u8, u8, u8) {
		let mut assigned = self.assigned.lock().unwrap();
		let next = assigned.len();
		let idx = *assigned
			.entry(format!("{workspace}:{task}"))
			.or_insert(next);
		LABEL_PALETTE[idx % LABEL_PALETTE.len()]
	}

	/// `workspace:task` in this label's color, or bare when color is off — the
	/// text is identical either way, so a piped run keeps the same shape.
	pub fn paint(&self, workspace: &str, task: &str) -> String {
		let label = format!("{workspace}:{task}");
		if console::colors_enabled() {
			label_escapes(&label, self.color(workspace, task))
		} else {
			label
		}
	}
}

/// True when the run executed nothing: every task it scheduled came back from
/// cache. An empty run (no workspace matched the filter) does not qualify —
/// there was no work to skip.
pub fn is_full_cache(total: usize, cached: usize, failed: usize) -> bool {
	total > 0 && failed == 0 && cached == total
}

/// The banner printed under the summary when the whole run came from cache,
/// walking the teal ramp a character at a time.
pub fn full_cache_banner() -> String {
	paint_gradient(
		&format!("{ROSETTE}{ROSETTE}{ROSETTE} FULL CACHE"),
		&TEAL_RAMP,
	)
}

/// The lowercase `lattice` wordmark, bold. The wordmark stays ink/paper; only
/// the rosette and product words carry the teal accent.
pub fn wordmark() -> String {
	style("lattice").bold().to_string()
}

/// The rosette logo painted in true teal for a [`Theme`], ready to print as a
/// splash block.
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
/// (arch)` lockup, and the tagline. Shared by `version` and the bare `lattice`
/// invocation.
pub fn splash(version: &str, theme: Theme) -> String {
	format!(
		"{}\n{} {}  {}  {}\n{}",
		logo_for(theme),
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
/// Shown for a binary Lattice did not install, where switching is the reader's
/// call rather than something to do behind their back.
pub fn version_nag(binary_version: &str, pinned_version: &str) -> String {
	format!(
		"{} {} {} · this repo pins {} · run {}",
		paint_teal(ROSETTE),
		wordmark(),
		binary_version,
		style(pinned_version).bold(),
		style(format!("`lattice upgrade {pinned_version}`")).dim()
	)
}

/// The one-line notice printed when an invocation is handed to the version the
/// repo pins. Not advisory: by the time this prints, the switch is happening.
pub fn switching_notice(binary_version: &str, pinned_version: &str) -> String {
	format!(
		"{} {} {} · this repo pins {} · switching",
		paint_teal(ROSETTE),
		wordmark(),
		binary_version,
		style(pinned_version).bold(),
	)
}

// The events and the trait live in `lattice-events`, which depends on nothing but
// `serde`, so a front end can consume a run without linking `console` and
// `indicatif`. Re-exported here because this crate's two reporters are the
// terminal implementations of that trait.
pub use lattice_events::{CacheMiss, OutputLine, Reporter, TaskEvent};

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

/// How a persistent task's exit reads in a status line: its code, or a note that
/// a signal ended it, which on unix leaves no code behind.
fn exit_desc(code: Option<i32>) -> String {
	match code {
		Some(c) => format!("code {c}"),
		None => "killed by signal".to_string(),
	}
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

/// Line stream: `workspace:task: <message>`, one line per event. In loquacious
/// mode it also prints `note()` trace lines and per-task output. This is the
/// reporter used when there is no TTY, or `-v` or `settings.loquacious` is set.
///
/// The `workspace:task` label carries a per-task color from [`LabelColors`] so
/// interleaved lines can be told apart at a glance; nothing else in the line is
/// styled, and off a terminal the labels print bare.
pub struct CiReporter {
	pub loquacious: bool,
	labels: LabelColors,
}

impl CiReporter {
	pub fn new(loquacious: bool) -> Self {
		Self {
			loquacious,
			labels: LabelColors::new(),
		}
	}

	fn label(&self, workspace: &str, task: &str) -> String {
		self.labels.paint(workspace, task)
	}
}

impl Reporter for CiReporter {
	fn run_start(&self, task: &str, workspaces: usize) {
		if self.loquacious {
			println!(
				"lattice: running `{}` across {} workspace{}",
				task,
				workspaces,
				if workspaces == 1 { "" } else { "s" }
			);
		}
	}

	fn event(&self, ev: TaskEvent) {
		match ev {
			TaskEvent::Started { workspace, task } => {
				println!("{}: running", self.label(&workspace, &task));
			}
			TaskEvent::CacheHit {
				workspace,
				task,
				key,
			} => {
				println!(
					"{}: cache hit [{}]",
					self.label(&workspace, &task),
					short_key(&key)
				);
			}
			TaskEvent::CacheMiss {
				workspace,
				task,
				miss,
			} => {
				self.task_note(&workspace, &task, &miss.describe());
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
					let label = self.label(&workspace, &task);
					if stderr {
						eprintln!("{}: {}", label, line);
					} else {
						println!("{}: {}", label, line);
					}
				}
			}
			TaskEvent::Finished {
				workspace,
				task,
				duration_ms,
			} => {
				println!(
					"{}: done ({})",
					self.label(&workspace, &task),
					fmt_secs(duration_ms)
				);
			}
			TaskEvent::Failed { workspace, task } => {
				eprintln!("{}: FAILED", self.label(&workspace, &task));
			}
			TaskEvent::PersistentExited {
				workspace,
				task,
				code,
				duration_ms,
			} => {
				let label = self.label(&workspace, &task);
				let (desc, secs) = (exit_desc(code), fmt_secs(duration_ms));
				if code == Some(0) {
					println!("{}: exited ({}) after {}", label, desc, secs);
				} else {
					eprintln!("{}: EXITED ({}) after {}", label, desc, secs);
				}
			}
			TaskEvent::Skipped {
				workspace,
				task,
				reason,
			} => {
				if self.loquacious {
					println!("{}: skipped ({})", self.label(&workspace, &task), reason);
				}
			}
		}
	}

	fn surface_failure(&self, workspace: &str, task: &str, captured: &[(bool, String)]) {
		let label = self.label(workspace, task);
		for (stderr, line) in captured {
			if *stderr {
				eprintln!("{}: {}", label, line);
			} else {
				println!("{}: {}", label, line);
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
		if is_full_cache(total, cached, failed) {
			println!("lattice: full cache, nothing to run");
		}
	}

	fn note(&self, msg: &str) {
		if self.loquacious {
			println!("lattice: {}", msg);
		}
	}

	fn warn(&self, msg: &str) {
		eprintln!("lattice: warning: {}", msg);
	}

	fn task_note(&self, workspace: &str, task: &str, msg: &str) {
		if self.loquacious {
			println!("lattice: {}: {}", self.label(workspace, task), msg);
		}
	}

	fn task_warn(&self, workspace: &str, task: &str, msg: &str) {
		eprintln!("lattice: warning: {}: {}", self.label(workspace, task), msg);
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
			TaskEvent::CacheMiss {
				workspace,
				task,
				miss,
			} => {
				self.task_note(&workspace, &task, &miss.describe());
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
			TaskEvent::PersistentExited {
				workspace,
				task,
				code,
				duration_ms,
			} => {
				let detail = format!("exited ({})", exit_desc(code));
				let line = if code == Some(0) {
					format!(
						"{} {} {} {}",
						style("○").dim(),
						self.label(&workspace, &task),
						style(detail).dim(),
						style(fmt_secs(duration_ms)).dim()
					)
				} else {
					format!(
						"{} {} {} {}",
						style("✗").red().bold(),
						self.label(&workspace, &task),
						style(detail.to_uppercase()).red(),
						style(fmt_secs(duration_ms)).dim()
					)
				};
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
		if is_full_cache(total, cached, failed) {
			self.mp.println(format!("\n{}", full_cache_banner())).ok();
		}
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
	fn color_enabled_for_interactive_without_no_color() {
		assert!(should_enable_color(OutputMode::Interactive, false, true));
	}

	#[test]
	fn no_color_disables_color_in_either_mode() {
		assert!(!should_enable_color(OutputMode::Interactive, true, true));
		assert!(!should_enable_color(OutputMode::Raw, true, true));
	}

	#[test]
	fn raw_colors_only_on_a_terminal() {
		// `-v` at a real shell: colored labels.
		assert!(should_enable_color(OutputMode::Raw, false, true));
		// Piped, redirected, or a CI log: nothing to strip.
		assert!(!should_enable_color(OutputMode::Raw, false, false));
	}

	/// A test-only reporter that records every event in order.
	struct RecordingReporter {
		events: Mutex<Vec<TaskEvent>>,
		lines: Mutex<Vec<String>>,
	}

	impl RecordingReporter {
		fn new() -> Self {
			Self {
				events: Mutex::new(Vec::new()),
				lines: Mutex::new(Vec::new()),
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
		fn note(&self, msg: &str) {
			self.lines.lock().unwrap().push(format!("note: {msg}"));
		}
		fn warn(&self, msg: &str) {
			self.lines.lock().unwrap().push(format!("warn: {msg}"));
		}
		fn finish(&self) {}
	}

	#[test]
	fn task_trace_lines_default_to_a_labeled_prefix() {
		// A reporter that does not color labels still gets `workspace:task: msg`,
		// so the raw stream reads the same as it did before the split.
		let r = RecordingReporter::new();
		r.task_note("web", "build", "hash deadbeef");
		r.task_warn("web", "build", "cache lookup failed");
		assert_eq!(
			*r.lines.lock().unwrap(),
			vec![
				"note: web:build: hash deadbeef".to_string(),
				"warn: web:build: cache lookup failed".to_string(),
			]
		);
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
	fn exit_desc_names_the_code_or_the_signal() {
		assert_eq!(exit_desc(Some(0)), "code 0");
		assert_eq!(exit_desc(Some(1)), "code 1");
		assert_eq!(exit_desc(None), "killed by signal");
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
		// The suggested command has to be runnable as printed; `upgrade` requires
		// a version.
		assert!(n.contains("lattice upgrade 1.2.0"));
	}

	#[test]
	fn switching_notice_contains_both_versions() {
		let n = switching_notice("1.0.0", "1.2.0");
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
		assert_eq!(resolve_theme(Some("light"), Some("15;0")), Theme::Light);
		assert_eq!(resolve_theme(Some("dark"), Some("0;15")), Theme::Dark);
		// Case-insensitive and whitespace-tolerant.
		assert_eq!(resolve_theme(Some(" LIGHT "), None), Theme::Light);
	}

	#[test]
	fn theme_reads_colorfgbg_background_field() {
		// Trailing field is the background: 15/7 → light, else dark.
		assert_eq!(resolve_theme(None, Some("0;15")), Theme::Light);
		assert_eq!(resolve_theme(None, Some("0;7")), Theme::Light);
		assert_eq!(resolve_theme(None, Some("15;0")), Theme::Dark);
		// The three-field "fg;default;bg" form still reads the last field.
		assert_eq!(resolve_theme(None, Some("15;default;0")), Theme::Dark);
		assert_eq!(resolve_theme(None, Some("0;default;15")), Theme::Light);
	}

	#[test]
	fn theme_defaults_to_dark_without_signal() {
		assert_eq!(resolve_theme(None, None), Theme::Dark);
		// Unparseable / bogus values fall through to the dark default.
		assert_eq!(resolve_theme(Some("teal"), None), Theme::Dark);
		assert_eq!(resolve_theme(None, Some("nonsense")), Theme::Dark);
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
	fn full_cache_needs_every_task_cached_and_none_failed() {
		assert!(is_full_cache(6, 6, 0));
		assert!(is_full_cache(1, 1, 0));
		// One task actually ran.
		assert!(!is_full_cache(6, 5, 0));
		// Everything cached but something still failed.
		assert!(!is_full_cache(6, 6, 1));
		// Nothing scheduled: no work was skipped, so nothing to celebrate.
		assert!(!is_full_cache(0, 0, 0));
	}

	#[test]
	fn full_cache_banner_carries_the_phrase() {
		let b = full_cache_banner();
		assert!(b.contains("FULL CACHE"));
		assert_eq!(b.matches(ROSETTE).count(), 3);
	}

	#[test]
	fn ramp_hits_each_stop_exactly() {
		assert_eq!(ramp_at(&TEAL_RAMP, 0.0), TEAL_700);
		assert_eq!(ramp_at(&TEAL_RAMP, 0.5), TEAL);
		assert_eq!(ramp_at(&TEAL_RAMP, 1.0), TEAL_300);
		// Out-of-range t clamps to the ends rather than extrapolating.
		assert_eq!(ramp_at(&TEAL_RAMP, -1.0), TEAL_700);
		assert_eq!(ramp_at(&TEAL_RAMP, 2.0), TEAL_300);
	}

	#[test]
	fn ramp_interpolates_between_stops() {
		// Quarter of the way is halfway through the first segment.
		let (r, g, b) = ramp_at(&TEAL_RAMP, 0.25);
		assert_eq!(
			(r, g, b),
			(
				(TEAL_700.0 as u16 + TEAL.0 as u16).div_ceil(2) as u8,
				(TEAL_700.1 as u16 + TEAL.1 as u16).div_ceil(2) as u8,
				(TEAL_700.2 as u16 + TEAL.2 as u16).div_ceil(2) as u8,
			)
		);
	}

	#[test]
	fn gradient_emits_no_escapes_when_color_is_off() {
		// Tests run without a TTY, so `colors_enabled()` is false here and the
		// banner has to degrade to bare text a log can carry.
		assert_eq!(paint_gradient("FULL CACHE", &TEAL_RAMP), "FULL CACHE");
		assert!(!full_cache_banner().contains('\u{1b}'));
	}

	#[test]
	fn gradient_walks_the_ramp_end_to_end() {
		let painted = gradient_escapes("ab", &TEAL_RAMP);
		// Two chars: first takes the dark end, second the light end.
		assert_eq!(
			painted,
			format!(
				"\u{1b}[38;2;{};{};{}ma\u{1b}[38;2;{};{};{}mb\u{1b}[39m",
				TEAL_700.0, TEAL_700.1, TEAL_700.2, TEAL_300.0, TEAL_300.1, TEAL_300.2
			)
		);
	}

	#[test]
	fn gradient_leaves_whitespace_unpainted_and_resets_once() {
		// The space rides the preceding color instead of getting an escape of its
		// own, and the whole string resets exactly once at the end.
		assert_eq!(
			gradient_escapes("a b", &TEAL_RAMP),
			format!(
				"\u{1b}[38;2;{};{};{}ma \u{1b}[38;2;{};{};{}mb\u{1b}[39m",
				TEAL_700.0, TEAL_700.1, TEAL_700.2, TEAL_300.0, TEAL_300.1, TEAL_300.2
			)
		);
	}

	#[test]
	fn gradient_handles_a_single_character() {
		// span guards against a divide-by-zero on a one-char string.
		let painted = gradient_escapes("x", &TEAL_RAMP);
		assert!(painted.contains('x'));
		assert_eq!(painted.matches("\u{1b}[38;2;").count(), 1);
	}

	#[test]
	fn label_palette_is_evenly_weighted() {
		// One hue step apart at a fixed saturation and lightness: every entry
		// spans the same distance between its brightest and dimmest channel, so
		// no label reads as louder than another.
		for c in LABEL_PALETTE {
			let (max, min) = (c.0.max(c.1).max(c.2), c.0.min(c.1).min(c.2));
			assert_eq!(max - min, 142, "{c:?} is off the ramp");
		}
		// Nothing in the palette is the red a `FAILED` marker uses.
		for c in LABEL_PALETTE {
			assert!(
				!(c.0 > 0xC0 && c.1 < 0x60 && c.2 < 0x60),
				"{c:?} reads as red"
			);
		}
	}

	#[test]
	fn labels_get_distinct_colors_until_the_palette_wraps() {
		let colors = LabelColors::new();
		let assigned: Vec<_> = (0..LABEL_PALETTE.len())
			.map(|i| colors.color(&format!("ws{i}"), "build"))
			.collect();
		let mut unique = assigned.clone();
		unique.sort();
		unique.dedup();
		assert_eq!(unique.len(), LABEL_PALETTE.len());
		// The ninth label wraps back to the first color.
		assert_eq!(colors.color("ws8", "build"), assigned[0]);
	}

	#[test]
	fn a_label_keeps_its_color_for_the_whole_run() {
		let colors = LabelColors::new();
		let first = colors.color("web", "build");
		let _ = colors.color("api", "build");
		assert_eq!(colors.color("web", "build"), first);
	}

	#[test]
	fn changing_either_the_scope_or_the_task_changes_the_color() {
		let colors = LabelColors::new();
		let web_build = colors.color("web", "build");
		let web_test = colors.color("web", "test");
		let api_build = colors.color("api", "build");
		assert_ne!(web_build, web_test);
		assert_ne!(web_build, api_build);
		assert_ne!(web_test, api_build);
	}

	#[test]
	fn label_prints_bare_when_color_is_off() {
		// Tests run without a TTY, so a piped or CI run gets exactly the text it
		// got before labels were colored.
		let colors = LabelColors::new();
		assert_eq!(colors.paint("web", "build"), "web:build");
		assert!(!colors.paint("web", "build").contains('\u{1b}'));
	}

	#[test]
	fn label_escapes_wrap_the_label_once() {
		let (r, g, b) = LABEL_PALETTE[0];
		assert_eq!(
			label_escapes("web:build", LABEL_PALETTE[0]),
			format!("\u{1b}[38;2;{r};{g};{b}mweb:build\u{1b}[39m")
		);
	}

	#[test]
	fn splash_contains_wordmark_and_version() {
		for theme in [Theme::Light, Theme::Dark] {
			let s = splash("9.9.9", theme);
			assert!(s.contains("lattice"));
			assert!(s.contains("9.9.9"));
		}
	}
}
