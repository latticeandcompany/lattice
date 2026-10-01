//! The live display a run gets on a terminal.
//!
//! Finished tasks settle into permanent lines, one each, and scroll away like
//! any other output. Below them a live region redraws in place: every running
//! task with its elapsed time and the last few lines it printed, the next tasks
//! still waiting and what they wait on, and a progress count. A failure prints
//! its whole output under its line, so nothing that matters is left in the part
//! of the screen that gets redrawn.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use console::{measure_text_width, strip_ansi_codes, style, truncate_str, Term};
use indicatif::TermLike;

use crate::{
	exit_desc, fail_detail, fmt_secs, fmt_span, full_cache_banner, is_full_cache, paint_teal,
	wordmark, PlannedTask, Reporter, RunSummary, TaskEvent, ROSETTE,
};

const TAIL_LINES: usize = 3;
const WAITING_SHOWN: usize = 3;
const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const TICK: Duration = Duration::from_millis(100);

enum State {
	Waiting {
		on: Vec<String>,
	},
	Running {
		since: Instant,
		tail: VecDeque<String>,
	},
	Settled,
}

struct Row {
	label: String,
	state: State,
	miss: Option<String>,
}

struct Live {
	term: Box<dyn TermLike>,
	verbose: bool,
	rows: Vec<Row>,
	index: HashMap<(String, String), usize>,
	/// Whether labels carry the task name, which they only need when a run
	/// covers more than one task.
	show_task: bool,
	label_width: usize,
	drawn: usize,
	frame: usize,
	settled: usize,
	cached: usize,
	failed: usize,
	scope: Option<String>,
}

impl Live {
	fn label_for(&self, workspace: &str, task: &str) -> String {
		if self.show_task {
			format!("{workspace}:{task}")
		} else {
			workspace.to_string()
		}
	}

	fn row(&mut self, workspace: &str, task: &str) -> usize {
		let key = (workspace.to_string(), task.to_string());
		if let Some(&i) = self.index.get(&key) {
			return i;
		}
		let label = self.label_for(workspace, task);
		self.label_width = self.label_width.max(measure_text_width(&label)).min(40);
		self.rows.push(Row {
			label,
			state: State::Waiting { on: Vec::new() },
			miss: None,
		});
		self.index.insert(key, self.rows.len() - 1);
		self.rows.len() - 1
	}

	fn padded(&self, label: &str) -> String {
		let shown = truncate_str(label, self.label_width, "…");
		let pad = self.label_width.saturating_sub(measure_text_width(&shown));
		format!("{shown}{}", " ".repeat(pad))
	}

	fn settle(&mut self, workspace: &str, task: &str, glyph: String, detail: String) {
		let i = self.row(workspace, task);
		if matches!(self.rows[i].state, State::Settled) {
			return;
		}
		self.rows[i].state = State::Settled;
		self.settled += 1;
		let mut line = format!("  {glyph} {}  {detail}", self.padded(&self.rows[i].label));
		if self.verbose {
			if let Some(miss) = &self.rows[i].miss {
				line.push_str(&format!("  {}", style(miss).dim()));
			}
		}
		self.paint(vec![line]);
	}

