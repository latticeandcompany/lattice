//! The tasks a workspace can run without `lattice.json` saying anything.
//!
//! Two sources, and nothing is invented beyond them. A tool with a script map —
//! `package.json` scripts, deno tasks, a justfile, a Taskfile, a Rakefile —
//! runs exactly what that map declares. A build tool with no such map gets the
//! handful of verbs it genuinely has: `cargo clippy` is `lint`, `go vet ./...`
//! is `check`. Where a verb depends on a plugin or a dev dependency (spotless,
//! pytest, rubocop) it is offered only when the manifest shows that plugin or
//! dependency. A workspace without a task is skipped for it, not failed.

use std::path::Path;

use indexmap::IndexMap;
use serde_json::Value;

use crate::read;

/// One task a workspace's own files define.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeTask {
	pub command: String,
	/// The file that declared it, or the tool whose verb it is.
	pub source: String,
	/// `npm run` needs `--` before arguments meant for the script.
	pub args_after_separator: bool,
}

#[derive(Debug, Default)]
pub struct NativeTasks {
	pub tasks: IndexMap<String, NativeTask>,
	/// Why a file that should have listed tasks could not be read.
	pub problem: Option<String>,
}

impl NativeTasks {
	fn add(&mut self, name: &str, command: impl Into<String>, source: &str) {
		self.tasks
			.entry(name.to_string())
			.or_insert_with(|| NativeTask {
				command: command.into(),
				source: source.to_string(),
				args_after_separator: false,
			});
	}

	fn add_script_map(&mut self, names: Vec<String>, prefix: &str, source: &str) {
		for name in names {
			let command = format!("{prefix} {name}");
			self.tasks.entry(name).or_insert_with(|| NativeTask {
				command,
				source: source.to_string(),
				args_after_separator: prefix == "npm run",
			});
		}
	}
}

