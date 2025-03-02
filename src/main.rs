mod summarizer;
mod stats;

use anyhow::{Context, Result};
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(author, version, about = "Generate a summary of a repository or directory")]
struct Args {
    /// Directory to summarize
    #[arg(required = true)]
    input_dir: PathBuf,

    /// Output file path
    #[arg(required = true)]
    output_file: PathBuf,

    /// Patterns to exclude (comma-separated glob patterns)
    #[arg(short, long)]
    exclude: Option<String>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    
    // Parse exclude patterns
    let exclude_patterns: Vec<String> = match args.exclude {
        Some(ref patterns) => patterns
            .split(',')
            .map(|s| s.trim().to_string())
            .collect(),
        None => Vec::new(),
    };

    println!("Starting directory analysis...");
    
    summarizer::generate_summary(&args.input_dir, &args.output_file, &exclude_patterns)
        .context("Failed to generate summary")?;
    
    println!("Summary generated successfully at: {}", args.output_file.display());
    Ok(())
}
