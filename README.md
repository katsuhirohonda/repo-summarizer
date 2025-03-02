# Repo Summarizer

A command-line tool that generates a comprehensive summary of a repository or directory structure.

## Features

- Recursively traverses directories
- Displays directory structure as a tree
- Shows file contents with line numbers
- Ignores binary files
- Provides statistics about file types and line counts
- Excludes specified directories or files (e.g., `.git`, `node_modules`, etc.)
- Notes symlinks without following them

## Installation

```
cargo install repo-summarizer
```

## Usage

```
repo-summarizer [OPTIONS] <INPUT_DIR> <OUTPUT_FILE>
```

### Options

```
-e, --exclude <PATTERNS>    Patterns to exclude (comma-separated glob patterns)
-h, --help                  Print help
-V, --version               Print version
```

### Example

```
repo-summarizer ~/projects/my-rust-app ./summary.txt --exclude "target,node_modules,.git"
```

## Output Format

The output file will include:
- A tree view of the directory structure
- File contents with line numbers
- Statistics about the project (number of files, directories, lines of code)

## License

This project is licensed under the MIT License - see the LICENSE file for details.
