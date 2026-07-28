use anyhow::{anyhow, bail, Context, Result};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

pub const CONFIG_FILE: &str = "lattice.json";

/// The canonical list of engine names Lattice knows how to version-check with a
/// built-in rule. A string-form engine whose name is not in this set is rejected;
/// it must use the object form with an explicit `versionCmd`.
pub const WELL_KNOWN_ENGINES: &[&str] = &[
    "node", "deno", "bun", "pnpm", "yarn", "npm", "rust", "cargo", "go", "python", "python3",
    "ruby", "bundler", "java", "gradle", "maven", "dotnet",
];

pub fn is_well_known_engine(name: &str) -> bool {
    WELL_KNOWN_ENGINES.contains(&name)
}

fn default_true() -> bool {
    true
}

/// Name-keyed map of engine constraints. Declaration order is preserved.
pub type EngineMap = IndexMap<String, EngineSpec>;

/// An engine constraint. Either a bare version-constraint string, or a detailed
/// object with an explicit version command / install command / bin dir.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EngineSpec {
    /// A bare version constraint string (e.g. `">=20.0.0"`).
    Version(String),
    /// A detailed engine specification.
    Detailed(EngineSpecObject),
}

impl EngineSpec {
    pub fn version(&self) -> Option<&str> {
        match self {
            EngineSpec::Version(v) => Some(v.as_str()),
            EngineSpec::Detailed(o) => o.version.as_deref(),
        }
    }

    pub fn version_cmd(&self) -> Option<&str> {
        match self {
            EngineSpec::Version(_) => None,
            EngineSpec::Detailed(o) => o.version_cmd.as_deref(),
        }
    }

    pub fn install_cmd(&self) -> Option<&str> {
        match self {
            EngineSpec::Version(_) => None,
            EngineSpec::Detailed(o) => o.install_cmd.as_deref(),
        }
    }

    /// The bin dir relative to the toolchain install, if any.
    pub fn bin(&self) -> Option<&str> {
        match self {
            EngineSpec::Version(_) => None,
            EngineSpec::Detailed(o) => o.bin.as_deref(),
        }
    }

    pub fn is_detailed(&self) -> bool {
        matches!(self, EngineSpec::Detailed(_))
    }
}

/// The detailed object form of an engine constraint.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct EngineSpecObject {
    pub version: Option<String>,
    pub version_cmd: Option<String>,
    pub install_cmd: Option<String>,
    pub bin: Option<String>,
}

/// One workspace: a single project directory that is the unit of task running
/// and caching. Declared explicitly; never a glob.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceConfig {
    pub name: String,
    /// Literal directory path (relative to repo root). Never a glob.
    pub path: String,
    /// When true (default), infer engine and task commands from the native manifest.
    #[serde(default = "default_true")]
    pub auto: bool,
    /// Per-workspace toolchain constraints; overrides/pins the inferred engine.
    #[serde(default)]
    pub engines: EngineMap,
    /// Other workspaces (by name) this one depends on.
    #[serde(default)]
    pub depends_on: Option<Vec<String>>,
    /// Explicit script-name -> command overrides.
    #[serde(default)]
    pub scripts: IndexMap<String, String>,
}

/// A named task in the root task graph and how it relates across workspaces.
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

impl PipelineTask {
    /// A persistent task is long-running (e.g. a dev server) and never cached.
    pub fn is_persistent(&self) -> bool {
        self.persistent == Some(true)
    }

    /// A task is cacheable unless it is persistent or has explicitly disabled caching.
    pub fn is_cacheable(&self) -> bool {
        !self.is_persistent() && self.cache != Some(false)
    }
}

/// A human byte-size such as `"10GB"`, `"512MB"`, or a bare integer of bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheSize(pub u64);

impl CacheSize {
    pub fn as_bytes(&self) -> u64 {
        self.0
    }

    /// Parse a human byte-size. Supports `B`/`KB`/`MB`/`GB`/`TB` (base 1024,
    /// case-insensitive) and a bare integer (interpreted as bytes).
    pub fn parse(s: &str) -> Result<CacheSize> {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            bail!("cache size is empty");
        }

        // Split into the leading numeric part and the trailing unit.
        let split = trimmed
            .find(|c: char| !(c.is_ascii_digit() || c == '.'))
            .unwrap_or(trimmed.len());
        let (num_part, unit_part) = trimmed.split_at(split);
        let num_part = num_part.trim();
        let unit_part = unit_part.trim();

