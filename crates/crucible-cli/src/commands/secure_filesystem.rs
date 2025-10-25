//! Secure filesystem operations with comprehensive security measures
//!
//! This module provides secure alternatives to standard filesystem operations,
//! implementing protection against:
//! - Path traversal attacks
//! - Circular symlink infinite loops
//! - Permission error handling
//! - Symlinks pointing outside security boundaries
//! - Resource exhaustion attacks

use anyhow::{Context, Result, anyhow};
use std::fs;
use std::path::{Path, PathBuf, Component};
use std::collections::HashSet;
use std::time::{Duration, Instant};

/// Configuration for secure filesystem operations
#[derive(Debug, Clone)]
pub struct SecureFileSystemConfig {
    /// Maximum number of symlink redirects to prevent infinite loops
    pub max_symlink_depth: usize,
    /// Maximum file size to process (bytes)
    pub max_file_size: u64,
    /// Maximum directory depth to traverse
    pub max_directory_depth: usize,
    /// Timeout for filesystem operations
    pub operation_timeout: Duration,
    /// Whether to continue processing on permission errors
    pub continue_on_permission_error: bool,
}

impl Default for SecureFileSystemConfig {
    fn default() -> Self {
        Self {
            max_symlink_depth: 10,
            max_file_size: 10 * 1024 * 1024, // 10MB
            max_directory_depth: 50,
            operation_timeout: Duration::from_secs(30),
            continue_on_permission_error: true,
        }
    }
}

/// Secure file walker with comprehensive security checks
pub struct SecureFileWalker {
    config: SecureFileSystemConfig,
    kiln_path: PathBuf,
    visited_paths: HashSet<PathBuf>,
    symlink_chain: Vec<PathBuf>,
}

impl SecureFileWalker {
    /// Create a new secure file walker for the given kiln path
    pub fn new(kiln_path: &Path, config: SecureFileSystemConfig) -> Self {
        Self {
            config,
            kiln_path: kiln_path.to_path_buf(),
            visited_paths: HashSet::new(),
            symlink_chain: Vec::new(),
        }
    }

    /// Securely collect all markdown files in the kiln directory
    pub fn collect_markdown_files(&mut self) -> Result<Vec<String>> {
        let mut files = Vec::new();
        let start_time = Instant::now();
        let kiln_path = self.kiln_path.clone(); // Clone to avoid borrowing issues

        self.visit_directory_secure(&kiln_path, &mut files, 0, start_time)?;

        Ok(files)
    }

