//! What each command actually does.
//!
//! Every function here is a plain one over `&AppState`: no `State`, no `AppHandle`,
//! no `Channel`. The commands in `commands.rs` are the adapters that unwrap those
//! and map an error to a string. Keeping the logic on this side is what lets it be
//! tested without a webview or a Tauri runtime.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use lattice_project::scaffold;
use lattice_project::view::{Catalog, ProjectView};
use lattice_project::{Plan, RunOptions, RunOutcome, RunRequest};
use lattice_workspace::scan::{scan_engine_pins, scan_workspaces, EnginePin, WorkspaceCandidate};

use crate::reporter::{ChannelReporter, EventSink, RunLog};
use crate::state::AppState;

/// The running binary's own version, written into a config it scaffolds.
pub const BIN_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
	pub lattice_version: String,
	pub platform: &'static str,
	pub key_components: Vec<String>,
}

pub fn app_info() -> AppInfo {
	AppInfo {
		lattice_version: BIN_VERSION.to_string(),
		platform: if cfg!(target_os = "macos") {
			"macos"
		} else if cfg!(target_os = "windows") {
			"windows"
		} else {
			"linux"
		},
		key_components: lattice_project::view::catalog().key_components,
	}
}

/// A diagnostic about a config, with a position when the failure carried one.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigDiagnostic {
	pub severity: &'static str,
	pub message: String,
	pub line: Option<u32>,
	pub column: Option<u32>,
}

impl ConfigDiagnostic {
	fn error(message: String) -> Self {
		// `parse_config` already formats a position into its message when it has
		// one; pulling it back out as a number is what lets the editor point at a
		// line rather than only print the sentence.
		let (line, column) = extract_position(&message);
		Self {
			severity: "error",
			message,
			line,
			column,
		}
	}
}

/// Read `line N, column M` out of a config error, when it names one.
fn extract_position(message: &str) -> (Option<u32>, Option<u32>) {
	let line = capture_number(message, "line ");
	let column = capture_number(message, "column ");
	(line, column)
}

fn capture_number(haystack: &str, prefix: &str) -> Option<u32> {
	let start = haystack.find(prefix)? + prefix.len();
	let digits: String = haystack[start..]
		.chars()
		.take_while(char::is_ascii_digit)
		.collect();
	digits.parse().ok()
}

/// Everything the window needs after opening a directory.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSnapshot {
	/// `None` when the directory holds no `lattice.json`, which is the wizard's
	/// entry point rather than an error.
	pub project: Option<ProjectView>,
	pub config_text: Option<String>,
	pub diagnostics: Vec<ConfigDiagnostic>,
}

/// Walk up from `start` for a repo root, so picking a subdirectory still works.
pub fn find_root(start: &Path) -> Option<PathBuf> {
	lattice_config::find_root(start)
}

pub fn open_project(state: &AppState, start: &Path) -> Result<ProjectSnapshot> {
	let root = find_root(start).unwrap_or_else(|| start.to_path_buf());
	let config_path = root.join(lattice_config::CONFIG_FILE);
	if !config_path.exists() {
		return Ok(ProjectSnapshot {
			project: None,
			config_text: None,
			diagnostics: Vec::new(),
		});
	}

	let config_text = std::fs::read_to_string(&config_path)
		.with_context(|| format!("failed to read {}", config_path.display()))?;

	match state.open(&root) {
		Ok(project) => Ok(ProjectSnapshot {
			project: Some(project.view()),
			config_text: Some(config_text),
			diagnostics: Vec::new(),
		}),
		// A config that does not load is still something to show and fix, so the
		// error becomes a diagnostic rather than a failed command.
		Err(err) => Ok(ProjectSnapshot {
			project: None,
			config_text: Some(config_text),
			diagnostics: vec![ConfigDiagnostic::error(format!("{err:#}"))],
		}),
	}
}

pub fn current_project(state: &AppState) -> Option<ProjectView> {
	state.peek_project().map(|p| p.view())
}

pub fn reload_project(state: &AppState) -> Result<ProjectSnapshot> {
	let root = state.project()?.root.clone();
	open_project(state, &root)
}

pub fn catalog() -> Catalog {
	lattice_project::view::catalog()
}

// ---------- config ----------

/// Parse and validate config text without writing anything.
pub fn validate_config(text: &str) -> Vec<ConfigDiagnostic> {
	match lattice_config::parse_config(text) {
		Ok(_) => Vec::new(),
		Err(err) => vec![ConfigDiagnostic::error(format!("{err:#}"))],
	}
}

/// Write config text verbatim, once it validates.
///
/// Verbatim is the point: the window edits the text, not a parsed object, so an
/// unknown key it has never heard of survives a save. Re-serializing here would
/// throw that away.
pub fn save_config(state: &AppState, text: &str) -> Result<ProjectSnapshot> {
	let root = state.project()?.root.clone();
	let diagnostics = validate_config(text);
	if !diagnostics.is_empty() {
		bail!("{}", diagnostics[0].message);
	}
	let path = root.join(lattice_config::CONFIG_FILE);
	std::fs::write(&path, text).with_context(|| format!("failed to write {}", path.display()))?;
	lattice_config::schema::ensure_schema(&root);
	open_project(state, &root)
}

