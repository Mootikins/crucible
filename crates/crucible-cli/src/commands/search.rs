use anyhow::{Context, Result};
use std::fs;
use std::path::Path;
use std::io::{BufReader, Read};
use crate::config::CliConfig;
use crate::interactive::{FuzzyPicker, SearchResultWithScore};
use crate::output;

/// Maximum file size to process (10MB)
const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024;
/// Maximum content length to keep in memory for search (1MB)
const MAX_CONTENT_LENGTH: usize = 1024 * 1024;
/// Maximum search query length (1000 characters)
const MAX_QUERY_LENGTH: usize = 1000;
/// Minimum meaningful search query length (2 characters)
const MIN_QUERY_LENGTH: usize = 2;

/// Validate and sanitize search query
fn validate_search_query(query: &str) -> Result<String> {
    // Check query length limits
    if query.len() > MAX_QUERY_LENGTH {
        return Err(anyhow::anyhow!(
            "Search query too long ({} > {} characters). Please use a shorter query.",
            query.len(),
            MAX_QUERY_LENGTH
        ));
    }

    // Trim whitespace
    let trimmed = query.trim();

    if trimmed.is_empty() {
        return Err(anyhow::anyhow!(
            "Search query cannot be empty or only whitespace."
        ));
    }

    if trimmed.len() < MIN_QUERY_LENGTH {
        return Err(anyhow::anyhow!(
            "Search query too short ({} < {} characters). Please provide a more specific query.",
            trimmed.len(),
            MIN_QUERY_LENGTH
        ));
    }

    // Check for potentially problematic patterns
    if trimmed.contains('\0') {
        return Err(anyhow::anyhow!(
            "Search query contains invalid null characters."
        ));
    }

    // Remove excessive whitespace
    let normalized = trimmed.split_whitespace().collect::<Vec<_>>().join(" ");

    Ok(normalized)
}

pub async fn execute(
    config: CliConfig,
    query: Option<String>,
    limit: u32,
    format: String,
    show_content: bool,
) -> Result<()> {
    let vault_path = &config.vault.path;

    // Check if kiln path exists
    if !vault_path.exists() {
        eprintln!("Error: Kiln path does not exist: {}", vault_path.display());
        eprintln!("Please set OBSIDIAN_VAULT_PATH to a valid kiln directory.");
        return Err(anyhow::anyhow!("Kiln path does not exist"));
    }

    // Validate query if provided
    let query_ref = if let Some(ref q) = query {
        let validated_query = validate_search_query(q)?;
        Some(validated_query)
    } else {
        None
    };

    let results = if let Some(q) = query_ref {
        // Direct search with query using file system
        search_files_in_kiln(vault_path, &q, limit, show_content)?
    } else {
        // Interactive picker with available files
        let files = get_markdown_files(vault_path)?;

        if files.is_empty() {
            println!("No markdown files found in kiln: {}", vault_path.display());
            return Ok(());
        }

        let mut picker = FuzzyPicker::new();
        let filtered_indices = picker.filter_items(&files, "");

        let results: Vec<SearchResultWithScore> = filtered_indices
            .into_iter()
            .take(limit as usize)
            .filter_map(|(idx, score)| {
                files.get(idx).map(|path| SearchResultWithScore {
                    id: path.clone(),
                    title: path.split('/').next_back().unwrap_or(path).to_string(),
                    content: String::new(),
                    score: score as f64,
                })
            })
            .collect();

        if let Some(selection) = picker.pick_result(&results)? {
            let selected = &results[selection];
            println!("\nSelected: {}\n", selected.title);

            // Get document content from file
            if let Ok(content) = get_file_content(&selected.id) {
                println!("{}", content);
            }
            return Ok(());
        }
        results
    };

    // Output results
    if results.is_empty() {
        if let Some(q) = query {
            println!("No matches found for query: '{}'", q);
        } else {
            println!("No files found in kiln.");
        }
    } else {
        let output = output::format_search_results(&results, &format, false, show_content)?;
        println!("{}", output);

        if let Some(q) = query {
            println!("Found {} results for query: '{}'", results.len(), q);
        } else {
            println!("Found {} files", results.len());
        }
    }

    Ok(())
}

