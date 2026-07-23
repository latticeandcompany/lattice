use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub const CONFIG_FILE: &str = "lattice.json";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LatticeConfig {
    pub lattice_version: Option<String>,
    #[serde(default)]
    pub workspaces: Vec<WorkspaceConfig>,
    pub engines: Option<EngineConfig>,
    #[serde(default)]
    pub pipeline: HashMap<String, PipelineTask>,
    pub max_cache_size: Option<String>,
    pub logging: Option<String>,
}

fn default_true() -> bool {
    true
}

/// One workspace: a single project directory that is the unit of task
/// running and caching. Declared explicitly — no globs (PRD §2.1).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceConfig {
    /// Literal path (relative to repo root) to the workspace directory. Not a glob.
    pub path: String,
    /// Workspace name; defaults to the basename of `path`.
    pub name: Option<String>,
    /// When true (default), infer engine and task commands from the native manifest.
    /// When false, use only what this entry declares.
    #[serde(default = "default_true")]
    pub auto: bool,
    /// Per-workspace toolchain constraints; overrides/pins the inferred engine.
    pub engines: Option<EngineConfig>,
    /// Other workspaces (by name) this one depends on.
    pub depends_on: Option<Vec<String>>,
    /// Explicit task-name -> command overrides.
    pub tasks: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EngineConfig {
    pub node: Option<String>,
    pub rust: Option<String>,
    pub python: Option<String>,
    pub go: Option<String>,
    pub custom: Option<HashMap<String, CustomEngine>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomEngine {
    pub bin: String,
    pub version: String,
    pub version_cmd: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PipelineTask {
    pub depends_on: Option<Vec<String>>,
    pub inputs: Option<Vec<String>>,
    pub outputs: Option<Vec<String>>,
    pub ignore: Option<Vec<String>>,
    pub env: Option<Vec<String>>,
    pub persistent: Option<bool>,
    pub cache: Option<bool>,
}

pub fn load_config(root: &Path) -> Result<LatticeConfig> {
    let config_path = root.join(CONFIG_FILE);
    let content = std::fs::read_to_string(&config_path).with_context(|| {
        format!(
            "Failed to read {} from {}",
            CONFIG_FILE,
            root.display()
        )
    })?;
    let config: LatticeConfig =
        serde_json::from_str(&content).with_context(|| "Failed to parse lattice.json")?;
    Ok(config)
}

pub fn find_root(start: &Path) -> Option<PathBuf> {
    let mut current = start.to_path_buf();
    loop {
        if current.join(CONFIG_FILE).exists() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}
