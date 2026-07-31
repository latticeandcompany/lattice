//! Lattice task-output cache.
//!
//! This crate owns cache *identity* (hashing the inputs that define a task's
//! result) and cache *storage* (storing/restoring task outputs as content-
//! addressed tarballs). A cache hit is reported only when the stored artifact
//! exists and its bytes match the digest recorded when it was written.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

// Lockfiles are hashed into a task's cache key when present in the workspace,
// since their contents change what a build produces.
use lattice_config::{PipelineTask, LOCKFILES};

/// Metadata recorded alongside every cached artifact.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheMeta {
	/// Also the artifact and meta filename stem.
	pub key: String,
	pub task: String,
	pub workspace: String,
	pub duration_ms: u64,
	/// Last time this entry was written or read; drives LRU pruning.
	pub last_used: DateTime<Utc>,
	/// Resolved environment variables captured with the task, for round-tripping.
	pub env: HashMap<String, String>,
	/// sha256 hex of the artifact tarball's bytes. Used to detect corruption.
	pub output_digest: String,
}

/// A verified cache entry returned by [`CacheStore::lookup`].
#[derive(Debug, Clone)]
pub struct CacheEntry {
	pub key: String,
	pub meta: CacheMeta,
}

impl CacheEntry {
	pub fn env(&self) -> &HashMap<String, String> {
		&self.meta.env
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PruneReport {
	pub removed: usize,
	pub bytes_freed: u64,
}

/// A swappable cache backend. [`LocalStore`] is the on-disk implementation.
pub trait CacheStore: Send + Sync {
	/// Look up a key. Returns `Some` only if the meta file parses, the artifact
	/// tarball opens, and its sha256 matches `meta.output_digest`. A missing or
	/// corrupt tarball is a miss, not a hit.
	fn lookup(&self, key: &str) -> Result<Option<CacheEntry>>;
	/// Store outputs (globbed under `workspace_path`) as `<key>.tar.gz` plus
	/// `<key>.meta.json`. Computes and records `output_digest` = sha256 of the
	/// tarball bytes.
	fn store(
		&self,
		key: &str,
		workspace_path: &Path,
		outputs: &[String],
		meta: CacheMeta,
	) -> Result<()>;
	/// Unpack a looked-up entry's tarball into `workspace_path`. The caller
	/// reads [`CacheEntry::env`] to re-export the stored variables.
	fn restore(&self, entry: &CacheEntry, workspace_path: &Path) -> Result<()>;
	/// Evict oldest-by-`last_used` until total cache bytes <= `max_bytes`.
	fn prune(&self, max_bytes: u64) -> Result<PruneReport>;
	/// Touch `last_used` on a hit so LRU ordering reflects reads.
	fn touch(&self, key: &str) -> Result<()>;
}

/// The on-disk, content-addressed cache. Artifacts live at
/// `<cache_dir>/<key>.tar.gz` with metadata at `<cache_dir>/<key>.meta.json`.
pub struct LocalStore {
	pub cache_dir: PathBuf,
}

impl LocalStore {
	pub fn new(cache_dir: PathBuf) -> Self {
		Self { cache_dir }
	}

	fn tar_path(&self, key: &str) -> PathBuf {
		self.cache_dir.join(format!("{key}.tar.gz"))
	}

	fn meta_path(&self, key: &str) -> PathBuf {
		self.cache_dir.join(format!("{key}.meta.json"))
	}

	fn read_meta(&self, key: &str) -> Result<Option<CacheMeta>> {
		let path = self.meta_path(key);
		match std::fs::read_to_string(&path) {
			Ok(content) => Ok(Some(serde_json::from_str(&content).with_context(|| {
				format!("failed to parse cache metadata at {}", path.display())
			})?)),
			Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
			Err(e) => Err(e).with_context(|| format!("failed to read {}", path.display())),
		}
	}

	fn write_meta(&self, meta: &CacheMeta) -> Result<()> {
		let path = self.meta_path(&meta.key);
		let content = serde_json::to_string_pretty(meta)?;
		std::fs::write(&path, content)
			.with_context(|| format!("failed to write cache metadata at {}", path.display()))?;
		Ok(())
	}

	/// sha256 hex of a file's bytes, or `None` if the file cannot be opened.
	fn digest_file(path: &Path) -> Option<String> {
		use std::io::Read;
		let mut file = std::fs::File::open(path).ok()?;
		let mut hasher = Sha256::new();
		let mut buf = [0u8; 8192];
		loop {
			let n = file.read(&mut buf).ok()?;
			if n == 0 {
				break;
			}
			hasher.update(&buf[..n]);
		}
		Some(hex::encode(hasher.finalize()))
	}
}

impl CacheStore for LocalStore {
	fn lookup(&self, key: &str) -> Result<Option<CacheEntry>> {
		// Meta must exist and parse.
		let meta = match self.read_meta(key)? {
			Some(m) => m,
			None => return Ok(None),
		};

		// Tarball must exist.
		let tar_path = self.tar_path(key);
		if !tar_path.exists() {
			return Ok(None);
		}

		// Tarball must open and its digest must match what we recorded.
		match Self::digest_file(&tar_path) {
			Some(digest) if digest == meta.output_digest => Ok(Some(CacheEntry {
				key: key.to_string(),
				meta,
			})),
			// Unreadable or a digest mismatch means the artifact is corrupt.
			_ => Ok(None),
		}
	}

	fn store(
		&self,
		key: &str,
		workspace_path: &Path,
		outputs: &[String],
		mut meta: CacheMeta,
	) -> Result<()> {
		std::fs::create_dir_all(&self.cache_dir)
			.with_context(|| format!("failed to create cache dir {}", self.cache_dir.display()))?;

		let tar_path = self.tar_path(key);
		{
			let file = std::fs::File::create(&tar_path)
				.with_context(|| format!("failed to create artifact {}", tar_path.display()))?;
			let enc = flate2::write::GzEncoder::new(file, flate2::Compression::default());
			let mut builder = tar::Builder::new(enc);

			// Collect output files deterministically so the tarball (and its
			// digest) is reproducible for identical inputs. Directory patterns
			// like `dist/**` capture every file beneath the directory.
			let files = collect_matching_files(workspace_path, outputs, &[])?;

			for entry in &files {
				let rel = entry.strip_prefix(workspace_path).unwrap_or(entry);
				builder
					.append_path_with_name(entry, rel)
					.with_context(|| format!("failed to add {} to artifact", entry.display()))?;
			}

			let enc = builder.into_inner()?;
			enc.finish()?;
		}

		// Record the digest of the finished tarball so lookup can verify it.
		meta.key = key.to_string();
		meta.output_digest = Self::digest_file(&tar_path)
			.with_context(|| format!("failed to digest artifact {}", tar_path.display()))?;

		self.write_meta(&meta)?;
		Ok(())
	}

	fn restore(&self, entry: &CacheEntry, workspace_path: &Path) -> Result<()> {
		let tar_path = self.tar_path(&entry.key);
		let file = std::fs::File::open(&tar_path)
			.with_context(|| format!("failed to open artifact {}", tar_path.display()))?;
		let dec = flate2::read::GzDecoder::new(file);
		let mut archive = tar::Archive::new(dec);
		archive.unpack(workspace_path).with_context(|| {
			format!(
				"failed to unpack artifact into {}",
				workspace_path.display()
			)
		})?;
		Ok(())
	}

	fn touch(&self, key: &str) -> Result<()> {
		if let Some(mut meta) = self.read_meta(key)? {
			meta.last_used = Utc::now();
			self.write_meta(&meta)?;
		}
		Ok(())
	}

	fn prune(&self, max_bytes: u64) -> Result<PruneReport> {
		// Enumerate every cache entry (keyed by its .meta.json), recording the
		// combined on-disk size of its tarball + meta and its last_used time.
		struct Row {
			key: String,
			last_used: DateTime<Utc>,
			size: u64,
		}

		let mut rows: Vec<Row> = Vec::new();
		let mut total: u64 = 0;

		let read_dir = match std::fs::read_dir(&self.cache_dir) {
			Ok(rd) => rd,
			// No cache dir yet => nothing to prune.
			Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
				return Ok(PruneReport {
					removed: 0,
					bytes_freed: 0,
				})
			}
			Err(e) => {
				return Err(e).with_context(|| {
					format!("failed to read cache dir {}", self.cache_dir.display())
				})
			}
		};

		for entry in read_dir.flatten() {
			let name = entry.file_name();
			let name = name.to_string_lossy();
			let key = match name.strip_suffix(".meta.json") {
				Some(k) => k.to_string(),
				None => continue,
			};
			let Some(meta) = self.read_meta(&key)? else {
				continue;
			};

			let meta_size = std::fs::metadata(self.meta_path(&key))
				.map(|m| m.len())
				.unwrap_or(0);
			let tar_size = std::fs::metadata(self.tar_path(&key))
				.map(|m| m.len())
				.unwrap_or(0);
			let size = meta_size + tar_size;

			total += size;
			rows.push(Row {
				key,
				last_used: meta.last_used,
				size,
			});
		}

		if total <= max_bytes {
			return Ok(PruneReport {
				removed: 0,
				bytes_freed: 0,
			});
		}

		// Evict oldest first until under budget.
		rows.sort_by_key(|r| r.last_used);

		let mut removed = 0usize;
		let mut bytes_freed = 0u64;
		for row in &rows {
			if total <= max_bytes {
				break;
			}
			let _ = std::fs::remove_file(self.tar_path(&row.key));
			let _ = std::fs::remove_file(self.meta_path(&row.key));
			total -= row.size;
			bytes_freed += row.size;
			removed += 1;
		}

		Ok(PruneReport {
			removed,
			bytes_freed,
		})
	}
}

/// All inputs that define a task's cache identity.
pub struct HashInputs<'a> {
	pub task: &'a str,
	pub command: &'a str,
	pub workspace_path: &'a Path,
	pub pipeline_task: &'a PipelineTask,
	/// Resolved `(name, value)` environment pairs. Sorted by name here.
	pub env_values: &'a [(String, String)],
	/// Identity string of the resolved toolchains (empty if none).
	pub toolchain_identity: &'a str,
	pub lattice_version: &'a str,
}