pub fn config_schema() -> Result<serde_json::Value> {
	Ok(serde_json::from_str(lattice_config::schema::SCHEMA_JSON)?)
}

// ---------- the wizard ----------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
	pub root: String,
	pub already_initialized: bool,
	pub candidates: Vec<WorkspaceCandidate>,
	pub pins: Vec<EnginePin>,
}

pub fn scan_for_init(dir: &Path) -> Result<ScanResult> {
	Ok(ScanResult {
		root: dir.display().to_string(),
		already_initialized: dir.join(lattice_config::CONFIG_FILE).exists(),
		candidates: scan_workspaces(dir),
		pins: scan_engine_pins(dir),
	})
}

/// Which of a scan's findings the user kept.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct InitSelection {
	pub candidates: Vec<WorkspaceCandidate>,
	pub pins: Vec<EnginePin>,
}

/// The exact text `write_init` would write, so the wizard can show it first.
///
/// Built by the same function `lattice init` uses. Assembling it in the front end
/// instead would guarantee the two drift: the rules about when a `dist/**` output
/// is justified and when `engines` is omitted live in one place on purpose.
pub fn preview_init(selection: &InitSelection) -> Result<String> {
	scaffold::render_config(&config_value(selection))
}

fn config_value(selection: &InitSelection) -> serde_json::Value {
	let candidates: Vec<&WorkspaceCandidate> = selection.candidates.iter().collect();
	let pins: Vec<&EnginePin> = selection.pins.iter().collect();
	if candidates.is_empty() && pins.is_empty() {
		return scaffold::default_skeleton(BIN_VERSION);
	}
	scaffold::build_config(&candidates, &pins, BIN_VERSION)
}

pub fn write_init(
	state: &AppState,
	dir: &Path,
	selection: &InitSelection,
	force: bool,
) -> Result<ProjectSnapshot> {
	let existing = dir.join(lattice_config::CONFIG_FILE);
	if existing.exists() && !force {
		bail!(
			"{} already exists. Open the project instead, or overwrite it",
			existing.display()
		);
	}
	scaffold::write_artifacts(dir, &config_value(selection))?;
	open_project(state, dir)
}

// ---------- graph ----------

pub fn graph_dump(
	state: &AppState,
	request: &lattice_project::PlanRequest,
) -> Result<dagger::GraphDump> {
	let project = state.project()?;
	project.require_known_tasks(&request.tasks)?;
	match project.plan(request)? {
		// Nothing to draw is an empty graph, not a failure: the window shows its
		// own empty state for it.
		Plan::NoWorkspaces | Plan::NoMatch { .. } => Ok(dagger::GraphDump {
			nodes: Vec::new(),
			edges: Vec::new(),
		}),
		Plan::Phases(phases) => {
			let mut nodes = Vec::new();
			let mut edges = Vec::new();
			let mut seen_nodes = std::collections::HashSet::new();
			let mut seen_edges = std::collections::HashSet::new();
			// Sequential phases are separate graphs over the same task set; merged
			// into one picture they are the dependency structure of the whole run.
			for phase in &phases {
				let dump = dagger::dump_graph(phase);
				for node in dump.nodes {
					if seen_nodes.insert(node.id.clone()) {
						nodes.push(node);
					}
				}
				for edge in dump.edges {
					if seen_edges.insert((edge.from.clone(), edge.to.clone())) {
						edges.push(edge);
					}
				}
			}
			Ok(dagger::GraphDump { nodes, edges })
		}
	}
}

// ---------- running ----------

/// Start a run and wait for it to end.
///
/// Errors only when the run could not be attempted. A build that failed is an `Ok`
/// carrying [`RunOutcome::Failed`], because that is an answer to what happened.
pub async fn start_run<S: EventSink>(
	state: &AppState,
	request: RunRequest,
	sink: S,
) -> Result<(String, RunOutcome)> {
	let project = state.project()?;
	project.require_known_tasks(&request.plan.tasks)?;

	let log = Arc::new(Mutex::new(RunLog::default()));
	let (run_id, stop_rx) = state.begin_run(Arc::clone(&log))?;

	let reporter = Arc::new(ChannelReporter::new(sink, log));
	// Dropped with everything else at the end of this function, which is after the
	// final flush `finish` already does.
	let _ticker = reporter.ticker();
	let make_signal = || {
		let mut rx = stop_rx.clone();
		Box::pin(async move {
			// Already-true means a stop arrived before this phase started, which
			// should end it immediately rather than wait for another edge.
			let _ = rx.wait_for(|stopped| *stopped).await;
		}) as std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
	};

	let outcome = lattice_project::run(RunOptions {
		project: &project,
		request: &request,
		reporter: reporter.as_ref(),
		lattice_version: BIN_VERSION,
		// Both, from one signal: shutdown ends the wait on persistent tasks, cancel
		// aborts a graph still running. A Stop button means both.
		shutdown: Some(Box::new(make_signal)),
		cancel: Some(Box::new(make_signal)),
	})
	.await;

	state.end_run();

	match outcome {
		Ok(outcome) => Ok((run_id, outcome)),
		Err(err) => Err(err),
	}
}

