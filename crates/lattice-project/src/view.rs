//! What a project looks like to something outside the process.
//!
//! These are wire types, not domain types. They exist because the domain types
//! are not the right shape to send: [`lattice_workspace::Workspace`] carries a
//! `PathBuf`, which is not always valid UTF-8 and means nothing on the other
//! machine-shaped side of an IPC boundary, and a caller wants a repo-relative
//! path with forward slashes regardless of platform.

use std::path::Path;

use serde::Serialize;

use lattice_config::{EngineSpec, LatticeConfig};
use lattice_workspace::{DriverResolution, Evidence, Role, Workspace};

use crate::Project;

/// Everything a front end needs to draw a project it has just opened.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectView {
	/// Absolute, in display form.
	pub root: String,
	/// The root directory's own name, for a title.
	pub name: String,
	pub lattice_version: Option<String>,
	pub tasks: Vec<TaskDefView>,
	pub workspaces: Vec<WorkspaceView>,
	pub engines: Vec<EngineView>,
	pub global_dependencies: Vec<String>,
	pub global_env: Vec<String>,
	pub settings: SettingsView,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceView {
	pub name: String,
	/// Repo-relative, forward-slashed on every platform.
	pub path: String,
	pub auto: bool,
	pub depends_on: Vec<String>,
	pub driver: Option<DriverView>,
	pub engines: Vec<EngineView>,
	/// The tasks that resolve to a command here, in pipeline declaration order.
	/// A workspace whose toolchain has no `lint` simply has no `lint` entry.
	pub tasks: Vec<WorkspaceTaskView>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceTaskView {
	pub task: String,
	pub command: String,
	pub persistent: bool,
	pub cacheable: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriverView {
	pub tool: String,
	pub role: String,
	/// The ecosystem slug a language mark is keyed off. `None` for the agnostic
	/// task runners.
	pub language: Option<String>,
	pub via: EvidenceView,
}

/// Why a driver was chosen, kept structured so a front end can say so.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum EvidenceView {
	Declaration,
	NativeFile { file: String },
	Lockfile { file: String },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineView {
	pub name: String,
	pub version: Option<String>,
	pub version_cmd: Option<String>,
	pub install_cmd: Option<String>,
	pub bin: Option<String>,
	/// Whether the bare-string form is accepted for this name.
	pub well_known: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskDefView {
	pub name: String,
	pub depends_on: Vec<String>,
	pub inputs: Vec<String>,
	pub outputs: Vec<String>,
	pub ignore: Vec<String>,
	pub env: Vec<String>,
	pub persistent: bool,
	pub cache: bool,
	/// The canonical string form, e.g. `"90s"`. `None` when unset.
	pub timeout: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
	pub max_cache_size: Option<String>,
	pub cache_dir: String,
	pub loquacious: bool,
	pub version_check: bool,
}

/// One entry of the driver catalog: what Lattice can detect, so a front end can
/// offer the same set rather than hard-coding its own copy.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriverCatalogEntry {
	pub tool: String,
	pub roles: Vec<String>,
	pub language: Option<String>,
	pub fingerprint: Vec<String>,
	pub version_cmd: String,
}

/// One entry of the engine catalog: a name accepted in the bare-string form.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineCatalogEntry {
	pub name: String,
	pub version_cmd: Option<String>,
}

/// Everything Lattice knows how to recognize.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
	pub drivers: Vec<DriverCatalogEntry>,
	pub engines: Vec<EngineCatalogEntry>,
	pub lockfiles: Vec<String>,
	pub manifests: Vec<String>,
	/// The parts a cache key is composed from, in order, so a front end can label
	/// a miss without knowing the list.
	pub key_components: Vec<String>,
}

pub fn catalog() -> Catalog {
	Catalog {
		drivers: lattice_workspace::DriverRegistry::known()
			.iter()
			.map(|spec| DriverCatalogEntry {
				tool: spec.tool.to_string(),
				roles: spec
					.roles
					.iter()
					.map(role_name)
					.map(str::to_string)
					.collect(),
				language: spec.language.map(str::to_string),
				fingerprint: spec.fingerprint.iter().map(|f| f.to_string()).collect(),
				version_cmd: spec.version_cmd.to_string(),
			})
			.collect(),
		engines: lattice_config::well_known_engine_names()
			.into_iter()
			.map(|name| EngineCatalogEntry {
				version_cmd: lattice_config::builtin_version_cmd(name).map(str::to_string),
				name: name.to_string(),
			})
			.collect(),
		lockfiles: lattice_config::LOCKFILES
			.iter()
			.map(|f| f.to_string())
			.collect(),
		manifests: lattice_config::MANIFESTS
			.iter()
			.map(|f| f.to_string())
			.collect(),
		key_components: lattice_cache::KEY_COMPONENTS
			.iter()
			.map(|c| c.to_string())
			.collect(),
	}
}

fn role_name(role: &Role) -> &'static str {
	match role {
		Role::Runtime => "runtime",
		Role::PackageManager => "packageManager",
		Role::BuildTool => "buildTool",
		Role::TaskRunner => "taskRunner",
	}
}

/// A path under `root`, expressed the way `lattice.json` writes one: relative and
/// forward-slashed, so the same repo reads the same on every platform.
fn relative(root: &Path, path: &Path) -> String {
	let rel = path.strip_prefix(root).unwrap_or(path);
	let text = rel.to_string_lossy().replace('\\', "/");
	if text.is_empty() {
		".".to_string()
	} else {
		text
	}
}

