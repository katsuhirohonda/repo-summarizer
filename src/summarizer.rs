use anyhow::{Context, Result};
use ignore::{DirEntry, Walk, WalkBuilder};
use ptree::{Style, TreeBuilder, print_tree};
use rayon::prelude::*;
use std::collections::HashSet;
use std::fs::{File, read_link};
use std::io::{self, Write as IoWrite};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use crate::stats::{FileStats, collect_stats};

/// Generate a summary of the given directory
pub fn generate_summary(
    input_dir: &Path,
    output_file: &Path,
    exclude_patterns: &[String],
) -> Result<()> {
    // Ensure the input directory exists
    if !input_dir.exists() {
        anyhow::bail!("Input directory does not exist: {}", input_dir.display());
    }

    // Create or truncate the output file
    let file = File::create(output_file).context("Failed to create output file")?;
    let mut writer = BufWriter::new(file);

    // Build the directory tree and collect file information
    let mut tree = ptree::TreeBuilder::new(input_dir.to_string_lossy().to_string());
    let mut file_paths = Vec::new();
    let input_dir_canonicalized = input_dir
        .canonicalize()
        .unwrap_or_else(|_| input_dir.to_path_buf());

    // Collect all entries while building the tree
    let walker = build_walker(input_dir, exclude_patterns);
    process_entries(walker, &input_dir_canonicalized, &mut tree, &mut file_paths)?;

    // Write the tree structure to a string
    let mut tree_string = Vec::new();
    print_tree_to_writer(&mut tree_string, &tree.build())?;
    writeln!(writer, "{}", String::from_utf8_lossy(&tree_string))
        .context("Failed to write tree structure")?;
    writeln!(writer).context("Failed to write newline")?;

    // Process and write file contents
    process_file_contents(&mut writer, &file_paths)?;

    // Collect and write statistics
    let stats = collect_stats(&file_paths)?;
    write_statistics(&mut writer, &stats)?;

    writer.flush().context("Failed to flush output")?;

    Ok(())
}

/// Build a walker with excluded patterns
fn build_walker(input_dir: &Path, exclude_patterns: &[String]) -> Walk {
    let mut builder = WalkBuilder::new(input_dir);

    // Add standard excludes
    builder.filter_entry(|entry| {
        !entry
            .file_name()
            .to_str()
            .map(|s| s.starts_with('.') && s != "." && s != "..")
            .unwrap_or(false)
    });

    // Add custom exclude patterns
    for pattern in exclude_patterns {
        let pattern = pattern.clone();
        builder.filter_entry(move |entry| {
            !entry
                .path()
                .to_str()
                .map(|s| {
                    glob::Pattern::new(&pattern)
                        .map(|p| p.matches(s))
                        .unwrap_or(false)
                })
                .unwrap_or(false)
        });
    }

    builder.build()
}

/// Process entries from the walker, building the tree and collecting file paths
fn process_entries(
    walker: Walk,
    base_dir: &Path,
    tree_builder: &mut ptree::TreeBuilder,
    file_paths: &mut Vec<PathBuf>,
) -> Result<()> {
    // Keep track of directories we've added to the tree
    let mut added_dirs = HashSet::new();
    added_dirs.insert(base_dir.to_path_buf());

    // Process each entry
    for result in walker {
        let entry = match result {
            Ok(entry) => entry,
            Err(err) => {
                eprintln!("Warning: Failed to access entry: {}", err);
                continue;
            }
        };

        let path = entry.path();

        // Skip the root directory itself
        if path == base_dir {
            continue;
        }

        // Handle the entry based on its type
        if entry.file_type().map_or(false, |ft| ft.is_file()) {
            // Only include the file if it's not binary
            if !is_binary_file(path)? {
                // Add file to tree
                add_path_to_tree(path, base_dir, tree_builder, &mut added_dirs);
                file_paths.push(path.to_path_buf());
            }
        } else if entry.file_type().map_or(false, |ft| ft.is_dir()) {
            // Add directory to tree
            add_path_to_tree(path, base_dir, tree_builder, &mut added_dirs);
        } else if entry.file_type().map_or(false, |ft| ft.is_symlink()) {
            // Handle symlink - add to tree with target information
            let link_target = match read_link(path) {
                Ok(target) => format!(" -> {}", target.display()),
                Err(_) => " -> [unreadable link]".to_string(),
            };

            let rel_path = path.strip_prefix(base_dir).unwrap_or(path);
            let parent_path = match rel_path.parent() {
                Some(parent) if !parent.as_os_str().is_empty() => {
                    add_dir_to_tree(parent, base_dir, tree_builder, &mut added_dirs);
                    parent
                }
                _ => Path::new(""),
            };

            // Add the symlink with its target noted
            let item_name = format!(
                "{}{}",
                path.file_name().unwrap_or_default().to_string_lossy(),
                link_target
            );

            if !parent_path.as_os_str().is_empty() {
                tree_builder.begin_child(parent_path.to_string_lossy().to_string());
                tree_builder.add_empty_child(item_name);
                tree_builder.end_child();
            } else {
                tree_builder.begin_child(base_dir.to_string_lossy().to_string());
                tree_builder.add_empty_child(item_name);
                tree_builder.end_child();
            }
        }
    }

    Ok(())
}

