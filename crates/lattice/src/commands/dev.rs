use anyhow::{bail, Context, Result};
use clap::Args;
use console::style;
use std::path::{Path, PathBuf};

use lattice_config::{find_root, load_config};

/// Relative path (from repo root) of the stable symlink every command resolves.
const STABLE_LINK: &str = ".lattice/bin/lattice";
/// Relative path (from repo root) of the freshly compiled dev binary.
const DEV_BINARY: &str = "target/debug/lattice";

#[derive(Args, Debug)]
pub struct DevLinkArgs {
    #[arg(
        long,
        help = "Skip the `cargo build` step and only repoint the symlink at the existing dev binary"
    )]
    pub no_build: bool,
}

#[derive(Args, Debug)]
pub struct DevUnlinkArgs {}

impl DevLinkArgs {
    pub async fn execute(&self) -> Result<()> {
        let root = repo_root()?;

        if !self.no_build {
            println!(
                "{} building dev binary ({}) ...",
                style("◇").dim(),
                style("cargo build").dim()
            );
            run_cargo_build(&root)?;
        }

        let dev_binary = root.join(DEV_BINARY);
        if !dev_binary.exists() {
            bail!(
                "Dev binary not found at {}. Run `cargo build` first (or drop --no-build).",
                dev_binary.display()
            );
        }
        // Absolute, canonicalized target so the symlink is stable regardless of cwd.
        let target = dev_binary
            .canonicalize()
            .with_context(|| format!("Failed to resolve {}", dev_binary.display()))?;

        let link = root.join(STABLE_LINK);
        repoint_symlink(&link, &target)?;

        println!(
            "{} linked {} → {}",
            style("✓").green().bold(),
            style(STABLE_LINK).bold(),
            style(format!("dev ({})", DEV_BINARY)).cyan()
        );
        Ok(())
    }
}

impl DevUnlinkArgs {
    pub async fn execute(&self) -> Result<()> {
        let root = repo_root()?;
        let config = load_config(&root)?;

        let version = config.lattice_version.ok_or_else(|| {
            anyhow::anyhow!(
                "No \"latticeVersion\" is pinned in lattice.json; cannot determine the release binary to restore."
            )
        })?;

        let release_name = format!("lattice-{version}");
        let release_binary = root.join(".lattice").join("bin").join(&release_name);

        // Non-destructive: if the pinned release isn't installed, do NOT corrupt
        // anything. Leave the current symlink untouched and tell the user how to fix it.
        if !release_binary.exists() {
            bail!(
                "Pinned release binary {} is not installed at {}.\n\
                 The dev symlink was left unchanged. Bootstrap/install the pinned release first \
                 (e.g. run scripts/dev-unlink.sh after installing, or install lattice {version}),\n\
                 then re-run `lattice dev-unlink`.",
                style(&release_name).bold(),
                release_binary.display(),
            );
        }

        let link = root.join(STABLE_LINK);
        // Point the stable symlink at the versioned release binary (relative target keeps
        // it portable and mirrors the bootstrap install layout in PRD §9).
        let relative_target = Path::new(&release_name);
        repoint_symlink(&link, relative_target)?;

        println!(
            "{} linked {} → {}",
            style("✓").green().bold(),
            style(STABLE_LINK).bold(),
            style(format!("release {version}")).cyan()
        );
        Ok(())
    }
}

fn repo_root() -> Result<PathBuf> {
    let cwd = std::env::current_dir()?;
    find_root(&cwd).ok_or_else(|| {
        anyhow::anyhow!("No lattice.json found in this directory or any parent directory.")
    })
}

fn run_cargo_build(root: &Path) -> Result<()> {
    let status = std::process::Command::new("cargo")
        .arg("build")
        .current_dir(root)
        .status()
        .context("Failed to invoke `cargo build`. Is cargo installed and on PATH?")?;
    if !status.success() {
        bail!("`cargo build` failed; not repointing the symlink.");
    }
    Ok(())
}

/// Atomically-ish repoint `link` at `target`.
///
/// This is non-destructive with respect to the versioned release binary: it only ever
/// removes and recreates the `link` path itself (the symlink), never the file it points at.
/// The parent directory of `link` is created if it does not exist.
#[cfg(unix)]
pub fn repoint_symlink(link: &Path, target: &Path) -> Result<()> {
    use std::os::unix::fs::symlink;

    if let Some(parent) = link.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create {}", parent.display()))?;
    }

    // Remove only the existing symlink/file at the link path. symlink_metadata does not
    // follow the link, so this never touches the target of a pre-existing symlink.
    match std::fs::symlink_metadata(link) {
        Ok(meta) => {
            if meta.file_type().is_dir() {
                bail!(
                    "Refusing to replace {}: it is a directory, not a symlink.",
                    link.display()
                );
            }
            std::fs::remove_file(link)
                .with_context(|| format!("Failed to remove existing {}", link.display()))?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(e).with_context(|| format!("Failed to inspect {}", link.display()));
        }
    }

    symlink(target, link).with_context(|| {
        format!(
            "Failed to create symlink {} → {}",
            link.display(),
            target.display()
        )
    })?;
    Ok(())
}

#[cfg(not(unix))]
pub fn repoint_symlink(_link: &Path, _target: &Path) -> Result<()> {
    bail!("`dev-link`/`dev-unlink` symlink hotswap is only supported on Unix platforms.");
}

