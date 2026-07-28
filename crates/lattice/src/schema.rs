use std::path::Path;

/// The canonical JSON Schema for `lattice.json`, bundled into the binary. It is
/// written to `<root>/.lattice/schema.json` so the config self-validates in
/// editors — the config's `$schema` points at that local file. The copy is
/// committed and kept present by [`ensure_schema`].
pub const SCHEMA_JSON: &str = include_str!("../assets/schema.json");

/// Ensure `<root>/.lattice/schema.json` exists so editors can resolve the
/// config's `$schema`. Writes the bundled schema only when the file is absent,
/// leaving a committed (or newer) copy untouched to avoid churn. This is a
/// best-effort editor convenience: any I/O error is swallowed so it can never
/// fail a real command.
pub fn ensure_schema(root: &Path) {
	let schema_path = root.join(".lattice").join("schema.json");
	if schema_path.exists() {
		return;
	}
	let Some(dir) = schema_path.parent() else {
		return;
	};
	if std::fs::create_dir_all(dir).is_err() {
		return;
	}
	let _ = std::fs::write(&schema_path, SCHEMA_JSON);
}

#[cfg(test)]
mod tests {
	use super::*;
	use serde_json::Value;

	#[test]
	fn writes_schema_when_absent() {
		let dir = tempfile::tempdir().unwrap();
		ensure_schema(dir.path());
		let written = dir.path().join(".lattice").join("schema.json");
		assert!(written.exists(), "schema should be written when missing");
		let parsed: Value =
			serde_json::from_str(&std::fs::read_to_string(&written).unwrap()).unwrap();
		assert!(parsed.get("$defs").is_some());
	}

	#[test]
	fn leaves_existing_schema_untouched() {
		let dir = tempfile::tempdir().unwrap();
		let lattice_dir = dir.path().join(".lattice");
		std::fs::create_dir_all(&lattice_dir).unwrap();
		let schema_path = lattice_dir.join("schema.json");
		std::fs::write(&schema_path, "custom-pinned-copy").unwrap();

		ensure_schema(dir.path());

		assert_eq!(
			std::fs::read_to_string(&schema_path).unwrap(),
			"custom-pinned-copy",
			"an existing schema must not be clobbered"
		);
	}
}
