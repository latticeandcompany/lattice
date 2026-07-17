use console::style;

pub struct OutputManager {
    pub loquacious: bool,
    pub is_ci: bool,
}

impl OutputManager {
    pub fn new(loquacious: bool) -> Self {
        let is_ci = !console::user_attended() || std::env::var("CI").is_ok();
        Self { loquacious, is_ci }
    }

    pub fn header(&self, msg: &str) {
        if self.loquacious || self.is_ci {
            println!("{}", msg);
        } else {
            println!("{}", style(msg).bold());
        }
    }

    pub fn log_start(&self, workspace: &str, task: &str) {
        println!(
            "{} {} {}",
            style("▶").cyan().bold(),
            style(format!("{}:{}", workspace, task)).bold(),
            style("starting...").dim()
        );
    }

    pub fn log_cache_hit(&self, workspace: &str, task: &str, hash: &str) {
        println!(
            "{} {} {} {}",
            style("●").green().bold(),
            style(format!("{}:{}", workspace, task)).bold(),
            style("cache hit").green(),
            style(format!("[{}]", hash)).dim()
        );
    }

    pub fn log_success(&self, workspace: &str, task: &str, duration_ms: u64) {
        println!(
            "{} {} {} {}",
            style("✓").green().bold(),
            style(format!("{}:{}", workspace, task)).bold(),
            style(format!("{:.2}s", duration_ms as f64 / 1000.0)).dim(),
            style("done").green()
        );
    }

    pub fn log_failure(&self, workspace: &str, task: &str) {
        eprintln!(
            "{} {} {}",
            style("✗").red().bold(),
            style(format!("{}:{}", workspace, task)).bold(),
            style("FAILED").red().bold()
        );
    }

    pub fn log_skipped(&self, workspace: &str, task: &str) {
        if self.loquacious {
            println!(
                "{} {} {}",
                style("○").dim(),
                style(format!("{}:{}", workspace, task)).dim(),
                style("skipped (no command)").dim()
            );
        }
    }

    #[allow(dead_code)]
    pub fn info(&self, msg: &str) {
        println!("{} {}", style("info").cyan().bold(), msg);
    }

    pub fn warn(&self, msg: &str) {
        eprintln!("{} {}", style("warn").yellow().bold(), msg);
    }

    pub fn detail(&self, msg: &str) {
        if self.loquacious {
            println!("  {}", style(msg).dim());
        }
    }

    #[allow(dead_code)]
    pub fn task_output(&self, line: &str, is_stderr: bool) {
        if is_stderr {
            eprintln!("    {}", style(line).dim());
        } else {
            println!("    {}", line);
        }
    }

    pub fn summary(&self, total: usize, cached: usize, failed: usize, elapsed_ms: u64) {
        println!();
        println!(
            "{}  {} tasks, {} cached, {} failed  {}",
            style("◆").bold(),
            style(total).bold(),
            style(cached).green(),
            if failed > 0 {
                style(failed.to_string()).red()
            } else {
                style(failed.to_string()).dim()
            },
            style(format!("{:.2}s", elapsed_ms as f64 / 1000.0)).dim()
        );
    }
}
