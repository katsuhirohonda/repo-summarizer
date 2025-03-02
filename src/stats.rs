use anyhow::Result;
use rayon::prelude::*;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

/// Stores statistics about analyzed files
#[derive(Debug, Default)]
pub struct FileStats {
    /// Total number of files
    pub total_files: usize,
    
    /// Total number of directories
    pub total_directories: usize,
    
    /// Total lines of code
    pub total_lines: usize,
    
    /// Count of files by extension
    pub extension_counts: HashMap<String, usize>,
    
    /// Count of lines by extension
    pub extension_lines: HashMap<String, usize>,
}

/// Collect statistics about the given files
pub fn collect_stats(file_paths: &[PathBuf]) -> Result<FileStats> {
    let mut stats = FileStats::default();
    
    // Count files
    stats.total_files = file_paths.len();
    
    // Count unique directories
    let directories: std::collections::HashSet<_> = file_paths
        .iter()
        .filter_map(|path| path.parent().map(|p| p.to_path_buf()))
        .collect();
    stats.total_directories = directories.len();
    
    // Process files in parallel to collect extension and line counts
    let results: Vec<Result<(String, usize)>> = file_paths
        .par_iter()
        .map(|path| -> Result<(String, usize)> {
            // Get file extension
            let extension = path
                .extension()
                .and_then(|ext| ext.to_str())
                .unwrap_or("")
                .to_string();
            
            // Count lines
            let line_count = count_lines(path)?;
            
            Ok((extension, line_count))
        })
        .collect();
    
    // Process results
    for result in results {
        match result {
            Ok((extension, line_count)) => {
                *stats.extension_counts.entry(extension.clone()).or_insert(0) += 1;
                *stats.extension_lines.entry(extension).or_insert(0) += line_count;
                stats.total_lines += line_count;
            },
            Err(err) => {
                eprintln!("Warning: Failed to process file statistics: {}", err);
            }
        }
    }
    
    Ok(stats)
}

/// Count the number of lines in a file
fn count_lines(path: &Path) -> Result<usize> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    Ok(reader.lines().count())
}