/// Everything `tool` can run in `dir`. `package` is the workspace's package
/// name, for the verbs that have to name it.
pub fn native_tasks(tool: &str, dir: &Path, root: &Path, package: Option<&str>) -> NativeTasks {
	let mut out = NativeTasks::default();
	match tool {
		"npm" | "pnpm" | "yarn" | "bun" => {
			let prefix = match tool {
				"npm" => "npm run",
				"pnpm" => "pnpm run",
				"yarn" => "yarn run",
				_ => "bun run",
			};
			match script_map(&dir.join("package.json"), "scripts") {
				Ok(names) => out.add_script_map(names, prefix, "package.json"),
				Err(problem) => out.problem = Some(problem),
			}
		}
		"deno" => {
			let file = if dir.join("deno.jsonc").exists() {
				"deno.jsonc"
			} else {
				"deno.json"
			};
			match script_map(&dir.join(file), "tasks") {
				Ok(names) => out.add_script_map(names, "deno task", file),
				Err(problem) => out.problem = Some(problem),
			}
			out.add("test", "deno test", "deno");
			out.add("lint", "deno lint", "deno");
			out.add("format", "deno fmt", "deno");
		}
		"composer" => match script_map(&dir.join("composer.json"), "scripts") {
			Ok(names) => out.add_script_map(names, "composer run-script", "composer.json"),
			Err(problem) => out.problem = Some(problem),
		},
		"cargo" => {
			for (task, command) in [
				("build", "cargo build"),
				("test", "cargo test"),
				("check", "cargo check"),
				("lint", "cargo clippy"),
				("format", "cargo fmt"),
				("doc", "cargo doc"),
				("bench", "cargo bench"),
			] {
				out.add(task, command, "cargo");
			}
			// Members share one target directory, so a bare `cargo clean` in each
			// would wipe every other member's build too.
			match package {
				Some(name) => out.add("clean", format!("cargo clean -p {name}"), "cargo"),
				None => out.add("clean", "cargo clean", "cargo"),
			}
		}
		"go" => {
			for (task, command) in [
				("build", "go build ./..."),
				("test", "go test ./..."),
				("check", "go vet ./..."),
				("lint", "go vet ./..."),
				("format", "go fmt ./..."),
				("clean", "go clean"),
			] {
				out.add(task, command, "go");
			}
		}
		"gradle" => gradle(dir, root, &mut out),
		"maven" => maven(dir, root, &mut out),
		"dotnet" => {
			for (task, command) in [
				("build", "dotnet build"),
				("test", "dotnet test"),
				("clean", "dotnet clean"),
				("pack", "dotnet pack"),
				("format", "dotnet format"),
				("lint", "dotnet format --verify-no-changes"),
			] {
				out.add(task, command, "dotnet");
			}
		}
		"swift" => {
			out.add("build", "swift build", "swift");
			out.add("test", "swift test", "swift");
			out.add("clean", "swift package clean", "swift");
		}
		"stack" => {
			out.add("build", "stack build", "stack");
			out.add("test", "stack test", "stack");
			out.add("clean", "stack clean", "stack");
		}
		"cabal" => {
			out.add("build", "cabal build all", "cabal");
			out.add("test", "cabal test all", "cabal");
			out.add("clean", "cabal clean", "cabal");
		}
		"dart" => {
			let cli = if pubspec_uses_flutter(dir) {
				"flutter"
			} else {
				"dart"
			};
			out.add("test", format!("{cli} test"), cli);
			out.add("check", format!("{cli} analyze"), cli);
			out.add("lint", format!("{cli} analyze"), cli);
			out.add("format", "dart format .", cli);
		}
		"mix" => {
			out.add("build", "mix compile", "mix");
			out.add("test", "mix test", "mix");
			out.add("format", "mix format", "mix");
			out.add("clean", "mix clean", "mix");
			if read::text(&dir.join("mix.exs")).is_some_and(|m| m.contains(":credo")) {
				out.add("lint", "mix credo", "mix");
			}
		}
		"uv" | "poetry" | "pdm" | "pipenv" | "pip" => python(tool, dir, &mut out),
		"bundler" | "rake" => ruby(tool, dir, &mut out),
		"just" => out.add_script_map(just_recipes(dir), "just", "justfile"),
		"task" => out.add_script_map(taskfile_tasks(dir), "task", "Taskfile"),
		"turbo" => {
			let turbo = read::json(&dir.join("turbo.json"));
			let map = turbo
				.as_ref()
				.and_then(|t| t.get("tasks").or_else(|| t.get("pipeline")));
			let names = read::json_keys(map)
				.into_iter()
				.filter(|n| !n.contains('#'))
				.collect();
			out.add_script_map(names, "turbo run", "turbo.json");
		}
		"nx" => {
			let nx = read::json(&dir.join("nx.json"));
			let names = read::json_keys(nx.as_ref().and_then(|n| n.get("targetDefaults")));
			out.add_script_map(names, "nx run-many -t", "nx.json");
		}
		_ => {}
	}
	out
}

/// Outputs a `build` gets when nothing says otherwise: the one directory each
/// tool writes its build into by convention and never puts sources in.
pub fn default_build_outputs(tool: &str) -> Option<Vec<String>> {
	let dirs: &[&str] = match tool {
		"npm" | "pnpm" | "yarn" | "bun" | "deno" | "uv" | "poetry" | "pdm" => &["dist/**"],
		"maven" => &["target/**"],
		"gradle" => &["build/**"],
		"dotnet" => &["bin/**"],
		_ => return None,
	};
	Some(dirs.iter().map(|d| d.to_string()).collect())
}

/// `command` with `args` appended, each quoted for the platform shell.
pub fn append_args(command: &str, args: &[String], after_separator: bool) -> String {
	if args.is_empty() {
		return command.to_string();
	}
	let quoted: Vec<String> = args.iter().map(|a| quote(a)).collect();
	let separator = if after_separator { " --" } else { "" };
	format!("{command}{separator} {}", quoted.join(" "))
}

fn quote(arg: &str) -> String {
	let plain = !arg.is_empty()
		&& arg
			.chars()
			.all(|c| c.is_ascii_alphanumeric() || "-_./=:@%+,".contains(c));
	if plain {
		arg.to_string()
	} else if cfg!(windows) {
		format!("\"{}\"", arg.replace('"', "\\\""))
	} else {
		format!("'{}'", arg.replace('\'', "'\\''"))
	}
}

