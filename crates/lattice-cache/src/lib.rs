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

// Lockfiles and manifests are hashed into a task's cache key when present, since
// their contents change what a build produces.
use lattice_config::{PipelineTask, LOCKFILES, MANIFESTS};

/// Read buffer for hashing and digesting. Large enough that the syscall cost
/// disappears against the hash itself.
const READ_BUF: usize = 256 * 1024;

/// Depth cap for the input/output walk. A symlink is treated as a leaf, so this
/// is only a backstop against pathological trees.
const MAX_WALK_DEPTH: usize = 64;

/// Directories never walked when collecting inputs or outputs: our own state, and
/// version-control metadata that changes on every commit, checkout and fetch.
const NEVER_WALK: &[&str] = &[".lattice", ".git", ".hg", ".svn", ".jj"];

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
	/// Values of the task's declared `env` names as they were resolved when the
	/// key was computed. Recorded so an entry can be explained after the fact —
	/// the key itself is an opaque hash. Nothing re-applies them.
	pub env: HashMap<String, String>,
	/// sha256 hex of the artifact tarball's bytes. Used to detect corruption.
	pub output_digest: String,
	/// The task's `outputs` globs as they were when the entry was written.
	/// `restore` clears what these match before unpacking, so a hit reproduces the
	/// tree the run produced instead of layering onto whatever is already there.
	#[serde(default)]
	pub outputs: Vec<String>,
	/// Byte length of the artifact tarball. Lets `lookup` reject a truncated
	/// artifact without re-reading it.
	#[serde(default)]
	pub artifact_size: u64,
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
	/// Unpack a looked-up entry's tarball into `workspace_path`, overwriting
	/// whatever sits at those paths. Files are the whole contract: a hit runs no
	/// process, so [`CacheEntry::env`] is a record of what the key was computed
	/// from and must not be applied to the environment here.
	fn restore(&self, entry: &CacheEntry, workspace_path: &Path) -> Result<()>;
	/// Evict oldest-by-`last_used` until total cache bytes <= `max_bytes`.
	fn prune(&self, max_bytes: u64) -> Result<PruneReport>;
	/// Touch `last_used` on a hit so LRU ordering reflects reads.
	fn touch(&self, key: &str) -> Result<()>;

	/// Record what a task's key was composed of this run.
	///
	/// A miss is a key that is not there, so the entry that would explain it does
	/// not exist by definition. What does exist is the last key this task
	/// resolved to, kept here per `(workspace, task)` rather than per key.
	fn record_fingerprint(&self, _workspace: &str, _task: &str, _of: &KeyBreakdown) -> Result<()> {
		Ok(())
	}

	/// The breakdown this task last resolved to, if one was recorded.
	fn last_fingerprint(&self, _workspace: &str, _task: &str) -> Option<KeyBreakdown> {
		None
	}
}

/// The on-disk, content-addressed cache. Artifacts live at
/// `<cache_dir>/<key>.tar.gz` with metadata at `<cache_dir>/<key>.meta.json`,
/// where `cache_dir` is what `settings.cacheDir` points at.
///
/// Nothing here is versioned by layout: a key covers the running Lattice
/// version, so a release that changes what a key means retires the old entries
/// by never asking for them again, and prune reclaims them by age.
pub struct LocalStore {
	/// What `settings.cacheDir` points at.
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

	/// Where a `(workspace, task)` pair's last key breakdown is kept. One file per
	/// pair, overwritten each run, so these do not accumulate.
	fn fingerprint_path(&self, workspace: &str, task: &str) -> PathBuf {
		self.cache_dir
			.join("fingerprints")
			.join(format!("{}.json", fingerprint_id(workspace, task)))
	}