	/// The live region, never taller than the terminal: a frame that scrolls
	/// cannot be redrawn in place. Running rows come first, then waiting rows,
	/// then output tails with whatever room is left.
	fn frame_lines(&self) -> Vec<String> {
		let width = self.term.width().max(20) as usize;
		let mut room = (self.term.height().max(8) as usize).saturating_sub(2);
		let spinner = paint_teal(SPINNER[self.frame % SPINNER.len()]);
		let mut lines = Vec::new();

		let running: Vec<&Row> = self
			.rows
			.iter()
			.filter(|r| matches!(r.state, State::Running { .. }))
			.collect();
		let waiting: Vec<&Row> = self
			.rows
			.iter()
			.filter(|r| matches!(r.state, State::Waiting { .. }))
			.collect();
		if !self.rows.is_empty() {
			room = room.saturating_sub(2);
		}
		let (running_shown, more_running) = fit(running.len(), usize::MAX, room);
		room = room.saturating_sub(running_shown + usize::from(more_running));
		let (waiting_shown, more_waiting) = fit(waiting.len(), WAITING_SHOWN, room);
		room = room.saturating_sub(waiting_shown + usize::from(more_waiting));
		let tail_room = room / running_shown.max(1);

		for row in running.iter().take(running_shown) {
			let State::Running { since, tail } = &row.state else {
				continue;
			};
			lines.push(format!(
				"  {spinner} {}  {}",
				self.padded(&row.label),
				style(fmt_secs(since.elapsed().as_millis() as u64)).dim()
			));
			let keep = tail.len().min(TAIL_LINES).min(tail_room);
			for out in tail.iter().skip(tail.len() - keep) {
				lines.push(format!("    {} {}", style("│").dim(), style(out).dim()));
			}
		}

		if more_running {
			lines.push(format!(
				"  {}",
				style(format!("+{} more running", running.len() - running_shown)).dim()
			));
		}

		for row in waiting.iter().take(waiting_shown) {
			let State::Waiting { on } = &row.state else {
				continue;
			};
			let pending: Vec<&str> = on
				.iter()
				.filter(|dep| {
					self.index
						.iter()
						.find(|((w, t), _)| format!("{w}:{t}") == **dep)
						.is_some_and(|(_, &i)| !matches!(self.rows[i].state, State::Settled))
				})
				.map(|dep| self.short(dep))
				.collect();
			let why = if pending.is_empty() {
				"queued".to_string()
			} else {
				format!("waiting on {}", pending.join(", "))
			};
			lines.push(format!(
				"  {} {}  {}",
				style("·").dim(),
				style(self.padded(&row.label)).dim(),
				style(why).dim()
			));
		}
		if more_waiting {
			lines.push(format!(
				"  {}",
				style(format!("+{} more waiting", waiting.len() - waiting_shown)).dim()
			));
		}

		if !self.rows.is_empty() {
			let mut progress = format!("{}/{}", self.settled, self.rows.len());
			if self.cached > 0 {
				progress.push_str(&format!(" · {} cached", self.cached));
			}
			if self.failed > 0 {
				progress.push_str(&format!(
					" · {}",
					style(format!("{} failed", self.failed)).red()
				));
			}
			lines.push(String::new());
			lines.push(format!("  {}", style(progress).dim()));
		}

		lines
			.into_iter()
			.map(|l| truncate_str(&l, width.saturating_sub(1), "…").into_owned())
			.collect()
	}

	/// A dependency's label as the rows show it.
	fn short<'a>(&self, dep: &'a str) -> &'a str {
		if self.show_task {
			dep
		} else {
			dep.rsplit_once(':').map(|(ws, _)| ws).unwrap_or(dep)
		}
	}

	/// Print `permanent` above the live region and redraw the region under it.
	fn paint(&mut self, permanent: Vec<String>) {
		let frame = self.frame_lines();
		self.draw(permanent, frame);
	}

	fn draw(&mut self, permanent: Vec<String>, frame: Vec<String>) {
		let term = &self.term;
		if self.drawn > 0 {
			let _ = term.move_cursor_up(self.drawn);
		}
		let written = permanent.len() + frame.len();
		for line in permanent.iter().chain(frame.iter()) {
			let _ = term.clear_line();
			let _ = term.write_line(line);
		}
		let leftover = self.drawn.saturating_sub(written);
		for _ in 0..leftover {
			let _ = term.clear_line();
			let _ = term.write_line("");
		}
		if leftover > 0 {
			let _ = term.move_cursor_up(leftover);
		}
		let _ = term.flush();
		self.drawn = frame.len();
	}
}

/// How many of `n` rows to show, at most `cap`, in `room` lines, and whether a
/// "+N more" line follows them. That line takes a row's place when they do not fit.
fn fit(n: usize, cap: usize, room: usize) -> (usize, bool) {
	let mut shown = n.min(cap).min(room);
	if shown < n && shown == room {
		shown = room.saturating_sub(1);
	}
	(shown, shown < n && room > 0)
}

