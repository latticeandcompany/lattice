use anyhow::{bail, Result};
use clap::Args;
use console::style;
use std::path::PathBuf;

use lattice_config::find_root;

#[derive(Args, Debug)]
pub struct GenerateArgs {
    #[arg(help = "Template name (from .lattice/templates/)")]
    pub template: String,

    #[arg(help = "Name for the new workspace")]
    pub name: Option<String>,

    #[arg(long, help = "Destination directory (default: current directory)")]
    pub path: Option<PathBuf>,
}

impl GenerateArgs {
    pub async fn execute(&self) -> Result<()> {
        let cwd = std::env::current_dir()?;
        let root = find_root(&cwd).ok_or_else(|| {
            anyhow::anyhow!("No lattice.json found in this directory or any parent directory.")
        })?;

        let templates_dir = root.join(".lattice").join("templates");
        let template_dir = templates_dir.join(&self.template);

        if !template_dir.exists() {
            bail!(
                "Template '{}' not found. Run 'lattice template list' to see available templates.",
                self.template
            );
        }

        let workspace_name = match &self.name {
            Some(n) => n.clone(),
            None => {
                bail!("Please provide a name for the new workspace. Usage: lattice generate <template> <name>");
            }
        };

        let dest_base = self.path.clone().unwrap_or_else(|| cwd.clone());
        let dest = dest_base.join(&workspace_name);

        if dest.exists() {
            bail!(
                "Destination '{}' already exists. Choose a different name.",
                dest.display()
            );
        }

        println!(
            "{} Generating workspace '{}' from template '{}'...",
            style("◆").cyan().bold(),
            style(&workspace_name).bold(),
            style(&self.template).dim()
        );

        copy_template(&template_dir, &dest, &workspace_name)?;

        let desc_path = template_dir.join("lattice-template.json");
        if let Ok(content) = std::fs::read_to_string(&desc_path) {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(instructions) = val.get("postScaffold").and_then(|i| i.as_str()) {
                    let instructions = instructions
                        .replace("{{name}}", &workspace_name)
                        .replace("{{Name}}", &capitalize_first(&workspace_name))
                        .replace("{{NAME}}", &workspace_name.to_uppercase());
                    println!();
                    println!("{}", style("Next steps:").bold());
                    for line in instructions.lines() {
                        println!("  {}", line);
                    }
                }
            }
        }

        println!(
            "{} Workspace '{}' created at {}",
            style("✓").green().bold(),
            style(&workspace_name).bold(),
            style(dest.display().to_string()).dim()
        );

        println!();
        println!(
            "  {} Add '{}' to your lattice.json workspaces glob if not already covered.",
            style("→").dim(),
            style(dest.strip_prefix(&root).unwrap_or(&dest).display()).cyan()
        );

        Ok(())
    }
}

fn copy_template(src: &PathBuf, dest: &PathBuf, workspace_name: &str) -> Result<()> {
    std::fs::create_dir_all(dest)?;

    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let file_name = entry.file_name();
        let file_name_str = file_name.to_string_lossy();

        if file_name_str == "lattice-template.json" {
            continue;
        }

        if file_name_str == ".git" {
            continue;
        }

        let src_path = entry.path();
        let dest_name = file_name_str.replace("{{name}}", workspace_name);
        let dest_path = dest.join(&dest_name);

        if src_path.is_dir() {
            copy_template(&src_path, &dest_path, workspace_name)?;
        } else {
            let content = std::fs::read(&src_path)?;

            match String::from_utf8(content.clone()) {
                Ok(text) => {
                    let substituted = text
                        .replace("{{name}}", workspace_name)
                        .replace("{{Name}}", &capitalize_first(workspace_name))
                        .replace("{{NAME}}", &workspace_name.to_uppercase());
                    std::fs::write(&dest_path, substituted.as_bytes())?;
                }
                Err(_) => {
                    std::fs::write(&dest_path, &content)?;
                }
            }
        }
    }

    Ok(())
}

fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
}
