//! Tolerant readers for the manifests discovery looks at. A file that is
//! missing or does not parse reads as nothing: discovery reports on the repo it
//! can see, and the tool that owns the file will say what is wrong with it.

use std::path::Path;

use serde_json::Value;

pub(crate) fn json(file: &Path) -> Option<Value> {
	let raw = std::fs::read_to_string(file).ok()?;
	let jsonc = file
		.extension()
		.is_some_and(|ext| ext == "jsonc" || file.ends_with("deno.json"));
	let text = if jsonc {
		crate::strip_json_comments(&raw)
	} else {
		raw
	};
	serde_json::from_str(&text).ok()
}

pub(crate) fn toml(file: &Path) -> Option<toml::Table> {
	let raw = std::fs::read_to_string(file).ok()?;
	raw.parse::<toml::Table>().ok()
}

pub(crate) fn text(file: &Path) -> Option<String> {
	std::fs::read_to_string(file).ok()
}

pub(crate) fn toml_path<'a>(table: &'a toml::Table, path: &[&str]) -> Option<&'a toml::Value> {
	let (first, rest) = path.split_first()?;
	let mut value = table.get(*first)?;
	for key in rest {
		value = value.as_table()?.get(*key)?;
	}
	Some(value)
}

pub(crate) fn toml_strings(value: Option<&toml::Value>) -> Vec<String> {
	value
		.and_then(|v| v.as_array())
		.map(|items| {
			items
				.iter()
				.filter_map(|i| i.as_str())
				.map(String::from)
				.collect()
		})
		.unwrap_or_default()
}

pub(crate) fn json_strings(value: Option<&Value>) -> Vec<String> {
	value
		.and_then(Value::as_array)
		.map(|items| {
			items
				.iter()
				.filter_map(Value::as_str)
				.map(String::from)
				.collect()
		})
		.unwrap_or_default()
}

pub(crate) fn json_keys(value: Option<&Value>) -> Vec<String> {
	value
		.and_then(Value::as_object)
		.map(|map| map.keys().cloned().collect())
		.unwrap_or_default()
}

/// `path` relative to `root`, forward-slashed, with the root itself as `"."`.
pub fn rel_path(root: &Path, path: &Path) -> String {
	let rel = path.strip_prefix(root).unwrap_or(path);
	let parts: Vec<String> = rel
		.components()
		.map(|c| c.as_os_str().to_string_lossy().into_owned())
		.collect();
	if parts.is_empty() {
		".".to_string()
	} else {
		parts.join("/")
	}
}