/// The reporter for a run on a terminal. See the module docs for what it draws.
pub struct InteractiveReporter {
	live: Arc<Mutex<Live>>,
	stop: Arc<AtomicBool>,
	/// Whether to animate at all. A `--sequentially` run finishes each phase, which
	/// stops the ticker, so the next phase's `run_start` starts it again.
	tick: bool,
	ticker: Mutex<Option<JoinHandle<()>>>,
}

impl InteractiveReporter {
	pub fn new(verbose: bool) -> Self {
		Self::on(Box::new(Term::stderr()), verbose, true)
	}

	/// Draw into `term`. `tick` animates the spinners; a test turns it off to
	/// read a screen that is not changing under it.
	pub(crate) fn on(term: Box<dyn TermLike>, verbose: bool, tick: bool) -> Self {
		let live = Arc::new(Mutex::new(Live {
			term,
			verbose,
			rows: Vec::new(),
			index: HashMap::new(),
			show_task: false,
			label_width: 0,
			drawn: 0,
			frame: 0,
			settled: 0,
			cached: 0,
			failed: 0,
			scope: None,
		}));
		let reporter = Self {
			live,
			stop: Arc::new(AtomicBool::new(false)),
			tick,
			ticker: Mutex::new(None),
		};
		reporter.start_ticker();
		reporter
	}

	fn start_ticker(&self) {
		let mut ticker = self.ticker.lock().unwrap();
		if !self.tick || ticker.is_some() {
			return;
		}
		self.stop.store(false, Ordering::Relaxed);
		let (live, stop) = (self.live.clone(), self.stop.clone());
		*ticker = Some(std::thread::spawn(move || {
			while !stop.load(Ordering::Relaxed) {
				std::thread::sleep(TICK);
				let mut live = live.lock().unwrap();
				if live
					.rows
					.iter()
					.any(|r| matches!(r.state, State::Running { .. }))
				{
					live.frame += 1;
					live.paint(Vec::new());
				}
			}
		}));
	}
}

