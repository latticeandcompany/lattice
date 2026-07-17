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
    pub workspaces: Vec<String>,
    pub engines: Option<EngineConfig>,
    #[serde(default)]
    pub pipeline: HashMap<String, PipelineTask>,
    pub projects: Option<HashMap<String, ProjectConfig>>,
    pub max_cache_size: Option<String>,
    pub logging: Option<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProjectConfig {
    pub depends_on: Option<Vec<String>>,
    #[serde(default)]
    pub tasks: HashMap<String, String>,
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
