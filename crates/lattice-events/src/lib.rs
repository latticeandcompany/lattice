//! What a run reports, and the trait that consumes it.
//!
//! The runner emits [`TaskEvent`]s and calls [`Reporter`] hooks; it never decides
//! how any of it is displayed. Both halves live here rather than beside the
//! terminal renderers so that a front end can consume a run without linking a
//! terminal stack: this crate depends on `serde` and nothing else.
//!
//! Every type here is `Serialize` because a front end outside the process is a
//! first-class consumer. Field names cross the wire in `camelCase`, and enums are
//! internally tagged so a JavaScript consumer gets a discriminated union rather
//! than a wrapper object.

use serde::Serialize;

/// One line of a task's captured output.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputLine {
	pub stderr: bool,
	pub line: String,
}

/// Why a task did not come back from the cache.
///
/// A key is one hash: on its own it can say a task missed, never what moved. The
/// components behind it can, measured against what the task resolved to the last
/// time it ran.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CacheMiss {
	/// Nothing has ever been cached for this task.
	FirstRun,
	/// The key matches what last ran, so the entry itself is gone: evicted by a
	/// prune, or dropped as corrupt on the way in.
	EntryEvicted,
	/// Component names, in the order the cache composes them.
	Changed { components: Vec<String> },
}

impl CacheMiss {
	/// The sentence a terminal prints for this miss. Kept here so the reporters
	/// and any other front end share one wording.
	pub fn describe(&self) -> String {
		match self {
			Self::FirstRun => "cache miss (nothing cached for this task yet)".to_string(),
			Self::EntryEvicted => {
				"cache miss (the entry for this key is no longer in the cache)".to_string()
			}
			Self::Changed { components } => {
				format!("cache miss: {} changed", components.join(", "))
			}
		}
	}
}

