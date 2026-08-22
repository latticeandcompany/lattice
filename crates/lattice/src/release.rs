//! Fetching and laying out version-stamped release binaries under `.lattice/bin`.
//!
//! A repo holds as many versions as it has used: each is `lattice-<version>`, and
//! `lattice` is a relative symlink to whichever one `latticeVersion` names. Moving
//! to a version already on disk is a symlink swap, so switching branches costs
//! nothing; only a version that is missing is downloaded.
//!
//! Downloads go through `curl` or `wget` rather than a linked HTTP client: the
//! bootstrap installer already needs one of them, and this keeps a TLS stack out
//! of the dependency tree.

use std::ffi::OsStr;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{anyhow, bail, Context, Result};
use sha2::{Digest, Sha256};

/// Where release assets live, minus the `/v<version>/<asset>` tail.
const DEFAULT_BASE_URL: &str = "https://github.com/latticeandcompany/lattice/releases/download";

/// The API endpoint naming the newest release. It means the newest *stable* one,
/// so it 404s until a project has shipped something that is not a pre-release.
const DEFAULT_LATEST_URL: &str =
	"https://api.github.com/repos/latticeandcompany/lattice/releases/latest";

/// Every release, newest first — the fallback for when `DEFAULT_LATEST_URL` has
/// nothing to name.
const DEFAULT_LIST_URL: &str =
	"https://api.github.com/repos/latticeandcompany/lattice/releases?per_page=20";

/// The environment fallback for each flag, kept so a value already exported in
/// CI keeps working. The flag wins where both are given.
const BASE_URL_ENV: &str = "LATTICE_RELEASE_BASE_URL";
const LATEST_URL_ENV: &str = "LATTICE_RELEASE_LATEST_URL";
const LIST_URL_ENV: &str = "LATTICE_RELEASE_LIST_URL";

/// Where release lookups point, as `--release-base-url` and friends left them.
///
/// A `file://` base makes the whole install path testable without a network.
#[derive(Clone, Debug, Default)]
pub struct ReleaseUrls {
	pub base: Option<String>,
	pub latest: Option<String>,
	pub list: Option<String>,
}

impl ReleaseUrls {
	fn base(&self) -> String {
		resolve_url(self.base.as_deref(), BASE_URL_ENV, DEFAULT_BASE_URL)
	}

	fn latest(&self) -> String {
		resolve_url(self.latest.as_deref(), LATEST_URL_ENV, DEFAULT_LATEST_URL)
	}

	fn list(&self) -> String {
		resolve_url(self.list.as_deref(), LIST_URL_ENV, DEFAULT_LIST_URL)
	}
}

/// An endpoint: the flag, then the environment, then the built-in default. A
/// blank value at either stage does not count as given, so an inherited
/// `LATTICE_RELEASE_BASE_URL=` cannot silently break the download path.
fn resolve_url(flag: Option<&str>, env_name: &str, default: &str) -> String {
	let given = |v: String| Some(v).filter(|v| !v.trim().is_empty());
	flag.map(str::to_string)
		.and_then(given)
		.or_else(|| std::env::var(env_name).ok().and_then(given))
		.unwrap_or_else(|| default.to_string())
}

#[cfg(windows)]
const BIN_FILE: &str = "lattice.exe";
#[cfg(not(windows))]
const BIN_FILE: &str = "lattice";

/// Strip an optional leading `v` and reject anything that is not a version.
///
/// The pin ends up in a URL and a filename, so a value like `../../etc` must not
/// survive this.
pub fn normalize_version(raw: &str) -> Result<String> {
	let trimmed = raw.trim();
	let stripped = trimmed.strip_prefix('v').unwrap_or(trimmed);
	semver::Version::parse(stripped)
		.with_context(|| format!("'{raw}' is not a version. Write it like 0.2.0"))?;
	Ok(stripped.to_string())
}

/// The release archive for a version and target triple.
fn asset_name(version: &str, target: &str) -> String {
	format!("lattice-{version}-{target}.tar.gz")
}

/// The one checksums file covering every archive in a release.
fn checksums_name(version: &str) -> String {
	format!("lattice-{version}-checksums.txt")
}

/// The target triple this binary was built for, which is the asset it needs.
fn host_target() -> &'static str {
	env!("LATTICE_TARGET")
}

