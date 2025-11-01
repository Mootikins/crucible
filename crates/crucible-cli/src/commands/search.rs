use super::secure_filesystem::{
    PathValidator, SecureFileReader, SecureFileSystemConfig, SecureFileWalker,
};
use crate::config::CliConfig;
use crate::interactive::{FuzzyPicker, SearchResultWithScore};
use crate::output;
use anyhow::{Context, Result};
use std::fs;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Binary file detection constants
const NULL_BYTE: u8 = 0x00;
const BINARY_DETECTION_SAMPLE_SIZE: usize = 8192; // First 8KB for binary detection
const MAX_NULL_BYTE_COUNT: usize = 3; // More than this indicates binary
const BINARY_CONTENT_RATIO: f32 = 0.3; // 30% binary indicators = binary file

/// Common binary file signatures
const BINARY_SIGNATURES: &[&[u8]] = &[
    // Image formats
    &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A], // PNG
    &[0xFF, 0xD8, 0xFF],                               // JPEG
    &[0x47, 0x49, 0x46, 0x38, 0x39, 0x61],             // GIF89a
    &[0x42, 0x4D],                                     // BMP
    &[0x52, 0x49, 0x46, 0x46],                         // RIFF (WebP, AVI, etc.)
    // Archive formats
    &[0x50, 0x4B, 0x03, 0x04],             // ZIP
    &[0x50, 0x4B, 0x05, 0x06],             // ZIP (empty)
    &[0x50, 0x4B, 0x07, 0x08],             // ZIP (spanned)
    &[0x1F, 0x8B, 0x08],                   // GZIP
    &[0x42, 0x5A, 0x68],                   // BZIP2
    &[0xFD, 0x37, 0x7A, 0x58, 0x5A, 0x00], // XZ
    &[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C], // 7Z
    // Executable formats
    &[0x7F, 0x45, 0x4C, 0x46], // ELF
    &[0x4D, 0x5A],             // PE/DOS
    &[0xFE, 0xED, 0xFA, 0xCE], // Mach-O (32-bit)
    &[0xFE, 0xED, 0xFA, 0xCF], // Mach-O (64-bit)
    &[0xCE, 0xFA, 0xED, 0xFE], // Mach-O (reverse 32-bit)
    &[0xCF, 0xFA, 0xED, 0xFE], // Mach-O (reverse 64-bit)
    // Document formats
    b"%PDF",                                           // PDF
    &[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1], // Microsoft Office
    // Audio/Video formats
    &[0x49, 0x44, 0x33],       // MP3
    &[0xFF, 0xFB],             // MP3 (MPEG)
    &[0xFF, 0xF3],             // MP3 (MPEG)
    &[0xFF, 0xF2],             // MP3 (MPEG)
    &[0x52, 0x49, 0x46, 0x46], // RIFF/WAV
    &[0x1A, 0x45, 0xDF, 0xA3], // Matroska/WebM
    // Other binary formats
    &[0x00, 0x00, 0x01, 0x00], // ICO
    &[0x00, 0x00, 0x02, 0x00], // CUR
];

/// Abstraction over filesystem and validation operations used by search commands.
pub trait SearchBackend: Send + Sync {
    fn validate_query(&self, kiln_path: &Path, query: &str) -> Result<String>;
    fn list_markdown_files(&self, kiln_path: &Path) -> Result<Vec<String>>;
    fn read_file_content(&self, kiln_path: &Path, file_id: &str) -> Result<String>;
}

#[derive(Clone)]
pub struct SecureSearchBackend {
    config: SecureFileSystemConfig,
}

impl SecureSearchBackend {
    pub fn new() -> Self {
        Self {
            config: SecureFileSystemConfig::default(),
        }
    }

    pub fn with_config(config: SecureFileSystemConfig) -> Self {
        Self { config }
    }
}

impl Default for SecureSearchBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl SearchBackend for SecureSearchBackend {
    fn validate_query(&self, kiln_path: &Path, query: &str) -> Result<String> {
        PathValidator::new(kiln_path)
            .validate_search_query(query)
            .map_err(|e| anyhow::anyhow!("Invalid search query: {}", e))
    }

