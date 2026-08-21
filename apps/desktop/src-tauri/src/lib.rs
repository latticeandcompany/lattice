//! The desktop front end's backend.
//!
//! It holds the open project and the one run in flight, adapts every Lattice call
//! into an IPC command, and turns `Reporter` callbacks into an ordered channel.
//! It renders nothing: the window is React, and every string it shows arrives as a
//! typed payload.
//!
//! Nothing here reimplements the engine. Scheduling, caching, and driver detection
//! come from the same crates the CLI uses, in process, so the two front ends cannot
//! disagree about what a task is or whether it needs to run.

mod commands;
mod reporter;
mod service;
mod state;

use std::sync::Arc;

use tauri::Manager;

pub fn run() {
	// No #[tokio::main] on main: Tauri owns the runtime, and a second one would
	// leave the runner's tasks on an executor nothing is driving.
	tauri::Builder::default()
		.plugin(tauri_plugin_dialog::init())
		.setup(|app| {
			let shared = Arc::new(state::AppState::new());
			if let Ok(dir) = app.path().app_config_dir() {
				shared.set_recents_path(service::recents_file(&dir));
			}
			app.manage(shared);
			Ok(())
		})
		.invoke_handler(tauri::generate_handler![
			commands::app_info,
			commands::catalog,
			commands::pick_directory,
			commands::project_open,
			commands::project_current,
			commands::project_reload,
			commands::project_close,
			commands::project_recent,
			commands::project_forget,
			commands::project_find_root,
			commands::config_validate,
			commands::config_save,
			commands::config_schema,
			commands::config_scan,
			commands::config_preview,
			commands::config_init,
			commands::graph_dump,
			commands::run_start,
			commands::run_stop,
			commands::run_active,
			commands::run_log,
		])
		.run(tauri::generate_context!())
		.expect("the Lattice window could not start");
}