pub fn stop_run(state: &AppState) -> bool {
	state.stop_run()
}

pub fn run_log(state: &AppState, workspace: &str, task: &str) -> Vec<lattice_events::OutputLine> {
	state.run_log(workspace, task)
}

/// The message a front end shows for a run it did not watch start.
pub fn active_run(state: &AppState) -> Option<String> {
	state.active_run_id()
}

/// Where recents live, under the OS's per-app config directory.
pub fn recents_file(config_dir: &Path) -> PathBuf {
	config_dir.join("recents.json")
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn a_position_is_pulled_out_of_a_config_error() {
		let (line, column) = extract_position(
			"unknown field `output` in tasks.build (lattice.json line 3, column 12)",
		);
		assert_eq!(line, Some(3));
		assert_eq!(column, Some(12));
	}

	#[test]
	fn an_error_with_no_position_still_becomes_a_diagnostic() {
		let diagnostic = ConfigDiagnostic::error("workspace names must be unique".to_string());
		assert_eq!(diagnostic.severity, "error");
		assert_eq!(diagnostic.line, None);
		assert_eq!(diagnostic.column, None);
	}

	#[test]
	fn valid_config_text_produces_no_diagnostics() {
		let text = r#"{
			"workspaces": [{ "name": "web", "path": "apps/web" }],
			"tasks": { "build": {} }
		}"#;
		assert!(validate_config(text).is_empty());
	}

	#[test]
	fn an_unknown_key_is_reported_rather_than_ignored() {
		// deny_unknown_fields is what makes the editor's round trip matter: a key
		// this build does not know is still the user's, and must not be dropped.
		let text = r#"{ "workspaces": [], "tasks": {}, "nonsense": 1 }"#;
		let diagnostics = validate_config(text);
		assert_eq!(diagnostics.len(), 1);
		assert!(diagnostics[0].message.contains("nonsense"));
	}

	#[test]
	fn an_empty_selection_falls_back_to_the_skeleton() {
		let text = preview_init(&InitSelection::default()).unwrap();
		assert!(text.contains("\"workspaces\": []"));
		assert!(text.ends_with("}\n"));
	}

	#[test]
	fn a_preview_is_exactly_what_would_be_written() {
		let selection = InitSelection {
			candidates: vec![WorkspaceCandidate {
				name: "web".into(),
				path: "apps/web".into(),
				marker: "package.json".into(),
				driver: Some("npm".into()),
				default_selected: true,
			}],
			pins: Vec::new(),
		};
		let preview = preview_init(&selection).unwrap();

		let dir = tempfile::tempdir().unwrap();
		scaffold::write_artifacts(dir.path(), &config_value(&selection)).unwrap();
		let written = std::fs::read_to_string(dir.path().join("lattice.json")).unwrap();

		assert_eq!(
			preview, written,
			"the preview must not be a second renderer"
		);
	}

	#[test]
	fn writing_over_an_existing_config_needs_to_be_asked_for() {
		let dir = tempfile::tempdir().unwrap();
		std::fs::write(dir.path().join("lattice.json"), "{}").unwrap();
		let state = AppState::new();

		let err = write_init(&state, dir.path(), &InitSelection::default(), false)
			.expect_err("an existing config is not silently replaced");
		assert!(err.to_string().contains("already exists"));
	}

	#[test]
	fn opening_a_directory_with_no_config_is_not_an_error() {
		let dir = tempfile::tempdir().unwrap();
		let state = AppState::new();
		let snapshot = open_project(&state, dir.path()).unwrap();
		assert!(snapshot.project.is_none());
		assert!(snapshot.config_text.is_none());
		assert!(snapshot.diagnostics.is_empty());
	}

	#[test]
	fn a_config_that_does_not_load_comes_back_as_a_diagnostic() {
		let dir = tempfile::tempdir().unwrap();
		std::fs::write(dir.path().join("lattice.json"), "{ not json").unwrap();
		let state = AppState::new();

		let snapshot = open_project(&state, dir.path()).unwrap();
		assert!(snapshot.project.is_none());
		assert!(snapshot.config_text.is_some(), "the text is still editable");
		assert_eq!(snapshot.diagnostics.len(), 1);
	}

	#[test]
	fn app_info_names_a_platform_the_front_end_knows() {
		let info = app_info();
		assert!(matches!(info.platform, "macos" | "windows" | "linux"));
		assert!(!info.key_components.is_empty());
	}
}