fn script_map(file: &Path, key: &str) -> Result<Vec<String>, String> {
	let name = file
		.file_name()
		.map(|n| n.to_string_lossy().into_owned())
		.unwrap_or_default();
	let Some(raw) = read::text(file) else {
		return Ok(Vec::new());
	};
	let text = if name.ends_with(".jsonc") || name == "deno.json" {
		crate::strip_json_comments(&raw)
	} else {
		raw
	};
	let json: Value =
		serde_json::from_str(&text).map_err(|err| format!("{name} could not be parsed: {err}"))?;
	match json.get(key) {
		None => Ok(Vec::new()),
		Some(Value::Object(map)) => Ok(map.keys().cloned().collect()),
		Some(_) => Err(format!("{name} has a \"{key}\" that is not an object")),
	}
}

/// How to call a build tool's wrapper from `dir`: the nearest one in `dir` or
/// above it inside the repo, or the tool on `PATH` when there is none.
pub fn wrapper(dir: &Path, root: &Path, script: &str, fallback: &str) -> String {
	let file = if cfg!(windows) {
		format!("{script}.bat")
	} else {
		script.to_string()
	};
	let mut ups = 0usize;
	for ancestor in dir.ancestors() {
		if ancestor.join(&file).is_file() {
			let sep = if cfg!(windows) { "\\" } else { "/" };
			let prefix = if ups == 0 {
				format!(".{sep}")
			} else {
				format!("..{sep}").repeat(ups)
			};
			return format!("{prefix}{file}");
		}
		if ancestor == root {
			break;
		}
		ups += 1;
	}
	fallback.to_string()
}

fn gradle(dir: &Path, root: &Path, out: &mut NativeTasks) {
	let gradle = wrapper(dir, root, "gradlew", "gradle");
	let scripts: String = [
		"build.gradle",
		"build.gradle.kts",
		"settings.gradle",
		"settings.gradle.kts",
	]
	.iter()
	.filter_map(|f| read::text(&dir.join(f)))
	.collect::<Vec<_>>()
	.join("\n");

	for task in ["build", "assemble", "check", "test", "clean"] {
		out.add(task, format!("{gradle} {task}"), "gradle");
	}
	if scripts.contains("application") {
		out.add("run", format!("{gradle} run"), "gradle");
	}
	if scripts.contains("org.springframework.boot") {
		out.add("bootRun", format!("{gradle} bootRun"), "gradle");
	}
	if scripts.contains("spotless") {
		out.add("format", format!("{gradle} spotlessApply"), "gradle");
		out.add("lint", format!("{gradle} spotlessCheck"), "gradle");
	}
	if scripts.contains("com.android") {
		out.add("lint", format!("{gradle} lint"), "gradle");
	}
	for name in gradle_registered_tasks(&scripts) {
		out.add(&name, format!("{gradle} {name}"), "gradle");
	}
}

/// Task names a Gradle script registers itself: `tasks.register("x")`,
/// `tasks.create("x")`, `task x`, `task("x")`.
fn gradle_registered_tasks(scripts: &str) -> Vec<String> {
	let mut names = Vec::new();
	for marker in ["register(", "create(", "task("] {
		let mut rest = scripts;
		while let Some(at) = rest.find(marker) {
			rest = &rest[at + marker.len()..];
			let trimmed = rest.trim_start();
			let Some(quote) = trimmed.chars().next().filter(|c| *c == '"' || *c == '\'') else {
				continue;
			};
			if let Some(end) = trimmed[1..].find(quote) {
				names.push(trimmed[1..=end].to_string());
			}
		}
	}
	for line in scripts.lines() {
		if let Some(rest) = line.trim_start().strip_prefix("task ") {
			let name: String = rest
				.chars()
				.take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
				.collect();
			if !name.is_empty() {
				names.push(name);
			}
		}
	}
	names.retain(|n| {
		!n.is_empty()
			&& n.chars()
				.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
	});
	names
}

