//! The IPC surface: thin adapters over `service`.
//!
//! Each one unwraps `State`/`AppHandle`/`Channel`, calls the plain function, and
//! turns an `anyhow::Error` into a string. `{:#}` rather than `{}` so the whole
//! context chain survives — that chain is where Lattice's error messages live, and
//! dropping it would leave the window showing only the outermost sentence.
//!
//! The state is managed as an `Arc<AppState>` so a command that has to hand work to
//! a blocking thread can clone an owned handle instead of borrowing one.

use std::path::PathBuf;
use std::sync::Arc;

use tauri::ipc::Channel;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use lattice_project::{PlanRequest, RunOutcome, RunRequest};

use crate::reporter::{EventSink, RunMessage};
use crate::service::{self, AppInfo, ConfigDiagnostic, InitSelection, ProjectSnapshot, ScanResult};
use crate::state::{AppState, RecentProject};

/// The managed handle every command takes.
type Shared<'a> = State<'a, Arc<AppState>>;

fn fail(err: anyhow::Error) -> String {
	format!("{err:#}")
}

impl EventSink for Channel<RunMessage> {
	fn send(&self, message: RunMessage) {
		// A closed channel means the window went away mid-run. The run itself is
		// still worth finishing: its cache entries are the point.
		let _ = Channel::send(self, message);
	}
}

#[tauri::command]
pub fn app_info() -> AppInfo {
	service::app_info()
}

#[tauri::command]
pub fn catalog() -> lattice_project::view::Catalog {
	service::catalog()
}

#[tauri::command]
pub async fn pick_directory(app: AppHandle) -> Result<Option<String>, String> {
	// The callback form, bridged to a oneshot: the blocking form would sit on the
	// thread that has to keep drawing the window behind the dialog.
	let (tx, rx) = tokio::sync::oneshot::channel();
	app.dialog()
		.file()
		.set_title("Open a Lattice repo")
		.pick_folder(move |picked| {
			let _ = tx.send(picked);
		});
	let picked = rx
		.await
		.map_err(|_| "the folder picker closed unexpectedly".to_string())?;
	Ok(picked
		.and_then(|path| path.into_path().ok())
		.map(|path| path.display().to_string()))
}

#[tauri::command]
pub async fn project_open(state: Shared<'_>, path: String) -> Result<ProjectSnapshot, String> {
	// Opening walks the tree to resolve workspaces, which is synchronous and can
	// take a moment on a large repo, so it does not belong on an executor thread.
	let shared = Arc::clone(state.inner());
	let start = PathBuf::from(path);
	tauri::async_runtime::spawn_blocking(move || service::open_project(&shared, &start))
		.await
		.map_err(|e| format!("opening the project panicked: {e}"))?
		.map_err(fail)
}

#[tauri::command]
pub fn project_current(state: Shared<'_>) -> Option<lattice_project::view::ProjectView> {
	service::current_project(state.inner())
}

#[tauri::command]
pub async fn project_reload(state: Shared<'_>) -> Result<ProjectSnapshot, String> {
	let shared = Arc::clone(state.inner());
	tauri::async_runtime::spawn_blocking(move || service::reload_project(&shared))
		.await
		.map_err(|e| format!("reloading the project panicked: {e}"))?
		.map_err(fail)
}

#[tauri::command]
pub fn project_close(state: Shared<'_>) {
	state.close();
}

#[tauri::command]
pub fn project_recent(state: Shared<'_>) -> Vec<RecentProject> {
	state.recents()
}

#[tauri::command]
pub fn project_forget(state: Shared<'_>, root: String) {
	state.forget_recent(&root);
}

#[tauri::command]
pub fn project_find_root(start: String) -> Option<String> {
	service::find_root(&PathBuf::from(start)).map(|path| path.display().to_string())
}

#[tauri::command]
pub fn config_validate(text: String) -> Vec<ConfigDiagnostic> {
	service::validate_config(&text)
}

#[tauri::command]
pub async fn config_save(state: Shared<'_>, text: String) -> Result<ProjectSnapshot, String> {
	let shared = Arc::clone(state.inner());
	tauri::async_runtime::spawn_blocking(move || service::save_config(&shared, &text))
		.await
		.map_err(|e| format!("saving the config panicked: {e}"))?
		.map_err(fail)
}

#[tauri::command]
pub fn config_schema() -> Result<serde_json::Value, String> {
	service::config_schema().map_err(fail)
}

#[tauri::command]
pub async fn config_scan(path: String) -> Result<ScanResult, String> {
	// A scan walks the whole directory tree; the same reasoning as project_open.
	tauri::async_runtime::spawn_blocking(move || service::scan_for_init(&PathBuf::from(path)))
		.await
		.map_err(|e| format!("scanning panicked: {e}"))?
		.map_err(fail)
}

#[tauri::command]
pub fn config_preview(selection: InitSelection) -> Result<String, String> {
	service::preview_init(&selection).map_err(fail)
}

#[tauri::command]
pub async fn config_init(
	state: Shared<'_>,
	path: String,
	selection: InitSelection,
	force: bool,
) -> Result<ProjectSnapshot, String> {
	let shared = Arc::clone(state.inner());
	tauri::async_runtime::spawn_blocking(move || {
		service::write_init(&shared, &PathBuf::from(path), &selection, force)
	})
	.await
	.map_err(|e| format!("writing the config panicked: {e}"))?
	.map_err(fail)
}

#[tauri::command]
pub fn graph_dump(state: Shared<'_>, request: PlanRequest) -> Result<dagger::GraphDump, String> {
	service::graph_dump(state.inner(), &request).map_err(fail)
}

#[tauri::command]
pub async fn run_start(
	state: Shared<'_>,
	request: RunRequest,
	on_message: Channel<RunMessage>,
) -> Result<RunOutcome, String> {
	let (_id, outcome) = service::start_run(state.inner(), request, on_message)
		.await
		.map_err(fail)?;
	Ok(outcome)
}

#[tauri::command]
pub fn run_stop(state: Shared<'_>) -> bool {
	service::stop_run(state.inner())
}

#[tauri::command]
pub fn run_active(state: Shared<'_>) -> Option<String> {
	service::active_run(state.inner())
}

#[tauri::command]
pub fn run_log(
	state: Shared<'_>,
	workspace: String,
	task: String,
) -> Vec<lattice_events::OutputLine> {
	service::run_log(state.inner(), &workspace, &task)
}
