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
    let content = std::fs::read_to_string(&config_path)
        .with_context(|| format!("Failed to read {} from {}", CONFIG_FILE, root.display()))?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use jsonschema::Validator;
    use serde_json::{json, Value};

    /// Repo root, resolved from this crate's manifest dir (`crates/lattice-config`).
    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
    }

    fn read_json(path: &Path) -> Value {
        let content = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
        serde_json::from_str(&content)
            .unwrap_or_else(|e| panic!("failed to parse {} as JSON: {e}", path.display()))
    }

    fn compiled_schema() -> Validator {
        let schema = read_json(&repo_root().join(".lattice").join("schema.json"));
        jsonschema::validator_for(&schema).expect("schema.json must be a valid JSON Schema")
    }

    #[test]
    fn schema_validates_repo_config() {
        let validator = compiled_schema();
        let config = read_json(&repo_root().join(CONFIG_FILE));
        if let Err(error) = validator.validate(&config) {
            panic!("repo lattice.json failed schema validation: {error}");
        }
    }

    #[test]
    fn schema_validates_polyglot_example_config() {
        let validator = compiled_schema();
        let config = read_json(
            &repo_root()
                .join("examples")
                .join("polyglot")
                .join(CONFIG_FILE),
        );
        if let Err(error) = validator.validate(&config) {
            panic!("examples/polyglot/lattice.json failed schema validation: {error}");
        }
    }

    #[test]
    fn schema_rejects_old_glob_string_workspaces() {
        let validator = compiled_schema();
        // Old model: `workspaces` was an array of glob strings.
        let old = json!({ "workspaces": ["apps/*"] });
        assert!(
            !validator.is_valid(&old),
            "glob-string workspaces must be rejected by the schema"
        );
    }

    #[test]
    fn schema_rejects_old_projects_key() {
        let validator = compiled_schema();
        // Old model: a top-level `projects` map. It has been removed.
        let old = json!({
            "workspaces": [{ "path": "a" }],
            "projects": {}
        });
        assert!(
            !validator.is_valid(&old),
            "a top-level `projects` key must be rejected by the schema"
        );
    }

    #[test]
    fn schema_rejects_glob_key_on_workspace() {
        let validator = compiled_schema();
        let old = json!({
            "workspaces": [{ "path": "a", "glob": "a/*" }]
        });
        assert!(
            !validator.is_valid(&old),
            "a `glob` key on a workspace must be rejected by the schema"
        );
    }

    #[test]
    fn schema_requires_workspace_path() {
        let validator = compiled_schema();
        let bad = json!({ "workspaces": [{ "name": "no-path" }] });
        assert!(
            !validator.is_valid(&bad),
            "a workspace without `path` must be rejected by the schema"
        );
    }

    #[test]
    fn schema_allows_self_reference() {
        let validator = compiled_schema();
        let ok = json!({
            "$schema": ".lattice/schema.json",
            "workspaces": [{ "path": "apps/web" }]
        });
        assert!(
            validator.is_valid(&ok),
            "a top-level `$schema` self-reference must be allowed"
        );
    }

    #[test]
    fn schema_validated_configs_load_and_round_trip() {
        // Both shipped configs must parse via `load_config`, re-serialize, and
        // parse again to an identical structure. (The serialized form may carry
        // explicit `null`s for absent optional fields, so we compare parsed
        // structures rather than raw JSON against the strict input schema.)
        for dir in [repo_root(), repo_root().join("examples").join("polyglot")] {
            let config = load_config(&dir)
                .unwrap_or_else(|e| panic!("load_config failed for {}: {e:#}", dir.display()));
            let serialized =
                serde_json::to_string(&config).expect("LatticeConfig must serialize back to JSON");
            let reparsed: LatticeConfig = serde_json::from_str(&serialized)
                .expect("re-serialized LatticeConfig must parse again");
            assert_eq!(
                serde_json::to_value(&config).unwrap(),
                serde_json::to_value(&reparsed).unwrap(),
                "config from {} must round-trip through serde without loss",
                dir.display()
            );
        }
    }
}
