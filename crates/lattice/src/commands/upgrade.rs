use std::path::Path;

use anyhow::{Context, Result};
use clap::Args;
use console::style;

use lattice_config::find_root;
use lattice_output::banner_line;

use crate::cli::BIN_VERSION;
use crate::release;

#[derive(Args, Debug)]
#[command(long_about = "Move this repo to another version of Lattice.\n\n\
Installs the version into .lattice/bin, points .lattice/bin/lattice at it, and \
writes it to `latticeVersion` in lattice.json. Every later invocation reads that \
pin, so commit the change and everyone on the repo gets the same build.\n\n\
Examples:\n  \
lattice upgrade 0.2.0\n  \
lattice upgrade latest")]
pub struct UpgradeArgs {
	/// Version to move to (e.g. 0.2.0), or `latest` for the newest release.
	#[arg(value_name = "VERSION")]
	pub version: String,
}

impl UpgradeArgs {
	pub async fn execute(&self) -> Result<()> {
		let cwd = std::env::current_dir()?;
		let root = find_root(&cwd).ok_or_else(|| {
			anyhow::anyhow!(
				"no lattice.json found in this directory or any parent; \
                 run `lattice init` to create one"
			)
		})?;

		let config_path = root.join("lattice.json");
		let text = std::fs::read_to_string(&config_path)
			.with_context(|| format!("failed to read {}", config_path.display()))?;
		let current = crate::drift::parse_pin(&text).version;

		let target = if self.version.eq_ignore_ascii_case("latest") {
			println!("{}", banner_line("upgrade"));
			println!("  resolving the newest release ...");
			let latest = release::resolve_latest()?;
			if latest.prerelease {
				println!(
					"  {} is a pre-release — no stable release yet",
					style(&latest.version).bold()
				);
			}
			latest.version
		} else {
			let target = release::normalize_version(&self.version)?;
			println!("{}", banner_line("upgrade"));
			target
		};

		let already_pinned = current.as_deref() == Some(target.as_str());
		if already_pinned && release::is_installed(&root, &target) {
			// Still relink: a pin that is right while the symlink is not is the one
			// case where doing nothing leaves the repo running the wrong binary.
			release::link_stable(&root, &target)?;
			println!("  already on {}", style(&target).bold());
			return Ok(());
		}

		release::ensure_installed(&root, &target, &mut |line| println!("  {line}"))?;
		release::link_stable(&root, &target)?;

		if !already_pinned {
			let updated = set_pin(&text, &target);
			std::fs::write(&config_path, updated)
				.with_context(|| format!("failed to write {}", config_path.display()))?;
		}

		match current.as_deref() {
			Some(from) => println!("  {} → {}", from, style(&target).bold()),
			None => println!("  pinned {}", style(&target).bold()),
		}
		println!();
		println!(
			"lattice.json now pins {}. Commit it so the whole repo moves together.",
			target
		);
		if BIN_VERSION != target {
			println!("Run {} to use it.", style(run_hint(&root, &cwd)).bold());
		}
		Ok(())
	}
}

/// How to invoke the newly linked binary from where the user is standing.
fn run_hint(root: &Path, cwd: &Path) -> String {
	let link = release::stable_link(root);
	let shown = link.strip_prefix(cwd).unwrap_or(&link);
	if shown == link {
		link.display().to_string()
	} else {
		format!("./{}", shown.display())
	}
}