fn maven(dir: &Path, root: &Path, out: &mut NativeTasks) {
	let mvn = wrapper(dir, root, "mvnw", "mvn");
	out.add("build", format!("{mvn} package -DskipTests"), "maven");
	out.add("test", format!("{mvn} test"), "maven");
	out.add("check", format!("{mvn} verify"), "maven");
	out.add("install", format!("{mvn} install"), "maven");
	out.add("clean", format!("{mvn} clean"), "maven");
	if read::text(&dir.join("pom.xml")).is_some_and(|pom| pom.contains("spotless-maven-plugin")) {
		out.add("format", format!("{mvn} spotless:apply"), "maven");
		out.add("lint", format!("{mvn} spotless:check"), "maven");
	}
}

fn python(tool: &str, dir: &Path, out: &mut NativeTasks) {
	let manifest = read::toml(&dir.join("pyproject.toml"));
	if let Some(scripts) = manifest
		.as_ref()
		.and_then(|m| read::toml_path(m, &["tool", "pdm", "scripts"]))
		.and_then(|s| s.as_table())
		.filter(|_| tool == "pdm")
	{
		let names = scripts.keys().filter(|k| *k != "_").cloned().collect();
		out.add_script_map(names, "pdm run", "pyproject.toml");
	}
	if tool == "pipenv" {
		if let Some(scripts) = read::toml(&dir.join("Pipfile"))
			.and_then(|p| p.get("scripts").and_then(|s| s.as_table()).cloned())
		{
			out.add_script_map(scripts.keys().cloned().collect(), "pipenv run", "Pipfile");
		}
	}

	let run = match tool {
		"uv" => "uv run ",
		"poetry" => "poetry run ",
		"pdm" => "pdm run ",
		"pipenv" => "pipenv run ",
		_ => "",
	};
	if matches!(tool, "uv" | "poetry" | "pdm") {
		out.add("build", format!("{tool} build"), tool);
	}

	let deps = python_dependency_names(dir, manifest.as_ref());
	let has = |name: &str| deps.iter().any(|d| d == name);
	if has("pytest") {
		let pytest = if run.is_empty() {
			"python -m pytest".to_string()
		} else {
			format!("{run}pytest")
		};
		out.add("test", pytest, "pytest");
	}
	if has("ruff") {
		out.add("lint", format!("{run}ruff check ."), "ruff");
		out.add("format", format!("{run}ruff format ."), "ruff");
	}
	if has("black") {
		out.add("format", format!("{run}black ."), "black");
	}
	if has("flake8") {
		out.add("lint", format!("{run}flake8"), "flake8");
	}
	if has("mypy") {
		out.add("check", format!("{run}mypy ."), "mypy");
	}
}

/// Every package name a Python project depends on, from whichever of its files
/// list them, lowercased.
fn python_dependency_names(dir: &Path, manifest: Option<&toml::Table>) -> Vec<String> {
	let mut specs: Vec<String> = Vec::new();
	if let Some(m) = manifest {
		specs.extend(read::toml_strings(read::toml_path(
			m,
			&["project", "dependencies"],
		)));
		for path in [
			&["project", "optional-dependencies"][..],
			&["dependency-groups"][..],
			&["tool", "pdm", "dev-dependencies"][..],
		] {
			if let Some(groups) = read::toml_path(m, path).and_then(|g| g.as_table()) {
				for group in groups.values() {
					specs.extend(read::toml_strings(Some(group)));
				}
			}
		}
		for path in [
			&["tool", "poetry", "dependencies"][..],
			&["tool", "poetry", "dev-dependencies"][..],
		] {
			if let Some(table) = read::toml_path(m, path).and_then(|t| t.as_table()) {
				specs.extend(table.keys().cloned());
			}
		}
		if let Some(groups) =
			read::toml_path(m, &["tool", "poetry", "group"]).and_then(|g| g.as_table())
		{
			for group in groups.values() {
				if let Some(deps) = group.get("dependencies").and_then(|d| d.as_table()) {
					specs.extend(deps.keys().cloned());
				}
			}
		}
	}
	for file in ["Pipfile"] {
		if let Some(pipfile) = read::toml(&dir.join(file)) {
			for section in ["packages", "dev-packages"] {
				if let Some(t) = pipfile.get(section).and_then(|t| t.as_table()) {
					specs.extend(t.keys().cloned());
				}
			}
		}
	}
	if let Ok(entries) = std::fs::read_dir(dir) {
		for entry in entries.flatten() {
			let name = entry.file_name().to_string_lossy().into_owned();
			if name.starts_with("requirements") && name.ends_with(".txt") {
				if let Some(raw) = read::text(&entry.path()) {
					specs.extend(raw.lines().map(String::from));
				}
			}
		}
	}
	specs
		.iter()
		.map(|spec| {
			spec.trim()
				.split(|c: char| " <>=!~;[(@#".contains(c))
				.next()
				.unwrap_or("")
				.to_lowercase()
		})
		.filter(|name| !name.is_empty())
		.collect()
}