/// Write one domain-separated, length-prefixed field into the hasher.
///
/// Both the tag and the payload are length-prefixed (u64 LE), so field
/// boundaries are unambiguous regardless of the bytes involved.
fn hash_field(hasher: &mut Sha256, tag: &str, bytes: &[u8]) {
	hasher.update((tag.len() as u64).to_le_bytes());
	hasher.update(tag.as_bytes());
	hasher.update((bytes.len() as u64).to_le_bytes());
	hasher.update(bytes);
}

/// Compute the full-width (64-hex-char, 32-byte) sha256 cache key for a task.
///
/// The key incorporates the task name, command, resolved input files (globbed
/// from `pipeline_task.inputs`, honoring `pipeline_task.ignore`), tool-unique
/// lockfiles present in the workspace, resolved env values, toolchain identity,
/// and the Lattice version. Input files and env pairs are sorted before
/// hashing; lockfiles are visited in `LOCKFILES` order.
pub fn compute_key(inputs: &HashInputs) -> Result<String> {
	let mut hasher = Sha256::new();

	hash_field(
		&mut hasher,
		"lattice_version",
		inputs.lattice_version.as_bytes(),
	);
	hash_field(&mut hasher, "task", inputs.task.as_bytes());
	hash_field(&mut hasher, "command", inputs.command.as_bytes());
	hash_field(
		&mut hasher,
		"toolchain_identity",
		inputs.toolchain_identity.as_bytes(),
	);

	// Env values: sort by name for determinism.
	let mut env: Vec<&(String, String)> = inputs.env_values.iter().collect();
	env.sort_by(|a, b| a.0.cmp(&b.0));
	for (name, value) in env {
		hash_field(&mut hasher, "env.name", name.as_bytes());
		hash_field(&mut hasher, "env.value", value.as_bytes());
	}

	// Input files: resolve globs, honor ignore, sort by relative path.
	let ignore_pats = inputs.pipeline_task.ignore.as_deref().unwrap_or(&[]);
	if let Some(patterns) = &inputs.pipeline_task.inputs {
		let mut files = collect_matching_files(inputs.workspace_path, patterns, ignore_pats)?;
		files.sort();
		files.dedup();
		for file_path in &files {
			let rel = file_path
				.strip_prefix(inputs.workspace_path)
				.unwrap_or(file_path)
				.to_string_lossy()
				.into_owned();
			let content = std::fs::read(file_path)
				.with_context(|| format!("failed to read input file {}", file_path.display()))?;
			hash_field(&mut hasher, "input.path", rel.as_bytes());
			hash_field(&mut hasher, "input.content", &content);
		}
	}

	// Tool-unique lockfiles present in the workspace, in fixed order.
	for lockfile in LOCKFILES {
		let lf = inputs.workspace_path.join(lockfile);
		if lf.is_file() {
			let content = std::fs::read(&lf)
				.with_context(|| format!("failed to read lockfile {}", lf.display()))?;
			hash_field(&mut hasher, "lockfile.name", lockfile.as_bytes());
			hash_field(&mut hasher, "lockfile.content", &content);
		}
	}

	Ok(hex::encode(hasher.finalize()))
}