fn engine_views(engines: &lattice_config::EngineMap) -> Vec<EngineView> {
	engines
		.iter()
		.map(|(name, spec)| {
			let mut view = EngineView {
				name: name.clone(),
				version: None,
				version_cmd: None,
				install_cmd: None,
				bin: None,
				well_known: lattice_config::is_well_known_engine(name),
			};
			match spec {
				EngineSpec::Version(v) => view.version = Some(v.clone()),
				EngineSpec::Detailed(detail) => {
					view.version = detail.version.clone();
					view.version_cmd = detail.version_cmd.clone();
					view.install_cmd = detail.install_cmd.clone();
					view.bin = detail.bin.clone();
				}
			}
			view
		})
		.collect()
}

fn driver_view(resolution: &DriverResolution) -> DriverView {
	DriverView {
		tool: resolution.tool.clone(),
		role: role_name(&resolution.role).to_string(),
		language: lattice_workspace::DriverRegistry::get(&resolution.tool)
			.and_then(|spec| spec.language)
			.map(str::to_string),
		via: match &resolution.via {
			Evidence::Declaration => EvidenceView::Declaration,
			Evidence::NativeFile(file) => EvidenceView::NativeFile { file: file.clone() },
			Evidence::Lockfile(file) => EvidenceView::Lockfile { file: file.clone() },
		},
	}
}

fn workspace_view(root: &Path, ws: &Workspace, config: &LatticeConfig) -> WorkspaceView {
	// Pipeline order, not command-map order: the front end lists tasks in the
	// order the config declares them, the same order the CLI reports.
	let tasks = config
		.tasks
		.iter()
		.filter_map(|(name, task)| {
			ws.command_for(name).map(|command| WorkspaceTaskView {
				task: name.clone(),
				command: command.to_string(),
				persistent: task.is_persistent(),
				cacheable: task.is_cacheable(),
			})
		})
		.collect();

	WorkspaceView {
		name: ws.name.clone(),
		path: relative(root, &ws.path),
		auto: ws.auto,
		depends_on: ws.depends_on.clone(),
		driver: ws.driver.as_ref().map(driver_view),
		engines: engine_views(&ws.engines),
		tasks,
	}
}

impl Project {
	/// This project as a front end sees it.
	pub fn view(&self) -> ProjectView {
		let config = &self.config;
		ProjectView {
			root: self.root.display().to_string(),
			name: self
				.root
				.file_name()
				.map(|n| n.to_string_lossy().to_string())
				.unwrap_or_else(|| self.root.display().to_string()),
			lattice_version: config.lattice_version.clone(),
			tasks: config
				.tasks
				.iter()
				.map(|(name, task)| TaskDefView {
					name: name.clone(),
					depends_on: task.depends_on.clone().unwrap_or_default(),
					inputs: task.inputs.clone().unwrap_or_default(),
					outputs: task.outputs.clone().unwrap_or_default(),
					ignore: task.ignore.clone().unwrap_or_default(),
					env: task.env.clone().unwrap_or_default(),
					persistent: task.is_persistent(),
					cache: task.is_cacheable(),
					timeout: task.timeout.map(|t| t.to_string()),
				})
				.collect(),
			workspaces: self
				.workspaces
				.iter()
				.map(|ws| workspace_view(&self.root, ws, config))
				.collect(),
			engines: engine_views(&config.engines),
			global_dependencies: config.global_dependencies.clone(),
			global_env: config.global_env.clone(),
			settings: SettingsView {
				max_cache_size: config.settings.max_cache_size.map(|s| s.to_string()),
				cache_dir: config.settings.cache_dir().to_string(),
				loquacious: config.settings.loquacious,
				version_check: config.settings.version_check,
			},
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn the_catalog_reports_every_driver_and_engine_lattice_knows() {
		let catalog = catalog();
		assert_eq!(
			catalog.drivers.len(),
			lattice_workspace::DriverRegistry::known().len()
		);
		assert_eq!(
			catalog.engines.len(),
			lattice_config::well_known_engine_names().len()
		);
		assert!(!catalog.key_components.is_empty());
		assert!(catalog.lockfiles.contains(&"package-lock.json".to_string()));
	}

	#[test]
	fn a_driver_carries_the_language_a_mark_is_keyed_off() {
		let catalog = catalog();
		let cargo = catalog.drivers.iter().find(|d| d.tool == "cargo").unwrap();
		assert_eq!(cargo.language.as_deref(), Some("rust"));
		let just = catalog.drivers.iter().find(|d| d.tool == "just").unwrap();
		assert_eq!(just.language, None);
	}

	#[test]
	fn a_path_is_relative_and_forward_slashed() {
		let root = Path::new("/repo");
		assert_eq!(relative(root, Path::new("/repo/apps/web")), "apps/web");
		// The root itself is the "." a config would write.
		assert_eq!(relative(root, Path::new("/repo")), ".");
	}

	#[test]
	fn roles_cross_the_wire_in_camel_case() {
		assert_eq!(role_name(&Role::PackageManager), "packageManager");
		assert_eq!(role_name(&Role::TaskRunner), "taskRunner");
		assert_eq!(role_name(&Role::Runtime), "runtime");
		assert_eq!(role_name(&Role::BuildTool), "buildTool");
	}
}