fn ruby(tool: &str, dir: &Path, out: &mut NativeTasks) {
	let exec = if tool == "bundler" || dir.join("Gemfile").is_file() {
		"bundle exec "
	} else {
		""
	};
	let rakefile = read::text(&dir.join("Rakefile"));
	if let Some(raw) = &rakefile {
		let names = rake_tasks(raw);
		out.add_script_map(names, &format!("{exec}rake"), "Rakefile");
		if raw.contains("TestTask") {
			out.add("test", format!("{exec}rake test"), "Rakefile");
		}
	}
	let gems = read::text(&dir.join("Gemfile.lock"))
		.or_else(|| read::text(&dir.join("Gemfile")))
		.unwrap_or_default();
	if gems.contains("rspec") {
		out.add("test", format!("{exec}rspec"), "rspec");
	}
	if gems.contains("rubocop") {
		out.add("lint", format!("{exec}rubocop"), "rubocop");
	}
}

/// `task :build`, `task build: [:deps]`, `task "build"`.
fn rake_tasks(raw: &str) -> Vec<String> {
	let mut names = Vec::new();
	for line in raw.lines() {
		let Some(rest) = line.trim_start().strip_prefix("task ") else {
			continue;
		};
		let rest = rest.trim_start().trim_start_matches([':', '"', '\'']);
		let name: String = rest
			.chars()
			.take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
			.collect();
		if !name.is_empty() {
			names.push(name);
		}
	}
	names
}

fn pubspec_uses_flutter(dir: &Path) -> bool {
	read::text(&dir.join("pubspec.yaml")).is_some_and(|p| p.contains("sdk: flutter"))
}

/// A justfile's recipe names: the lines that start in the first column and
/// carry a `:`. Assignments, settings and attributes look similar enough to be
/// ruled out by hand.
pub(crate) fn just_recipes(dir: &Path) -> Vec<String> {
	let Some(raw) = ["justfile", ".justfile", "Justfile"]
		.iter()
		.find_map(|f| read::text(&dir.join(f)))
	else {
		return Vec::new();
	};
	let mut names = Vec::new();
	for line in raw.lines() {
		if line.starts_with([' ', '\t']) || line.trim().is_empty() {
			continue;
		}
		if line.starts_with(['#', '[', '@']) || line.contains(":=") {
			continue;
		}
		let Some((head, _)) = line.split_once(':') else {
			continue;
		};
		let Some(name) = head.split_whitespace().next() else {
			continue;
		};
		if name.contains('=') || matches!(name, "export" | "set" | "alias" | "mod" | "import") {
			continue;
		}
		if name
			.chars()
			.all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
		{
			names.push(name.to_string());
		}
	}
	names
}

/// A Taskfile's task names: the keys one level under the top-level `tasks:`.
pub(crate) fn taskfile_tasks(dir: &Path) -> Vec<String> {
	let Some(raw) = ["Taskfile.yml", "Taskfile.yaml"]
		.iter()
		.find_map(|f| read::text(&dir.join(f)))
	else {
		return Vec::new();
	};
	let mut names = Vec::new();
	let mut in_tasks = false;
	let mut depth: Option<usize> = None;
	for line in raw.lines() {
		let trimmed = line.trim();
		if trimmed.is_empty() || trimmed.starts_with('#') {
			continue;
		}
		let indent = line.len() - line.trim_start().len();
		if !in_tasks {
			in_tasks = indent == 0 && trimmed == "tasks:";
			continue;
		}
		if indent == 0 {
			break;
		}
		let Some(name) = trimmed.strip_suffix(':') else {
			continue;
		};
		if *depth.get_or_insert(indent) != indent {
			continue;
		}
		let name = name.trim_matches('"').trim_matches('\'');
		if !name.is_empty() && !name.contains(' ') {
			names.push(name.to_string());
		}
	}
	names
}

