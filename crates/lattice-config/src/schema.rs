use std::path::Path;

/// The canonical JSON Schema for `lattice.json`, bundled into the binary. It is
/// written to `<root>/.lattice/schema.json` so the config self-validates in
/// editors. The config's `$schema` points at that local file. The copy is
/// committed and kept present by [`ensure_schema`].
pub const SCHEMA_JSON: &str = include_str!("../assets/schema.json");

/// Keep `<root>/.lattice/schema.json` in step with this binary so editors
/// validate the config against the version that will read it. Written when
/// absent or out of date; line endings are ignored in the comparison so a
/// checkout that converted them is not churned. Best effort: an I/O error never
/// fails a real command.
pub fn ensure_schema(root: &Path) {
	let schema_path = root.join(".lattice").join("schema.json");
	if let Ok(existing) = std::fs::read_to_string(&schema_path) {
		if existing.replace("\r\n", "\n") == SCHEMA_JSON.replace("\r\n", "\n") {
			return;
		}
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

	/// The schema must not advertise a setting the config type has no field for:
	/// such a key validates in an editor and is then dropped on the floor.
	#[test]
	fn schema_settings_match_the_config_type() {
		let schema: Value = serde_json::from_str(SCHEMA_JSON).unwrap();
		let advertised: Vec<&String> = schema["$defs"]["settings"]["properties"]
			.as_object()
			.expect("settings has a properties object")
			.keys()
			.collect();

		let populated = crate::Settings {
			max_cache_size: Some(crate::CacheSize::parse("1GB").unwrap()),
			cache_dir: Some(".lattice/cache".to_string()),
			loquacious: true,
			version_check: false,
		};
		let serialized = serde_json::to_value(&populated).unwrap();
		let actual: Vec<&String> = serialized
			.as_object()
			.unwrap()
			.keys()
			.filter(|k| !advertised.contains(k))
			.collect();
		assert!(
			actual.is_empty(),
			"settings fields missing from the schema: {actual:?}"
		);

		let orphans: Vec<&&String> = advertised
			.iter()
			.filter(|k| serialized.get(k.as_str()).is_none())
			.collect();
		assert!(
			orphans.is_empty(),
			"schema advertises settings the config type does not have: {orphans:?}"
		);
	}

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
	fn replaces_a_schema_from_another_version() {
		let dir = tempfile::tempdir().unwrap();
		let lattice_dir = dir.path().join(".lattice");
		std::fs::create_dir_all(&lattice_dir).unwrap();
		let schema_path = lattice_dir.join("schema.json");
		std::fs::write(&schema_path, "a schema from 1.x").unwrap();

		ensure_schema(dir.path());

		assert_eq!(std::fs::read_to_string(&schema_path).unwrap(), SCHEMA_JSON);
	}

	#[test]
	fn leaves_a_current_schema_with_converted_line_endings_alone() {
		let dir = tempfile::tempdir().unwrap();
		let lattice_dir = dir.path().join(".lattice");
		std::fs::create_dir_all(&lattice_dir).unwrap();
		let schema_path = lattice_dir.join("schema.json");
		let crlf = SCHEMA_JSON.replace("\r\n", "\n").replace('\n', "\r\n");
		std::fs::write(&schema_path, &crlf).unwrap();

		ensure_schema(dir.path());

		assert_eq!(std::fs::read_to_string(&schema_path).unwrap(), crlf);
	}
}