fn asset_url(urls: &ReleaseUrls, version: &str, asset: &str) -> String {
	format!("{}/v{version}/{asset}", urls.base().trim_end_matches('/'))
}

pub fn bin_dir(root: &Path) -> PathBuf {
	root.join(".lattice").join("bin")
}

/// The version-stamped binary for `version`, installed or not.
pub fn versioned_bin(root: &Path, version: &str) -> PathBuf {
	let name = if cfg!(windows) {
		format!("lattice-{version}.exe")
	} else {
		format!("lattice-{version}")
	};
	bin_dir(root).join(name)
}

/// The stable path every caller invokes: a symlink to one versioned binary.
pub fn stable_link(root: &Path) -> PathBuf {
	bin_dir(root).join(BIN_FILE)
}

pub fn is_installed(root: &Path, version: &str) -> bool {
	let path = versioned_bin(root, version);
	#[cfg(unix)]
	{
		use std::os::unix::fs::PermissionsExt;
		std::fs::metadata(&path)
			.map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
			.unwrap_or(false)
	}
	#[cfg(not(unix))]
	{
		path.is_file()
	}
}

/// Ensure `version` is on disk and return its path, downloading only if it is
/// missing. `log` receives progress lines for whichever stream the caller uses.
pub fn ensure_installed(
	root: &Path,
	version: &str,
	urls: &ReleaseUrls,
	log: &mut dyn FnMut(&str),
) -> Result<PathBuf> {
	let dest = versioned_bin(root, version);
	if is_installed(root, version) {
		return Ok(dest);
	}

	let target = host_target();
	let asset = asset_name(version, target);
	let bin_dir = bin_dir(root);
	std::fs::create_dir_all(&bin_dir)
		.with_context(|| format!("failed to create {}", bin_dir.display()))?;

	// Per-process staging: two invocations installing the same version at once
	// must not clear each other's half-finished download.
	let staging = bin_dir.join(format!(".staging-{version}-{}", std::process::id()));
	if staging.exists() {
		std::fs::remove_dir_all(&staging).ok();
	}
	std::fs::create_dir_all(&staging)
		.with_context(|| format!("failed to create {}", staging.display()))?;
	let result = install_from_release(&staging, version, &asset, &dest, urls, log);
	std::fs::remove_dir_all(&staging).ok();
	result?;

	Ok(dest)
}

fn install_from_release(
	staging: &Path,
	version: &str,
	asset: &str,
	dest: &Path,
	urls: &ReleaseUrls,
	log: &mut dyn FnMut(&str),
) -> Result<()> {
	log(&format!(
		"downloading lattice {version} ({})",
		host_target()
	));

	let archive = staging.join(asset);
	download(&asset_url(urls, version, asset), &archive)?;

	let checksums = fetch_text(&asset_url(urls, version, &checksums_name(version)))?;
	let expected = digest_for(&checksums, asset).ok_or_else(|| {
		anyhow!(
			"{} does not list {asset}. This platform may have no published build for {version}",
			checksums_name(version)
		)
	})?;
	let actual = sha256_file(&archive)?;
	if !actual.eq_ignore_ascii_case(expected) {
		bail!(
			"checksum mismatch for {asset}\n  expected {expected}\n  actual   {actual}\n\
			 Lattice installs only a binary whose checksum matches the published release"
		);
	}

	let extracted = staging.join(BIN_FILE);
	extract_binary(&archive, &extracted)?;
	if dest.exists() {
		std::fs::remove_file(dest).ok();
	}
	std::fs::rename(&extracted, dest).with_context(|| {
		format!(
			"failed to move the binary into place: {} -> {}",
			extracted.display(),
			dest.display()
		)
	})?;
	Ok(())
}

