use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::commands::{
    generate::GenerateArgs, run::RunArgs, setup::SetupArgs, template::TemplateArgs,
    version::VersionArgs,
};

#[derive(Parser, Debug)]
#[command(
    name = "lattice",
    about = "Cross-language monorepo task orchestrator",
    version = env!("CARGO_PKG_VERSION"),
    long_about = "Lattice is a local-first, high-performance, language-agnostic monorepo build system \
for polyglot codebases, with robust dependency-graph resolution and parallel task execution.\n\n\
If you know Turborepo, you already know the shape: define a 'pipeline' in lattice.json, then \
`lattice run <task>` to execute it across your workspaces. Scope with --filter, tune parallelism \
with --concurrency, and keep going past failures with --continue. Familiar — not a clone: Lattice \
keeps its own voice, output, and extras like -l/--loquacious."
)]
pub struct Cli {
    #[arg(
        short,
        long,
        global = true,
        help = "Stream detailed line-by-line output (bypasses interactive UI)"
    )]
    pub loquacious: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    #[command(about = "Print version information")]
    Version(VersionArgs),

    #[command(about = "Run tasks across all workspaces")]
    Run(RunArgs),

    #[command(about = "Run native dependency installers for all workspaces")]
    Setup(SetupArgs),

    #[command(about = "Manage workspace templates")]
    Template(TemplateArgs),

    #[command(about = "Scaffold a new workspace from a template")]
    Generate(GenerateArgs),
}

impl Cli {
    pub async fn execute(self) -> Result<()> {
        match self.command {
            Commands::Version(args) => args.execute().await,
            Commands::Run(args) => args.execute(self.loquacious).await,
            Commands::Setup(args) => args.execute(self.loquacious).await,
            Commands::Template(args) => args.execute().await,
            Commands::Generate(args) => args.execute().await,
        }
    }
}