#[cfg(test)]
mod tests {
	use super::*;

	fn write(root: &Path, rel: &str, contents: &str) {
		let path = root.join(rel);
		std::fs::create_dir_all(path.parent().unwrap()).unwrap();
		std::fs::write(path, contents).unwrap();
	}

	#[test]
	fn cargo_lint_is_clippy_and_clean_names_the_package() {
		let dir = tempfile::TempDir::new().unwrap();
		let tasks = native_tasks("cargo", dir.path(), dir.path(), Some("core")).tasks;
		assert_eq!(tasks["lint"].command, "cargo clippy");
		assert_eq!(tasks["clean"].command, "cargo clean -p core");
	}

	#[cfg(unix)]
	#[test]
	fn a_gradle_subproject_calls_the_wrapper_at_the_build_root() {
		let dir = tempfile::TempDir::new().unwrap();
		write(dir.path(), "gradlew", "");
		write(
			dir.path(),
			"core/build.gradle.kts",
			"plugins { `java-library` }\n",
		);
		let tasks = native_tasks("gradle", &dir.path().join("core"), dir.path(), None).tasks;
		assert_eq!(tasks["build"].command, "../gradlew build");
		assert!(!tasks.contains_key("lint"));
	}

	#[test]
	fn gradle_without_a_wrapper_uses_gradle_and_finds_registered_tasks() {
		let dir = tempfile::TempDir::new().unwrap();
		write(
			dir.path(),
			"build.gradle.kts",
			"plugins { application }\ntasks.register(\"generateDocs\") {}\n",
		);
		let tasks = native_tasks("gradle", dir.path(), dir.path(), None).tasks;
		assert_eq!(tasks["build"].command, "gradle build");
		assert_eq!(tasks["run"].command, "gradle run");
		assert_eq!(tasks["generateDocs"].command, "gradle generateDocs");
	}

	#[test]
	fn npm_scripts_are_the_tasks_and_forward_args_after_a_separator() {
		let dir = tempfile::TempDir::new().unwrap();
		write(
			dir.path(),
			"package.json",
			r#"{"scripts": {"build": "vite build"}}"#,
		);
		let tasks = native_tasks("npm", dir.path(), dir.path(), None).tasks;
		assert_eq!(tasks.len(), 1);
		let build = &tasks["build"];
		assert_eq!(
			append_args(
				&build.command,
				&["--mode".into(), "prod".into()],
				build.args_after_separator
			),
			"npm run build -- --mode prod"
		);
	}

	#[test]
	fn an_unparseable_package_json_is_a_problem_not_an_empty_task_list() {
		let dir = tempfile::TempDir::new().unwrap();
		write(dir.path(), "package.json", "{ nope");
		let native = native_tasks("pnpm", dir.path(), dir.path(), None);
		assert!(native.tasks.is_empty());
		assert!(native.problem.unwrap().contains("could not be parsed"));
	}

	#[test]
	fn python_verbs_follow_the_dependencies_the_project_declares() {
		let dir = tempfile::TempDir::new().unwrap();
		write(
			dir.path(),
			"pyproject.toml",
			"[project]\nname = \"app\"\n[dependency-groups]\ndev = [\"pytest>=8\", \"ruff\"]\n",
		);
		let tasks = native_tasks("uv", dir.path(), dir.path(), None).tasks;
		assert_eq!(tasks["test"].command, "uv run pytest");
		assert_eq!(tasks["lint"].command, "uv run ruff check .");
		assert_eq!(tasks["build"].command, "uv build");
		assert!(!tasks.contains_key("check"));
	}

	#[test]
	fn rake_tasks_in_their_common_spellings() {
		assert_eq!(
			rake_tasks("task :build do\nend\ntask test: [:build]\ntask \"lint\"\n"),
			["build", "test", "lint"]
		);
	}

	#[cfg(unix)]
	#[test]
	fn an_argument_with_a_space_is_quoted() {
		assert_eq!(
			append_args("cargo test", &["--".into(), "a b".into()], false),
			"cargo test -- 'a b'"
		);
	}
}