	/// A staging path unique to this process and moment, so two concurrent stores
	/// of the same key never write to each other's temporary file.
	fn tmp_path(&self, key: &str, suffix: &str) -> PathBuf {
		let nanos = std::time::SystemTime::now()
			.duration_since(std::time::UNIX_EPOCH)
			.map(|d| d.as_nanos())
			.unwrap_or(0);
		self.cache_dir
			.join(format!("{key}.{suffix}.{}.{nanos}.tmp", std::process::id()))
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

	/// Write metadata by staging it and renaming into place, so a reader either
	/// sees the previous metadata or the new metadata, never a half-written file.
	fn write_meta(&self, meta: &CacheMeta) -> Result<()> {
		std::fs::create_dir_all(&self.cache_dir)
			.with_context(|| format!("failed to create cache dir {}", self.cache_dir.display()))?;
		let path = self.meta_path(&meta.key);
		let tmp = self.tmp_path(&meta.key, "meta");
		let content = serde_json::to_string_pretty(meta)?;
		std::fs::write(&tmp, content)
			.with_context(|| format!("failed to write cache metadata at {}", tmp.display()))?;
		if let Err(e) = std::fs::rename(&tmp, &path) {
			let _ = std::fs::remove_file(&tmp);
			return Err(e)
				.with_context(|| format!("failed to write cache metadata at {}", path.display()));
		}
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
		// Meta must exist and parse. Unparseable metadata is a miss, not an error:
		// the task simply re-runs and overwrites it.
		let meta = match self.read_meta(key) {
			Ok(Some(m)) => m,
			Ok(None) => return Ok(None),
			Err(_) => return Ok(None),
		};

		// The artifact must exist, and its length must match what we recorded. The
		// length is a cheap reject that catches the common damage (a truncated or
		// half-written file) without reading anything.
		let tar_path = self.tar_path(key);
		let Ok(fs_meta) = std::fs::metadata(&tar_path) else {
			return Ok(None);
		};
		if meta.artifact_size != 0 && fs_meta.len() != meta.artifact_size {
			return Ok(None);
		}

		// Then the recorded digest, which catches damage that preserves length.
		// This re-reads the whole artifact on every lookup, so it is the obvious
		// thing to relax once there is a benchmark to relax it against.
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

		// Collect first. A task that declares `outputs` and produced none of them
		// has not done what the config says it does, and storing that as a valid
		// empty artifact would make every later run a hit that restores nothing —
		// permanently, and silently. Refuse instead; the caller surfaces it.
		let entries = collect_output_entries(workspace_path, outputs)?;
		if !outputs.is_empty() && entries.is_empty() {
			anyhow::bail!(
				"no files matched outputs {:?} — nothing was cached. Check the \
				 patterns are relative to the workspace and that the task writes there",
				outputs
			);
		}

		let tar_path = self.tar_path(key);
		let tmp_tar = self.tmp_path(key, "tar");
		let written = (|| -> Result<u64> {
			let file = std::fs::File::create(&tmp_tar)
				.with_context(|| format!("failed to create artifact {}", tmp_tar.display()))?;
			let enc = flate2::write::GzEncoder::new(file, flate2::Compression::default());
			let mut builder = tar::Builder::new(enc);
			// Store symlinks as symlinks. Following them would flatten a link into a
			// fat copy and, worse, pull in whatever it points at outside the tree.
			builder.follow_symlinks(false);

			for entry in &entries {
				let rel = entry
					.path
					.strip_prefix(workspace_path)
					.unwrap_or(&entry.path);
				builder
					.append_path_with_name(&entry.path, rel)
					.with_context(|| {
						format!("failed to add {} to artifact", entry.path.display())
					})?;
			}

			let enc = builder.into_inner()?;
			let file = enc.finish()?;
			let len = file.metadata()?.len();
			file.sync_all()?;
			Ok(len)
		})();

		let size = match written {
			Ok(size) => size,
			Err(e) => {
				let _ = std::fs::remove_file(&tmp_tar);
				return Err(e);
			}
		};

		meta.key = key.to_string();
		meta.outputs = outputs.to_vec();
		meta.artifact_size = size;
		meta.output_digest = Self::digest_file(&tmp_tar)
			.with_context(|| format!("failed to digest artifact {}", tmp_tar.display()))?;

		// Rename last: until this succeeds no reader can see a partial artifact, and
		// after it the artifact at the key is always complete.
		if let Err(e) = std::fs::rename(&tmp_tar, &tar_path) {
			let _ = std::fs::remove_file(&tmp_tar);
			return Err(e)
				.with_context(|| format!("failed to write artifact {}", tar_path.display()));
		}

		self.write_meta(&meta)?;
		Ok(())
	}

	fn restore(&self, entry: &CacheEntry, workspace_path: &Path) -> Result<()> {
		let tar_path = self.tar_path(&entry.key);
		// Open before clearing anything: if the artifact has been pruned out from
		// under us the caller re-runs the task, and it must not find the outputs
		// half-deleted when it does. Holding the handle also keeps the bytes alive
		// on unix even if a concurrent prune unlinks the path.
		let file = std::fs::File::open(&tar_path)
			.with_context(|| format!("failed to open artifact {}", tar_path.display()))?;

		// Clear what the recorded `outputs` match before unpacking, so a hit
		// reproduces the tree the run produced: files the task deletes stay
		// deleted, and content-hashed names don't pile up across generations.
		clear_outputs(workspace_path, &entry.meta.outputs)?;

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

	fn record_fingerprint(&self, workspace: &str, task: &str, of: &KeyBreakdown) -> Result<()> {
		let path = self.fingerprint_path(workspace, task);
		let Some(dir) = path.parent() else {
			return Ok(());
		};
		std::fs::create_dir_all(dir)?;
		// Staged and renamed like the metadata, so a reader never sees half a file.
		let tmp = dir.join(format!(
			"{}.{}.tmp",
			fingerprint_id(workspace, task),
			std::process::id()
		));
		std::fs::write(&tmp, serde_json::to_string(of)?)?;
		if std::fs::rename(&tmp, &path).is_err() {
			let _ = std::fs::remove_file(&tmp);
		}
		Ok(())
	}

	fn last_fingerprint(&self, workspace: &str, task: &str) -> Option<KeyBreakdown> {
		let raw = std::fs::read_to_string(self.fingerprint_path(workspace, task)).ok()?;
		serde_json::from_str(&raw).ok()
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
		let mut removed = 0usize;
		let mut bytes_freed = 0u64;

		// Retire any artifact left without metadata by an interrupted store, and
		// any staging file left behind by one. Neither can ever be read, so they
		// are pure leaked bytes — and because prune enumerates by metadata, an
		// orphaned artifact would otherwise be invisible to it and could never be
		// reclaimed.
		let (orphan_count, orphan_bytes) = self.sweep_unreadable()?;
		removed += orphan_count;
		bytes_freed += orphan_bytes;

		let read_dir = match std::fs::read_dir(&self.cache_dir) {
			Ok(rd) => rd,
			// Nothing cached yet: the sweep above was all there was to do.
			Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
				return Ok(PruneReport {
					removed,
					bytes_freed,
				})
			}
			Err(e) => {
				return Err(e).with_context(|| {
					format!("failed to read cache dir {}", self.cache_dir.display())
				})
			}
		};

		let mut keys_seen: Vec<String> = Vec::new();
		for entry in read_dir.flatten() {
			let name = entry.file_name();
			let name = name.to_string_lossy();
			let Some(key) = name.strip_suffix(".meta.json") else {
				continue;
			};
			keys_seen.push(key.to_string());
		}

		for key in keys_seen {
			// Metadata that no longer parses describes an entry nothing can use.
			// Evict it rather than aborting the whole prune on one bad file.
			let meta = match self.read_meta(&key) {
				Ok(Some(meta)) => meta,
				Ok(None) => continue,
				Err(_) => {
					let (n, b) = self.remove_entry(&key);
					removed += n;
					bytes_freed += b;
					continue;
				}
			};

			let size = self.entry_size(&key);
			total += size;
			rows.push(Row {
				key,
				last_used: meta.last_used,
				size,
			});
		}

		if total <= max_bytes {
			return Ok(PruneReport {
				removed,
				bytes_freed,
			});
		}

		// Evict oldest first until under budget.
		rows.sort_by_key(|r| r.last_used);

		for row in &rows {
			if total <= max_bytes {
				break;
			}
			let (n, freed) = self.remove_entry(&row.key);
			total -= row.size.min(total);
			removed += n;
			bytes_freed += freed;
		}

		Ok(PruneReport {
			removed,
			bytes_freed,
		})
	}
}

impl LocalStore {
	/// Combined on-disk size of an entry's artifact and metadata.
	fn entry_size(&self, key: &str) -> u64 {
		let meta = std::fs::metadata(self.meta_path(key))
			.map(|m| m.len())
			.unwrap_or(0);
		let tar = std::fs::metadata(self.tar_path(key))
			.map(|m| m.len())
			.unwrap_or(0);
		meta + tar
	}

	/// Remove one entry, metadata first.
	///
	/// The order matters: dropping the metadata makes the entry a miss immediately,
	/// so if removing the artifact then fails the entry is still unreachable and
	/// still visible to the next prune as an orphan. Removing the artifact first
	/// would leave metadata pointing at nothing, and a failure in between would
	/// leak the artifact permanently.
	fn remove_entry(&self, key: &str) -> (usize, u64) {
		let size = self.entry_size(key);
		let meta_gone = std::fs::remove_file(self.meta_path(key)).is_ok();
		let tar_gone = std::fs::remove_file(self.tar_path(key)).is_ok();
		if meta_gone || tar_gone {
			(1, size)
		} else {
			(0, 0)
		}
	}

