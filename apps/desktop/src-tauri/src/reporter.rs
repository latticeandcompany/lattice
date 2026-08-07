//! Turning `Reporter` calls into messages the window can read.
//!
//! Two things here are not incidental.
//!
//! Output is batched. A compiler emits tens of thousands of lines, and one IPC
//! message per line is the single easiest way to make this app feel bad. Lines
//! accumulate and flush on a count or a tick, whichever comes first, and always
//! before a summary so nothing arrives after the run is reported done.
//!
//! `task_note` and `task_warn` are overridden. Their default implementations fold
//! the label into the message text, which is right for a terminal and wrong here:
//! the window routes a message to a pane by its label, so it needs it separate.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use serde::Serialize;

use lattice_events::{OutputLine, Reporter, TaskEvent};
use lattice_runner::RunResult;

/// Flush after this many buffered lines.
const OUTPUT_BATCH: usize = 256;

/// Lines kept per task so a reloaded webview can redraw its pane. The runner
/// buffers more than this for failure surfacing; this is only what the window
/// shows live.
const LOG_TAIL: usize = 2000;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputChunk {
	pub workspace: String,
	pub task: String,
	pub stderr: bool,
	pub line: String,
}

/// One message on the run channel.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RunMessage {
	Event {
		event: TaskEvent,
	},
	OutputBatch {
		lines: Vec<OutputChunk>,
	},
	FailureOutput {
		workspace: String,
		task: String,
		lines: Vec<OutputLine>,
	},
	Note {
		workspace: Option<String>,
		task: Option<String>,
		message: String,
	},
	Warn {
		workspace: Option<String>,
		task: Option<String>,
		message: String,
	},
	Summary {
		result: RunResult,
	},
	Finished,
}

/// Where a reporter's messages go. A trait so the reporter can be tested against a
/// vector instead of a webview.
pub trait EventSink: Send + Sync + 'static {
	fn send(&self, message: RunMessage);
}

/// A bounded per-task tail of a run's output.
#[derive(Default)]
pub struct RunLog {
	tasks: std::collections::HashMap<String, Vec<OutputLine>>,
}

impl RunLog {
	pub fn push(&mut self, workspace: &str, task: &str, stderr: bool, line: &str) {
		let entry = self.tasks.entry(format!("{workspace}:{task}")).or_default();
		entry.push(OutputLine {
			stderr,
			line: line.to_string(),
		});
		if entry.len() > LOG_TAIL {
			let excess = entry.len() - LOG_TAIL;
			entry.drain(0..excess);
		}
	}

	pub fn lines(&self, workspace: &str, task: &str) -> Vec<OutputLine> {
		self.tasks
			.get(&format!("{workspace}:{task}"))
			.cloned()
			.unwrap_or_default()
	}
}

pub struct ChannelReporter<S: EventSink> {
	sink: S,
	pending: Mutex<Vec<OutputChunk>>,
	log: std::sync::Arc<Mutex<RunLog>>,
	flushes: AtomicU64,
}

impl<S: EventSink> ChannelReporter<S> {
	pub fn new(sink: S, log: std::sync::Arc<Mutex<RunLog>>) -> Self {
		Self {
			sink,
			pending: Mutex::new(Vec::new()),
			log,
			flushes: AtomicU64::new(0),
		}
	}

	/// Send whatever output has accumulated. Safe to call when there is none.
	pub fn flush(&self) {
		let batch = {
			let mut pending = self.pending.lock().unwrap();
			if pending.is_empty() {
				return;
			}
			std::mem::take(&mut *pending)
		};
		self.flushes.fetch_add(1, Ordering::SeqCst);
		self.sink.send(RunMessage::OutputBatch { lines: batch });
	}

	/// How many batches have been sent. Lets a test assert that output was
	/// coalesced rather than sent a line at a time.
	#[cfg(test)]
	pub fn flush_count(&self) -> u64 {
		self.flushes.load(Ordering::SeqCst)
	}
}

impl<S: EventSink> Reporter for ChannelReporter<S> {
	fn run_start(&self, _task: &str, _workspaces: usize) {
		// The window already knows what it asked for, and it draws the task list
		// from the graph rather than from a count.
	}

	fn event(&self, ev: TaskEvent) {
		if let TaskEvent::Output {
			workspace,
			task,
			line,
			stderr,
			..
		} = ev
		{
			self.log
				.lock()
				.unwrap()
				.push(&workspace, &task, stderr, &line);
			let full = {
				let mut pending = self.pending.lock().unwrap();
				pending.push(OutputChunk {
					workspace,
					task,
					stderr,
					line,
				});
				pending.len() >= OUTPUT_BATCH
			};
			if full {
				self.flush();
			}
			return;
		}
		// Anything that changes a task's state is worth arriving in order with
		// respect to its output, so pending lines go first.
		self.flush();
		self.sink.send(RunMessage::Event { event: ev });
	}

	fn surface_failure(&self, workspace: &str, task: &str, captured: &[(bool, String)]) {
		self.flush();
		self.sink.send(RunMessage::FailureOutput {
			workspace: workspace.to_string(),
			task: task.to_string(),
			lines: captured
				.iter()
				.map(|(stderr, line)| OutputLine {
					stderr: *stderr,
					line: line.clone(),
				})
				.collect(),
		});
	}

	fn run_summary(&self, total: usize, cached: usize, failed: usize, elapsed_ms: u64) {
		self.flush();
		self.sink.send(RunMessage::Summary {
			result: RunResult {
				total,
				cached,
				failed,
				elapsed_ms,
			},
		});
	}

