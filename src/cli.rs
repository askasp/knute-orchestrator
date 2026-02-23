use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "knute", about = "Multi-session Claude Code manager for monorepos")]
pub struct Cli {
    /// Path to the git repository (defaults to current directory)
    #[arg(short, long)]
    pub repo: Option<PathBuf>,
}