/// Search for files containing the query string in their content
pub fn search_files_in_kiln(kiln_path: &Path, query: &str, limit: u32, include_content: bool) -> Result<Vec<SearchResultWithScore>> {
    let mut results = Vec::new();
    let query_lower = query.to_lowercase();

    // Get all markdown files
    let files = get_markdown_files(kiln_path)?;

    for file_path in files {
        if results.len() >= limit as usize {
            break;
        }

        // Read file content
        match get_file_content(&file_path) {
            Ok(content) => {
                // Check if content contains the query (case-insensitive)
                let content_lower = content.to_lowercase();
                if content_lower.contains(&query_lower) {
                    // Extract a snippet around the match
                    let snippet = extract_snippet(&content, query, include_content);

                    // Calculate a simple relevance score based on match position and frequency
                    let score = calculate_relevance_score(&content, &query_lower);

                    // Get title from filename
                    let title = file_path
                        .split('/')
                        .next_back()
                        .unwrap_or(&file_path)
                        .trim_end_matches(".md")
                        .to_string();

                    results.push(SearchResultWithScore {
                        id: file_path,
                        title,
                        content: snippet,
                        score,
                    });
                }
            }
            Err(_) => {
                // Skip files that can't be read
                continue;
            }
        }
    }

    // Sort by score (descending)
    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

    Ok(results)
}

/// Get all markdown files in the kiln directory recursively
pub fn get_markdown_files(kiln_path: &Path) -> Result<Vec<String>> {
    let mut files = Vec::new();

    // Use WalkDir or manual recursion to find all .md files
    visit_dirs(kiln_path, &mut files)?;

    Ok(files)
}

/// Recursively visit directories and collect markdown files
fn visit_dirs(dir: &Path, files: &mut Vec<String>) -> Result<()> {
    if dir.is_dir() {
        for entry in fs::read_dir(dir)
            .with_context(|| format!("Failed to read directory: {}", dir.display()))?
        {
            let entry = entry
                .with_context(|| format!("Failed to read directory entry in: {}", dir.display()))?;
            let path = entry.path();

            if path.is_dir() {
                // Skip hidden directories and .git
                if let Some(name) = path.file_name() {
                    if let Some(name_str) = name.to_str() {
                        if name_str.starts_with('.') && name_str != ".obsidian" {
                            continue;
                        }
                    }
                }
                visit_dirs(&path, files)?;
            } else if let Some(extension) = path.extension() {
                if extension == "md" {
                    if let Some(path_str) = path.to_str() {
                        files.push(path_str.to_string());
                    }
                }
            }
        }
    }
    Ok(())
}

/// Get file content as a string, handling UTF-8 properly with memory protection
pub fn get_file_content(file_path: &str) -> Result<String> {
    // First check file size to avoid loading huge files into memory
    let metadata = fs::metadata(file_path)
        .with_context(|| format!("Failed to read file metadata: {}", file_path))?;

    let file_size = metadata.len();
    if file_size > MAX_FILE_SIZE {
        return Err(anyhow::anyhow!(
            "File too large ({}MB > {}MB limit): {}",
            file_size / (1024 * 1024),
            MAX_FILE_SIZE / (1024 * 1024),
            file_path
        ));
    }

    // For smaller files, use the efficient read_to_string with UTF-8 handling
    if file_size <= MAX_CONTENT_LENGTH as u64 {
        match fs::read_to_string(file_path) {
            Ok(content) => return Ok(content),
            Err(e) => {
                // If it's a UTF-8 error, try to recover by reading as bytes and cleaning
                if e.to_string().contains("utf-8") || e.to_string().contains("Utf8Error") {
                    return read_file_with_utf8_recovery(file_path);
                }
                return Err(anyhow::anyhow!("Failed to read file: {}: {}", file_path, e));
            }
        }
    }

    // For larger files, read with memory limit
    let file = fs::File::open(file_path)
        .with_context(|| format!("Failed to open file: {}", file_path))?;
    let mut reader = BufReader::new(file);

    let mut content = String::new();
    let mut buffer = [0; 8192]; // 8KB buffer
    let mut bytes_read = 0usize;

    loop {
        let bytes_read_this_time = reader.read(&mut buffer)
            .with_context(|| format!("Failed to read from file: {}", file_path))?;

        if bytes_read_this_time == 0 {
            break; // EOF reached
        }

        bytes_read += bytes_read_this_time;

        // Check memory limit
        if bytes_read > MAX_CONTENT_LENGTH {
            return Err(anyhow::anyhow!(
                "File content exceeds memory limit ({}MB): {}",
                MAX_CONTENT_LENGTH / (1024 * 1024),
                file_path
            ));
        }

        // Convert buffer to string chunk with UTF-8 error handling
        match std::str::from_utf8(&buffer[..bytes_read_this_time]) {
            Ok(chunk) => content.push_str(chunk),
            Err(_) => {
                // Handle UTF-8 errors by replacing invalid sequences
                let chunk = String::from_utf8_lossy(&buffer[..bytes_read_this_time]);
                content.push_str(&chunk);
            }
        }
    }

    Ok(content)
}