	fn note(&self, msg: &str) {
		self.sink.send(RunMessage::Note {
			workspace: None,
			task: None,
			message: msg.to_string(),
		});
	}

	fn warn(&self, msg: &str) {
		self.sink.send(RunMessage::Warn {
			workspace: None,
			task: None,
			message: msg.to_string(),
		});
	}

	fn task_note(&self, workspace: &str, task: &str, msg: &str) {
		self.sink.send(RunMessage::Note {
			workspace: Some(workspace.to_string()),
			task: Some(task.to_string()),
			message: msg.to_string(),
		});
	}

	fn task_warn(&self, workspace: &str, task: &str, msg: &str) {
		self.sink.send(RunMessage::Warn {
			workspace: Some(workspace.to_string()),
			task: Some(task.to_string()),
			message: msg.to_string(),
		});
	}

	fn finish(&self) {
		self.flush();
		self.sink.send(RunMessage::Finished);
	}
}

#[cfg(test)]
mod tests {
	use std::sync::Arc;

	use super::*;

	#[derive(Default, Clone)]
	struct VecSink(Arc<Mutex<Vec<RunMessage>>>);

	impl EventSink for VecSink {
		fn send(&self, message: RunMessage) {
			self.0.lock().unwrap().push(message);
		}
	}

	fn reporter() -> (ChannelReporter<VecSink>, VecSink) {
		let sink = VecSink::default();
		let log = Arc::new(Mutex::new(RunLog::default()));
		(ChannelReporter::new(sink.clone(), log), sink)
	}

	fn output(i: usize) -> TaskEvent {
		TaskEvent::Output {
			workspace: "web".into(),
			task: "build".into(),
			line: format!("line {i}"),
			stderr: false,
			persistent: false,
		}
	}

	#[test]
	fn output_is_coalesced_rather_than_sent_a_line_at_a_time() {
		let (reporter, sink) = reporter();
		for i in 0..OUTPUT_BATCH {
			reporter.event(output(i));
		}
		assert_eq!(reporter.flush_count(), 1, "one batch, not {OUTPUT_BATCH}");

		let messages = sink.0.lock().unwrap();
		assert_eq!(messages.len(), 1);
		match &messages[0] {
			RunMessage::OutputBatch { lines } => assert_eq!(lines.len(), OUTPUT_BATCH),
			other => panic!("expected a batch, got {other:?}"),
		}
	}

	#[test]
	fn a_partial_batch_still_arrives_before_the_summary() {
		let (reporter, sink) = reporter();
		reporter.event(output(1));
		reporter.run_summary(1, 0, 0, 10);

		let messages = sink.0.lock().unwrap();
		let kinds: Vec<&str> = messages
			.iter()
			.map(|m| match m {
				RunMessage::OutputBatch { .. } => "output",
				RunMessage::Summary { .. } => "summary",
				_ => "other",
			})
			.collect();
		assert_eq!(
			kinds,
			vec!["output", "summary"],
			"a line must not arrive after the run is reported done"
		);
	}

	#[test]
	fn a_state_change_flushes_the_output_before_it() {
		let (reporter, sink) = reporter();
		reporter.event(output(1));
		reporter.event(TaskEvent::Finished {
			workspace: "web".into(),
			task: "build".into(),
			duration_ms: 5,
		});

		let messages = sink.0.lock().unwrap();
		assert!(matches!(messages[0], RunMessage::OutputBatch { .. }));
		assert!(matches!(messages[1], RunMessage::Event { .. }));
	}

	#[test]
	fn a_task_note_keeps_its_label_separate_from_its_message() {
		// The default trait impl folds them into one string, which a window cannot
		// route to a pane.
		let (reporter, sink) = reporter();
		reporter.task_note("web", "build", "cache miss: inputs changed");

		let messages = sink.0.lock().unwrap();
		match &messages[0] {
			RunMessage::Note {
				workspace,
				task,
				message,
			} => {
				assert_eq!(workspace.as_deref(), Some("web"));
				assert_eq!(task.as_deref(), Some("build"));
				assert_eq!(message, "cache miss: inputs changed");
			}
			other => panic!("expected a note, got {other:?}"),
		}
	}

	#[test]
	fn captured_failure_output_is_named_rather_than_a_tuple() {
		let (reporter, sink) = reporter();
		reporter.surface_failure("api", "test", &[(true, "boom".to_string())]);

		let messages = sink.0.lock().unwrap();
		match &messages[0] {
			RunMessage::FailureOutput { lines, .. } => {
				assert!(lines[0].stderr);
				assert_eq!(lines[0].line, "boom");
			}
			other => panic!("expected failure output, got {other:?}"),
		}
	}

	#[test]
	fn the_log_keeps_a_bounded_tail() {
		let mut log = RunLog::default();
		for i in 0..(LOG_TAIL + 500) {
			log.push("web", "build", false, &format!("line {i}"));
		}
		let lines = log.lines("web", "build");
		assert_eq!(lines.len(), LOG_TAIL);
		// The tail keeps the end, which is where a failure is.
		assert_eq!(
			lines.last().unwrap().line,
			format!("line {}", LOG_TAIL + 499)
		);
	}

	#[test]
	fn a_task_with_no_output_has_an_empty_log_rather_than_missing() {
		let log = RunLog::default();
		assert!(log.lines("web", "build").is_empty());
	}
}