/// Point `.lattice/bin/lattice` at `lattice-<version>`.
///
/// The symlink target is relative, so moving or copying the repo does not break
/// it, and the swap goes through a rename so no invocation ever sees a missing
/// `lattice`.
pub fn link_stable(root: &Path, version: &str) -> Result<()> {
	let target = versioned_bin(root, version);
	if !target.exists() {
		bail!("{} is not installed", target.display());
	}
	let link = stable_link(root);
	let relative = target
		.file_name()
		.map(PathBuf::from)
		.unwrap_or_else(|| target.clone());

	#[cfg(unix)]
	{
		let tmp = bin_dir(root).join(format!(".lattice.link-tmp-{}", std::process::id()));
		std::fs::remove_file(&tmp).ok();
		std::os::unix::fs::symlink(&relative, &tmp)
			.with_context(|| format!("failed to create symlink {}", tmp.display()))?;
		std::fs::rename(&tmp, &link)
			.with_context(|| format!("failed to move symlink into place at {}", link.display()))?;
	}
	#[cfg(not(unix))]
	{
		// Symlinks need a privilege Windows does not grant by default, so the
		// stable path is a copy there and is replaced wholesale.
		let _ = &relative;
		std::fs::remove_file(&link).ok();
		std::fs::copy(&target, &link).with_context(|| {
			format!("failed to copy {} to {}", target.display(), link.display())
		})?;
	}
	Ok(())
}

/// The newest published release.
pub struct Latest {
	pub version: String,
	/// Whether that release is a pre-release, which it is whenever the project has
	/// not cut a stable one yet. Worth saying out loud to anyone who asked only for
	/// "latest".
	pub prerelease: bool,
}

/// The newest published release, preferring the newest stable one.
///
/// `/releases/latest` skips pre-releases entirely, so a project whose every
/// release so far is a beta gets a 404 from it and needs the full list, which is
/// ordered newest-first.
pub fn resolve_latest(urls: &ReleaseUrls) -> Result<Latest> {
	let latest_url = urls.latest();
	// A 404 from this one is an answer rather than a failure — it is what GitHub
	// says when no release is stable — so the error is dropped for the fallback.
	let stable = fetch_text(&latest_url).ok();
	if let Some(tag) = stable.as_deref().and_then(parse_tag_name) {
		return Ok(Latest {
			version: normalize_version(&tag)?,
			prerelease: false,
		});
	}

	let list_url = urls.list();
	let body = fetch_text(&list_url).context("failed to ask GitHub for the newest release")?;
	let (tag, prerelease) = parse_first_release(&body)
		.ok_or_else(|| anyhow!("no release to install. Tried {latest_url} and {list_url}"))?;
	Ok(Latest {
		version: normalize_version(&tag)?,
		prerelease,
	})
}

fn parse_tag_name(body: &str) -> Option<String> {
	let value: serde_json::Value = serde_json::from_str(body).ok()?;
	value
		.get("tag_name")?
		.as_str()
		.map(|s| s.trim().to_string())
}

/// The tag and pre-release flag of the newest entry in a `/releases` array.
///
/// Drafts are skipped rather than assumed absent: they are invisible to an
/// anonymous request but not to a request carrying a token.
fn parse_first_release(body: &str) -> Option<(String, bool)> {
	let value: serde_json::Value = serde_json::from_str(body).ok()?;
	let flag = |release: &serde_json::Value, key: &str| {
		release
			.get(key)
			.and_then(serde_json::Value::as_bool)
			.unwrap_or(false)
	};
	let release = value
		.as_array()?
		.iter()
		.find(|release| !flag(release, "draft"))?;
	let tag = release.get("tag_name")?.as_str()?.trim().to_string();
	Some((tag, flag(release, "prerelease")))
}

/// The digest a `sha256sum`-style checksums file records for one file name.
fn digest_for<'a>(checksums: &'a str, asset: &str) -> Option<&'a str> {
	checksums.lines().find_map(|line| {
		let mut parts = line.split_whitespace();
		let digest = parts.next()?;
		// `sha256sum` writes "<digest>  <name>" and BSD `shasum -a 256 -b` writes
		// "<digest> *<name>"; the leading marker is not part of the name.
		let name = parts.next()?.trim_start_matches('*');
		(name == asset || Path::new(name).file_name() == Some(OsStr::new(asset))).then_some(digest)
	})
}

fn sha256_file(path: &Path) -> Result<String> {
	let mut file =
		File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
	let mut hasher = Sha256::new();
	let mut buf = [0u8; 64 * 1024];
	loop {
		let n = file.read(&mut buf)?;
		if n == 0 {
			break;
		}
		hasher.update(&buf[..n]);
	}
	Ok(hex::encode(hasher.finalize()))
}