    fn list_markdown_files(&self, kiln_path: &Path) -> Result<Vec<String>> {
        collect_markdown_files(kiln_path, &self.config)
    }

    fn read_file_content(&self, kiln_path: &Path, file_id: &str) -> Result<String> {
        SecureFileReader::new(kiln_path, self.config.clone()).read_file_content(file_id)
    }
}

/// Coordinator that orchestrates search operations using an injected backend.
pub struct SearchExecutor {
    adapter: Arc<dyn SearchBackend>,
}

impl Default for SearchExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl SearchExecutor {
    /// Create a service with the default secure adapter.
    pub fn new() -> Self {
        Self::with_adapter(Arc::new(SecureSearchBackend::default()))
    }

    /// Create a service with a custom adapter (useful for testing).
    pub fn with_adapter(adapter: Arc<dyn SearchBackend>) -> Self {
        Self { adapter }
    }

    pub fn validate_query(&self, kiln_path: &Path, query: &str) -> Result<String> {
        self.adapter.validate_query(kiln_path, query)
    }

    pub fn list_markdown_files(&self, kiln_path: &Path) -> Result<Vec<String>> {
        self.adapter.list_markdown_files(kiln_path)
    }

    pub fn read_file_content(&self, kiln_path: &Path, file_id: &str) -> Result<String> {
        self.adapter.read_file_content(kiln_path, file_id)
    }

