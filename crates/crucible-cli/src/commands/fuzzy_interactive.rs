//! Interactive fuzzy search using nucleo-picker
//!
//! This module provides an FZF-like interactive interface for searching files
//! in the kiln, with support for:
//! - Real-time fuzzy filtering
//! - Content and filename search
//! - Search mode toggling
//! - Multi-select
//!
//! Implementation follows TDD principles - tests drive the development.

use crate::commands::search::SearchExecutor;
use crate::config::CliConfig;
use anyhow::{Context, Result};
use nucleo_matcher::{
    pattern::{CaseMatching, Normalization, Pattern},
    Config, Matcher, Utf32Str,
};
use nucleo_picker::{nucleo::Utf32String, Picker};
use std::path::Path;

/// Result from content search with snippet
#[derive(Debug, Clone)]
pub struct ContentSearchResult {
    pub path: String,
    pub snippet: String,
}

/// Format file paths for display in nucleo picker
fn format_file_for_picker(file: &String, cols: &mut [Utf32String]) {
    // Show just the filename for cleaner display
    let display = file.split('/').last().unwrap_or(file);
    cols[0] = display.into();
}

/// Execute interactive fuzzy search
///
/// This is the main entry point for the interactive picker.
/// Currently a basic implementation that will evolve through TDD cycles.
pub async fn execute(
    config: CliConfig,
    initial_query: String,
    _limit: u32,
) -> Result<()> {
    let kiln_path = &config.kiln.path;

    // Validate kiln path exists
    if !kiln_path.exists() {
        anyhow::bail!("kiln path does not exist: {}", kiln_path.display());
    }

    // Get files (filtered if query provided)
    let files = if initial_query.is_empty() {
        list_files_in_kiln(kiln_path)?
    } else {
        filter_files_by_query(kiln_path, &initial_query)?
    };

    if files.is_empty() {
        println!("No files found in kiln");
        return Ok(());
    }

    // Create picker
    let mut picker = Picker::default();

    // Populate picker
    let injector = picker.injector();
    for file in files {
        injector.push(file, format_file_for_picker);
    }

    // Open interactive picker
    match picker.pick()? {
        Some(selected_file) => {
            // Build full path to file
            let file_path = kiln_path.join(selected_file);

            // Get editor from environment (default to 'vi' if not set)
            let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());

            // Launch editor
            let status = std::process::Command::new(&editor)
                .arg(&file_path)
                .status()
                .with_context(|| format!("Failed to launch editor: {}", editor))?;

            if !status.success() {
                eprintln!("Editor exited with non-zero status");
            }
        }
        None => {
            println!("No selection made");
        }
    }

    Ok(())
}

/// List all markdown files in a kiln directory
///
/// This function is public to allow integration tests to verify file listing behavior.
/// It's the core functionality that the interactive picker builds upon.
pub fn list_files_in_kiln(kiln_path: &Path) -> Result<Vec<String>> {
    let executor = SearchExecutor::new();
    executor.list_markdown_files(kiln_path)
}

/// Filter files by query using fuzzy matching
///
/// This function filters markdown files in the kiln by matching the query against filenames.
/// It uses nucleo-matcher for fuzzy matching and returns results sorted by match score.
///
/// # Arguments
///
/// * `kiln_path` - Path to the kiln directory
/// * `query` - Query string to match against filenames
///
/// # Returns
///
/// A list of file paths that match the query, sorted by match score (best matches first)
pub fn filter_files_by_query(kiln_path: &Path, query: &str) -> Result<Vec<String>> {
    // Get all files
    let all_files = list_files_in_kiln(kiln_path)?;

    // Set up nucleo matcher with default config
    let mut matcher = Matcher::new(Config::DEFAULT);

    // Create pattern for case-insensitive fuzzy matching
    let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);

    // Filter and score files
    let mut scored: Vec<(String, u32)> = Vec::new();
    let mut buf = Vec::new();

    for file in all_files {
        // Extract basename for matching
        if let Some(basename) = file.split('/').last() {
            // Convert to Utf32Str for matching
            let haystack = Utf32Str::new(basename, &mut buf);
            // Perform fuzzy match using pattern
            if let Some(score) = pattern.score(haystack, &mut matcher) {
                scored.push((file, score));
            }
        }
    }

    // Sort by score (descending - higher scores first)
    scored.sort_by(|a, b| b.1.cmp(&a.1));

    // Return just the file paths
    Ok(scored.into_iter().map(|(file, _)| file).collect())
}

/// Search files by content and return results with snippets
///
/// This function searches all markdown files in the kiln for the query string
/// within their content. It returns matching files with extracted snippets
/// showing context around the match.
///
/// # Arguments
///
/// * `kiln_path` - Path to the kiln directory
/// * `query` - Query string to search for in file contents
///
/// # Returns
///
/// A list of ContentSearchResult with path and snippet for each match
pub fn search_files_by_content(kiln_path: &Path, query: &str) -> Result<Vec<ContentSearchResult>> {
    let executor = SearchExecutor::new();
    let all_files = executor.list_markdown_files(kiln_path)?;

    let mut results = Vec::new();
    let query_lower = query.to_lowercase();

    for file_path in all_files {
        // Try to read the file content
        match executor.read_file_content(kiln_path, &file_path) {
            Ok(content) => {
                let content_lower = content.to_lowercase();

                // Check if the content contains the query (case-insensitive)
                if content_lower.contains(&query_lower) {
                    // Extract a snippet around the match
                    let snippet = extract_snippet(&content, query);

                    results.push(ContentSearchResult {
                        path: file_path,
                        snippet,
                    });
                }
            }
            Err(_) => {
                // Skip files that can't be read (binary, invalid UTF-8, etc.)
                continue;
            }
        }
    }

    Ok(results)
}

/// Extract a snippet around the matching text
///
/// Finds the first occurrence of the query in the content and extracts
/// approximately 100 characters of context around it.
fn extract_snippet(content: &str, query: &str) -> String {
    let content_lower = content.to_lowercase();
    let query_lower = query.to_lowercase();

    if let Some(match_pos) = content_lower.find(&query_lower) {
        // Calculate snippet boundaries with context
        let start = match_pos.saturating_sub(100);
        let end = (match_pos + query.len() + 100).min(content.len());

        // Extract the snippet
        let snippet = &content[start..end];

        // Add ellipsis if we're not at the start
        if start > 0 {
            format!("...{}", snippet)
        } else {
            snippet.to_string()
        }
    } else {
        // Fallback: return first line if no match found (shouldn't happen)
        content.lines().next().unwrap_or("").to_string()
    }
}
