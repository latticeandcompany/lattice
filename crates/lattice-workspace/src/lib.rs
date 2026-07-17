use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use lattice_config::LatticeConfig;

#[derive(Debug, Clone)]
pub struct Workspace {
    pub name: String,
    pub path: PathBuf,
    pub language: Language,
    pub tasks: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Language {
    JavaScript,
    Rust,
    Python,
    Go,
    Ruby,
    CSharp,
    JavaGradle,
    JavaMaven,
    Cpp,
    Unknown,
}

impl Language {
    pub fn name(&self) -> &'static str {
        match self {
            Language::JavaScript => "JavaScript/TypeScript",
            Language::Rust => "Rust",
            Language::Python => "Python",
            Language::Go => "Go",
            Language::Ruby => "Ruby",
            Language::CSharp => "C#/.NET",
            Language::JavaGradle => "Java (Gradle)",
            Language::JavaMaven => "Java (Maven)",
            Language::Cpp => "C/C++",
            Language::Unknown => "Unknown",
        }
    }

    pub fn setup_command(&self, path: &Path) -> Option<String> {
        match self {
            Language::JavaScript => {
                let pm = detect_js_package_manager(path);
                Some(format!("{} install", pm))
            }
            Language::Rust => Some("cargo fetch".to_string()),
            Language::Python => {
                if path.join("uv.lock").exists() || path.join("pyproject.toml").exists() {
                    Some("uv sync".to_string())
                } else if path.join("requirements.txt").exists() {
                    Some("pip install -r requirements.txt".to_string())
                } else {
                    None
                }
            }
            Language::Go => Some("go mod download".to_string()),
            Language::Ruby => Some("bundle install".to_string()),
            Language::CSharp => Some("dotnet restore".to_string()),
            Language::JavaGradle => {
                if path.join("gradlew").exists() {
                    Some("./gradlew dependencies".to_string())
                } else {
                    Some("gradle dependencies".to_string())
                }
            }
            Language::JavaMaven => {
                if path.join("mvnw").exists() {
                    Some("./mvnw dependency:resolve".to_string())
                } else {
                    Some("mvn dependency:resolve".to_string())
                }
            }
            Language::Cpp | Language::Unknown => None,
        }
    }
}

pub fn discover_workspaces(root: &Path, config: &LatticeConfig) -> Result<Vec<Workspace>> {
    let mut workspaces = Vec::new();

    for glob_pattern in &config.workspaces {
        let full_pattern = root.join(glob_pattern).display().to_string();

        for entry in glob::glob(&full_pattern)? {
            match entry {
                Ok(path) => {
                    if path.is_dir() {
                        let name = path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("unknown")
                            .to_string();

                        let language = detect_language(&path);
                        let tasks = build_task_map(&name, &path, &language, config);

                        workspaces.push(Workspace {
                            name,
                            path,
                            language,
                            tasks,
                        });
                    }
                }
                Err(e) => eprintln!("Warning: glob error: {}", e),
            }
        }
    }

    Ok(workspaces)
}

pub fn detect_language(path: &Path) -> Language {
    if path.join("package.json").exists() {
        Language::JavaScript
    } else if path.join("Cargo.toml").exists() {
        Language::Rust
    } else if path.join("pyproject.toml").exists()
        || path.join("setup.py").exists()
        || path.join("requirements.txt").exists()
    {
        Language::Python
    } else if path.join("go.mod").exists() {
        Language::Go
    } else if path.join("Gemfile").exists() || path.join("Rakefile").exists() {
        Language::Ruby
    } else if path.join("build.gradle").exists() || path.join("build.gradle.kts").exists() {
        Language::JavaGradle
    } else if path.join("pom.xml").exists() {
        Language::JavaMaven
    } else if has_csproj(path) {
        Language::CSharp
    } else if path.join("CMakeLists.txt").exists() || path.join("Makefile").exists() {
        Language::Cpp
    } else {
        Language::Unknown
    }
}

fn has_csproj(path: &Path) -> bool {
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if name.ends_with(".csproj") || name == "project.json" {
                    return true;
                }
            }
        }
    }
    false
}

fn build_task_map(
    workspace_name: &str,
    path: &Path,
    language: &Language,
    config: &LatticeConfig,
) -> HashMap<String, String> {
    let mut tasks = HashMap::new();

    for task_name in config.pipeline.keys() {
        if let Some(projects) = &config.projects {
            if let Some(project) = projects.get(workspace_name) {
                if let Some(cmd) = project.tasks.get(task_name) {
                    tasks.insert(task_name.clone(), cmd.clone());
                    continue;
                }
            }
        }

        if let Some(cmd) = infer_task_command(task_name, language, path) {
            tasks.insert(task_name.clone(), cmd);
        }
    }

    tasks
}

fn infer_task_command(task_name: &str, language: &Language, path: &Path) -> Option<String> {
    match language {
        Language::JavaScript => {
            let pm = detect_js_package_manager(path);
            Some(format!("{} run {}", pm, task_name))
        }
        Language::Rust => {
            let cmd = match task_name {
                "build" => "cargo build".to_string(),
                "test" => "cargo test".to_string(),
                "check" => "cargo check".to_string(),
                "fmt" => "cargo fmt".to_string(),
                "lint" => "cargo clippy".to_string(),
                "doc" => "cargo doc".to_string(),
                "clean" => "cargo clean".to_string(),
                other => format!("cargo {}", other),
            };
            Some(cmd)
        }
        Language::Python => {
            if path.join("pyproject.toml").exists() {
                Some(format!("uv run {}", task_name))
            } else {
                Some(format!("python -m {}", task_name))
            }
        }
        Language::Go => {
            let cmd = match task_name {
                "build" => "go build ./...".to_string(),
                "test" => "go test ./...".to_string(),
                "run" => "go run .".to_string(),
                "fmt" => "go fmt ./...".to_string(),
                "lint" => "go vet ./...".to_string(),
                other => format!("go {}", other),
            };
            Some(cmd)
        }
        Language::Ruby => Some(format!("rake {}", task_name)),
        Language::CSharp => {
            let cmd = match task_name {
                "build" => "dotnet build".to_string(),
                "test" => "dotnet test".to_string(),
                "run" => "dotnet run".to_string(),
                "clean" => "dotnet clean".to_string(),
                other => format!("dotnet {}", other),
            };
            Some(cmd)
        }
        Language::JavaGradle => {
            if path.join("gradlew").exists() {
                Some(format!("./gradlew {}", task_name))
            } else {
                Some(format!("gradle {}", task_name))
            }
        }
        Language::JavaMaven => {
            if path.join("mvnw").exists() {
                Some(format!("./mvnw {}", task_name))
            } else {
                Some(format!("mvn {}", task_name))
            }
        }
        Language::Cpp => {
            if path.join("Makefile").exists() {
                Some(format!("make {}", task_name))
            } else {
                None
            }
        }
        Language::Unknown => None,
    }
}

pub fn detect_js_package_manager(path: &Path) -> &'static str {
    if path.join("bun.lockb").exists() || path.join("bun.lock").exists() {
        "bun"
    } else if path.join("pnpm-lock.yaml").exists() {
        "pnpm"
    } else if path.join("yarn.lock").exists() {
        "yarn"
    } else {
        "npm"
    }
}
