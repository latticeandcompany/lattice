//! What the backend holds between commands.
//!
//! The locking discipline is one rule: a guard is never held across an `await`.
//! Every command takes what it needs out of the mutex, drops the guard, and then
//! does the slow part. That is why these are `std::sync::Mutex` and not tokio's —
//! a lock that cannot be held across a suspension point does not need to be async.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use lattice_project::Project;

use crate::reporter::RunLog;

/// How many recent projects the switcher remembers.
const RECENT_LIMIT: usize = 10;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentProject {
	pub root: String,
	pub name: String,
	/// Unix milliseconds.
	pub opened_at: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Recents {
	pub projects: Vec<RecentProject>,
}

impl Recents {
	fn touch(&mut self, root: &Path) {
		let root_text = root.display().to_string();
		self.projects.retain(|p| p.root != root_text);
		self.projects.insert(
			0,
			RecentProject {
				name: root
					.file_name()
					.map(|n| n.to_string_lossy().to_string())
					.unwrap_or_else(|| root_text.clone()),
				root: root_text,
				opened_at: now_ms(),
			},
		);
		self.projects.truncate(RECENT_LIMIT);
	}

	fn forget(&mut self, root: &str) {
		self.projects.retain(|p| p.root != root);
	}
}

fn now_ms() -> u64 {
	std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.map(|d| d.as_millis() as u64)
		.unwrap_or(0)
}

/// The run currently in flight.
pub struct ActiveRun {
	pub id: String,
	/// Resolves the runner's shutdown and cancel futures together, which is what
	/// makes one Stop button behave the way Ctrl-C does.
	pub stop: tokio::sync::watch::Sender<bool>,
	/// A bounded tail per task, so a reloaded webview can redraw its log panes
	/// instead of showing empty ones.
	pub log: Arc<Mutex<RunLog>>,
}

#[derive(Default)]
pub struct AppState {
	project: Mutex<Option<Arc<Project>>>,
	run: Mutex<Option<ActiveRun>>,
	recents: Mutex<Recents>,
	recents_path: Mutex<Option<PathBuf>>,
	next_run: AtomicU64,
}

impl AppState {
	pub fn new() -> Self {
		Self::default()
	}

	/// Where recents are kept. Ten strings in a JSON file: a store plugin would be
	/// a Rust crate, an npm package, and capability entries to hold them.
	pub fn set_recents_path(&self, path: PathBuf) {
		if let Ok(text) = std::fs::read_to_string(&path) {
			if let Ok(recents) = serde_json::from_str::<Recents>(&text) {
				*self.recents.lock().unwrap() = recents;
			}
		}
		*self.recents_path.lock().unwrap() = Some(path);
	}

	pub fn open(&self, root: &Path) -> Result<Arc<Project>> {
		let project = Arc::new(Project::open_root(root)?);
		*self.project.lock().unwrap() = Some(Arc::clone(&project));
		{
			let mut recents = self.recents.lock().unwrap();
			recents.touch(root);
		}
		self.persist_recents();
		Ok(project)
	}

	/// The open project, or an error naming what to do about it.
	pub fn project(&self) -> Result<Arc<Project>> {
		self.project
			.lock()
			.unwrap()
			.as_ref()
			.map(Arc::clone)
			.ok_or_else(|| anyhow!("no project is open"))
	}

	pub fn peek_project(&self) -> Option<Arc<Project>> {
		self.project.lock().unwrap().as_ref().map(Arc::clone)
	}

	pub fn close(&self) {
		*self.project.lock().unwrap() = None;
	}

	pub fn recents(&self) -> Vec<RecentProject> {
		self.recents.lock().unwrap().projects.clone()
	}

	pub fn forget_recent(&self, root: &str) {
		self.recents.lock().unwrap().forget(root);
		self.persist_recents();
	}

	fn persist_recents(&self) {
		let Some(path) = self.recents_path.lock().unwrap().clone() else {
			return;
		};
		let snapshot = self.recents.lock().unwrap().projects.clone();
		let payload = Recents { projects: snapshot };
		if let Ok(text) = serde_json::to_string_pretty(&payload) {
			if let Some(dir) = path.parent() {
				let _ = std::fs::create_dir_all(dir);
			}
			let _ = std::fs::write(&path, text);
		}
	}

	/// Claim the single run slot.
	///
	/// One run at a time, and no queue: two runs against one repo would race on
	/// cache writes and on toolchain provisioning, neither of which is safe across
	/// processes let alone tasks. A queue is a thing the front end can build on top.
	pub fn begin_run(
		&self,
		log: Arc<Mutex<RunLog>>,
	) -> Result<(String, tokio::sync::watch::Receiver<bool>)> {
		let mut slot = self.run.lock().unwrap();
		if slot.is_some() {
			return Err(anyhow!("a run is already in progress"));
		}
		let id = format!("run-{}", self.next_run.fetch_add(1, Ordering::SeqCst) + 1);
		let (stop, rx) = tokio::sync::watch::channel(false);
		*slot = Some(ActiveRun {
			id: id.clone(),
			stop,
			log,
		});
		Ok((id, rx))
	}

	pub fn end_run(&self) {
		*self.run.lock().unwrap() = None;
	}

	/// Ask the run in flight to stop. `false` when there was nothing to stop.
	pub fn stop_run(&self) -> bool {
		let slot = self.run.lock().unwrap();
		match slot.as_ref() {
			Some(active) => {
				let _ = active.stop.send(true);
				true
			}
			None => false,
		}
	}

	pub fn active_run_id(&self) -> Option<String> {
		self.run.lock().unwrap().as_ref().map(|a| a.id.clone())
	}

	pub fn run_log(&self, workspace: &str, task: &str) -> Vec<lattice_events::OutputLine> {
		let slot = self.run.lock().unwrap();
		match slot.as_ref() {
			Some(active) => active.log.lock().unwrap().lines(workspace, task),
			None => Vec::new(),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn a_second_run_is_refused_while_one_is_in_flight() {
		let state = AppState::new();
		let log = Arc::new(Mutex::new(RunLog::default()));
		let (id, _rx) = state.begin_run(Arc::clone(&log)).unwrap();
		assert_eq!(id, "run-1");

		let err = state
			.begin_run(Arc::clone(&log))
			.expect_err("two runs against one repo would race on the cache");
		assert!(err.to_string().contains("already in progress"));

		state.end_run();
		let (second, _rx) = state.begin_run(log).unwrap();
		assert_eq!(second, "run-2", "run ids keep counting up");
	}

	#[test]
	fn stopping_nothing_says_so_rather_than_failing() {
		let state = AppState::new();
		assert!(!state.stop_run());
	}

	#[test]
	fn asking_for_a_project_before_one_is_open_names_the_problem() {
		let state = AppState::new();
		let err = state.project().expect_err("nothing is open yet");
		assert!(err.to_string().contains("no project is open"));
	}

	#[test]
	fn reopening_a_project_moves_it_to_the_front_without_duplicating() {
		let mut recents = Recents::default();
		recents.touch(Path::new("/repos/alpha"));
		recents.touch(Path::new("/repos/beta"));
		recents.touch(Path::new("/repos/alpha"));

		assert_eq!(recents.projects.len(), 2);
		assert_eq!(recents.projects[0].root, "/repos/alpha");
		assert_eq!(recents.projects[0].name, "alpha");
	}

	#[test]
	fn the_recent_list_is_capped() {
		let mut recents = Recents::default();
		for i in 0..(RECENT_LIMIT + 5) {
			recents.touch(Path::new(&format!("/repos/p{i}")));
		}
		assert_eq!(recents.projects.len(), RECENT_LIMIT);
	}

	#[test]
	fn forgetting_removes_only_that_entry() {
		let mut recents = Recents::default();
		recents.touch(Path::new("/repos/alpha"));
		recents.touch(Path::new("/repos/beta"));
		recents.forget("/repos/alpha");
		assert_eq!(recents.projects.len(), 1);
		assert_eq!(recents.projects[0].root, "/repos/beta");
	}
}
