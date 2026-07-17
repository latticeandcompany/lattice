use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use lattice_config::PipelineTask;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheMeta {
    pub hash: String,
    pub task: String,
    pub workspace: String,
    pub duration_ms: u64,
    pub last_used: DateTime<Utc>,
    pub env: HashMap<String, String>,
}

pub struct CacheManager {
    pub cache_dir: PathBuf,
}

impl CacheManager {
    pub fn new(root: &Path) -> Self {
        Self {
            cache_dir: root.join(".lattice").join("cache"),
        }
    }

    pub fn compute_hash(
        &self,
        workspace_path: &Path,
        task: &str,
        command: &str,
        pipeline_task: &PipelineTask,
    ) -> Result<String> {
        let mut hasher = Sha256::new();

        hasher.update(task.as_bytes());
        hasher.update(b"\x00");
        hasher.update(command.as_bytes());
        hasher.update(b"\x00");

        if let Some(inputs) = &pipeline_task.inputs {
            let ignore_pats = pipeline_task.ignore.as_deref().unwrap_or(&[]);
            let mut files = collect_files(workspace_path, inputs, ignore_pats)?;
            files.sort();

            for file_path in &files {
                if let Ok(content) = std::fs::read(file_path) {
                    let rel = file_path
                        .strip_prefix(workspace_path)
                        .unwrap_or(file_path)
                        .display()
                        .to_string();
                    hasher.update(rel.as_bytes());
                    hasher.update(b"\x00");
                    hasher.update(&content);
                    hasher.update(b"\x00");
                }
            }
        }

        if let Some(env_vars) = &pipeline_task.env {
            for var in env_vars {
                hasher.update(var.as_bytes());
                hasher.update(b"=");
                let val = std::env::var(var).unwrap_or_default();
                hasher.update(val.as_bytes());
                hasher.update(b"\x00");
            }
        }

        for lockfile in &[
            "package-lock.json",
            "yarn.lock",
            "pnpm-lock.yaml",
            "bun.lockb",
            "bun.lock",
            "Cargo.lock",
            "go.sum",
            "poetry.lock",
            "uv.lock",
        ] {
            let lf = workspace_path.join(lockfile);
            if lf.exists() {
                if let Ok(content) = std::fs::read(&lf) {
                    hasher.update(lockfile.as_bytes());
                    hasher.update(b"\x00");
                    hasher.update(&content);
                    hasher.update(b"\x00");
                }
            }
        }

        let result = hasher.finalize();
        Ok(hex::encode(&result[..10]))
    }

    pub fn is_cached(&self, hash: &str) -> bool {
        self.cache_dir
            .join(format!("{}.meta.json", hash))
            .exists()
    }

    #[allow(dead_code)]
    pub fn get_meta(&self, hash: &str) -> Result<CacheMeta> {
        let meta_path = self.cache_dir.join(format!("{}.meta.json", hash));
        let content = std::fs::read_to_string(&meta_path)?;
        Ok(serde_json::from_str(&content)?)
    }

    pub fn store_meta(&self, meta: &CacheMeta) -> Result<()> {
        std::fs::create_dir_all(&self.cache_dir)?;
        let meta_path = self.cache_dir.join(format!("{}.meta.json", meta.hash));
        let content = serde_json::to_string_pretty(meta)?;
        std::fs::write(meta_path, content)?;
        Ok(())
    }

    pub fn store_outputs(
        &self,
        hash: &str,
        workspace_path: &Path,
        output_patterns: &[String],
    ) -> Result<()> {
        std::fs::create_dir_all(&self.cache_dir)?;
        let tar_path = self.cache_dir.join(format!("{}.tar.gz", hash));
        let file = std::fs::File::create(&tar_path)?;
        let enc = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        let mut tar = tar::Builder::new(enc);

        for pattern in output_patterns {
            let full_pattern = workspace_path.join(pattern).display().to_string();
            if let Ok(entries) = glob::glob(&full_pattern) {
                for entry in entries.flatten() {
                    if entry.is_file() {
                        let rel = entry
                            .strip_prefix(workspace_path)
                            .unwrap_or(&entry)
                            .to_path_buf();
                        tar.append_path_with_name(&entry, &rel)?;
                    }
                }
            }
        }

        let inner = tar.into_inner()?;
        inner.finish()?;
        Ok(())
    }

    pub fn restore_outputs(&self, hash: &str, workspace_path: &Path) -> Result<()> {
        let tar_path = self.cache_dir.join(format!("{}.tar.gz", hash));
        if !tar_path.exists() {
            return Ok(());
        }
        let file = std::fs::File::open(&tar_path)?;
        let dec = flate2::read::GzDecoder::new(file);
        let mut archive = tar::Archive::new(dec);
        archive.unpack(workspace_path)?;
        Ok(())
    }
}

fn collect_files(base: &Path, patterns: &[String], ignore_patterns: &[String]) -> Result<Vec<PathBuf>> {
    let mut ignore_builder = globset::GlobSetBuilder::new();
    for pat in ignore_patterns {
        ignore_builder.add(globset::Glob::new(pat)?);
    }
    let ignore_set = ignore_builder.build()?;

    let mut files = Vec::new();
    for pattern in patterns {
        let full_pattern = base.join(pattern).display().to_string();
        if let Ok(entries) = glob::glob(&full_pattern) {
            for entry in entries.flatten() {
                if entry.is_file() {
                    let rel = entry.strip_prefix(base).unwrap_or(&entry).to_path_buf();
                    if !ignore_set.is_match(&rel) {
                        files.push(entry);
                    }
                }
            }
        }
    }

    Ok(files)
}