	/// Delete artifacts with no metadata and stale staging files. Returns what it
	/// reclaimed.
	fn sweep_unreadable(&self) -> Result<(usize, u64)> {
		let mut removed = 0usize;
		let mut freed = 0u64;

		if let Ok(read_dir) = std::fs::read_dir(&self.cache_dir) {
			for entry in read_dir.flatten() {
				let name = entry.file_name();
				let name = name.to_string_lossy().into_owned();
				let stale_tmp = name.ends_with(".tmp");
				let orphan_tar = name
					.strip_suffix(".tar.gz")
					.map(|key| !self.meta_path(key).exists())
					.unwrap_or(false);
				if !stale_tmp && !orphan_tar {
					continue;
				}
				let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
				if std::fs::remove_file(entry.path()).is_ok() {
					removed += 1;
					freed += size;
				}
			}
		}

		Ok((removed, freed))
	}
}

/// A filename-safe id for a `(workspace, task)` pair. Hashed rather than
/// concatenated: either half can contain a path separator.
fn fingerprint_id(workspace: &str, task: &str) -> String {
	let mut hasher = Sha256::new();
	hash_field(&mut hasher, "workspace", workspace.as_bytes());
	hash_field(&mut hasher, "task", task.as_bytes());
	hex::encode(hasher.finalize())[..32].to_string()
}

/// All inputs that define a task's cache identity.
pub struct HashInputs<'a> {
	pub task: &'a str,
	pub command: &'a str,
	/// Declared workspace name. Two workspaces that run the same command must not
	/// share a key, or a hit in one restores the other's outputs.
	pub workspace_name: &'a str,
	pub workspace_path: &'a Path,
	/// Repo root, so lockfiles hoisted above the workspace are hashed too.
	pub repo_root: &'a Path,
	pub pipeline_task: &'a PipelineTask,
	/// Every name the task declares in `env`, each with its resolved value when it
	/// is set. Sorted by name here.
	pub env_values: &'a [(String, Option<String>)],
	/// Identity string of the resolved toolchains (empty if none).
	pub toolchain_identity: &'a str,
	pub lattice_version: &'a str,
	/// Resolved cache keys of this task's prerequisites. Sorted here. This is what
	/// makes a change anywhere upstream reach every task downstream of it.
	pub dep_keys: &'a [String],
	/// Digest of the repo's `globalDependencies`, from
	/// [`global_dependencies_digest`]. The same for every task in a run, so it is
	/// computed once rather than per task.
	pub global_digest: &'a str,
	/// Every name the repo declares in `globalEnv`, each with its resolved value
	/// when it is set. Sorted here.
	pub global_env_values: &'a [(String, Option<String>)],
}

/// The names of the parts a cache key is composed from, in hashing order.
///
/// A key is one opaque hash, which answers "did anything change" and nothing
/// else. Hashing each part separately and then hashing the parts together keeps
/// the same answer while making a miss attributable to the part that moved.
pub const KEY_COMPONENTS: &[&str] = &[
	"environment",
	"command",
	"toolchain",
	"dependencies",
	"patterns",
	"env",
	"globalEnv",
	"inputs",
	"manifests",
	"globalDependencies",
];

/// A cache key and the per-part digests it was composed from.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct KeyBreakdown {
	pub key: String,
	/// Component name -> digest, for every name in [`KEY_COMPONENTS`].
	pub components: HashMap<String, String>,
}

impl KeyBreakdown {
	/// The components whose digests differ from `previous`, in [`KEY_COMPONENTS`]
	/// order. This is what turns "cache miss" into a sentence worth reading.
	pub fn changed_from(&self, previous: &KeyBreakdown) -> Vec<&'static str> {
		KEY_COMPONENTS
			.iter()
			.copied()
			.filter(|name| self.components.get(*name) != previous.components.get(*name))
			.collect()
	}
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
/// Everything that can change what the task produces goes in: where it runs
/// (workspace name, platform, shell), what it runs (command, and the manifests a
/// command like `npm run build` indirects through), what it reads (declared
/// inputs, or the whole workspace when none are declared), what it depends on
/// (each prerequisite's own key), and what it runs with (toolchain identity,
/// declared env values, lockfiles in the workspace and at the repo root).
///
/// Input files, env pairs and dependency keys are sorted before hashing;
/// lockfiles and manifests are visited in their table order.
pub fn compute_key(inputs: &HashInputs) -> Result<String> {
	Ok(compute_key_detailed(inputs)?.key)
}

/// [`compute_key`], keeping the per-component digests it was built from.
///
/// The key is the hash of the components rather than of the fields directly, so
/// the two can never disagree about what a task's identity covers.
pub fn compute_key_detailed(inputs: &HashInputs) -> Result<KeyBreakdown> {
	let mut components: HashMap<String, String> = HashMap::with_capacity(KEY_COMPONENTS.len());

	// Where the task runs. Two workspaces running the same command must not share
	// a key, and neither must two platforms or two shells.
	components.insert(
		"environment".to_string(),
		digest_of(|h| {
			hash_field(h, "lattice_version", inputs.lattice_version.as_bytes());
			hash_field(h, "platform", platform_tag().as_bytes());
			hash_field(h, "shell", shell_tag().as_bytes());
			hash_field(h, "workspace", inputs.workspace_name.as_bytes());
			hash_field(h, "task", inputs.task.as_bytes());
		}),
	);

	components.insert(
		"command".to_string(),
		digest_of(|h| hash_field(h, "command", inputs.command.as_bytes())),
	);
	components.insert(
		"toolchain".to_string(),
		digest_of(|h| {
			hash_field(
				h,
				"toolchain_identity",
				inputs.toolchain_identity.as_bytes(),
			)
		}),
	);

	// Prerequisite keys: sorted, so scheduling order can't move the key.
	components.insert(
		"dependencies".to_string(),
		digest_of(|h| {
			let mut dep_keys: Vec<&String> = inputs.dep_keys.iter().collect();
			dep_keys.sort();
			dep_keys.dedup();
			for dep in dep_keys {
				hash_field(h, "dep.key", dep.as_bytes());
			}
		}),
	);

	// The raw pattern lists, so widening `outputs` (or narrowing `ignore`) is a
	// different key rather than a hit that restores the older, smaller file set.
	let ignore_pats = inputs.pipeline_task.ignore.as_deref().unwrap_or(&[]);
	components.insert(
		"patterns".to_string(),
		digest_of(|h| {
			for (tag, pats) in [
				("pattern.inputs", inputs.pipeline_task.inputs.as_deref()),
				("pattern.outputs", inputs.pipeline_task.outputs.as_deref()),
				("pattern.ignore", Some(ignore_pats)),
			] {
				match pats {
					None => hash_field(h, tag, b"<unset>"),
					Some(list) => {
						for pat in list {
							hash_field(h, tag, pat.as_bytes());
						}
					}
				}
			}
		}),
	);

	components.insert("env".to_string(), digest_of_env(inputs.env_values));
	components.insert(
		"globalEnv".to_string(),
		digest_of_env(inputs.global_env_values),
	);

	// A task's own outputs are never its inputs. If they were, hashing them would
	// move the key that the previous run just wrote under, and the task could never
	// hit its own entry again. Excluding them here means `ignore` does not have to.
	let mut excluded: Vec<String> = ignore_pats.to_vec();
	if let Some(outputs) = inputs.pipeline_task.outputs.as_deref() {
		excluded.extend(expand_dir_patterns(inputs.workspace_path, outputs));
	}

	// Input files. With `inputs` declared, hash exactly what it matches. Without
	// it, hash the whole workspace (minus ignored files) rather than nothing: an
	// undeclared task should re-run too eagerly, never too rarely.
	let mut files = match &inputs.pipeline_task.inputs {
		Some(patterns) => collect_matching_files(inputs.workspace_path, patterns, &excluded)?,
		None => collect_workspace_files(inputs.workspace_path, &excluded)?,
	};
	files.sort();
	files.dedup();
	components.insert(
		"inputs".to_string(),
		try_digest_of(|h| {
			for file_path in &files {
				let rel = file_path
					.strip_prefix(inputs.workspace_path)
					.unwrap_or(file_path)
					.to_string_lossy()
					.into_owned();
				hash_field(h, "input.path", rel.as_bytes());
				hash_file_into(h, "input.content", file_path)?;
			}
			Ok(())
		})?,
	);

	// Manifests and lockfiles in the workspace, then lockfiles at the repo root
	// (hoisted layouts keep the only lockfile there), in fixed order.
	components.insert(
		"manifests".to_string(),
		try_digest_of(|h| {
			for manifest in MANIFESTS {
				let path = inputs.workspace_path.join(manifest);
				if path.is_file() {
					hash_field(h, "manifest.name", manifest.as_bytes());
					hash_file_into(h, "manifest.content", &path)?;
				}
			}
			for lockfile in LOCKFILES {
				let lf = inputs.workspace_path.join(lockfile);
				if lf.is_file() {
					hash_field(h, "lockfile.name", lockfile.as_bytes());
					hash_file_into(h, "lockfile.content", &lf)?;
				}
			}
			if inputs.repo_root != inputs.workspace_path {
				for lockfile in LOCKFILES {
					let lf = inputs.repo_root.join(lockfile);
					if lf.is_file() {
						hash_field(h, "root.lockfile.name", lockfile.as_bytes());
						hash_file_into(h, "root.lockfile.content", &lf)?;
					}
				}
			}
			Ok(())
		})?,
	);

	components.insert(
		"globalDependencies".to_string(),
		digest_of(|h| hash_field(h, "global.digest", inputs.global_digest.as_bytes())),
	);

	// The key is the hash of the components, in a fixed order.
	let mut hasher = Sha256::new();
	for name in KEY_COMPONENTS {
		let digest = components
			.get(*name)
			.expect("every component in KEY_COMPONENTS is computed above");
		hash_field(&mut hasher, name, digest.as_bytes());
	}

	Ok(KeyBreakdown {
		key: hex::encode(hasher.finalize()),
		components,
	})
}