/// Rewrite `latticeVersion` in a `lattice.json` without reformatting the rest.
///
/// The file is the user's, hand-edited and diffed, so it is edited as text
/// rather than reserialized: a round trip through `serde_json` would drop
/// comment-free formatting choices, key order, and indentation.
fn set_pin(text: &str, version: &str) -> String {
	if let Some((start, end)) = pin_value_span(text) {
		let mut out = String::with_capacity(text.len() + version.len());
		out.push_str(&text[..start]);
		out.push_str(version);
		out.push_str(&text[end..]);
		return out;
	}

	// No pin yet: add one as the first key, after `$schema` when it leads.
	let indent = detect_indent(text);
	let entry = format!("{indent}\"latticeVersion\": \"{version}\",");
	let anchor = text
		.find("\"$schema\"")
		.and_then(|i| text[i..].find('\n').map(|nl| i + nl))
		.or_else(|| text.find('{').map(|i| i + 1));
	match anchor {
		Some(at) if text[..at].ends_with('\n') => {
			format!("{}{}\n{}", &text[..at], entry, &text[at..])
		}
		Some(at) => format!("{}\n{}{}", &text[..at], entry, &text[at..]),
		None => format!("{{\n{entry}\n}}\n"),
	}
}

/// Byte range of the `latticeVersion` string value, excluding its quotes.
fn pin_value_span(text: &str) -> Option<(usize, usize)> {
	let key = text.find("\"latticeVersion\"")?;
	let colon = key + text[key..].find(':')?;
	let rest = &text[colon + 1..];
	let open = rest.find('"')?;
	let value_start = colon + 1 + open + 1;
	let close = text[value_start..].find('"')?;
	Some((value_start, value_start + close))
}

/// The indentation the file already uses for top-level keys, defaulting to two
/// spaces (what `lattice init` writes).
fn detect_indent(text: &str) -> String {
	for line in text.lines().skip(1) {
		let trimmed = line.trim_start();
		if trimmed.starts_with('"') {
			return line[..line.len() - trimmed.len()].to_string();
		}
	}
	"  ".to_string()
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn replaces_an_existing_pin_and_nothing_else() {
		let before = "{\n\t\"$schema\": \".lattice/schema.json\",\n\t\"latticeVersion\": \"0.1.0\",\n\t\"workspaces\": []\n}\n";
		let after = set_pin(before, "0.2.0");
		assert_eq!(
			after,
			"{\n\t\"$schema\": \".lattice/schema.json\",\n\t\"latticeVersion\": \"0.2.0\",\n\t\"workspaces\": []\n}\n"
		);
	}

	#[test]
	fn adds_a_pin_after_schema_when_absent() {
		let before = "{\n  \"$schema\": \".lattice/schema.json\",\n  \"workspaces\": []\n}\n";
		let after = set_pin(before, "0.2.0");
		assert_eq!(
			after,
			"{\n  \"$schema\": \".lattice/schema.json\",\n  \"latticeVersion\": \"0.2.0\",\n  \"workspaces\": []\n}\n"
		);
		assert!(serde_json::from_str::<serde_json::Value>(&after).is_ok());
	}

	#[test]
	fn adds_a_pin_as_the_first_key_without_schema() {
		let before = "{\n  \"workspaces\": []\n}\n";
		let after = set_pin(before, "1.0.0");
		assert_eq!(
			after,
			"{\n  \"latticeVersion\": \"1.0.0\",\n  \"workspaces\": []\n}\n"
		);
		assert!(serde_json::from_str::<serde_json::Value>(&after).is_ok());
	}

	#[test]
	fn pin_edit_survives_odd_spacing() {
		let before = "{\"latticeVersion\"   :    \"0.1.0\" }";
		assert_eq!(
			set_pin(before, "9.9.9"),
			"{\"latticeVersion\"   :    \"9.9.9\" }"
		);
	}

	#[test]
	fn indent_follows_the_file() {
		assert_eq!(detect_indent("{\n\t\"a\": 1\n}"), "\t");
		assert_eq!(detect_indent("{\n    \"a\": 1\n}"), "    ");
		assert_eq!(detect_indent("{}"), "  ");
	}

	#[test]
	fn run_hint_is_relative_inside_the_repo() {
		let root = Path::new("/repo");
		assert!(run_hint(root, root).starts_with("./.lattice/bin/lattice"));
		// From outside the repo the absolute path is the only useful form.
		assert!(run_hint(root, Path::new("/elsewhere")).starts_with("/repo/.lattice/bin/lattice"));
	}
}