        if num_part.is_empty() {
            bail!("cache size '{s}' has no numeric component");
        }

        let value: f64 = num_part
            .parse()
            .with_context(|| format!("invalid numeric component in cache size '{s}'"))?;

        let multiplier: u64 = match unit_part.to_ascii_uppercase().as_str() {
            "" | "B" => 1,
            "KB" | "K" => 1024,
            "MB" | "M" => 1024 * 1024,
            "GB" | "G" => 1024 * 1024 * 1024,
            "TB" | "T" => 1024u64 * 1024 * 1024 * 1024,
            other => bail!("unknown cache size unit '{other}' in '{s}'"),
        };

        Ok(CacheSize((value * multiplier as f64) as u64))
    }
}

impl FromStr for CacheSize {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        CacheSize::parse(s)
    }
}

impl fmt::Display for CacheSize {
    /// Render to a canonical human string using the largest exact unit.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let bytes = self.0;
        const TB: u64 = 1024 * 1024 * 1024 * 1024;
        const GB: u64 = 1024 * 1024 * 1024;
        const MB: u64 = 1024 * 1024;
        const KB: u64 = 1024;
        if bytes == 0 {
            return write!(f, "0B");
        }
        for (unit, name) in [(TB, "TB"), (GB, "GB"), (MB, "MB"), (KB, "KB")] {
            if bytes.is_multiple_of(unit) {
                return write!(f, "{}{}", bytes / unit, name);
            }
        }
        write!(f, "{}B", bytes)
    }
}

impl Serialize for CacheSize {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for CacheSize {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        CacheSize::parse(&s).map_err(serde::de::Error::custom)
    }
}

/// Repo-wide knobs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_cache_size: Option<CacheSize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logging: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_dir: Option<String>,
    #[serde(default)]
    pub loquacious: bool,
    #[serde(default = "default_true")]
    pub version_check: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            max_cache_size: None,
            logging: None,
            cache_dir: None,
            loquacious: false,
            version_check: true,
        }
    }
}

impl Settings {
    /// The cache directory, defaulting to `.lattice/cache`.
    pub fn cache_dir(&self) -> &str {
        self.cache_dir.as_deref().unwrap_or(".lattice/cache")
    }
}

/// The root `lattice.json` configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LatticeConfig {
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub lattice_version: Option<String>,
    #[serde(default)]
    pub workspaces: Vec<WorkspaceConfig>,
    #[serde(default)]
    pub engines: EngineMap,
    #[serde(default)]
    pub tasks: IndexMap<String, PipelineTask>,
    #[serde(default)]
    pub settings: Settings,
}

impl LatticeConfig {
    /// Any engine (root or per-workspace) declared in string form whose key is
    /// not a well-known engine is an error. Workspace names must be unique and
    /// workspace paths must be non-empty.
    pub fn validate(&self) -> Result<()> {
        check_string_engines(&self.engines, "root")?;
        for ws in &self.workspaces {
            check_string_engines(&ws.engines, &format!("workspace '{}'", ws.name))?;
        }

        // Workspace names unique.
        let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for ws in &self.workspaces {
            if ws.path.trim().is_empty() {
                bail!("workspace '{}' has an empty path", ws.name);
            }
            if !seen.insert(ws.name.as_str()) {
                bail!(
                    "duplicate workspace name '{}': workspace names must be unique",
                    ws.name
                );
            }
        }

        Ok(())
    }
}

/// Reject string-form engines whose key is not a well-known engine.
fn check_string_engines(engines: &EngineMap, scope: &str) -> Result<()> {
    for (name, spec) in engines {
        if matches!(spec, EngineSpec::Version(_)) && !is_well_known_engine(name) {
            return Err(anyhow!(
                "engine '{name}' in {scope} uses the string (version-only) form, but '{name}' is \
                 not a well-known engine Lattice can version-check on its own. Use the object form \
                 with an explicit `versionCmd`, e.g. \"{name}\": {{ \"version\": \">=1.0.0\", \
                 \"versionCmd\": \"{name} --version\" }}"
            ));
        }
    }
    Ok(())
}