/// Add a path to the tree, ensuring all parent directories exist
fn add_path_to_tree(
    path: &Path,
    base_dir: &Path,
    tree_builder: &mut ptree::TreeBuilder,
    added_dirs: &mut HashSet<PathBuf>,
) {
    let rel_path = path.strip_prefix(base_dir).unwrap_or(path);

    if path.is_dir() {
        add_dir_to_tree(rel_path, base_dir, tree_builder, added_dirs);
    } else {
        let parent_path = match rel_path.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => {
                add_dir_to_tree(parent, base_dir, tree_builder, added_dirs);
                parent.to_string_lossy().to_string()
            }
            _ => base_dir.to_string_lossy().to_string(),
        };

        let file_name = path.file_name().unwrap_or_default().to_string_lossy();
        tree_builder.begin_child(parent_path);
        tree_builder.add_empty_child(file_name.to_string());
        tree_builder.end_child();
    }
}

/// Add a directory and all its parents to the tree
fn add_dir_to_tree(
    dir_path: &Path,
    base_dir: &Path,
    tree_builder: &mut ptree::TreeBuilder,
    added_dirs: &mut HashSet<PathBuf>,
) {
    let mut current = PathBuf::new();
    let full_path = base_dir.join(dir_path);

    if added_dirs.contains(&full_path) {
        return;
    }

    for component in dir_path.components() {
        let prev_path = current.clone();
        current.push(component);

        let full_current = base_dir.join(&current);
        if !added_dirs.contains(&full_current) {
            let component_name = component.as_os_str().to_string_lossy();
            if prev_path.as_os_str().is_empty() {
                tree_builder.begin_child(base_dir.to_string_lossy().to_string());
                tree_builder.add_empty_child(component_name.to_string());
                tree_builder.end_child();
            } else {
                let prev_str = prev_path.to_string_lossy().to_string();
                tree_builder.begin_child(prev_str);
                tree_builder.add_empty_child(component_name.to_string());
                tree_builder.end_child();
            }

            added_dirs.insert(full_current);
        }
    }
}

/// Check if a file is binary
fn is_binary_file(path: &Path) -> Result<bool> {
    // Use the infer crate to detect file type
    if let Ok(buffer) = std::fs::read(path) {
        if infer::is_binary(&buffer) {
            return Ok(true);
        }

        // Also check for null bytes which often indicate binary files
        for byte in buffer.iter().take(8000) {
            if *byte == 0 {
                return Ok(true);
            }
        }
    }

    Ok(false)
}

/// Process and write the contents of each file
fn process_file_contents(writer: &mut impl Write, file_paths: &[PathBuf]) -> Result<()> {
    for path in file_paths {
        // Create a separator line
        let separator = "-".repeat(80);
        writeln!(writer, "{}:", path.display()).context("Failed to write path")?;
        writeln!(writer, "{}", separator).context("Failed to write separator")?;

        // Try to read the file as text
        match std::fs::read_to_string(path) {
            Ok(content) => {
                // Write file contents with line numbers
                for (i, line) in content.lines().enumerate() {
                    writeln!(writer, "{} | {}", i + 1, line).context("Failed to write line")?;
                }
            }
            Err(err) => {
                writeln!(writer, "Error reading file: {}", err).context("Failed to write error")?;
            }
        }

        writeln!(writer, "{}", separator).context("Failed to write separator")?;
        writeln!(writer).context("Failed to write newline")?;
    }

    Ok(())
}

/// Print a tree structure to a writer
fn print_tree_to_writer(writer: &mut impl Write, tree: &ptree::item::StringItem) -> Result<()> {
    let config = Style::default();
    ptree::write_tree_with(tree, writer, &config).context("Failed to write tree")?;
    Ok(())
}

/// Write statistics about the analyzed files
fn write_statistics(writer: &mut impl Write, stats: &FileStats) -> Result<()> {
    writeln!(writer, "Project Statistics").context("Failed to write statistics header")?;
    writeln!(writer, "==================").context("Failed to write statistics header")?;
    writeln!(writer, "Total files: {}", stats.total_files).context("Failed to write statistics")?;
    writeln!(writer, "Total directories: {}", stats.total_directories)
        .context("Failed to write statistics")?;
    writeln!(writer, "Total lines of code: {}", stats.total_lines)
        .context("Failed to write statistics")?;

    if !stats.extension_counts.is_empty() {
        writeln!(writer, "\nFile types:").context("Failed to write statistics")?;

        // Sort extensions by count (descending)
        let mut extensions: Vec<_> = stats.extension_counts.iter().collect();
        extensions.sort_by(|a, b| b.1.cmp(a.1));

        for (ext, count) in extensions {
            let ext_name = if ext.is_empty() {
                "[no extension]"
            } else {
                ext
            };
            writeln!(writer, "  {}: {} files", ext_name, count)
                .context("Failed to write statistics")?;
        }
    }

    if !stats.extension_lines.is_empty() {
        writeln!(writer, "\nLines of code by file type:").context("Failed to write statistics")?;

        // Sort extensions by line count (descending)
        let mut extensions: Vec<_> = stats.extension_lines.iter().collect();
        extensions.sort_by(|a, b| b.1.cmp(a.1));

        for (ext, lines) in extensions {
            let ext_name = if ext.is_empty() {
                "[no extension]"
            } else {
                ext
            };
            writeln!(writer, "  {}: {} lines", ext_name, lines)
                .context("Failed to write statistics")?;
        }
    }

    Ok(())
}