/// Collect files under `base` whose `base`-relative path matches any of
/// `include_patterns` and none of `ignore_patterns`.
///
/// Uses `globset` for matching, whose `**` spans both path segments and files,
/// and walks only the literal-prefix subtree of each pattern so unrelated
/// sibling trees (node_modules, target, …) are never traversed for a scoped
/// pattern like `src/**/*`.
fn collect_matching_files(
	base: &Path,
	include_patterns: &[String],
	ignore_patterns: &[String],
) -> Result<Vec<PathBuf>> {
	let mut inc = globset::GlobSetBuilder::new();
	for pat in include_patterns {
		inc.add(globset::Glob::new(pat)?);
	}
	let include_set = inc.build()?;

	let mut ign = globset::GlobSetBuilder::new();
	for pat in ignore_patterns {
		ign.add(globset::Glob::new(pat)?);
	}
	let ignore_set = ign.build()?;

	// Walk from each pattern's literal prefix. Exact-dedup the roots; any
	// remaining overlap is harmless (the final file list is deduped).
	let mut roots: Vec<PathBuf> = include_patterns
		.iter()
		.map(|p| base.join(literal_prefix(p)))
		.collect();
	roots.sort();
	roots.dedup();

	let mut files = Vec::new();
	for root in &roots {
		walk_matching(root, base, &include_set, &ignore_set, &mut files);
	}
	files.sort();
	files.dedup();
	Ok(files)
}

