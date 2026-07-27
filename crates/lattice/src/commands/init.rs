use std::collections::HashSet;
use std::path::Path;

use anyhow::{bail, Result};
use clap::Args;
use console::style;
use dialoguer::{theme::ColorfulTheme, Confirm, Input, Select};
use serde_json::{json, Map, Value};

use lattice_output::{logo, teal};

use crate::cli::BIN_VERSION;

/// The canonical schema, bundled into the binary and written to
/// `<cwd>/.lattice/schema.json` on init so the config self-validates in editors.
const SCHEMA_JSON: &str = include_str!("../../assets/schema.json");

/// Lines that init ensures are present in `.gitignore` (cache + provisioned
/// toolchains are local artifacts). The committed `.lattice/schema.json` stays
/// tracked and is deliberately NOT ignored.
const GITIGNORE_LINES: &[&str] = &[".lattice/cache/", ".lattice/toolchains/"];

#[derive(Args, Debug)]
#[command(long_about = "Scaffold a lattice.json in the current directory.\n\n\
With --yes (or no TTY) writes a minimal default skeleton without prompting. On an \
interactive terminal, a short wizard tailors the config to your repo. Also writes a \
committed .lattice/schema.json and ensures .gitignore covers local artifacts.")]
pub struct InitArgs {
    /// Overwrite an existing lattice.json.
    #[arg(long)]
    pub force: bool,

    /// Accept defaults and write the skeleton without prompting.
    #[arg(short, long)]
    pub yes: bool,
}

impl InitArgs {
    pub async fn execute(&self) -> Result<()> {
        let cwd = std::env::current_dir()?;
        let config_path = cwd.join("lattice.json");
        if config_path.exists() && !self.force {
            bail!("lattice.json already exists (use --force to overwrite)");
        }

        // Never hang a pipeline: no-TTY OR `-y/--yes` writes the skeleton.
        let tty = console::user_attended();

        // Lead an interactive init with the branded mark (BRAND.md §6/§7); skip
        // it for pipes/CI so scripted `init --yes` output stays clean.
        if tty {
            println!("{}", logo());
            println!();
        }

        let config = if self.yes || !tty {
            default_skeleton(BIN_VERSION)
        } else {
            interactive_wizard()?
        };

        write_artifacts(&cwd, &config)?;
        print_success(&config);
        Ok(())
    }
}

/// The default no-prompt skeleton (decisions #22/#23).
pub fn default_skeleton(version: &str) -> Value {
    json!({
        "$schema": ".lattice/schema.json",
        "latticeVersion": version,
        "workspaces": [],
        "tasks": {
            "build": { "dependsOn": ["^build"], "outputs": ["dist/**"] }
        }
    })
}

/// Idempotently ensure each line in `lines` is present in `.gitignore` content.
/// Existing content and ordering are preserved; only missing lines are appended.
pub fn ensure_gitignore_lines(existing: &str, lines: &[&str]) -> String {
    let present: HashSet<&str> = existing.lines().map(|l| l.trim()).collect();
    let missing: Vec<&&str> = lines.iter().filter(|l| !present.contains(**l)).collect();
    if missing.is_empty() {
        return existing.to_string();
    }

    let mut out = existing.to_string();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    for line in missing {
        out.push_str(line);
        out.push('\n');
    }
    out
}

fn write_artifacts(cwd: &Path, config: &Value) -> Result<()> {
    let pretty = serde_json::to_string_pretty(config)?;
    std::fs::write(cwd.join("lattice.json"), format!("{pretty}\n"))?;

    let lattice_dir = cwd.join(".lattice");
    std::fs::create_dir_all(&lattice_dir)?;
    std::fs::write(lattice_dir.join("schema.json"), SCHEMA_JSON)?;

    let gi_path = cwd.join(".gitignore");
    let existing = std::fs::read_to_string(&gi_path).unwrap_or_default();
    let updated = ensure_gitignore_lines(&existing, GITIGNORE_LINES);
    if updated != existing {
        std::fs::write(&gi_path, updated)?;
    }
    Ok(())
}

fn print_success(config: &Value) {
    let check = teal().apply_to("✓");
    println!();
    println!("{} {}", check, style("wrote lattice.json").bold());
    println!("{} {}", check, style("wrote .lattice/schema.json").bold());
    println!("{} {}", check, style("updated .gitignore").bold());

    let has_tasks = config
        .get("tasks")
        .and_then(Value::as_object)
        .map(|m| !m.is_empty())
        .unwrap_or(false);
    println!();
    if has_tasks {
        println!("next: {}", style("lattice run build").bold());
    } else {
        println!("next: {}", style("lattice setup").bold());
    }
}

// ---------------------------------------------------------------------------
// Interactive wizard (TTY only; not unit-tested)
// ---------------------------------------------------------------------------