    pub fn search_with_query(
        &self,
        kiln_path: &Path,
        query: &str,
        limit: u32,
        include_content: bool,
    ) -> Result<Vec<SearchResultWithScore>> {
        let mut results = Vec::new();
        let query_lower = query.to_lowercase();

        for file_path in self.list_markdown_files(kiln_path)? {
            if results.len() >= limit as usize {
                break;
            }

            match self.read_file_content(kiln_path, &file_path) {
                Ok(content) => {
                    let content_lower = content.to_lowercase();
                    if content_lower.contains(&query_lower) {
                        let snippet = extract_snippet(&content, query, include_content);
                        let score = calculate_relevance_score(&content, &query_lower);

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
                Err(_) => continue,
            }
        }

        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(results)
    }
}

/// Detect if file content is binary by analyzing byte patterns
pub fn is_binary_content(content: &[u8]) -> bool {
    // Check for known binary signatures
    for signature in BINARY_SIGNATURES {
        if content.starts_with(signature) {
            return true;
        }
    }

    // Count null bytes in the sample
    let sample_size = content.len().min(BINARY_DETECTION_SAMPLE_SIZE);
    let sample = &content[..sample_size];

    let null_byte_count = sample.iter().filter(|&&b| b == NULL_BYTE).count();
    if null_byte_count > MAX_NULL_BYTE_COUNT {
        return true;
    }

    // Count non-printable ASCII bytes (excluding common whitespace)
    let non_printable_count = sample
        .iter()
        .filter(|&&b| {
            b < 32 && b != 9 && b != 10 && b != 13 // Not tab, newline, or carriage return
        })
        .count();

    // If more than 30% of bytes are non-printable, consider it binary
    let binary_ratio = non_printable_count as f32 / sample_size as f32;
    if binary_ratio > BINARY_CONTENT_RATIO {
        return true;
    }

    // Check for UTF-8 validity
    if let Ok(content_str) = std::str::from_utf8(sample) {
        // If it's valid UTF-8, check for suspicious patterns
        let replacement_chars = content_str.chars().filter(|&c| c == '\u{FFFD}').count();
        if replacement_chars > 10 {
            return true;
        }
    } else {
        // Invalid UTF-8 indicates binary content
        return true;
    }

    false
}

/// Read first bytes of a file for binary detection
fn read_file_sample(file_path: &str, sample_size: usize) -> Result<Vec<u8>> {
    let mut file =
        fs::File::open(file_path).with_context(|| format!("Failed to open file: {}", file_path))?;

    let mut buffer = vec![0; sample_size];
    let bytes_read = file
        .read(&mut buffer)
        .with_context(|| format!("Failed to read from file: {}", file_path))?;

    buffer.truncate(bytes_read);
    Ok(buffer)
}

/// Maximum file size to process (10MB)
const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024;
/// Maximum content length to keep in memory for search (1MB)
const MAX_CONTENT_LENGTH: usize = 1024 * 1024;
/// Maximum search query length (1000 characters)
const MAX_QUERY_LENGTH: usize = 1000;
/// Minimum meaningful search query length (2 characters)
const MIN_QUERY_LENGTH: usize = 2;

/// Validate and sanitize search query using secure validator
fn validate_search_query(query: &str, validator: &PathValidator) -> Result<String> {
    validator
        .validate_search_query(query)
        .map_err(|e| anyhow::anyhow!("Invalid search query: {}", e))
}

pub async fn execute(
    config: CliConfig,
    query: Option<String>,
    limit: u32,
    format: String,
    show_content: bool,
) -> Result<()> {
    let kiln_path = &config.kiln.path;

    // Check if kiln path exists
    if !kiln_path.exists() {
        eprintln!("Error: kiln path does not exist: {}", kiln_path.display());
        eprintln!("Please configure kiln.path in your config file (see: cru config show)");
        return Err(anyhow::anyhow!("kiln path does not exist"));
    }

    let service = SearchExecutor::new();

    // Validate query if provided (sanitized copy used for execution)
    let sanitized_query = if let Some(ref q) = query {
        Some(service.validate_query(kiln_path, q)?)
    } else {
        None
    };

    let results = if let Some(ref q) = sanitized_query {
        // Direct search with query using the service layer
        service.search_with_query(kiln_path, q, limit, show_content)?
    } else {
        // Interactive picker with available files
        let files = service.list_markdown_files(kiln_path)?;

        if files.is_empty() {
            println!("No markdown files found in kiln: {}", kiln_path.display());
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

            // Get document content from file using secure reader
            if let Ok(content) = service.read_file_content(kiln_path, &selected.id) {
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
pub fn search_files_in_kiln(
    kiln_path: &Path,
    query: &str,
    limit: u32,
    include_content: bool,
) -> Result<Vec<SearchResultWithScore>> {
    SearchExecutor::new().search_with_query(kiln_path, query, limit, include_content)
}

/// Secure version of search_files_in_kiln using secure filesystem utilities
pub fn search_files_in_kiln_secure(
    kiln_path: &Path,
    query: &str,
    limit: u32,
    include_content: bool,
    config: &SecureFileSystemConfig,
) -> Result<Vec<SearchResultWithScore>> {
    let adapter = Arc::new(SecureSearchBackend::with_config(config.clone()));
    SearchExecutor::with_adapter(adapter).search_with_query(
        kiln_path,
        query,
        limit,
        include_content,
    )
}

/// Get all markdown files in the kiln directory recursively using secure walker
pub fn get_markdown_files(kiln_path: &Path) -> Result<Vec<String>> {
    SearchExecutor::new().list_markdown_files(kiln_path)
}

/// Secure version of get_markdown_files using the secure file walker
pub fn get_markdown_files_secure(
    kiln_path: &Path,
    config: &SecureFileSystemConfig,
) -> Result<Vec<String>> {
    let adapter = Arc::new(SecureSearchBackend::with_config(config.clone()));
    SearchExecutor::with_adapter(adapter).list_markdown_files(kiln_path)
}

fn collect_markdown_files(
    kiln_path: &Path,
    config: &SecureFileSystemConfig,
) -> Result<Vec<String>> {
    let mut walker = SecureFileWalker::new(kiln_path, config.clone());
    walker.collect_markdown_files()
}

/// Legacy function for backward compatibility - uses insecure approach
/// This function is deprecated but kept for existing tests that expect it to fail
pub fn get_markdown_files_legacy(kiln_path: &Path) -> Result<Vec<String>> {
    let mut files = Vec::new();

    // Use WalkDir or manual recursion to find all .md files
    visit_dirs(kiln_path, &mut files)?;

    Ok(files)
}

/// Recursively visit directories and collect markdown files (legacy - insecure)
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

/// Get file content as a string using secure reader
pub fn get_file_content(file_path: &str) -> Result<String> {
    let path_buf = PathBuf::from(file_path);

    // For absolute paths, we need to determine a reasonable kiln path
    // For files discovered by the secure walker, they should already be validated
    let kiln_path = if path_buf.is_absolute() {
        // Try to find a reasonable parent directory as kiln
        // Look for common kiln indicators or use parent directory
        find_kiln_path_for_file(&path_buf)?
    } else {
        // For relative paths, use current directory
        Path::new(".").to_path_buf()
    };

    let secure_config = SecureFileSystemConfig::default();
    let secure_reader = SecureFileReader::new(&kiln_path, secure_config);

    // Read the file with the secure reader (no fallback for security)
    secure_reader.read_file_content(file_path)
}

/// Find a reasonable kiln path for a given file
fn find_kiln_path_for_file(file_path: &Path) -> Result<PathBuf> {
    // Try different strategies to find the kiln path

    // Strategy 1: Use parent directory
    if let Some(parent) = file_path.parent() {
        return Ok(parent.to_path_buf());
    }

    // Strategy 2: Use current directory
    Ok(Path::new(".").to_path_buf())
}

/// Read file with UTF-8 error recovery, replacing invalid sequences
fn read_file_with_utf8_recovery(file_path: &str) -> Result<String> {
    // Perform binary detection first for recovery function as well
    let file_sample = read_file_sample(file_path, BINARY_DETECTION_SAMPLE_SIZE)?;
    if is_binary_content(&file_sample) {
        return Err(anyhow::anyhow!(
            "Binary file detected and skipped for safety: {}",
            file_path
        ));
    }

    let file =
        fs::File::open(file_path).with_context(|| format!("Failed to open file: {}", file_path))?;
    let mut reader = BufReader::new(file);

    let mut content = String::new();
    let mut buffer = [0; 8192]; // 8KB buffer
    let mut bytes_read = 0usize;

    loop {
        let bytes_read_this_time = reader
            .read(&mut buffer)
            .with_context(|| format!("Failed to read from file: {}", file_path))?;

        if bytes_read_this_time == 0 {
            break; // EOF reached
        }

        bytes_read += bytes_read_this_time;

        // Check memory limit - if we're about to exceed it, stop reading and truncate
        if bytes_read > MAX_CONTENT_LENGTH {
            // We've hit the memory limit, stop reading more content
            break;
        }

        // Convert with UTF-8 error recovery
        let chunk = String::from_utf8_lossy(&buffer[..bytes_read_this_time]);
        content.push_str(&chunk);
    }

    Ok(content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct MockSearchBackend {
        files: HashMap<String, String>,
    }

    impl SearchBackend for MockSearchBackend {
        fn validate_query(&self, _kiln_path: &Path, query: &str) -> Result<String> {
            Ok(query.to_string())
        }

        fn list_markdown_files(&self, _kiln_path: &Path) -> Result<Vec<String>> {
            Ok(self.files.keys().cloned().collect())
        }

        fn read_file_content(&self, _kiln_path: &Path, file_id: &str) -> Result<String> {
            self.files
                .get(file_id)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("missing file"))
        }
    }

    fn build_mock_adapter() -> Arc<dyn SearchBackend> {
        let mut files = HashMap::new();
        files.insert(
            "notes/rust.md".to_string(),
            "Rust programming language overview".to_string(),
        );
        files.insert(
            "notes/python.md".to_string(),
            "Python scripting tips".to_string(),
        );
        Arc::new(MockSearchBackend { files })
    }

    #[test]
    fn search_executor_returns_matching_results() {
        let service = SearchExecutor::with_adapter(build_mock_adapter());
        let results = service
            .search_with_query(Path::new("/tmp"), "rust", 10, false)
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "notes/rust.md");
    }

    #[test]
    fn search_executor_respects_limit() {
        let service = SearchExecutor::with_adapter(build_mock_adapter());
        let results = service
            .search_with_query(Path::new("/tmp"), "", 1, false)
            .unwrap();

        assert_eq!(results.len(), 1);
    }
}

/// Extract a snippet around the matching text
fn extract_snippet(content: &str, query: &str, include_full: bool) -> String {
    if include_full {
        // Return first few lines of content
        content.lines().take(5).collect::<Vec<_>>().join("\n")
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
            content.lines().next().unwrap_or("").to_string()
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