/// The digest of whatever `f` writes into a fresh hasher.
fn digest_of(f: impl FnOnce(&mut Sha256)) -> String {
	let mut hasher = Sha256::new();
	f(&mut hasher);
	hex::encode(hasher.finalize())
}

/// [`digest_of`] for a body that reads files and can fail.
fn try_digest_of(f: impl FnOnce(&mut Sha256) -> Result<()>) -> Result<String> {
	let mut hasher = Sha256::new();
	f(&mut hasher)?;
	Ok(hex::encode(hasher.finalize()))
}

/// Digest of the declared env names and their resolved values, sorted by name.
///
/// The name is hashed whether or not the variable is set, so declaring one is
/// itself a change: a name that resolves to nothing today is still a statement
/// about what the task depends on, and "declared but unset" has to be a
/// different key from "never declared" — otherwise adding a name to the list
/// quietly hits an entry computed without it.
fn digest_of_env(values: &[(String, Option<String>)]) -> String {
	let mut sorted: Vec<&(String, Option<String>)> = values.iter().collect();
	sorted.sort_by(|a, b| a.0.cmp(&b.0));
	digest_of(|h| {
		for (name, value) in sorted {
			hash_field(h, "env.name", name.as_bytes());
			match value {
				Some(v) => hash_field(h, "env.value", v.as_bytes()),
				None => hash_field(h, "env.unset", b""),
			}
		}
	})
}

/// Digest of the repo's `globalDependencies`: the pattern list, plus the path
/// and contents of every repo-root-relative file it matches.
///
/// A task's `inputs` are workspace-relative, so a file above the workspace
/// cannot be named there in a way that means the same thing for every
/// workspace. This is the same value for every task in a run, so the runner
/// computes it once and hands it to each of them.
pub fn global_dependencies_digest(repo_root: &Path, patterns: &[String]) -> Result<String> {
	if patterns.is_empty() {
		return Ok(String::new());
	}
	let expanded = expand_dir_patterns(repo_root, patterns);
	let mut files = collect_matching_files(repo_root, &expanded, &[])?;
	files.sort();
	files.dedup();
	try_digest_of(|h| {
		for pat in patterns {
			hash_field(h, "global.pattern", pat.as_bytes());
		}
		for file_path in &files {
			let rel = file_path
				.strip_prefix(repo_root)
				.unwrap_or(file_path)
				.to_string_lossy()
				.into_owned();
			hash_field(h, "global.path", rel.as_bytes());
			hash_file_into(h, "global.content", file_path)?;
		}
		Ok(())
	})
}

/// `<os>-<arch>`. A cache directory shared between runners (or between a host and
/// a container) must not let one platform's artifacts answer another's lookup.
fn platform_tag() -> String {
	format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

/// Which shell the runner will hand the command to. The same command string
/// means different things to `sh` and to `cmd`.
fn shell_tag() -> &'static str {
	if cfg!(windows) {
		"cmd /C"
	} else {
		"sh -c"
	}
}

/// Stream a file's bytes into the hasher under `tag`, length-prefixed like every
/// other field. Streaming keeps peak memory flat for large inputs.
fn hash_file_into(hasher: &mut Sha256, tag: &str, path: &Path) -> Result<()> {
	use std::io::Read;
	let mut file = std::fs::File::open(path)
		.with_context(|| format!("failed to read input file {}", path.display()))?;
	let len = file
		.metadata()
		.with_context(|| format!("failed to stat {}", path.display()))?
		.len();
	hasher.update((tag.len() as u64).to_le_bytes());
	hasher.update(tag.as_bytes());
	hasher.update(len.to_le_bytes());
	let mut buf = vec![0u8; READ_BUF];
	let mut read_total = 0u64;
	loop {
		let n = file
			.read(&mut buf)
			.with_context(|| format!("failed to read {}", path.display()))?;
		if n == 0 {
			break;
		}
		hasher.update(&buf[..n]);
		read_total += n as u64;
	}
	// A file that changed size mid-hash would desync the length prefix from the
	// bytes, so the key would be ambiguous. Refuse rather than record a lie.
	if read_total != len {
		anyhow::bail!(
			"{} changed while it was being hashed ({} bytes expected, {} read)",
			path.display(),
			len,
			read_total
		);
	}
	Ok(())
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
		walk_matching(root, base, &include_set, &ignore_set, 0, &mut files);
	}
	files.sort();
	files.dedup();
	Ok(files)
}

/// One node destined for (or restored from) an artifact.
struct OutputEntry {
	path: PathBuf,
}

/// Expand any metacharacter-free pattern that names an existing directory into
/// that directory plus everything beneath it.
///
/// Only paths are matched, and a bare directory name never equals a file path, so
/// `"dist"` on its own would otherwise capture nothing at all.
fn expand_dir_patterns(base: &Path, patterns: &[String]) -> Vec<String> {
	let mut out = Vec::with_capacity(patterns.len());
	for pat in patterns {
		out.push(pat.clone());
		let has_meta = pat.contains(['*', '?', '[', ']', '{', '}']);
		if has_meta {
			continue;
		}
		let trimmed = pat.trim_end_matches('/');
		if base.join(trimmed).is_dir() {
			out.push(format!("{trimmed}/**"));
		}
	}
	out
}

/// Collect what a task's `outputs` globs match, for archiving.
///
/// Unlike the input walk this keeps directories and symlinks as well as files, so
/// an output directory that must exist but is empty survives a round trip, and a
/// symlink comes back a symlink.
///
/// A pattern with no glob metacharacter that names a directory (`"dist"`) is
/// expanded to that directory and everything beneath it — otherwise it would
/// match nothing, since only paths are matched and a bare directory name never
/// equals a file path.
fn collect_output_entries(base: &Path, outputs: &[String]) -> Result<Vec<OutputEntry>> {
	if outputs.is_empty() {
		return Ok(Vec::new());
	}

	let patterns = expand_dir_patterns(base, outputs);

	let mut set = globset::GlobSetBuilder::new();
	for pat in &patterns {
		set.add(globset::Glob::new(pat)?);
	}
	let set = set.build()?;

	let mut roots: Vec<PathBuf> = patterns
		.iter()
		.map(|p| base.join(literal_prefix(p)))
		.collect();
	roots.sort();
	roots.dedup();

	let mut out = Vec::new();
	for root in &roots {
		walk_outputs(root, base, &set, 0, &mut out);
	}
	out.sort_by(|a, b| a.path.cmp(&b.path));
	out.dedup_by(|a, b| a.path == b.path);
	Ok(out)
}