/// Read file with UTF-8 error recovery, replacing invalid sequences
fn read_file_with_utf8_recovery(file_path: &str) -> Result<String> {
    let file = fs::File::open(file_path)
        .with_context(|| format!("Failed to open file: {}", file_path))?;
    let mut reader = BufReader::new(file);

    let mut content = String::new();
    let mut buffer = [0; 8192]; // 8KB buffer
    let mut bytes_read = 0usize;

    loop {
        let bytes_read_this_time = reader.read(&mut buffer)
            .with_context(|| format!("Failed to read from file: {}", file_path))?;

        if bytes_read_this_time == 0 {
            break; // EOF reached
        }

        bytes_read += bytes_read_this_time;

        // Check memory limit
        if bytes_read > MAX_CONTENT_LENGTH {
            return Err(anyhow::anyhow!(
                "File content exceeds memory limit ({}MB): {}",
                MAX_CONTENT_LENGTH / (1024 * 1024),
                file_path
            ));
        }

        // Convert with UTF-8 error recovery
        let chunk = String::from_utf8_lossy(&buffer[..bytes_read_this_time]);
        content.push_str(&chunk);
    }

    Ok(content)
}

/// Extract a snippet around the matching text
fn extract_snippet(content: &str, query: &str, include_full: bool) -> String {
    if include_full {
        // Return first few lines of content
        content
            .lines()
            .take(5)
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        // Find first occurrence of query and extract context
        let content_lower = content.to_lowercase();
        let query_lower = query.to_lowercase();

        if let Some(start_pos) = content_lower.find(&query_lower) {
            // Extract some context around the match
            let start = start_pos.saturating_sub(100);
            let end = (start_pos + query.len() + 100).min(content.len());

            let snippet = &content[start..end];

            if start > 0 {
                format!("...{}", snippet)
            } else {
                snippet.to_string()
            }
        } else {
            // Fallback: return first line
            content
                .lines()
                .next()
                .unwrap_or("")
                .to_string()
        }
    }
}

/// Calculate a simple relevance score for search results
fn calculate_relevance_score(content: &str, query_lower: &str) -> f64 {
    let mut score = 0.0;

    // Title matches (first line) get higher score
    let lines: Vec<&str> = content.lines().collect();
    if let Some(first_line) = lines.first() {
        if first_line.to_lowercase().contains(query_lower) {
            score += 100.0;
        }
    }

    // Count occurrences of the query
    let content_lower = content.to_lowercase();
    let mut start = 0;
    let mut count = 0;
    while let Some(pos) = content_lower[start..].find(query_lower) {
        count += 1;
        start += pos + query_lower.len();

        // Early exit if we already have many matches
        if count >= 10 {
            break;
        }
    }

    score += count as f64 * 10.0;

    // Prefer shorter documents (higher score for concise content)
    let length_factor = 1000.0 / (content.len() as f64 + 1.0);
    score += length_factor;

    score
}