/// Pull the `lattice` entry out of a release archive, ignoring the rest (the
/// license and the completion scripts).
fn extract_binary(archive: &Path, dest: &Path) -> Result<()> {
	let file =
		File::open(archive).with_context(|| format!("failed to open {}", archive.display()))?;
	let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(file));
	let entries = tar
		.entries()
		.with_context(|| format!("{} is not a readable tar.gz", archive.display()))?;
	for entry in entries {
		let mut entry = entry?;
		let path = entry.path()?.to_path_buf();
		if path.file_name() != Some(OsStr::new(BIN_FILE)) {
			continue;
		}
		entry
			.unpack(dest)
			.with_context(|| format!("failed to unpack {} from {}", BIN_FILE, archive.display()))?;
		#[cfg(unix)]
		{
			use std::os::unix::fs::PermissionsExt;
			std::fs::set_permissions(dest, std::fs::Permissions::from_mode(0o755))?;
		}
		return Ok(());
	}
	bail!("{} contains no {BIN_FILE}", archive.display())
}

fn have(tool: &str) -> bool {
	Command::new(tool)
		.arg("--version")
		.stdin(Stdio::null())
		.stdout(Stdio::null())
		.stderr(Stdio::null())
		.status()
		.is_ok()
}

const NO_FETCHER: &str = "neither `curl` nor `wget` is on PATH. Install one of them, or \
download the release archive into .lattice/bin by hand";

fn download(url: &str, dest: &Path) -> Result<()> {
	let output = if have("curl") {
		Command::new("curl")
			.args(["-fsSL", "--retry", "2", "-o"])
			.arg(dest)
			.arg(url)
			.output()
			.context("failed to run curl")?
	} else if have("wget") {
		Command::new("wget")
			.args(["-q", "-O"])
			.arg(dest)
			.arg(url)
			.output()
			.context("failed to run wget")?
	} else {
		bail!(NO_FETCHER)
	};
	if !output.status.success() {
		std::fs::remove_file(dest).ok();
		bail!(
			"failed to download {url}{}",
			trailing_detail(&output.stderr)
		);
	}
	Ok(())
}