/// Which build the stable `.lattice/bin/lattice` symlink currently resolves to.
#[derive(Debug, PartialEq, Eq)]
pub enum LinkedBuild {
    /// Symlink is absent (or unreadable): no build is linked.
    None,
    /// Symlink points into `target/debug` — a locally built dev binary.
    Dev,
    /// Symlink points at a `lattice-<version>` release binary.
    Release(String),
    /// Symlink exists but points somewhere unexpected.
    Unknown(String),
}

impl LinkedBuild {
    /// Human-readable summary suitable for `lattice version` output.
    pub fn describe(&self) -> String {
        match self {
            LinkedBuild::None => "none".to_string(),
            LinkedBuild::Dev => format!("dev ({DEV_BINARY})"),
            LinkedBuild::Release(v) => format!("release {v}"),
            LinkedBuild::Unknown(t) => format!("unknown ({t})"),
        }
    }
}

/// Classify a resolved symlink target path into a [`LinkedBuild`]. Pure function so it's
/// unit-testable without touching the filesystem. `target` is the raw symlink target
/// (may be relative, e.g. `lattice-0.1.0`, or absolute, e.g. `/repo/target/debug/lattice`).
pub fn classify_link_target(target: &Path) -> LinkedBuild {
    let target_str = target.to_string_lossy();

    // A dev binary lives under `target/debug/`.
    if target
        .components()
        .collect::<Vec<_>>()
        .windows(2)
        .any(|w| w[0].as_os_str() == "target" && w[1].as_os_str() == "debug")
    {
        return LinkedBuild::Dev;
    }

    // A release binary is named `lattice-<version>`.
    if let Some(file_name) = target.file_name().and_then(|n| n.to_str()) {
        if let Some(version) = file_name.strip_prefix("lattice-") {
            if !version.is_empty() {
                return LinkedBuild::Release(version.to_string());
            }
        }
    }

    LinkedBuild::Unknown(target_str.into_owned())
}

/// Read the stable symlink at `root/.lattice/bin/lattice` and classify what it points at.
/// Returns [`LinkedBuild::None`] if the symlink does not exist.
pub fn linked_build(root: &Path) -> LinkedBuild {
    let link = root.join(STABLE_LINK);
    match std::fs::read_link(&link) {
        Ok(target) => classify_link_target(&target),
        Err(_) => LinkedBuild::None,
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn scratch_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "lattice-dev-test-{}-{}-{}",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn classify_dev_target_absolute() {
        let t = Path::new("/home/dev/lattice/target/debug/lattice");
        assert_eq!(classify_link_target(t), LinkedBuild::Dev);
    }

    #[test]
    fn classify_dev_target_relative() {
        let t = Path::new("../../target/debug/lattice");
        assert_eq!(classify_link_target(t), LinkedBuild::Dev);
    }

    #[test]
    fn classify_release_target() {
        let t = Path::new("lattice-0.1.0");
        assert_eq!(
            classify_link_target(t),
            LinkedBuild::Release("0.1.0".to_string())
        );
    }

    #[test]
    fn classify_release_target_absolute() {
        let t = Path::new("/repo/.lattice/bin/lattice-1.2.3");
        assert_eq!(
            classify_link_target(t),
            LinkedBuild::Release("1.2.3".to_string())
        );
    }

    #[test]
    fn classify_unknown_target() {
        let t = Path::new("/usr/local/bin/lattice");
        assert_eq!(
            classify_link_target(t),
            LinkedBuild::Unknown("/usr/local/bin/lattice".to_string())
        );
    }

    #[test]
    fn describe_variants() {
        assert_eq!(LinkedBuild::None.describe(), "none");
        assert_eq!(LinkedBuild::Dev.describe(), "dev (target/debug/lattice)");
        assert_eq!(
            LinkedBuild::Release("0.1.0".to_string()).describe(),
            "release 0.1.0"
        );
    }

    #[test]
    fn repoint_creates_and_replaces_symlink() {
        let dir = scratch_dir("repoint");
        let bin_dir = dir.join(".lattice").join("bin");
        let link = bin_dir.join("lattice");

        // Two fake targets.
        let target_a = dir.join("target-a");
        let target_b = dir.join("target-b");
        std::fs::write(&target_a, b"a").unwrap();
        std::fs::write(&target_b, b"b").unwrap();

        // First link: parent dir is created, symlink is made.
        repoint_symlink(&link, &target_a).unwrap();
        assert_eq!(std::fs::read_link(&link).unwrap(), target_a);

        // Repoint: existing symlink replaced, target file untouched.
        repoint_symlink(&link, &target_b).unwrap();
        assert_eq!(std::fs::read_link(&link).unwrap(), target_b);
        assert!(target_a.exists(), "repoint must not delete the old target");
        assert!(target_b.exists());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn linked_build_reads_symlink() {
        let dir = scratch_dir("linked");
        let bin_dir = dir.join(".lattice").join("bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        let link = bin_dir.join("lattice");

        // No symlink yet.
        assert_eq!(linked_build(&dir), LinkedBuild::None);

        // Point at a release-style target.
        let release = bin_dir.join("lattice-0.1.0");
        std::fs::write(&release, b"x").unwrap();
        repoint_symlink(&link, Path::new("lattice-0.1.0")).unwrap();
        assert_eq!(
            linked_build(&dir),
            LinkedBuild::Release("0.1.0".to_string())
        );

        std::fs::remove_dir_all(&dir).ok();
    }
}