/// Walk `path` collecting every file, symlink and directory whose `base`-relative
/// path matches `set`. Symlinks are leaves, so a link is recorded but never
/// descended into.
fn walk_outputs(
	path: &Path,
	base: &Path,
	set: &globset::GlobSet,
	depth: usize,
	out: &mut Vec<OutputEntry>,
) {
	if depth > MAX_WALK_DEPTH {
		return;
	}
	let Ok(meta) = std::fs::symlink_metadata(path) else {
		return;
	};
	let is_symlink = meta.file_type().is_symlink();

	if let Ok(rel) = path.strip_prefix(base) {
		if !rel.as_os_str().is_empty() && set.is_match(rel) {
			out.push(OutputEntry {
				path: path.to_path_buf(),
			});
		}
	}

	if is_symlink || !meta.is_dir() {
		return;
	}
	let entries = match std::fs::read_dir(path) {
		Ok(e) => e,
		Err(_) => return,
	};
	for entry in entries.flatten() {
		let child = entry.path();
		if child
			.file_name()
			.and_then(|n| n.to_str())
			.map(|n| NEVER_WALK.contains(&n))
			.unwrap_or(false)
		{
			continue;
		}
		walk_outputs(&child, base, set, depth + 1, out);
	}
}

/// Delete the files and symlinks under `base` that `outputs` matches, so a restore
/// starts from a clean slate. Directories are left in place; unpacking recreates
/// whatever it needs.
fn clear_outputs(base: &Path, outputs: &[String]) -> Result<()> {
	if outputs.is_empty() {
		return Ok(());
	}
	for entry in collect_output_entries(base, outputs)? {
		// Never step outside the workspace, whatever the pattern said.
		if entry.path.strip_prefix(base).is_err() {
			continue;
		}
		let Ok(meta) = std::fs::symlink_metadata(&entry.path) else {
			continue;
		};
		if meta.is_dir() && !meta.file_type().is_symlink() {
			continue;
		}
		let _ = std::fs::remove_file(&entry.path);
	}
	Ok(())
}