/// Merge root engines with per-workspace engines; workspace entries win per-key.
pub fn resolve_engines(root: &EngineMap, workspace: &EngineMap) -> EngineMap {
    let mut merged = root.clone();
    for (name, spec) in workspace {
        merged.insert(name.clone(), spec.clone());
    }
    merged
}

/// Read `lattice.json` from `root`, parse it, and run [`LatticeConfig::validate`].
pub fn load_config(root: &Path) -> Result<LatticeConfig> {
    let config_path = root.join(CONFIG_FILE);
    let content = std::fs::read_to_string(&config_path)
        .with_context(|| format!("failed to read {} from {}", CONFIG_FILE, root.display()))?;
    let config: LatticeConfig =
        serde_json::from_str(&content).with_context(|| "failed to parse lattice.json")?;
    config.validate()?;
    Ok(config)
}

/// Walk up from `start` looking for a directory containing `lattice.json`.
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
        // The schema is bundled with the `lattice` crate and written to
        // `.lattice/schema.json` by `lattice init`.
        let schema = read_json(
            &repo_root()
                .join("crates")
                .join("lattice")
                .join("assets")
                .join("schema.json"),
        );
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
        let old = json!({ "workspaces": ["apps/*"] });
        assert!(
            !validator.is_valid(&old),
            "glob-string workspaces must be rejected by the schema"
        );
    }

    #[test]
    fn schema_rejects_old_projects_key() {
        let validator = compiled_schema();
        let old = json!({
            "workspaces": [{ "name": "a", "path": "a" }],
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
            "workspaces": [{ "name": "a", "path": "a", "glob": "a/*" }]
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
    fn schema_requires_workspace_name() {
        let validator = compiled_schema();
        let bad = json!({ "workspaces": [{ "path": "apps/web" }] });
        assert!(
            !validator.is_valid(&bad),
            "a workspace without `name` must be rejected by the schema"
        );
    }

    #[test]
    fn schema_allows_self_reference() {
        let validator = compiled_schema();
        let ok = json!({
            "$schema": ".lattice/schema.json",
            "workspaces": [{ "name": "web", "path": "apps/web" }]
        });
        assert!(
            validator.is_valid(&ok),
            "a top-level `$schema` self-reference must be allowed"
        );
    }

    #[test]
    fn schema_allows_string_and_object_engines() {
        let validator = compiled_schema();
        let ok = json!({
            "engines": {
                "node": ">=20.0.0",
                "alpes": { "version": ">=2.6.7", "versionCmd": "alp --version" }
            }
        });
        assert!(
            validator.is_valid(&ok),
            "both string and object engine forms must be allowed by the schema"
        );
    }

    #[test]
    fn shipped_configs_load_and_validate() {
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
            let validator = compiled_schema();
            let raw = read_json(&dir.join(CONFIG_FILE));
            assert!(
                validator.is_valid(&raw),
                "shipped config at {} must validate against the schema",
                dir.display()
            );
        }
    }

    #[test]
    fn engine_string_form_parses_to_version() {
        let engines: EngineMap = serde_json::from_value(json!({ "node": ">=20.0.0" })).unwrap();
        let spec = &engines["node"];
        assert!(matches!(spec, EngineSpec::Version(_)));
        assert_eq!(spec.version(), Some(">=20.0.0"));
        assert!(!spec.is_detailed());
        assert_eq!(spec.version_cmd(), None);
    }

    #[test]
    fn engine_object_form_parses_to_detailed() {
        let engines: EngineMap = serde_json::from_value(json!({
            "alpes": {
                "version": ">=2.6.7",
                "versionCmd": "alp --version",
                "installCmd": "curl ... | sh",
                "bin": "bin"
            }
        }))
        .unwrap();
        let spec = &engines["alpes"];
        assert!(spec.is_detailed());
        assert_eq!(spec.version(), Some(">=2.6.7"));
        assert_eq!(spec.version_cmd(), Some("alp --version"));
        assert_eq!(spec.install_cmd(), Some("curl ... | sh"));
        assert_eq!(spec.bin(), Some("bin"));
    }

    #[test]
    fn validate_rejects_unknown_string_engine() {
        let config: LatticeConfig =
            serde_json::from_value(json!({ "engines": { "alpes": ">=2" } })).unwrap();
        let err = config
            .validate()
            .expect_err("string-form unknown engine must be rejected");
        let msg = format!("{err}");
        assert!(
            msg.contains("alpes"),
            "error should mention the engine: {msg}"
        );
        assert!(
            msg.contains("versionCmd"),
            "error should suggest the object form: {msg}"
        );
    }

    #[test]
    fn validate_accepts_unknown_object_engine() {
        let config: LatticeConfig = serde_json::from_value(json!({
            "engines": { "alpes": { "version": ">=2", "versionCmd": "alp --version" } }
        }))
        .unwrap();
        config
            .validate()
            .expect("object-form engine with versionCmd must be accepted");
    }

    #[test]
    fn validate_accepts_well_known_string_engine() {
        let config: LatticeConfig =
            serde_json::from_value(json!({ "engines": { "node": ">=20" } })).unwrap();
        config
            .validate()
            .expect("well-known string engine must be accepted");
    }

    #[test]
    fn validate_rejects_duplicate_workspace_names() {
        let config: LatticeConfig = serde_json::from_value(json!({
            "workspaces": [
                { "name": "dup", "path": "a" },
                { "name": "dup", "path": "b" }
            ]
        }))
        .unwrap();
        assert!(
            config.validate().is_err(),
            "duplicate workspace names must be rejected"
        );
    }

    #[test]
    fn resolve_engines_workspace_overrides_root() {
        let root: EngineMap =
            serde_json::from_value(json!({ "node": ">=18", "rust": ">=1.75" })).unwrap();
        let workspace: EngineMap =
            serde_json::from_value(json!({ "node": ">=20", "bun": ">=1.1" })).unwrap();
        let merged = resolve_engines(&root, &workspace);
        assert_eq!(merged["node"].version(), Some(">=20"), "workspace key wins");
        assert_eq!(
            merged["rust"].version(),
            Some(">=1.75"),
            "root-only key kept"
        );
        assert_eq!(
            merged["bun"].version(),
            Some(">=1.1"),
            "workspace-only key added"
        );
    }

    #[test]
    fn cache_size_parses_units() {
        assert_eq!(
            CacheSize::parse("10GB").unwrap().as_bytes(),
            10 * 1024 * 1024 * 1024
        );
        assert_eq!(
            CacheSize::parse("512MB").unwrap().as_bytes(),
            512 * 1024 * 1024
        );
        assert_eq!(CacheSize::parse("1024").unwrap().as_bytes(), 1024);
        assert_eq!(
            CacheSize::parse("2tb").unwrap().as_bytes(),
            2u64 * 1024 * 1024 * 1024 * 1024
        );
    }

    #[test]
    fn cache_size_round_trips_through_serde() {
        let cs = CacheSize::parse("10GB").unwrap();
        let json = serde_json::to_string(&cs).unwrap();
        assert_eq!(json, "\"10GB\"");
        let back: CacheSize = serde_json::from_str(&json).unwrap();
        assert_eq!(back, cs);
    }

    #[test]
    fn settings_defaults() {
        let s: Settings = serde_json::from_value(json!({})).unwrap();
        assert!(!s.loquacious);
        assert!(s.version_check);
        assert_eq!(s.cache_dir(), ".lattice/cache");
        assert!(s.max_cache_size.is_none());
    }

    #[test]
    fn settings_explicit_values() {
        let s: Settings =
            serde_json::from_value(json!({ "loquacious": true, "versionCheck": false })).unwrap();
        assert!(s.loquacious);
        assert!(!s.version_check);
    }

    #[test]
    fn tasks_preserve_declaration_order() {
        let config: LatticeConfig =
            serde_json::from_str(r#"{ "tasks": { "build": {}, "test": {}, "dev": {} } }"#).unwrap();
        let keys: Vec<&str> = config.tasks.keys().map(|s| s.as_str()).collect();
        assert_eq!(keys, vec!["build", "test", "dev"]);
    }

    #[test]
    fn pipeline_task_cacheability() {
        let persistent = PipelineTask {
            persistent: Some(true),
            ..Default::default()
        };
        assert!(persistent.is_persistent());
        assert!(!persistent.is_cacheable());

        let no_cache = PipelineTask {
            cache: Some(false),
            ..Default::default()
        };
        assert!(!no_cache.is_cacheable());

        let normal = PipelineTask::default();
        assert!(normal.is_cacheable());
    }
}