/// The literal directory prefix of a glob pattern: the leading components up to
/// (but not including) the first component containing a glob metacharacter.
fn literal_prefix(pattern: &str) -> PathBuf {
	let mut prefix = PathBuf::new();
	for comp in Path::new(pattern).components() {
		let s = comp.as_os_str().to_string_lossy();
		if s.contains(['*', '?', '[', ']', '{', '}']) {
			break;
		}
		prefix.push(comp);
	}
	prefix
}

/// Recursively walk `path`, pushing files whose `base`-relative path matches
/// `include_set` and not `ignore_set`. Never descends into `.lattice`.
fn walk_matching(
	path: &Path,
	base: &Path,
	include_set: &globset::GlobSet,
	ignore_set: &globset::GlobSet,
	out: &mut Vec<PathBuf>,
) {
	if path.is_file() {
		if let Ok(rel) = path.strip_prefix(base) {
			if include_set.is_match(rel) && !ignore_set.is_match(rel) {
				out.push(path.to_path_buf());
			}
		}
		return;
	}
	let entries = match std::fs::read_dir(path) {
		Ok(e) => e,
		Err(_) => return,
	};
	for entry in entries.flatten() {
		let child = entry.path();
		if child.file_name().map(|n| n == ".lattice").unwrap_or(false) {
			continue;
		}
		walk_matching(&child, base, include_set, ignore_set, out);
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::collections::HashMap;
	use tempfile::TempDir;

	fn write(path: &Path, content: &str) {
		if let Some(parent) = path.parent() {
			std::fs::create_dir_all(parent).unwrap();
		}
		std::fs::write(path, content).unwrap();
	}

	fn base_inputs<'a>(
		ws: &'a Path,
		task: &'a PipelineTask,
		env: &'a [(String, String)],
	) -> HashInputs<'a> {
		HashInputs {
			task: "build",
			command: "cargo build",
			workspace_path: ws,
			pipeline_task: task,
			env_values: env,
			toolchain_identity: "rust-1.80",
			lattice_version: "0.1.0",
		}
	}

	fn meta_for(key: &str) -> CacheMeta {
		let mut env = HashMap::new();
		env.insert("FOO".to_string(), "bar".to_string());
		CacheMeta {
			key: key.to_string(),
			task: "build".to_string(),
			workspace: "web".to_string(),
			duration_ms: 42,
			last_used: Utc::now(),
			env,
			output_digest: String::new(),
		}
	}

	#[test]
	fn directory_glob_output_captures_files_recursively() {
		// A `dist/**` output pattern matches the directory; the store must still
		// capture the files beneath it.
		let dir = TempDir::new().unwrap();
		let ws = dir.path();
		write(&ws.join("dist/out.txt"), "hello");
		write(&ws.join("dist/nested/deep.txt"), "deep");

		let store = LocalStore::new(ws.join(".lattice/cache"));
		let key = "abc123";
		store
			.store(key, ws, &["dist/**".to_string()], meta_for(key))
			.unwrap();

		// The lookup must verify (non-empty, digest matches) and restore must
		// recreate both files after we wipe dist/.
		let entry = store.lookup(key).unwrap().expect("must be a hit");
		std::fs::remove_dir_all(ws.join("dist")).unwrap();
		store.restore(&entry, ws).unwrap();
		assert_eq!(
			std::fs::read_to_string(ws.join("dist/out.txt")).unwrap(),
			"hello"
		);
		assert_eq!(
			std::fs::read_to_string(ws.join("dist/nested/deep.txt")).unwrap(),
			"deep"
		);
	}

	#[test]
	fn keys_are_full_width_hex() {
		let dir = TempDir::new().unwrap();
		let task = PipelineTask::default();
		let key = compute_key(&base_inputs(dir.path(), &task, &[])).unwrap();
		assert_eq!(key.len(), 64, "sha256 hex must be 64 chars");
		assert!(key.chars().all(|c| c.is_ascii_hexdigit()));
	}

	#[test]
	fn hash_is_stable_for_identical_inputs() {
		let dir = TempDir::new().unwrap();
		write(&dir.path().join("src/main.rs"), "fn main() {}");
		let task = PipelineTask {
			inputs: Some(vec!["src/*.rs".to_string()]),
			..Default::default()
		};
		let env = vec![("A".to_string(), "1".to_string())];
		let k1 = compute_key(&base_inputs(dir.path(), &task, &env)).unwrap();
		let k2 = compute_key(&base_inputs(dir.path(), &task, &env)).unwrap();
		assert_eq!(k1, k2);
	}

	#[test]
	fn hash_changes_with_command() {
		let dir = TempDir::new().unwrap();
		let task = PipelineTask::default();
		let base = compute_key(&base_inputs(dir.path(), &task, &[])).unwrap();
		let mut other = base_inputs(dir.path(), &task, &[]);
		other.command = "cargo test";
		assert_ne!(base, compute_key(&other).unwrap());
	}

	#[test]
	fn hash_changes_with_input_content() {
		let dir = TempDir::new().unwrap();
		let file = dir.path().join("src/main.rs");
		write(&file, "fn main() {}");
		let task = PipelineTask {
			inputs: Some(vec!["src/*.rs".to_string()]),
			..Default::default()
		};
		let before = compute_key(&base_inputs(dir.path(), &task, &[])).unwrap();
		write(&file, "fn main() { println!(); }");
		let after = compute_key(&base_inputs(dir.path(), &task, &[])).unwrap();
		assert_ne!(before, after);
	}

	#[test]
	fn hash_changes_with_env_value() {
		let dir = TempDir::new().unwrap();
		let task = PipelineTask::default();
		let a = vec![("KEY".to_string(), "one".to_string())];
		let b = vec![("KEY".to_string(), "two".to_string())];
		let ka = compute_key(&base_inputs(dir.path(), &task, &a)).unwrap();
		let kb = compute_key(&base_inputs(dir.path(), &task, &b)).unwrap();
		assert_ne!(ka, kb);
	}

	#[test]
	fn hash_changes_with_toolchain_and_version() {
		let dir = TempDir::new().unwrap();
		let task = PipelineTask::default();
		let base = compute_key(&base_inputs(dir.path(), &task, &[])).unwrap();

		let mut tc = base_inputs(dir.path(), &task, &[]);
		tc.toolchain_identity = "rust-1.81";
		assert_ne!(base, compute_key(&tc).unwrap());

		let mut ver = base_inputs(dir.path(), &task, &[]);
		ver.lattice_version = "0.2.0";
		assert_ne!(base, compute_key(&ver).unwrap());
	}

	#[test]
	fn hash_honors_ignore_globs() {
		let dir = TempDir::new().unwrap();
		write(&dir.path().join("src/main.rs"), "fn main() {}");
		write(&dir.path().join("src/main.log"), "noise");
		let task = PipelineTask {
			inputs: Some(vec!["src/*".to_string()]),
			ignore: Some(vec!["**/*.log".to_string()]),
			..Default::default()
		};
		let before = compute_key(&base_inputs(dir.path(), &task, &[])).unwrap();
		// Change an ignored file: the key must not move.
		write(&dir.path().join("src/main.log"), "different noise");
		let after = compute_key(&base_inputs(dir.path(), &task, &[])).unwrap();
		assert_eq!(before, after, "ignored files must not affect the key");
	}

	#[test]
	fn store_lookup_restore_round_trip() {
		let cache = TempDir::new().unwrap();
		let ws = TempDir::new().unwrap();
		write(&ws.path().join("dist/app.js"), "console.log(1)");
		write(&ws.path().join("dist/app.css"), "body{}");

		let store = LocalStore::new(cache.path().join("cache"));
		store
			.store(
				"abc",
				ws.path(),
				&["dist/**/*".to_string()],
				meta_for("abc"),
			)
			.unwrap();

		// lookup verifies and returns the entry with a matching digest.
		let entry = store.lookup("abc").unwrap().expect("expected a hit");
		assert_eq!(entry.key, "abc");
		assert_eq!(entry.meta.output_digest.len(), 64);
		assert_eq!(entry.env().get("FOO").map(String::as_str), Some("bar"));

		// restore recreates the files in a fresh workspace.
		let dest = TempDir::new().unwrap();
		store.restore(&entry, dest.path()).unwrap();
		assert_eq!(
			std::fs::read_to_string(dest.path().join("dist/app.js")).unwrap(),
			"console.log(1)"
		);
		assert_eq!(
			std::fs::read_to_string(dest.path().join("dist/app.css")).unwrap(),
			"body{}"
		);
	}

	#[test]
	fn missing_meta_is_a_miss() {
		let cache = TempDir::new().unwrap();
		let store = LocalStore::new(cache.path().join("cache"));
		assert!(store.lookup("nope").unwrap().is_none());
	}

	#[test]
	fn corrupt_tarball_is_a_miss_not_a_false_hit() {
		let cache = TempDir::new().unwrap();
		let ws = TempDir::new().unwrap();
		write(&ws.path().join("out.txt"), "hello");
		let store = LocalStore::new(cache.path().join("cache"));
		store
			.store("k", ws.path(), &["out.txt".to_string()], meta_for("k"))
			.unwrap();

		// It is a hit while intact.
		assert!(store.lookup("k").unwrap().is_some());

		// Corrupt the tarball bytes so the recorded digest no longer matches.
		std::fs::write(store.tar_path("k"), b"garbage").unwrap();
		assert!(
			store.lookup("k").unwrap().is_none(),
			"corrupt tarball must be a miss"
		);
	}

	#[test]
	fn missing_tarball_meta_only_is_a_miss() {
		let cache = TempDir::new().unwrap();
		let ws = TempDir::new().unwrap();
		write(&ws.path().join("out.txt"), "hello");
		let store = LocalStore::new(cache.path().join("cache"));
		store
			.store("k", ws.path(), &["out.txt".to_string()], meta_for("k"))
			.unwrap();

		std::fs::remove_file(store.tar_path("k")).unwrap();
		assert!(
			store.lookup("k").unwrap().is_none(),
			"meta without a tarball must be a miss"
		);
	}

	#[test]
	fn touch_updates_last_used() {
		let cache = TempDir::new().unwrap();
		let ws = TempDir::new().unwrap();
		write(&ws.path().join("out.txt"), "hello");
		let store = LocalStore::new(cache.path().join("cache"));
		let mut meta = meta_for("k");
		meta.last_used = Utc::now() - chrono::Duration::hours(1);
		let before = meta.last_used;
		store
			.store("k", ws.path(), &["out.txt".to_string()], meta)
			.unwrap();

		store.touch("k").unwrap();
		let entry = store.lookup("k").unwrap().unwrap();
		assert!(
			entry.meta.last_used > before,
			"touch must advance last_used"
		);
	}

	#[test]
	fn prune_evicts_oldest_first_under_budget() {
		let cache = TempDir::new().unwrap();
		let store = LocalStore::new(cache.path().join("cache"));

		// Three entries with distinct, increasing last_used times and known
		// content so each artifact has a nonzero size.
		let ws = TempDir::new().unwrap();
		write(&ws.path().join("out.txt"), &"x".repeat(4096));

		for (i, key) in ["old", "mid", "new"].iter().enumerate() {
			let mut meta = meta_for(key);
			meta.last_used = Utc::now() - chrono::Duration::hours(3 - i as i64);
			store
				.store(key, ws.path(), &["out.txt".to_string()], meta)
				.unwrap();
		}

		// Measure per-entry size, then set a budget that fits ~2 entries.
		let one = std::fs::metadata(store.tar_path("new")).unwrap().len()
			+ std::fs::metadata(store.meta_path("new")).unwrap().len();
		let budget = one * 2 + one / 2;

		let report = store.prune(budget).unwrap();
		assert_eq!(report.removed, 1, "exactly the oldest entry should go");
		assert!(report.bytes_freed > 0);

		// Oldest gone, newer two kept.
		assert!(store.lookup("old").unwrap().is_none());
		assert!(store.lookup("mid").unwrap().is_some());
		assert!(store.lookup("new").unwrap().is_some());

		// Total now under budget.
		let remaining: u64 = ["mid", "new"]
			.iter()
			.map(|k| {
				std::fs::metadata(store.tar_path(k)).unwrap().len()
					+ std::fs::metadata(store.meta_path(k)).unwrap().len()
			})
			.sum();
		assert!(remaining <= budget);
	}

	#[test]
	fn prune_is_noop_under_budget() {
		let cache = TempDir::new().unwrap();
		let ws = TempDir::new().unwrap();
		write(&ws.path().join("out.txt"), "hi");
		let store = LocalStore::new(cache.path().join("cache"));
		store
			.store("k", ws.path(), &["out.txt".to_string()], meta_for("k"))
			.unwrap();

		let report = store.prune(u64::MAX).unwrap();
		assert_eq!(report.removed, 0);
		assert_eq!(report.bytes_freed, 0);
		assert!(store.lookup("k").unwrap().is_some());
	}

	#[test]
	fn prune_missing_cache_dir_is_noop() {
		let cache = TempDir::new().unwrap();
		let store = LocalStore::new(cache.path().join("does-not-exist"));
		let report = store.prune(10).unwrap();
		assert_eq!(report.removed, 0);
		assert_eq!(report.bytes_freed, 0);
	}
}
