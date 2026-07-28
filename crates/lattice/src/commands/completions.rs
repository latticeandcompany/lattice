use anyhow::Result;
use clap::{Args, CommandFactory};
use clap_complete::{generate, Shell};

use crate::cli::Cli;

#[derive(Args, Debug)]
pub struct CompletionsArgs {
	/// Shell to generate completions for.
	#[arg(value_enum)]
	pub shell: Shell,
}

impl CompletionsArgs {
	pub fn execute(&self) -> Result<()> {
		let mut cmd = Cli::command();
		let name = cmd.get_name().to_string();
		generate(self.shell, &mut cmd, name, &mut std::io::stdout());
		Ok(())
	}
}