    /// Visit directory with comprehensive security checks
    fn visit_directory_secure(
        &mut self,
        dir: &Path,
        files: &mut Vec<String>,
        current_depth: usize,
        start_time: Instant,
    ) -> Result<()> {
        // Check timeout
        if start_time.elapsed() > self.config.operation_timeout {
            return Err(anyhow!("Filesystem operation timed out"));
        }

        // Check maximum depth
        if current_depth > self.config.max_directory_depth {
            return Err(anyhow!("Directory depth limit exceeded: {}", current_depth));
        }

        // Ensure directory is within kiln bounds
        if !self.is_within_kiln(dir)? {
            return Err(anyhow!("Directory outside kiln bounds: {}", dir.display()));
        }

        // Check if we've already visited this path (prevents infinite loops)
        let canonical_dir = match fs::canonicalize(dir) {
            Ok(path) => path,
            Err(_) => {
                // If we can't canonicalize, skip this directory
                tracing::warn!("Cannot canonicalize directory, skipping: {}", dir.display());
                return Ok(());
            }
        };

        if self.visited_paths.contains(&canonical_dir) {
            return Ok(()); // Already visited, skip
        }
        self.visited_paths.insert(canonical_dir);

        // Read directory entries with error handling
        let entries = match fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(e) => {
                if self.config.continue_on_permission_error {
                    tracing::warn!("Permission denied accessing directory: {} - {}", dir.display(), e);
                    return Ok(());
                } else {
                    return Err(anyhow!("Permission denied accessing directory: {} - {}", dir.display(), e));
                }
            }
        };

        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(e) => {
                    tracing::warn!("Error reading directory entry: {}", e);
                    continue;
                }
            };

            let path = entry.path();

            // Skip hidden directories and files (except .obsidian)
            if let Some(name) = path.file_name() {
                if let Some(name_str) = name.to_str() {
                    if name_str.starts_with('.') && name_str != ".obsidian" {
                        continue;
                    }
                }
            }

            // Handle path securely
            if path.is_dir() {
                self.visit_directory_secure(&path, files, current_depth + 1, start_time)?;
            } else if let Some(extension) = path.extension() {
                if extension == "md" {
                    // Validate file before adding
                    if self.validate_file_secure(&path)? {
                        if let Some(path_str) = path.to_str() {
                            files.push(path_str.to_string());
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Validate file with comprehensive security checks
    fn validate_file_secure(&self, file_path: &Path) -> Result<bool> {
        // Check if file is within kiln bounds
        if !self.is_within_kiln(file_path)? {
            return Ok(false);
        }

        // Get file metadata
        let metadata = match fs::metadata(file_path) {
            Ok(metadata) => metadata,
            Err(e) => {
                tracing::warn!("Cannot read file metadata: {} - {}", file_path.display(), e);
                return Ok(false);
            }
        };

        // Check file size limit
        if metadata.len() > self.config.max_file_size {
            tracing::warn!("File too large: {} ({} bytes)", file_path.display(), metadata.len());
            return Ok(false);
        }

        // Check if it's a regular file
        if !metadata.is_file() {
            return Ok(false);
        }

        Ok(true)
    }

    /// Check if path is within kiln boundaries
    fn is_within_kiln(&self, path: &Path) -> Result<bool> {
        // Canonicalize both paths to resolve symlinks
        let canonical_kiln = match fs::canonicalize(&self.kiln_path) {
            Ok(path) => path,
            Err(e) => {
                tracing::warn!("Cannot canonicalize kiln path: {} - {}", self.kiln_path.display(), e);
                return Ok(false);
            }
        };

        let canonical_path = match fs::canonicalize(path) {
            Ok(path) => path,
            Err(_) => {
                // If we can't canonicalize, check if it starts with kiln path
                return Ok(path.starts_with(&self.kiln_path));
            }
        };

        // Check if canonical path is within kiln
        Ok(canonical_path.starts_with(canonical_kiln))
    }

    /// Securely resolve a path with symlink validation
    pub fn resolve_path_secure(&mut self, path: &Path) -> Result<PathBuf> {
        self.symlink_chain.clear();
        self.resolve_path_recursive(path, 0)
    }

    /// Recursively resolve path with symlink chain detection
    fn resolve_path_recursive(&mut self, path: &Path, depth: usize) -> Result<PathBuf> {
        // Prevent infinite symlink loops
        if depth > self.config.max_symlink_depth {
            return Err(anyhow!("Symlink depth limit exceeded: {}", depth));
        }

        // Check if we've seen this path in the current chain (circular symlink)
        if self.symlink_chain.contains(&path.to_path_buf()) {
            return Err(anyhow!("Circular symlink detected: {:?}", self.symlink_chain));
        }

        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(e) => {
                return Err(anyhow!("Cannot read metadata for path {}: {}", path.display(), e));
            }
        };

        // If it's a symlink, resolve it
        if metadata.file_type().is_symlink() {
            self.symlink_chain.push(path.to_path_buf());

            let target = match fs::read_link(path) {
                Ok(target) => target,
                Err(e) => {
                    return Err(anyhow!("Cannot read symlink target for {}: {}", path.display(), e));
                }
            };

            // Resolve the target relative to the symlink's parent
            let resolved_target = if target.is_absolute() {
                target
            } else {
                path.parent()
                    .unwrap_or_else(|| Path::new("."))
                    .join(target)
            };

            // Recursively resolve the target
            let result = self.resolve_path_recursive(&resolved_target, depth + 1)?;
            self.symlink_chain.pop();
            Ok(result)
        } else {
            // Not a symlink, return the path as-is
            Ok(path.to_path_buf())
        }
    }
}

/// Secure path validator to prevent path traversal attacks
pub struct PathValidator {
    kiln_path: PathBuf,
}

impl PathValidator {
    /// Create a new path validator for the given kiln path
    pub fn new(kiln_path: &Path) -> Self {
        Self {
            kiln_path: kiln_path.to_path_buf(),
        }
    }

    /// Validate and sanitize a user-provided path
    pub fn validate_path(&self, path: &str) -> Result<PathBuf> {
        // Check for null bytes
        if path.contains('\0') {
            return Err(anyhow!("Path contains null bytes"));
        }

        // Expand ~ to home directory
        let expanded_path = if path.starts_with('~') && (path.len() == 1 || path.starts_with("~/")) {
            if let Some(home_dir) = dirs::home_dir() {
                if path == "~" {
                    home_dir
                } else {
                    home_dir.join(&path[2..])
                }
            } else {
                return Err(anyhow!("Cannot expand ~: home directory not found"));
            }
        } else {
            PathBuf::from(path)
        };

        // If it's already an absolute path, check it directly
        if expanded_path.is_absolute() {
            // Check for path traversal patterns
            self.check_path_traversal(&expanded_path)?;

            // Ensure result is within kiln
            self.ensure_within_kiln(&expanded_path)?;
            return Ok(expanded_path);
        }

        // Check for path traversal patterns
        self.check_path_traversal(&expanded_path)?;

        // Join with kiln path
        let full_path = self.kiln_path.join(expanded_path);

        // Ensure result is within kiln
        self.ensure_within_kiln(&full_path)?;

        Ok(full_path)
    }

    /// Check for path traversal patterns
    fn check_path_traversal(&self, path: &Path) -> Result<()> {
        for component in path.components() {
            match component {
                Component::ParentDir => {
                    return Err(anyhow!("Path traversal detected: '..' component"));
                }
                Component::RootDir => {
                    // Absolute paths are not allowed in user input
                    return Err(anyhow!("Absolute paths are not allowed"));
                }
                Component::CurDir => {
                    // Current dir is fine, but we'll normalize it out
                }
                Component::Normal(name) => {
                    // Check for suspicious patterns in the name
                    let name_str = name.to_string_lossy();
                    if name_str.contains("..") || name_str.contains("\\") {
                        return Err(anyhow!("Suspicious path component: {}", name_str));
                    }
                }
                Component::Prefix(_) => {
                    // Windows-style prefixes are not allowed
                    return Err(anyhow!("Path prefixes are not allowed"));
                }
            }
        }

        Ok(())
    }

    /// Ensure path is within kiln boundaries
    fn ensure_within_kiln(&self, path: &Path) -> Result<()> {
        match fs::canonicalize(path) {
            Ok(canonical_path) => {
                match fs::canonicalize(&self.kiln_path) {
                    Ok(kiln_canonical) => {
                        if !canonical_path.starts_with(kiln_canonical) {
                            return Err(anyhow!("Path outside kiln: {}", path.display()));
                        }
                    }
                    Err(e) => {
                        return Err(anyhow!("Cannot canonicalize kiln path: {}", e));
                    }
                }
            }
            Err(_) => {
                // If we can't canonicalize, check if the path starts with kiln path
                if !path.starts_with(&self.kiln_path) {
                    return Err(anyhow!("Path outside kiln: {}", path.display()));
                }
            }
        }

        Ok(())
    }

    /// Validate search query for malicious content
    pub fn validate_search_query(&self, query: &str) -> Result<String> {
        // Check query length limits
        if query.len() > 1000 {
            return Err(anyhow!("Search query too long ({} characters)", query.len()));
        }

        // Trim whitespace
        let trimmed = query.trim();

        if trimmed.is_empty() {
            return Err(anyhow!("Search query cannot be empty"));
        }

        if trimmed.len() < 2 {
            return Err(anyhow!("Search query too short"));
        }

        // Check for potentially problematic patterns
        if trimmed.contains('\0') {
            return Err(anyhow!("Search query contains invalid null characters"));
        }

        // Remove excessive whitespace
        let normalized = trimmed.split_whitespace().collect::<Vec<_>>().join(" ");

        Ok(normalized)
    }
}

/// Secure file content reader with protection against malicious files
pub struct SecureFileReader {
    config: SecureFileSystemConfig,
    path_validator: PathValidator,
}

impl SecureFileReader {
    /// Create a new secure file reader
    pub fn new(kiln_path: &Path, config: SecureFileSystemConfig) -> Self {
        Self {
            path_validator: PathValidator::new(kiln_path),
            config,
        }
    }

    /// Securely read file content with comprehensive checks
    pub fn read_file_content(&self, file_path: &str) -> Result<String> {
        // Validate file path
        let validated_path = self.path_validator.validate_path(file_path)?;

        // Check file exists and is accessible
        let metadata = match fs::metadata(&validated_path) {
            Ok(metadata) => metadata,
            Err(e) => {
                return Err(anyhow!("Cannot access file: {} - {}", validated_path.display(), e));
            }
        };

        // Check if it's a regular file
        if !metadata.is_file() {
            return Err(anyhow!("Path is not a regular file: {}", validated_path.display()));
        }

        // Check file size limits
        if metadata.len() > self.config.max_file_size {
            return Err(anyhow!(
                "File too large ({}MB > {}MB limit): {}",
                metadata.len() / (1024 * 1024),
                self.config.max_file_size / (1024 * 1024),
                validated_path.display()
            ));
        }

        // Read file content with binary detection
        self.read_file_with_binary_detection(&validated_path)
    }

    /// Read file with binary content detection and UTF-8 handling
    fn read_file_with_binary_detection(&self, file_path: &Path) -> Result<String> {
        use std::io::{BufReader, Read};

        // Open file
        let file = fs::File::open(file_path)
            .with_context(|| format!("Failed to open file: {}", file_path.display()))?;

        let mut reader = BufReader::new(file);

        // Read first bytes for binary detection
        let mut sample_buffer = vec![0u8; 8192];
        let bytes_read = match reader.read(&mut sample_buffer) {
            Ok(0) => return Ok(String::new()), // Empty file
            Ok(n) => n,
            Err(e) => {
                return Err(anyhow!("Failed to read file: {} - {}", file_path.display(), e));
            }
        };

        sample_buffer.truncate(bytes_read);

        // Check if content is binary
        if self.is_binary_content(&sample_buffer) {
            return Err(anyhow!(
                "Binary file detected and skipped for safety: {}",
                file_path.display()
            ));
        }

        // For smaller files, read the entire content
        if bytes_read < sample_buffer.len() {
            // File is smaller than our sample, return the sample as string
            return String::from_utf8(sample_buffer)
                .map_err(|e| anyhow!("Invalid UTF-8 in file: {} - {}", file_path.display(), e));
        }

        // For larger files, continue reading with UTF-8 error handling
        let mut content = String::from_utf8_lossy(&sample_buffer).into_owned();
        let mut buffer = [0u8; 8192];
        let mut total_read = bytes_read;

        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break, // EOF
                Ok(n) => {
                    total_read += n;

                    // Check if we're exceeding memory limits
                    if total_read > self.config.max_file_size as usize {
                        return Err(anyhow!("File size limit exceeded"));
                    }

                    // Convert buffer to string with UTF-8 error recovery
                    let chunk = String::from_utf8_lossy(&buffer[..n]);
                    content.push_str(&chunk);
                }
                Err(e) => {
                    return Err(anyhow!("Error reading file: {} - {}", file_path.display(), e));
                }
            }
        }

        Ok(content)
    }

    /// Detect if content is binary
    fn is_binary_content(&self, content: &[u8]) -> bool {
        // Check for null bytes
        let null_byte_count = content.iter().filter(|&&b| b == 0).count();
        if null_byte_count > 3 {
            return true;
        }

        // Check for non-printable characters
        let non_printable_count = content.iter().filter(|&&b| {
            b < 32 && b != 9 && b != 10 && b != 13 // Not tab, newline, or carriage return
        }).count();

        // If more than 30% of bytes are non-printable, consider it binary
        let binary_ratio = non_printable_count as f32 / content.len() as f32;
        if binary_ratio > 0.3 {
            return true;
        }

        // Check UTF-8 validity
        if std::str::from_utf8(content).is_err() {
            return true;
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn test_path_traversal_prevention() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let validator = PathValidator::new(temp_dir.path());

        let malicious_paths = vec![
            "../../../etc/passwd",
            "..\\..\\..\\windows\\system32",
            "/etc/passwd",
            "~/.ssh/id_rsa",
            "path/../../../outside",
        ];

        for path in malicious_paths {
            assert!(validator.validate_path(path).is_err(), "Should block path: {}", path);
        }

        Ok(())
    }

    #[test]
    fn test_valid_path_handling() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let validator = PathValidator::new(temp_dir.path());

        // Create a valid file
        let valid_file = temp_dir.path().join("test.md");
        fs::write(&valid_file, "# Test")?;

        assert!(validator.validate_path("test.md").is_ok());

        Ok(())
    }

    #[test]
    fn test_search_query_validation() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let validator = PathValidator::new(temp_dir.path());

        // Valid queries
        assert!(validator.validate_search_query("test query").is_ok());
        assert!(validator.validate_search_query("  spaced query  ").is_ok());

        // Invalid queries
        assert!(validator.validate_search_query("").is_err());
        assert!(validator.validate_search_query("a").is_err()); // Too short
        assert!(validator.validate_search_query("\0null byte").is_err());

        Ok(())
    }
}