/// Every file under `base` that isn't ignored, respecting `.gitignore` and the
/// usual VCS/`.lattice` exclusions.
///
/// This is what a task with no declared `inputs` hashes. Hashing nothing would
/// make such a task cache on its first run and never re-run; hashing everything
/// costs more but can only ever be too eager.
fn collect_workspace_files(base: &Path, ignore_patterns: &[String]) -> Result<Vec<PathBuf>> {
	let mut ign = globset::GlobSetBuilder::new();
	for pat in ignore_patterns {
		ign.add(globset::Glob::new(pat)?);
	}
	let ignore_set = ign.build()?;

	let mut files = Vec::new();
	let walker = ignore::WalkBuilder::new(base)
		// Hidden files are ordinary sources (`.env.example`, dotfile configs), so
		// they are hashed; the directories worth skipping are named explicitly below.
		.hidden(false)
		// A workspace is a subdirectory of the repo, so its own `.gitignore` is
		// rarely the whole story — `node_modules/`, `dist/` and `target/` are
		// normally ignored once at the repo root. Consult ancestors too.
		.parents(true)
		// Without this, ignore rules are only applied inside a directory that has
		// a `.git`, which a workspace subdirectory does not.
		.require_git(false)
		// The user's global gitignore is deliberately excluded: it lives outside the
		// repo, so honoring it would make a cache key depend on whose machine it was
		// computed on.
		.git_global(false)
		.follow_links(false)
		.filter_entry(|entry| {
			!entry
				.file_name()
				.to_str()
				.map(|n| NEVER_WALK.contains(&n))
				.unwrap_or(false)
		})
		.build();

	for entry in walker.flatten() {
		// Symlinks are leaves (follow_links is off), so file_type is Symlink for
		// them and they are skipped rather than followed out of the workspace.
		if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
			continue;
		}
		let path = entry.path();
		let Ok(rel) = path.strip_prefix(base) else {
			continue;
		};
		if ignore_set.is_match(rel) {
			continue;
		}
		files.push(path.to_path_buf());
	}
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
/// `include_set` and not `ignore_set`.
///
/// Uses `symlink_metadata`, so a symlink is a leaf: a link pointing at a
/// directory is never descended into (which would recurse forever on a cycle)
/// and a link pointing outside the workspace never pulls foreign files in.
fn walk_matching(
	path: &Path,
	base: &Path,
	include_set: &globset::GlobSet,
	ignore_set: &globset::GlobSet,
	depth: usize,
	out: &mut Vec<PathBuf>,
) {
	if depth > MAX_WALK_DEPTH {
		return;
	}
	let Ok(meta) = std::fs::symlink_metadata(path) else {
		return;
	};
	if meta.file_type().is_symlink() {
		return;
	}
	if meta.is_file() {
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
		if child
			.file_name()
			.and_then(|n| n.to_str())
			.map(|n| NEVER_WALK.contains(&n))
			.unwrap_or(false)
		{
			continue;
		}
		walk_matching(&child, base, include_set, ignore_set, depth + 1, out);
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
		env: &'a [(String, Option<String>)],
	) -> HashInputs<'a> {
		HashInputs {
			task: "build",
			command: "cargo build",
			workspace_name: "web",
			workspace_path: ws,
			repo_root: ws,
			pipeline_task: task,
			env_values: env,
			toolchain_identity: "rust-1.80",
			lattice_version: "0.1.0",
			dep_keys: &[],
			global_digest: "",
			global_env_values: &[],
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
			outputs: Vec::new(),
			artifact_size: 0,
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

	/// Two workspaces running the same task with the same command must not share a
	/// key. When they did, a hit in one restored the other's artifacts verbatim.
	#[test]
	fn hash_changes_with_workspace_name() {
		let dir = TempDir::new().unwrap();
		let task = PipelineTask::default();
		let base = compute_key(&base_inputs(dir.path(), &task, &[])).unwrap();
		let mut other = base_inputs(dir.path(), &task, &[]);
		other.workspace_name = "docs";
		assert_ne!(
			base,
			compute_key(&other).unwrap(),
			"workspace identity must be part of the key"
		);
	}

	/// A change anywhere upstream has to reach every task downstream of it. The
	/// dependency's key is the summary of that change, so it belongs in ours.
	#[test]
	fn hash_changes_with_dependency_keys() {
		let dir = TempDir::new().unwrap();
		let task = PipelineTask::default();
		let base = compute_key(&base_inputs(dir.path(), &task, &[])).unwrap();

		let deps_a = vec!["a".repeat(64)];
		let mut with_a = base_inputs(dir.path(), &task, &[]);
		with_a.dep_keys = &deps_a;
		let key_a = compute_key(&with_a).unwrap();
		assert_ne!(base, key_a);

		let deps_b = vec!["b".repeat(64)];
		let mut with_b = base_inputs(dir.path(), &task, &[]);
		with_b.dep_keys = &deps_b;
		assert_ne!(key_a, compute_key(&with_b).unwrap());
	}

	/// Prerequisites finish in whatever order the scheduler picks, so the same set
	/// of dependency keys must hash the same however it arrives.
	#[test]
	fn dependency_key_order_does_not_matter() {
		let dir = TempDir::new().unwrap();
		let task = PipelineTask::default();
		let forward = vec!["a".repeat(64), "b".repeat(64)];
		let reverse = vec!["b".repeat(64), "a".repeat(64)];

		let mut one = base_inputs(dir.path(), &task, &[]);
		one.dep_keys = &forward;
		let mut two = base_inputs(dir.path(), &task, &[]);
		two.dep_keys = &reverse;
		assert_eq!(compute_key(&one).unwrap(), compute_key(&two).unwrap());
	}

	/// With no `inputs` declared the key used to cover no files at all, so the task
	/// hit cache forever no matter what changed in the workspace.
	#[test]
	fn undeclared_inputs_still_hash_the_workspace() {
		let dir = TempDir::new().unwrap();
		let file = dir.path().join("src/main.rs");
		write(&file, "fn main() {}");
		let task = PipelineTask::default();
		assert!(task.inputs.is_none());

		let before = compute_key(&base_inputs(dir.path(), &task, &[])).unwrap();
		write(&file, "fn main() { println!(); }");
		let after = compute_key(&base_inputs(dir.path(), &task, &[])).unwrap();
		assert_ne!(
			before, after,
			"a task with no declared inputs must still notice its sources changing"
		);
	}

	#[test]
	fn undeclared_inputs_respect_gitignore() {
		let dir = TempDir::new().unwrap();
		write(&dir.path().join(".gitignore"), "generated/\n");
		write(&dir.path().join("src/main.rs"), "fn main() {}");
		let generated = dir.path().join("generated/out.txt");
		write(&generated, "one");

		let task = PipelineTask::default();
		let before = compute_key(&base_inputs(dir.path(), &task, &[])).unwrap();
		write(&generated, "two");
		let after = compute_key(&base_inputs(dir.path(), &task, &[])).unwrap();
		assert_eq!(before, after, "gitignored files must not affect the key");
	}

	/// A task's outputs sit inside its inputs all the time (`inputs: ["**/*"]`, or no
	/// `inputs` at all). Hashing them would move the key the previous run stored
	/// under, so the task could never hit its own entry — a permanent miss.
	#[test]
	fn a_tasks_own_outputs_are_not_part_of_its_key() {
		let dir = TempDir::new().unwrap();
		write(&dir.path().join("src/main.rs"), "fn main() {}");
		let task = PipelineTask {
			inputs: Some(vec!["**/*".to_string()]),
			outputs: Some(vec!["dist/**".to_string()]),
			..Default::default()
		};
		let before = compute_key(&base_inputs(dir.path(), &task, &[])).unwrap();

		// The build runs and writes its outputs into the input glob.
		write(&dir.path().join("dist/bundle.js"), "bundled");
		let after = compute_key(&base_inputs(dir.path(), &task, &[])).unwrap();
		assert_eq!(
			before, after,
			"a task's own outputs must not move its key, or it can never hit"
		);
	}

	/// The same guarantee for a task that declares no `inputs`, where the whole
	/// workspace is hashed and the outputs are not gitignored.
	#[test]
	fn undeclared_inputs_still_exclude_declared_outputs() {
		let dir = TempDir::new().unwrap();
		write(&dir.path().join("src/main.rs"), "fn main() {}");
		let task = PipelineTask {
			outputs: Some(vec!["dist/**".to_string()]),
			..Default::default()
		};
		let before = compute_key(&base_inputs(dir.path(), &task, &[])).unwrap();
		write(&dir.path().join("dist/bundle.js"), "bundled");
		let after = compute_key(&base_inputs(dir.path(), &task, &[])).unwrap();
		assert_eq!(before, after);
	}

	/// `npm run build` is an indirection: the work it does lives in `package.json`.
	/// Hashing only the invocation meant rewriting the script was a stale hit.
	#[test]
	fn hash_changes_with_manifest_content() {
		let dir = TempDir::new().unwrap();
		let manifest = dir.path().join("package.json");
		write(&manifest, r#"{"scripts":{"build":"tsc"}}"#);
		let task = PipelineTask {
			inputs: Some(vec!["src/**/*".to_string()]),
			..Default::default()
		};
		let before = compute_key(&base_inputs(dir.path(), &task, &[])).unwrap();
		write(&manifest, r#"{"scripts":{"build":"webpack"}}"#);
		let after = compute_key(&base_inputs(dir.path(), &task, &[])).unwrap();
		assert_ne!(
			before, after,
			"the manifest that defines the command must be in the key"
		);
	}

	/// Hoisted layouts keep the only lockfile at the repo root, so a workspace-local
	/// search alone let a dependency bump go unnoticed everywhere.
	#[test]
	fn hash_changes_with_root_lockfile() {
		let root = TempDir::new().unwrap();
		let ws = root.path().join("crates/app");
		std::fs::create_dir_all(&ws).unwrap();
		let lock = root.path().join("Cargo.lock");
		write(&lock, "version = 4");

		let task = PipelineTask::default();
		let mut inputs = base_inputs(&ws, &task, &[]);
		inputs.repo_root = root.path();
		let before = compute_key(&inputs).unwrap();

		write(&lock, "version = 4 # bumped");
		let mut inputs = base_inputs(&ws, &task, &[]);
		inputs.repo_root = root.path();
		let after = compute_key(&inputs).unwrap();
		assert_ne!(before, after, "a root lockfile change must move the key");
	}

	/// A task's `inputs` are workspace-relative, so a shared file above the
	/// workspace could not be named in a way that meant the same thing for every
	/// workspace. Nothing covered it, and editing one served a stale artifact.
	#[test]
	fn a_global_dependency_change_moves_the_key() {
		let root = TempDir::new().unwrap();
		let ws = root.path().join("apps/web");
		std::fs::create_dir_all(&ws).unwrap();
		let shared = root.path().join("tsconfig.base.json");
		write(&shared, r#"{"strict":true}"#);

		let patterns = vec!["tsconfig.base.json".to_string()];
		let task = PipelineTask::default();

		let digest_before = global_dependencies_digest(root.path(), &patterns).unwrap();
		let mut before = base_inputs(&ws, &task, &[]);
		before.repo_root = root.path();
		before.global_digest = &digest_before;
		let key_before = compute_key(&before).unwrap();

		write(&shared, r#"{"strict":false}"#);

		let digest_after = global_dependencies_digest(root.path(), &patterns).unwrap();
		let mut after = base_inputs(&ws, &task, &[]);
		after.repo_root = root.path();
		after.global_digest = &digest_after;
		assert_ne!(
			key_before,
			compute_key(&after).unwrap(),
			"a repo-root file every workspace shares must reach every workspace's key"
		);
	}

	/// A directory named without metacharacters covers its whole subtree, the same
	/// way a bare `outputs` directory does.
	#[test]
	fn a_bare_global_dependency_directory_covers_its_subtree() {
		let root = TempDir::new().unwrap();
		write(&root.path().join("proto/user.proto"), "message User {}");
		let patterns = vec!["proto".to_string()];

		let before = global_dependencies_digest(root.path(), &patterns).unwrap();
		write(
			&root.path().join("proto/user.proto"),
			"message User { int id = 1; }",
		);
		let after = global_dependencies_digest(root.path(), &patterns).unwrap();
		assert_ne!(before, after);

		// A file the patterns do not name stays out of it.
		write(&root.path().join("README.md"), "unrelated");
		assert_eq!(
			global_dependencies_digest(root.path(), &patterns).unwrap(),
			after
		);
	}

	/// Declaring the list at all has to move the key, or adding a pattern that
	/// currently matches nothing would hit entries computed without it.
	#[test]
	fn the_global_dependency_pattern_list_is_part_of_the_digest() {
		let root = TempDir::new().unwrap();
		assert_eq!(global_dependencies_digest(root.path(), &[]).unwrap(), "");
		let a = global_dependencies_digest(root.path(), &["a.json".to_string()]).unwrap();
		let b = global_dependencies_digest(root.path(), &["b.json".to_string()]).unwrap();
		assert_ne!(
			a, b,
			"neither pattern matches, but they are not the same rule"
		);
		assert_ne!(a, "");
	}

	#[test]
	fn a_global_env_change_moves_the_key() {
		let dir = TempDir::new().unwrap();
		let task = PipelineTask::default();
		let one = [("NODE_ENV".to_string(), Some("development".to_string()))];
		let two = [("NODE_ENV".to_string(), Some("production".to_string()))];

		let mut a = base_inputs(dir.path(), &task, &[]);
		a.global_env_values = &one;
		let mut b = base_inputs(dir.path(), &task, &[]);
		b.global_env_values = &two;
		assert_ne!(compute_key(&a).unwrap(), compute_key(&b).unwrap());
	}

	/// The key is the hash of the components, so the two can never disagree about
	/// what a task's identity covers.
	#[test]
	fn every_declared_component_is_computed() {
		let dir = TempDir::new().unwrap();
		let task = PipelineTask::default();
		let breakdown = compute_key_detailed(&base_inputs(dir.path(), &task, &[])).unwrap();
		for name in KEY_COMPONENTS {
			assert!(
				breakdown.components.contains_key(*name),
				"component '{name}' is declared but never computed"
			);
		}
		assert_eq!(breakdown.components.len(), KEY_COMPONENTS.len());
		assert_eq!(
			breakdown.key,
			compute_key(&base_inputs(dir.path(), &task, &[])).unwrap()
		);
	}

	/// A miss is a key that is not there, so the only thing that can explain it is
	/// what the task resolved to last time.
	#[test]
	fn a_breakdown_names_the_component_that_moved() {
		let dir = TempDir::new().unwrap();
		write(&dir.path().join("src/main.rs"), "fn main() {}");
		let task = PipelineTask {
			inputs: Some(vec!["src/**/*".to_string()]),
			..Default::default()
		};

		let before = compute_key_detailed(&base_inputs(dir.path(), &task, &[])).unwrap();
		write(&dir.path().join("src/main.rs"), "fn main() { println!(); }");
		let after = compute_key_detailed(&base_inputs(dir.path(), &task, &[])).unwrap();

		assert_eq!(after.changed_from(&before), vec!["inputs"]);
		assert!(before.changed_from(&before).is_empty());
	}

	#[test]
	fn a_fingerprint_round_trips_through_the_store() {
		let cache = TempDir::new().unwrap();
		let dir = TempDir::new().unwrap();
		let store = LocalStore::new(cache.path().to_path_buf());
		let task = PipelineTask::default();
		let breakdown = compute_key_detailed(&base_inputs(dir.path(), &task, &[])).unwrap();

		assert!(store.last_fingerprint("web", "build").is_none());
		store
			.record_fingerprint("web", "build", &breakdown)
			.unwrap();
		assert_eq!(store.last_fingerprint("web", "build"), Some(breakdown));
		// Recorded per (workspace, task): another task has its own slot.
		assert!(store.last_fingerprint("web", "test").is_none());
	}

	/// `cacheDir` can legitimately point at a directory Lattice does not own
	/// outright — `.lattice`, say, beside `toolchains/` and `bin/`. Prune reclaims
	/// cache entries and the debris of interrupted stores; everything else that
	/// happens to sit there is somebody else's.
	#[test]
	fn prune_leaves_everything_that_is_not_a_cache_entry() {
		let base = TempDir::new().unwrap();
		let store = LocalStore::new(base.path().to_path_buf());
		std::fs::create_dir_all(&store.cache_dir).unwrap();

		let toolchain = base.path().join("toolchains/faketool/1.0.0-abcd/bin");
		write(&toolchain.join("faketool"), "#!/bin/sh\n");
		let installed = base.path().join("bin");
		write(&installed.join("lattice"), "the binary in use");
		// Debris from an interrupted store, which should go.
		let orphan = base.path().join("deadbeef.tar.gz");
		write(&orphan, "an artifact whose metadata never landed");

		store.prune(u64::MAX).unwrap();

		assert!(
			toolchain.join("faketool").exists(),
			"prune must not take the provisioned toolchains"
		);
		assert!(
			installed.join("lattice").exists(),
			"prune must not take the installed binary"
		);
		assert!(!orphan.exists(), "an orphaned artifact is still reclaimed");
	}

	/// Widening `outputs` must not hit an entry that captured the narrower set —
	/// the restore would silently be missing the newly-declared files.
	#[test]
	fn hash_changes_with_pattern_lists() {
		let dir = TempDir::new().unwrap();
		let narrow = PipelineTask {
			outputs: Some(vec!["dist/**".to_string()]),
			..Default::default()
		};
		let wide = PipelineTask {
			outputs: Some(vec!["dist/**".to_string(), "build/**".to_string()]),
			..Default::default()
		};
		let a = compute_key(&base_inputs(dir.path(), &narrow, &[])).unwrap();
		let b = compute_key(&base_inputs(dir.path(), &wide, &[])).unwrap();
		assert_ne!(a, b);
	}

	/// An `outputs` list that matches nothing used to store a valid empty artifact,
	/// which then hit forever and restored nothing — so the task never ran again.
	#[test]
	fn store_refuses_when_declared_outputs_match_nothing() {
		let cache = TempDir::new().unwrap();
		let ws = TempDir::new().unwrap();
		let store = LocalStore::new(cache.path().to_path_buf());

		let err = store
			.store(
				"deadbeef",
				ws.path(),
				&["dist/**".to_string()],
				meta_for("deadbeef"),
			)
			.expect_err("storing nothing under a declared outputs glob must fail");
		assert!(
			err.to_string().contains("no files matched outputs"),
			"unexpected error: {err}"
		);
		assert!(
			store.lookup("deadbeef").unwrap().is_none(),
			"a refused store must leave no entry behind"
		);
	}

	/// A task that declares no outputs is still cacheable — a lint or test run has
	/// nothing to restore but should still be skippable.
	#[test]
	fn store_allows_an_empty_outputs_list() {
		let cache = TempDir::new().unwrap();
		let ws = TempDir::new().unwrap();
		let store = LocalStore::new(cache.path().to_path_buf());
		store
			.store("feed", ws.path(), &[], meta_for("feed"))
			.unwrap();
		assert!(store.lookup("feed").unwrap().is_some());
	}

	#[test]
	fn restore_removes_files_the_cached_run_did_not_produce() {
		let cache = TempDir::new().unwrap();
		let ws = TempDir::new().unwrap();
		let outputs = vec!["dist/**".to_string()];
		write(&ws.path().join("dist/keep.js"), "kept");

		let store = LocalStore::new(cache.path().to_path_buf());
		store
			.store("k1", ws.path(), &outputs, meta_for("k1"))
			.unwrap();

		// A later run leaves an extra file behind; a hit must not preserve it.
		write(&ws.path().join("dist/stale.js"), "stale");
		let entry = store.lookup("k1").unwrap().unwrap();
		store.restore(&entry, ws.path()).unwrap();

		assert!(ws.path().join("dist/keep.js").exists());
		assert!(
			!ws.path().join("dist/stale.js").exists(),
			"restore must clear outputs the cached run did not produce"
		);
	}

	#[cfg(unix)]
	#[test]
	fn round_trip_preserves_symlinks_and_empty_dirs() {
		let cache = TempDir::new().unwrap();
		let ws = TempDir::new().unwrap();
		let dist = ws.path().join("dist");
		write(&dist.join("real.js"), "body");
		std::fs::create_dir_all(dist.join("emptydir")).unwrap();
		std::os::unix::fs::symlink("real.js", dist.join("link.js")).unwrap();

		let outputs = vec!["dist/**".to_string()];
		let store = LocalStore::new(cache.path().to_path_buf());
		store
			.store("k2", ws.path(), &outputs, meta_for("k2"))
			.unwrap();

		std::fs::remove_dir_all(&dist).unwrap();
		let entry = store.lookup("k2").unwrap().unwrap();
		store.restore(&entry, ws.path()).unwrap();

		assert!(dist.join("emptydir").is_dir(), "empty dir must survive");
		let meta = std::fs::symlink_metadata(dist.join("link.js")).unwrap();
		assert!(
			meta.file_type().is_symlink(),
			"a symlink must come back a symlink, not a copy"
		);
	}

	/// A metacharacter-free pattern naming a directory matched no path at all, so
	/// `outputs: ["dist"]` cached nothing.
	#[test]
	fn a_bare_directory_output_captures_its_subtree() {
		let cache = TempDir::new().unwrap();
		let ws = TempDir::new().unwrap();
		write(&ws.path().join("dist/nested/app.js"), "body");

		let store = LocalStore::new(cache.path().to_path_buf());
		store
			.store("k3", ws.path(), &["dist".to_string()], meta_for("k3"))
			.unwrap();

		std::fs::remove_dir_all(ws.path().join("dist")).unwrap();
		let entry = store.lookup("k3").unwrap().unwrap();
		store.restore(&entry, ws.path()).unwrap();
		assert_eq!(
			std::fs::read_to_string(ws.path().join("dist/nested/app.js")).unwrap(),
			"body"
		);
	}

	/// A directory symlink used to be followed, so a link pointing at an ancestor
	/// recursed until the stack ran out.
	#[cfg(unix)]
	#[test]
	fn a_symlink_cycle_does_not_recurse_forever() {
		let dir = TempDir::new().unwrap();
		write(&dir.path().join("src/main.rs"), "fn main() {}");
		std::os::unix::fs::symlink(dir.path(), dir.path().join("src/loop")).unwrap();

		let task = PipelineTask {
			inputs: Some(vec!["src/**/*".to_string()]),
			..Default::default()
		};
		compute_key(&base_inputs(dir.path(), &task, &[])).expect("must terminate");
	}

	/// An interrupted store leaves an artifact with no metadata. Because prune
	/// enumerated entries by metadata alone, those bytes were invisible to it and
	/// could never be reclaimed — which is what made `maxCacheSize` unenforceable.
	#[test]
	fn prune_reclaims_artifacts_with_no_metadata() {
		let cache = TempDir::new().unwrap();
		let store = LocalStore::new(cache.path().to_path_buf());
		std::fs::create_dir_all(&store.cache_dir).unwrap();
		let orphan = store.cache_dir.join("orphan.tar.gz");
		std::fs::write(&orphan, vec![0u8; 4096]).unwrap();

		let report = store.prune(u64::MAX).unwrap();
		assert!(!orphan.exists(), "an orphaned artifact must be reclaimed");
		assert_eq!(report.removed, 1);
		assert_eq!(report.bytes_freed, 4096);
	}

	/// Entries from an earlier key composition live in their own directory, so they
	/// retire wholesale instead of risking a stale hit against a new key.
	#[test]
	fn entries_live_directly_under_the_configured_dir() {
		let cache = TempDir::new().unwrap();
		let store = LocalStore::new(cache.path().to_path_buf());
		assert_eq!(store.cache_dir, cache.path());
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
		let env = vec![("A".to_string(), Some("1".to_string()))];
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
		let a = vec![("KEY".to_string(), Some("one".to_string()))];
		let b = vec![("KEY".to_string(), Some("two".to_string()))];
		let ka = compute_key(&base_inputs(dir.path(), &task, &a)).unwrap();
		let kb = compute_key(&base_inputs(dir.path(), &task, &b)).unwrap();
		assert_ne!(ka, kb);
	}

	/// Declaring a name that resolves to nothing is still a statement about what
	/// the task depends on. If it did not move the key, adding it would hit an
	/// entry computed without it — and go on hitting once the variable was set.
	#[test]
	fn hash_changes_when_an_unset_name_is_declared() {
		let dir = TempDir::new().unwrap();
		let task = PipelineTask::default();
		let declared = vec![("NEVER_SET_ANYWHERE".to_string(), None)];

		let undeclared = compute_key(&base_inputs(dir.path(), &task, &[])).unwrap();
		let unset = compute_key(&base_inputs(dir.path(), &task, &declared)).unwrap();
		assert_ne!(
			undeclared, unset,
			"declared-but-unset must not share a key with never-declared"
		);

		// And once it is set, that is a third key again.
		let set = vec![("NEVER_SET_ANYWHERE".to_string(), Some("1".to_string()))];
		assert_ne!(
			unset,
			compute_key(&base_inputs(dir.path(), &task, &set)).unwrap()
		);
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
	fn restore_does_not_apply_the_recorded_env() {
		let cache = TempDir::new().unwrap();
		let ws = TempDir::new().unwrap();
		write(&ws.path().join("out.txt"), "hello");

		let store = LocalStore::new(cache.path().join("cache"));
		let mut meta = meta_for("envk");
		meta.env.insert(
			"LATTICE_CACHE_RESTORE_PROBE".to_string(),
			"not-exported".to_string(),
		);
		store
			.store("envk", ws.path(), &["out.txt".to_string()], meta)
			.unwrap();

		let entry = store.lookup("envk").unwrap().expect("expected a hit");
		let dest = TempDir::new().unwrap();
		store.restore(&entry, dest.path()).unwrap();

		// The values survive the round trip as a record of the key's inputs...
		assert_eq!(
			entry
				.env()
				.get("LATTICE_CACHE_RESTORE_PROBE")
				.map(String::as_str),
			Some("not-exported")
		);
		// ...and restoring files is all `restore` does with them.
		assert!(
			std::env::var("LATTICE_CACHE_RESTORE_PROBE").is_err(),
			"restore must not export the recorded env"
		);
		assert!(dest.path().join("out.txt").exists());
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
		assert_eq!(
			entry.env().get("FOO").map(String::as_str),
			Some("bar"),
			"rewriting meta must not drop the recorded env"
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