fn fetch_text(url: &str) -> Result<String> {
	let output = if have("curl") {
		Command::new("curl")
			.args(["-fsSL", "--retry", "2", url])
			.output()
			.context("failed to run curl")?
	} else if have("wget") {
		Command::new("wget")
			.args(["-q", "-O-", url])
			.output()
			.context("failed to run wget")?
	} else {
		bail!(NO_FETCHER)
	};
	if !output.status.success() {
		bail!("failed to fetch {url}{}", trailing_detail(&output.stderr));
	}
	Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn trailing_detail(stderr: &[u8]) -> String {
	let text = String::from_utf8_lossy(stderr).trim().to_string();
	if text.is_empty() {
		String::new()
	} else {
		format!(": {text}")
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn normalize_accepts_bare_and_v_prefixed() {
		assert_eq!(normalize_version("0.2.0").unwrap(), "0.2.0");
		assert_eq!(normalize_version("v0.2.0").unwrap(), "0.2.0");
		assert_eq!(normalize_version(" 1.0.0-rc.1 ").unwrap(), "1.0.0-rc.1");
	}

	#[test]
	fn normalize_rejects_non_versions() {
		// A pin reaches a URL and a filename; path traversal must not survive.
		assert!(normalize_version("../../etc/passwd").is_err());
		assert!(normalize_version("latest").is_err());
		assert!(normalize_version("").is_err());
	}

	#[test]
	fn asset_and_url_naming() {
		assert_eq!(
			asset_name("0.2.0", "aarch64-apple-darwin"),
			"lattice-0.2.0-aarch64-apple-darwin.tar.gz"
		);
		assert_eq!(checksums_name("0.2.0"), "lattice-0.2.0-checksums.txt");
		assert_eq!(
			asset_url(
				&ReleaseUrls::default(),
				"0.2.0",
				"lattice-0.2.0-x86_64-unknown-linux-gnu.tar.gz"
			),
			format!("{DEFAULT_BASE_URL}/v0.2.0/lattice-0.2.0-x86_64-unknown-linux-gnu.tar.gz")
		);
	}

	#[test]
	fn a_base_url_flag_replaces_the_default_and_loses_no_trailing_slash() {
		let urls = ReleaseUrls {
			base: Some("file:///tmp/rel/".to_string()),
			..ReleaseUrls::default()
		};
		assert_eq!(
			asset_url(&urls, "0.2.0", "lattice-0.2.0-aarch64-apple-darwin.tar.gz"),
			"file:///tmp/rel/v0.2.0/lattice-0.2.0-aarch64-apple-darwin.tar.gz"
		);
	}

	#[test]
	fn a_flag_beats_the_environment_and_a_blank_beats_neither() {
		// The env name is unset in this process, so only the flag and the default
		// are in play — enough to pin the two ends of the precedence chain.
		assert_eq!(
			resolve_url(Some("https://mirror.example/dl"), BASE_URL_ENV, "default"),
			"https://mirror.example/dl"
		);
		assert_eq!(resolve_url(None, BASE_URL_ENV, "default"), "default");
		// A blank flag is not a value; it must not shadow what comes after it.
		assert_eq!(resolve_url(Some("   "), BASE_URL_ENV, "default"), "default");
	}

	#[test]
	fn layout_is_version_stamped_under_dot_lattice() {
		let root = Path::new("/repo");
		assert_eq!(bin_dir(root), Path::new("/repo/.lattice/bin"));
		assert!(versioned_bin(root, "0.2.0")
			.file_name()
			.unwrap()
			.to_string_lossy()
			.starts_with("lattice-0.2.0"));
		assert_eq!(stable_link(root).file_name().unwrap(), BIN_FILE);
	}

	#[test]
	fn digest_lookup_handles_both_checksum_formats() {
		let file = "abc123  lattice-0.2.0-x86_64-unknown-linux-gnu.tar.gz\n\
		            def456 *lattice-0.2.0-aarch64-apple-darwin.tar.gz\n\
		            999999  ./lattice-0.2.0-x86_64-pc-windows-msvc.tar.gz\n";
		assert_eq!(
			digest_for(file, "lattice-0.2.0-x86_64-unknown-linux-gnu.tar.gz"),
			Some("abc123")
		);
		assert_eq!(
			digest_for(file, "lattice-0.2.0-aarch64-apple-darwin.tar.gz"),
			Some("def456")
		);
		assert_eq!(
			digest_for(file, "lattice-0.2.0-x86_64-pc-windows-msvc.tar.gz"),
			Some("999999")
		);
		assert_eq!(
			digest_for(file, "lattice-0.2.0-mips-unknown-none.tar.gz"),
			None
		);
	}

	#[test]
	fn tag_name_parsed_from_release_json() {
		assert_eq!(
			parse_tag_name(r#"{"tag_name":"v1.4.0","name":"1.4.0"}"#).unwrap(),
			"v1.4.0"
		);
		assert!(parse_tag_name("not json").is_none());
		assert!(parse_tag_name(r#"{"message":"Not Found"}"#).is_none());
	}

	#[test]
	fn first_release_is_the_newest_listed_and_reports_prerelease() {
		// What /releases answers while only betas exist, which is when
		// /releases/latest answers 404 and this list is all there is to read.
		let body = r#"[
			{"tag_name":"v1.0.0-beta-2","draft":false,"prerelease":true},
			{"tag_name":"v1.0.0-beta-1","draft":false,"prerelease":true}
		]"#;
		assert_eq!(
			parse_first_release(body),
			Some(("v1.0.0-beta-2".to_string(), true))
		);
	}

	#[test]
	fn first_release_skips_drafts_and_marks_a_stable_release() {
		let body = r#"[
			{"tag_name":"v2.0.0","draft":true,"prerelease":false},
			{"tag_name":"v1.4.0","draft":false,"prerelease":false}
		]"#;
		assert_eq!(
			parse_first_release(body),
			Some(("v1.4.0".to_string(), false))
		);
	}

	#[test]
	fn first_release_rejects_a_non_list() {
		assert!(parse_first_release("[]").is_none());
		assert!(parse_first_release(r#"{"message":"Not Found"}"#).is_none());
		assert!(parse_first_release("not json").is_none());
	}

	#[test]
	fn beta_tags_survive_normalization() {
		// The published tag shape: a semver pre-release whose identifier holds a
		// hyphen, which has to reach an asset filename intact.
		assert_eq!(normalize_version("v1.0.0-beta-1").unwrap(), "1.0.0-beta-1");
		assert_eq!(
			asset_name("1.0.0-beta-1", "aarch64-apple-darwin"),
			"lattice-1.0.0-beta-1-aarch64-apple-darwin.tar.gz"
		);
	}
}
