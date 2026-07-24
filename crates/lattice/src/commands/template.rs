use anyhow::{bail, Result};
use clap::{Args, Subcommand};
use console::style;
use std::path::{Path, PathBuf};

use lattice_config::find_root;

#[derive(Args, Debug)]
pub struct TemplateArgs {
    #[command(subcommand)]
    pub command: TemplateCommand,
}

#[derive(Subcommand, Debug)]
pub enum TemplateCommand {
    #[command(about = "Add a template from a git URL or HTTP endpoint")]
    Add(TemplateAddArgs),

    #[command(about = "List available local templates")]
    List,

    #[command(about = "Remove a local template")]
    Remove(TemplateRemoveArgs),
}

#[derive(Args, Debug)]
pub struct TemplateAddArgs {
    #[arg(help = "URL or git repository to fetch the template from")]
    pub url: String,

    #[arg(
        long,
        help = "Name to give this template locally (defaults to repo name)"
    )]
    pub name: Option<String>,
}

#[derive(Args, Debug)]
pub struct TemplateRemoveArgs {
    #[arg(help = "Name of the template to remove")]
    pub name: String,
}

impl TemplateArgs {
    pub async fn execute(&self) -> Result<()> {
        let cwd = std::env::current_dir()?;
        let root = find_root(&cwd).ok_or_else(|| {
            anyhow::anyhow!("No lattice.json found in this directory or any parent directory.")
        })?;

        let templates_dir = root.join(".lattice").join("templates");

        match &self.command {
            TemplateCommand::Add(args) => add_template(&templates_dir, args).await,
            TemplateCommand::List => list_templates(&templates_dir),
            TemplateCommand::Remove(args) => remove_template(&templates_dir, args),
        }
    }
}

async fn add_template(templates_dir: &PathBuf, args: &TemplateAddArgs) -> Result<()> {
    std::fs::create_dir_all(templates_dir)?;

    let url = &args.url;

    let name = if let Some(n) = &args.name {
        n.clone()
    } else {
        derive_template_name(url)?
    };

    let dest = templates_dir.join(&name);
    if dest.exists() {
        bail!(
            "Template '{}' already exists at {}. Use --name to specify a different name or remove it first.",
            name,
            dest.display()
        );
    }

    println!(
        "{} Fetching template {} ...",
        style("◆").cyan().bold(),
        style(&name).bold()
    );

    let is_git_url = url.ends_with(".git")
        || url.starts_with("git@")
        || url.starts_with("github.com/")
        || url.starts_with("gitlab.com/")
        || url.starts_with("bitbucket.org/");

    if is_git_url || !url.starts_with("http") {
        let clone_url = normalize_git_url(url);
        git_clone(&clone_url, &dest).await?;
    } else {
        http_fetch(url, &dest).await?;
    }

    println!(
        "{} Template '{}' added at {}",
        style("✓").green().bold(),
        style(&name).bold(),
        style(format!(".lattice/templates/{}", name)).dim()
    );

    Ok(())
}

fn list_templates(templates_dir: &PathBuf) -> Result<()> {
    if !templates_dir.exists() {
        println!("No templates found. Run 'lattice template add <url>' to add one.");
        return Ok(());
    }

    let mut found = false;
    let entries = std::fs::read_dir(templates_dir)?;

    println!("{}", style("Available templates:").bold());

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown");

            let desc = read_template_desc(&path);
            if let Some(d) = desc {
                println!("  {} — {}", style(name).bold().cyan(), d);
            } else {
                println!("  {}", style(name).bold().cyan());
            }
            found = true;
        }
    }

    if !found {
        println!("  No templates found. Run 'lattice template add <url>' to add one.");
    }

    Ok(())
}

fn remove_template(templates_dir: &Path, args: &TemplateRemoveArgs) -> Result<()> {
    let dest = templates_dir.join(&args.name);
    if !dest.exists() {
        bail!("Template '{}' not found.", args.name);
    }
    std::fs::remove_dir_all(&dest)?;
    println!(
        "{} Template '{}' removed.",
        style("✓").green().bold(),
        style(&args.name).bold()
    );
    Ok(())
}

fn derive_template_name(url: &str) -> Result<String> {
    let url = url.trim_end_matches('/');
    let last = url.rsplit('/').next().unwrap_or(url);
    let name = last.trim_end_matches(".git");
    if name.is_empty() {
        bail!("Could not derive template name from URL. Use --name to specify one.");
    }
    Ok(name.to_string())
}

fn normalize_git_url(url: &str) -> String {
    if url.starts_with("http://")
        || url.starts_with("https://")
        || url.starts_with("git@")
        || url.starts_with('/')
        || url.starts_with("./")
        || url.starts_with("../")
    {
        url.to_string()
    } else {
        format!("https://{}", url)
    }
}

async fn git_clone(url: &str, dest: &Path) -> Result<()> {
    let status = tokio::process::Command::new("git")
        .args(["clone", "--depth=1", url, dest.to_str().unwrap_or(".")])
        .status()
        .await?;

    if !status.success() {
        bail!("git clone failed for URL: {}", url);
    }
    Ok(())
}

async fn http_fetch(url: &str, dest: &Path) -> Result<()> {
    let tmp = dest.with_extension("tmp.tar.gz");
    std::fs::create_dir_all(dest.parent().unwrap_or(dest))?;

    let status = tokio::process::Command::new("curl")
        .args(["-fsSL", "-o", tmp.to_str().unwrap_or(""), url])
        .status()
        .await?;

    if !status.success() {
        bail!("Failed to download template from: {}", url);
    }

    std::fs::create_dir_all(dest)?;
    let status = tokio::process::Command::new("tar")
        .args([
            "-xzf",
            tmp.to_str().unwrap_or(""),
            "-C",
            dest.to_str().unwrap_or("."),
            "--strip-components=1",
        ])
        .status()
        .await?;

    let _ = std::fs::remove_file(&tmp);

    if !status.success() {
        bail!("Failed to unpack template tarball from: {}", url);
    }

    Ok(())
}

fn read_template_desc(template_dir: &std::path::Path) -> Option<String> {
    let desc_path = template_dir.join("lattice-template.json");
    if let Ok(content) = std::fs::read_to_string(&desc_path) {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
            return val
                .get("description")
                .and_then(|d| d.as_str())
                .map(|s| s.to_string());
        }
    }
    None
}