fn interactive_wizard() -> Result<Value> {
    let theme = ColorfulTheme::default();

    let capabilities = ["build tool", "toolchain manager", "both"];
    let cap = Select::with_theme(&theme)
        .with_prompt("What does this repo need Lattice for?")
        .items(&capabilities)
        .default(2)
        .interact()?;
    let do_build = cap == 0 || cap == 2;
    let do_toolchain = cap == 1 || cap == 2;

    let mut workspaces: Vec<Value> = Vec::new();
    let mut tasks = Map::new();
    let mut root_engines = Map::new();

    if do_build {
        loop {
            let name: String = Input::with_theme(&theme)
                .with_prompt("workspace name")
                .validate_with(|input: &String| -> Result<(), &str> {
                    if input.trim().is_empty() {
                        Err("name must not be empty")
                    } else {
                        Ok(())
                    }
                })
                .interact_text()?;
            let path: String = Input::with_theme(&theme)
                .with_prompt("workspace path (a literal directory)")
                .with_initial_text(name.clone())
                .interact_text()?;
            let auto = Confirm::with_theme(&theme)
                .with_prompt("auto-detect the engine and tasks?")
                .default(true)
                .interact()?;

            let mut ws = Map::new();
            ws.insert("name".into(), json!(name));
            ws.insert("path".into(), json!(path));
            if !auto {
                ws.insert("auto".into(), json!(false));
                let (engine, constraint) = pick_engine(&theme)?;
                let mut eng = Map::new();
                eng.insert(engine, json!(constraint));
                ws.insert("engines".into(), Value::Object(eng));
            }
            workspaces.push(Value::Object(ws));

            if !Confirm::with_theme(&theme)
                .with_prompt("add another workspace?")
                .default(false)
                .interact()?
            {
                break;
            }
        }

        if Confirm::with_theme(&theme)
            .with_prompt("add a starter `build` task?")
            .default(true)
            .interact()?
        {
            tasks.insert(
                "build".into(),
                json!({ "dependsOn": ["^build"], "outputs": ["dist/**"] }),
            );
        }
    }

    if do_toolchain {
        loop {
            let (engine, constraint) = pick_engine(&theme)?;
            root_engines.insert(engine, json!(constraint));
            if !Confirm::with_theme(&theme)
                .with_prompt("add another engine?")
                .default(false)
                .interact()?
            {
                break;
            }
        }
        println!(
            "{}",
            style(
                "note: add bespoke engines (versionCmd/installCmd) by hand-editing \
                 lattice.json afterwards (see .lattice/schema.json)."
            )
            .dim()
        );
    }

    let mut root = Map::new();
    root.insert("$schema".into(), json!(".lattice/schema.json"));
    root.insert("latticeVersion".into(), json!(BIN_VERSION));
    root.insert("workspaces".into(), Value::Array(workspaces));
    if !root_engines.is_empty() {
        root.insert("engines".into(), Value::Object(root_engines));
    }
    root.insert("tasks".into(), Value::Object(tasks));
    Ok(Value::Object(root))
}

/// Prompt for a well-known engine name + a version constraint. The wizard only
/// ever emits well-known, string-form engines (decision #11).
fn pick_engine(theme: &ColorfulTheme) -> Result<(String, String)> {
    let engines = lattice_config::WELL_KNOWN_ENGINES;
    let idx = Select::with_theme(theme)
        .with_prompt("engine")
        .items(engines)
        .default(0)
        .interact()?;
    let name = engines[idx].to_string();
    let constraint: String = Input::with_theme(theme)
        .with_prompt(format!("version constraint for {name} (e.g. >=20.0.0)"))
        .with_initial_text(">=0.0.0")
        .interact_text()?;
    Ok((name, constraint))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_skeleton_has_expected_shape() {
        let v = default_skeleton("1.2.3");
        assert_eq!(v["$schema"], json!(".lattice/schema.json"));
        assert_eq!(v["latticeVersion"], json!("1.2.3"));
        assert_eq!(v["workspaces"], json!([]));
        assert_eq!(v["tasks"]["build"]["dependsOn"], json!(["^build"]));
        assert_eq!(v["tasks"]["build"]["outputs"], json!(["dist/**"]));
    }

    #[test]
    fn gitignore_appends_missing_lines() {
        let out = ensure_gitignore_lines("", GITIGNORE_LINES);
        assert!(out.contains(".lattice/cache/"));
        assert!(out.contains(".lattice/toolchains/"));
    }

    #[test]
    fn gitignore_is_idempotent() {
        let once = ensure_gitignore_lines("", GITIGNORE_LINES);
        let twice = ensure_gitignore_lines(&once, GITIGNORE_LINES);
        assert_eq!(once, twice, "appending twice must not duplicate lines");
        // Exactly one occurrence of each line.
        assert_eq!(twice.matches(".lattice/cache/").count(), 1);
        assert_eq!(twice.matches(".lattice/toolchains/").count(), 1);
    }

    #[test]
    fn gitignore_preserves_existing_lines() {
        let existing = "node_modules/\ntarget/\n";
        let out = ensure_gitignore_lines(existing, GITIGNORE_LINES);
        assert!(out.starts_with("node_modules/\ntarget/\n"));
        assert!(out.contains(".lattice/cache/"));
        assert!(out.contains(".lattice/toolchains/"));
    }

    #[test]
    fn gitignore_does_not_readd_present_line() {
        let existing = ".lattice/cache/\n";
        let out = ensure_gitignore_lines(existing, GITIGNORE_LINES);
        assert_eq!(out.matches(".lattice/cache/").count(), 1);
        assert!(out.contains(".lattice/toolchains/"));
    }

    #[test]
    fn bundled_schema_is_valid_json() {
        let parsed: Value = serde_json::from_str(SCHEMA_JSON).expect("schema.json is valid JSON");
        assert!(parsed.get("$defs").is_some());
    }
}