impl Reporter for InteractiveReporter {
	fn run_start(&self, task: &str, workspaces: usize) {
		self.start_ticker();
		let mut live = self.live.lock().unwrap();
		let mut head = format!(
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
		if let Some(scope) = &live.scope {
			head.push_str(&format!(
				"  {}",
				style(format!("· in {scope}, the workspace you are in")).dim()
			));
		}
		live.show_task = task.contains('+');
		live.paint(vec![head, String::new()]);
	}

	fn plan(&self, tasks: &[PlannedTask]) {
		let mut live = self.live.lock().unwrap();
		let names: std::collections::HashSet<&str> =
			tasks.iter().map(|t| t.task.as_str()).collect();
		live.show_task = names.len() > 1;
		for t in tasks {
			let i = live.row(&t.workspace, &t.task);
			live.rows[i].state = State::Waiting {
				on: t.waits_on.clone(),
			};
		}
		live.paint(Vec::new());
	}

	fn scope(&self, workspace: &str) {
		self.live.lock().unwrap().scope = Some(workspace.to_string());
	}

	fn event(&self, ev: TaskEvent) {
		let mut live = self.live.lock().unwrap();
		match ev {
			TaskEvent::Started { workspace, task } => {
				let i = live.row(&workspace, &task);
				live.rows[i].state = State::Running {
					since: Instant::now(),
					tail: VecDeque::new(),
				};
				live.paint(Vec::new());
			}
			TaskEvent::Output {
				workspace,
				task,
				line,
				..
			} => {
				let i = live.row(&workspace, &task);
				if let State::Running { tail, .. } = &mut live.rows[i].state {
					let clean = strip_ansi_codes(&line);
					let last = clean.rsplit('\r').next().unwrap_or("").trim_end();
					if !last.trim().is_empty() {
						tail.push_back(last.replace('\t', "  "));
						if tail.len() > TAIL_LINES {
							tail.pop_front();
						}
					}
				}
			}
			TaskEvent::CacheMiss {
				workspace,
				task,
				miss,
			} => {
				let i = live.row(&workspace, &task);
				live.rows[i].miss = Some(miss.describe());
			}
			TaskEvent::CacheHit {
				workspace,
				task,
				saved_ms,
				..
			} => {
				live.cached += 1;
				let detail = if saved_ms > 0 {
					format!(
						"{} {}",
						paint_teal("cached"),
						style(format!("· {} saved", fmt_span(saved_ms))).dim()
					)
				} else {
					paint_teal("cached")
				};
				live.settle(&workspace, &task, paint_teal("●"), detail);
			}
			TaskEvent::Finished {
				workspace,
				task,
				duration_ms,
			} => {
				let glyph = style("✓").green().bold().to_string();
				let detail = style(fmt_secs(duration_ms)).dim().to_string();
				live.settle(&workspace, &task, glyph, detail);
			}
			TaskEvent::Failed {
				workspace,
				task,
				code,
				duration_ms,
			} => {
				live.failed += 1;
				let (desc, secs) = fail_detail(code, duration_ms);
				let mut detail = style(match desc {
					Some(desc) => format!("failed ({desc})"),
					None => "failed".to_string(),
				})
				.red()
				.to_string();
				if let Some(secs) = secs {
					detail.push_str(&format!(" {}", style(secs).dim()));
				}
				let glyph = style("✗").red().bold().to_string();
				live.settle(&workspace, &task, glyph, detail);
			}
			TaskEvent::PersistentExited {
				workspace,
				task,
				code,
				duration_ms,
			} => {
				let (glyph, detail) = if code == Some(0) {
					(
						style("○").dim().to_string(),
						style(format!("exited ({})", exit_desc(code)))
							.dim()
							.to_string(),
					)
				} else {
					live.failed += 1;
					(
						style("✗").red().bold().to_string(),
						style(format!("exited ({})", exit_desc(code)))
							.red()
							.to_string(),
					)
				};
				let detail = format!("{detail} {}", style(fmt_secs(duration_ms)).dim());
				live.settle(&workspace, &task, glyph, detail);
			}
			TaskEvent::Skipped {
				workspace,
				task,
				reason,
			} => {
				let glyph = style("○").dim().to_string();
				let detail = style(format!("skipped: {reason}")).dim().to_string();
				live.settle(&workspace, &task, glyph, detail);
			}
		}
	}

	fn surface_failure(&self, _workspace: &str, _task: &str, captured: &[(bool, String)]) {
		let mut live = self.live.lock().unwrap();
		let bar = style("│").red().to_string();
		let mut lines: Vec<String> = captured
			.iter()
			.map(|(_, line)| format!("    {bar} {}", strip_ansi_codes(line).trim_end()))
			.collect();
		if lines.is_empty() {
			lines.push(format!("    {bar} {}", style("(no output)").dim()));
		}
		lines.push(String::new());
		live.paint(lines);
	}

	fn run_summary(&self, s: RunSummary) {
		let mut live = self.live.lock().unwrap();
		let mut parts = vec![format!("{} tasks", style(s.total).bold())];
		if s.cached > 0 {
			parts.push(format!("{} cached", s.cached));
		}
		if s.failed > 0 {
			parts.push(style(format!("{} failed", s.failed)).red().to_string());
		}
		let saved = if s.saved_ms > 0 {
			format!(
				" {} {}",
				style("·").dim(),
				paint_teal(&format!("{} saved", fmt_span(s.saved_ms)))
			)
		} else {
			String::new()
		};
		let mut lines = vec![
			String::new(),
			format!(
				"{}  {}  {}{}",
				paint_teal(ROSETTE),
				parts.join(&style(" · ").dim().to_string()),
				style(fmt_secs(s.elapsed_ms)).dim(),
				saved
			),
		];
		if is_full_cache(s.total, s.cached, s.failed) {
			lines.push(String::new());
			lines.push(full_cache_banner());
		}
		live.rows.clear();
		live.index.clear();
		live.settled = 0;
		live.cached = 0;
		live.failed = 0;
		live.draw(lines, Vec::new());
	}

	fn note(&self, msg: &str) {
		let mut live = self.live.lock().unwrap();
		live.paint(vec![format!("  {}", style(format!("· {msg}")).dim())]);
	}

	fn task_note(&self, workspace: &str, task: &str, msg: &str) {
		let mut live = self.live.lock().unwrap();
		if live.verbose {
			let label = live.label_for(workspace, task);
			live.paint(vec![format!(
				"  {}",
				style(format!("· {label}: {msg}")).dim()
			)]);
		}
	}

	fn warn(&self, msg: &str) {
		let mut live = self.live.lock().unwrap();
		live.paint(vec![format!("  {} {msg}", style("!").yellow().bold())]);
	}

	fn task_warn(&self, workspace: &str, task: &str, msg: &str) {
		let mut live = self.live.lock().unwrap();
		let label = live.label_for(workspace, task);
		live.paint(vec![format!(
			"  {} {label}: {msg}",
			style("!").yellow().bold()
		)]);
	}

	fn finish(&self) {
		self.stop.store(true, Ordering::Relaxed);
		if let Some(handle) = self.ticker.lock().unwrap().take() {
			let _ = handle.join();
		}
		let mut live = self.live.lock().unwrap();
		live.draw(Vec::new(), Vec::new());
	}
}

impl Drop for InteractiveReporter {
	fn drop(&mut self) {
		self.stop.store(true, Ordering::Relaxed);
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use indicatif::InMemoryTerm;

	fn planned(ws: &str, task: &str, waits_on: &[&str]) -> PlannedTask {
		PlannedTask {
			workspace: ws.into(),
			task: task.into(),
			waits_on: waits_on.iter().map(|s| s.to_string()).collect(),
		}
	}

	fn screen(drive: impl FnOnce(&InteractiveReporter)) -> String {
		console::set_colors_enabled(false);
		console::set_colors_enabled_stderr(false);
		let term = InMemoryTerm::new(30, 100);
		let reporter = InteractiveReporter::on(Box::new(term.clone()), false, false);
		drive(&reporter);
		term.contents()
	}

	#[test]
	fn a_running_task_shows_the_tail_of_its_output_and_waiting_tasks_say_why() {
		let out = screen(|r| {
			r.run_start("build", 3);
			r.plan(&[
				planned("core", "build", &[]),
				planned("api", "build", &["core:build"]),
				planned("web", "build", &["api:build"]),
			]);
			r.event(TaskEvent::Started {
				workspace: "core".into(),
				task: "build".into(),
			});
			for line in ["one", "two", "three", "four"] {
				r.event(TaskEvent::Output {
					workspace: "core".into(),
					task: "build".into(),
					line: line.into(),
					stderr: false,
					persistent: false,
				});
			}
		});
		assert!(out.contains("lattice  build  · 3 workspaces"), "{out}");
		assert!(out.contains("api   waiting on core"), "{out}");
		assert!(out.contains("0/3"), "{out}");
	}

	#[test]
	fn the_tail_keeps_the_last_lines_only() {
		let out = screen(|r| {
			r.run_start("build", 1);
			r.plan(&[planned("core", "build", &[])]);
			r.event(TaskEvent::Started {
				workspace: "core".into(),
				task: "build".into(),
			});
			for line in ["one", "two", "three", "four"] {
				r.event(TaskEvent::Output {
					workspace: "core".into(),
					task: "build".into(),
					line: line.into(),
					stderr: false,
					persistent: false,
				});
			}
			r.event(TaskEvent::CacheMiss {
				workspace: "core".into(),
				task: "build".into(),
				miss: crate::CacheMiss::FirstRun,
			});
			let mut live = r.live.lock().unwrap();
			live.paint(Vec::new());
		});
		assert!(out.contains("│ two") && out.contains("│ four"), "{out}");
		assert!(!out.contains("│ one"), "{out}");
	}

	#[test]
	fn finished_tasks_settle_and_the_summary_replaces_the_live_region() {
		let out = screen(|r| {
			r.run_start("build", 2);
			r.plan(&[
				planned("core", "build", &[]),
				planned("web", "build", &["core:build"]),
			]);
			r.event(TaskEvent::CacheHit {
				workspace: "core".into(),
				task: "build".into(),
				key: "abc".into(),
				saved_ms: 2_000,
			});
			r.event(TaskEvent::Started {
				workspace: "web".into(),
				task: "build".into(),
			});
			r.event(TaskEvent::Failed {
				workspace: "web".into(),
				task: "build".into(),
				code: Some(2),
				duration_ms: Some(1_200),
			});
			r.surface_failure("web", "build", &[(true, "error: nope".into())]);
			r.run_summary(RunSummary {
				total: 2,
				cached: 1,
				failed: 1,
				elapsed_ms: 1_300,
				saved_ms: 2_000,
			});
			r.finish();
		});
		assert!(out.contains("● core  cached · 2.00s saved"), "{out}");
		assert!(out.contains("✗ web   failed (code 2) 1.20s"), "{out}");
		assert!(out.contains("│ error: nope"), "{out}");
		assert!(out.contains("2 tasks · 1 cached · 1 failed"), "{out}");
		assert!(
			!out.contains("waiting on"),
			"the live region is gone: {out}"
		);
		assert!(!out.contains("1/2"), "{out}");
	}

	#[test]
	fn several_tasks_label_rows_with_the_task_name() {
		let out = screen(|r| {
			r.run_start("build+test", 1);
			r.plan(&[
				planned("core", "build", &[]),
				planned("core", "test", &["core:build"]),
			]);
		});
		assert!(out.contains("core:test   waiting on core:build"), "{out}");
	}

	#[test]
	fn a_scoped_run_says_where_it_is() {
		let out = screen(|r| {
			r.scope("web");
			r.run_start("build", 2);
		});
		assert!(out.contains("in web, the workspace you are in"), "{out}");
	}

	#[test]
	fn a_full_cache_run_ends_on_the_banner() {
		let out = screen(|r| {
			r.run_start("build", 1);
			r.run_summary(RunSummary {
				total: 1,
				cached: 1,
				failed: 0,
				elapsed_ms: 20,
				saved_ms: 1_000,
			});
		});
		assert!(out.contains("FULL POWER"), "{out}");
	}
	#[test]
	fn the_live_region_never_outgrows_the_terminal() {
		console::set_colors_enabled_stderr(false);
		let term = InMemoryTerm::new(10, 100);
		let r = InteractiveReporter::on(Box::new(term), false, false);
		r.run_start("build", 12);
		let tasks: Vec<PlannedTask> = (0..12)
			.map(|i| planned(&format!("ws{i}"), "build", &[]))
			.collect();
		r.plan(&tasks);
		for i in 0..12 {
			r.event(TaskEvent::Started {
				workspace: format!("ws{i}"),
				task: "build".into(),
			});
		}
		let frame = r.live.lock().unwrap().frame_lines();
		assert!(frame.len() <= 8, "{} lines: {frame:#?}", frame.len());
		assert!(
			frame.iter().any(|l| l.contains("more running")),
			"{frame:#?}"
		);
	}

	#[test]
	fn a_second_phase_counts_from_zero_and_animates_again() {
		console::set_colors_enabled_stderr(false);
		let term = InMemoryTerm::new(30, 100);
		let r = InteractiveReporter::on(Box::new(term), false, true);
		r.run_start("build", 2);
		r.plan(&[planned("core", "build", &[]), planned("web", "build", &[])]);
		for ws in ["core", "web"] {
			r.event(TaskEvent::Finished {
				workspace: ws.into(),
				task: "build".into(),
				duration_ms: 1,
			});
		}
		r.run_summary(RunSummary {
			total: 2,
			cached: 0,
			failed: 0,
			elapsed_ms: 1,
			saved_ms: 0,
		});
		r.finish();
		assert!(r.ticker.lock().unwrap().is_none());

		r.run_start("test", 1);
		r.plan(&[planned("core", "test", &[])]);
		assert!(
			r.ticker.lock().unwrap().is_some(),
			"the next phase animates"
		);
		let frame = r.live.lock().unwrap().frame_lines();
		assert!(frame.iter().any(|l| l.contains("0/1")), "{frame:#?}");
		r.finish();
	}
}