/// Typed events the runner emits. Reporter impls decide how to render them.
#[derive(Debug, Clone, Serialize)]
#[serde(
	tag = "type",
	rename_all = "camelCase",
	rename_all_fields = "camelCase"
)]
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
	CacheMiss {
		workspace: String,
		task: String,
		miss: CacheMiss,
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
	/// A task failed. `code` is the command's exit code, `None` when a signal
	/// ended it or when the task failed before a child ever ran. `duration_ms` is
	/// `None` in that second case, because a task that never started has no run
	/// to time. It is not `0`: a command can fail inside a millisecond, and the
	/// two have to stay tellable apart.
	Failed {
		workspace: String,
		task: String,
		code: Option<i32>,
		duration_ms: Option<u64>,
	},
	/// A persistent task's process ended without being asked to. `code` is its
	/// exit code, or `None` when a signal ended it. Anything but `Some(0)` also
	/// counts as a run failure.
	PersistentExited {
		workspace: String,
		task: String,
		code: Option<i32>,
		duration_ms: u64,
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
	/// A [`Reporter::note`] about one specific task. Rendered as
	/// `workspace:task: msg`; a reporter that can label a line overrides it.
	fn task_note(&self, workspace: &str, task: &str, msg: &str) {
		self.note(&format!("{workspace}:{task}: {msg}"));
	}
	/// A [`Reporter::warn`] about one specific task, labeled the same way.
	fn task_warn(&self, workspace: &str, task: &str, msg: &str) {
		self.warn(&format!("{workspace}:{task}: {msg}"));
	}
	/// Called once at the end so the interactive impl can clear its progress surface.
	fn finish(&self);
}

#[cfg(test)]
mod tests {
	use super::*;
	use serde_json::json;

	// These assertions are the wire contract the desktop app codes against. A
	// rename that reaches the front end has to change an expected shape here
	// first, which puts the TypeScript types in the same diff.

	#[test]
	fn started_carries_a_camel_case_tag() {
		assert_eq!(
			serde_json::to_value(TaskEvent::Started {
				workspace: "web".into(),
				task: "build".into(),
			})
			.unwrap(),
			json!({ "type": "started", "workspace": "web", "task": "build" })
		);
	}

	#[test]
	fn finished_renames_duration_to_camel_case() {
		assert_eq!(
			serde_json::to_value(TaskEvent::Finished {
				workspace: "web".into(),
				task: "build".into(),
				duration_ms: 210,
			})
			.unwrap(),
			json!({ "type": "finished", "workspace": "web", "task": "build", "durationMs": 210 })
		);
	}

	#[test]
	fn a_signal_death_serializes_its_code_as_null() {
		// The key has to be present: a front end reads `null` as "killed by a
		// signal", which it cannot distinguish from a missing field.
		let value = serde_json::to_value(TaskEvent::PersistentExited {
			workspace: "web".into(),
			task: "dev".into(),
			code: None,
			duration_ms: 9000,
		})
		.unwrap();
		assert_eq!(value["code"], json!(null));
		assert!(value.as_object().unwrap().contains_key("code"));
	}

	#[test]
	fn a_failure_carries_its_exit_code_and_duration() {
		assert_eq!(
			serde_json::to_value(TaskEvent::Failed {
				workspace: "web".into(),
				task: "build".into(),
				code: Some(101),
				duration_ms: Some(1840),
			})
			.unwrap(),
			json!({
				"type": "failed", "workspace": "web", "task": "build",
				"code": 101, "durationMs": 1840
			})
		);
	}

	#[test]
	fn a_failure_before_the_command_ran_sends_both_fields_as_null() {
		// Both keys have to be present. A front end reads `null` as "there was
		// nothing to report", and a `durationMs` of `0` would instead claim a
		// task that ran and took no measurable time.
		let value = serde_json::to_value(TaskEvent::Failed {
			workspace: "web".into(),
			task: "build".into(),
			code: None,
			duration_ms: None,
		})
		.unwrap();
		assert_eq!(value["code"], json!(null));
		assert_eq!(value["durationMs"], json!(null));
		let keys = value.as_object().unwrap();
		assert!(keys.contains_key("code") && keys.contains_key("durationMs"));
	}

	#[test]
	fn output_keeps_both_of_its_flags() {
		assert_eq!(
			serde_json::to_value(TaskEvent::Output {
				workspace: "web".into(),
				task: "dev".into(),
				line: "listening".into(),
				stderr: false,
				persistent: true,
			})
			.unwrap(),
			json!({
				"type": "output", "workspace": "web", "task": "dev",
				"line": "listening", "stderr": false, "persistent": true
			})
		);
	}

	#[test]
	fn a_miss_nests_its_own_tag() {
		assert_eq!(
			serde_json::to_value(TaskEvent::CacheMiss {
				workspace: "web".into(),
				task: "build".into(),
				miss: CacheMiss::Changed {
					components: vec!["inputs".into(), "command".into()],
				},
			})
			.unwrap(),
			json!({
				"type": "cacheMiss", "workspace": "web", "task": "build",
				"miss": { "kind": "changed", "components": ["inputs", "command"] }
			})
		);
	}

	#[test]
	fn every_miss_has_the_wording_the_cli_has_always_printed() {
		assert_eq!(
			CacheMiss::FirstRun.describe(),
			"cache miss (nothing cached for this task yet)"
		);
		assert_eq!(
			CacheMiss::EntryEvicted.describe(),
			"cache miss (the entry for this key is no longer in the cache)"
		);
		assert_eq!(
			CacheMiss::Changed {
				components: vec!["inputs".into(), "manifests".into()],
			}
			.describe(),
			"cache miss: inputs, manifests changed"
		);
	}

	#[test]
	fn an_output_line_is_named_on_the_wire() {
		assert_eq!(
			serde_json::to_value(OutputLine {
				stderr: true,
				line: "boom".into(),
			})
			.unwrap(),
			json!({ "stderr": true, "line": "boom" })
		);
	}
}
