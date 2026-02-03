//! File information and metadata types for the crucible-watch system
//!
//! This module defines the core data structures used to represent file information
//! throughout the file watching architecture. It provides a clean, focused approach
//! to file metadata management with proper integration to the crucible-core hashing system.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::error::Error;
use crucible_core::types::hashing::{FileHash, HashAlgorithm};

/// Comprehensive file information for the crucible-watch system
///
/// This struct represents all the metadata needed for file operations, change detection,
/// and processing within the file watching architecture. It serves as the primary
/// data structure for file information throughout the system.
///
/// ## Design Principles
///
/// - **Type Safety**: Uses proper `FileHash` type instead of raw bytes
/// - **Performance Optimized**: Contains all metadata needed for efficient change detection
/// - **Serialization Ready**: Supports serde for persistent storage and IPC
/// - **Integration Focused**: Designed to work seamlessly with crucible-core traits
///
/// ## Fields
///
/// - `path`: Absolute filesystem path for direct file access
/// - `relative_path`: Relative path from the monitored root directory
/// - `content_hash`: BLAKE3 content hash for change detection (from crucible-core)
/// - `file_size`: File size in bytes for quick change detection
/// - `modified_time`: Last modification timestamp from filesystem metadata
/// - `file_type`: Categorized file type for processing decisions
/// - `is_accessible`: Whether the file can be read and processed
/// - `created_time`: Optional file creation timestamp for additional metadata
/// - `permissions`: Optional file permissions metadata
///
/// ## Examples
///
/// // TODO: Add example once API stabilizes
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileInfo {
    /// Absolute path to the file in the filesystem
    path: PathBuf,

    /// Relative path from the monitored root directory
    relative_path: String,

    /// BLAKE3 hash of file content for change detection
    content_hash: FileHash,

    /// File size in bytes
    file_size: u64,

    /// Last modification time from filesystem metadata
    modified_time: SystemTime,

    /// Categorized file type for processing decisions
    file_type: FileType,

    /// Whether the file is accessible and can be read
    is_accessible: bool,

    /// Optional file creation timestamp
    created_time: Option<SystemTime>,

    /// Optional file permissions metadata
    permissions: Option<FilePermissions>,
}

impl FileInfo {
    /// Create a new FileInfo with the essential fields
    ///
    /// This constructor creates a FileInfo with the minimum required fields.
    /// Use the builder pattern for more flexible construction with optional fields.
    ///
    /// # Arguments
    ///
    /// * `path` - Absolute path to the file
    /// * `relative_path` - Relative path from the monitored root
    /// * `content_hash` - Content hash for change detection
    /// * `file_size` - File size in bytes
    /// * `modified_time` - Last modification timestamp
    /// * `file_type` - Categorized file type
    ///
    /// # Returns
    ///
    /// A new FileInfo instance
    ///
    /// # Examples
    ///
    /// // TODO: Add example once API stabilizes
    pub fn new(
        path: PathBuf,
        relative_path: String,
        content_hash: FileHash,
        file_size: u64,
        modified_time: SystemTime,
        file_type: FileType,
    ) -> Self {
        Self {
            path,
            relative_path,
            content_hash,
            file_size,
            modified_time,
            file_type,
            is_accessible: true, // Default to accessible
            created_time: None,
            permissions: None,
        }
    }

    /// Create a FileInfo builder for flexible construction
    ///
    /// Returns a builder that allows setting all fields with validation.
    /// This is the preferred method for creating FileInfo instances with optional fields.
    ///
    /// # Returns
    ///
    /// A FileInfoBuilder instance
    ///
    /// # Examples
    ///
    /// // TODO: Add example once API stabilizes
    pub fn builder() -> FileInfoBuilder {
        FileInfoBuilder::new()
    }

    /// Create a FileInfo with zero hash for placeholder purposes
    ///
    /// This method is useful when you need to create a FileInfo entry
    /// before the actual content hash is calculated.
    ///
    /// # Arguments
    ///
    /// * `path` - Absolute path to the file
    /// * `relative_path` - Relative path from the monitored root
    /// * `file_size` - File size in bytes
    /// * `modified_time` - Last modification timestamp
    /// * `file_type` - Categorized file type
    ///
    /// # Returns
    ///
    /// A FileInfo with zero content hash
    pub fn with_zero_hash(
        path: PathBuf,
        relative_path: String,
        file_size: u64,
        modified_time: SystemTime,
        file_type: FileType,
    ) -> Self {
        Self::new(
            path,
            relative_path,
            FileHash::zero(),
            file_size,
            modified_time,
            file_type,
        )
    }

    /// Get the absolute path to the file
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Get the relative path from the monitored root
    pub fn relative_path(&self) -> &str {
        &self.relative_path
    }

    /// Get the content hash for change detection
    pub fn content_hash(&self) -> FileHash {
        self.content_hash
    }

    /// Get the content hash as a hex string
    pub fn content_hash_hex(&self) -> String {
        self.content_hash.to_hex()
    }

    /// Get the file size in bytes
    pub fn file_size(&self) -> u64 {
        self.file_size
    }

    /// Get the last modification time
    pub fn modified_time(&self) -> SystemTime {
        self.modified_time
    }

    /// Get the file type
    pub fn file_type(&self) -> FileType {
        self.file_type
    }

    /// Check if the file is accessible
    pub fn is_accessible(&self) -> bool {
        self.is_accessible
    }

    /// Get the optional creation time
    pub fn created_time(&self) -> Option<SystemTime> {
        self.created_time
    }

    /// Get the optional file permissions
    pub fn permissions(&self) -> Option<&FilePermissions> {
        self.permissions.as_ref()
    }

    /// Check if the file is a markdown file
    pub fn is_markdown(&self) -> bool {
        matches!(self.file_type, FileType::Markdown)
    }

    /// Check if the file is a text file
    pub fn is_text(&self) -> bool {
        matches!(
            self.file_type,
            FileType::Text | FileType::Markdown | FileType::Code
        )
    }

    /// Check if the file is a code file
    pub fn is_code(&self) -> bool {
        matches!(self.file_type, FileType::Code)
    }

    /// Check if the file is a binary file
    pub fn is_binary(&self) -> bool {
        matches!(self.file_type, FileType::Binary)
    }

    /// Check if the content hash is zero (placeholder)
    pub fn has_zero_hash(&self) -> bool {
        self.content_hash.is_zero()
    }

    /// Update the content hash
    ///
    /// This method is useful when you initially create a FileInfo with a zero hash
    /// and later calculate the actual content hash.
    ///
    /// # Arguments
    ///
    /// * `content_hash` - The new content hash
    pub fn update_content_hash(&mut self, content_hash: FileHash) {
        self.content_hash = content_hash;
    }

    /// Update file metadata from filesystem
    ///
    /// This method refreshes the file metadata by querying the filesystem.
    /// Returns an error if the file cannot be accessed.
    ///
    /// # Returns
    ///
    /// Ok(()) if metadata was updated successfully, Err(Error) otherwise
    pub fn update_metadata(&mut self) -> Result<(), Error> {
        use std::fs;

        let metadata = fs::metadata(&self.path).map_err(|e| Error::FileIoError {
            path: self.path.clone(),
            error: e.to_string(),
        })?;

        self.file_size = metadata.len();
        self.modified_time = metadata.modified().map_err(|e| Error::FileIoError {
            path: self.path.clone(),
            error: e.to_string(),
        })?;

        self.created_time = metadata.created().ok();
        self.is_accessible = true;

        // Update permissions if supported on this platform
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().readonly() {
                self.permissions = Some(FilePermissions::ReadOnly);
            } else {
                self.permissions = Some(FilePermissions::Writable(metadata.permissions().mode()));
            }
        }

        #[cfg(not(unix))]
        {
            if metadata.permissions().readonly() {
                self.permissions = Some(FilePermissions::ReadOnly);
            } else {
                self.permissions = Some(FilePermissions::Writable(0o666));
            }
        }

        Ok(())
    }

    /// Check if this file has the same content as another FileInfo
    ///
    /// This method compares only the content hash, making it efficient
    /// for change detection operations.
    ///
    /// # Arguments
    ///
    /// * `other` - The other FileInfo to compare with
    ///
    /// # Returns
    ///
    /// true if the content hashes are identical
    pub fn content_matches(&self, other: &FileInfo) -> bool {
        self.content_hash == other.content_hash
    }

    /// Check if file metadata matches (size and modification time)
    ///
    /// This method provides a quick check that can be used before
    /// calculating content hashes for efficiency.
    ///
    /// # Arguments
    ///
    /// * `other` - The other FileInfo to compare with
    ///
    /// # Returns
    ///
    /// true if size and modification time are identical
    pub fn metadata_matches(&self, other: &FileInfo) -> bool {
        self.file_size == other.file_size && self.modified_time == other.modified_time
    }

    /// Convert to crucible-core FileHashInfo for trait integration
    ///
    /// This method converts the FileInfo to a FileHashInfo which can be used
    /// with the crucible-core change detection traits.
    ///
    /// # Returns
    ///
    /// A FileHashInfo compatible with crucible-core traits
    pub fn to_file_hash_info(&self) -> crucible_core::types::hashing::FileHashInfo {
        crucible_core::types::hashing::FileHashInfo::new(
            self.content_hash,
            self.file_size,
            self.modified_time,
            HashAlgorithm::Blake3, // We always use BLAKE3 for content hashing
            self.relative_path.clone(),
        )
    }

    /// Create from crucible-core FileHashInfo
    ///
    /// This method creates a FileInfo from a FileHashInfo, useful when
    /// working with the crucible-core change detection system.
    ///
    /// # Arguments
    ///
    /// * `file_hash_info` - The FileHashInfo to convert
    /// * `root_path` - The root path to resolve absolute paths
    ///
    /// # Returns
    ///
    /// A FileInfo instance
    pub fn from_file_hash_info(
        file_hash_info: crucible_core::types::hashing::FileHashInfo,
        root_path: &Path,
    ) -> Self {
        let absolute_path = root_path.join(&file_hash_info.relative_path);
        let file_type = FileType::from_path(&absolute_path);

        Self {
            path: absolute_path,
            relative_path: file_hash_info.relative_path,
            content_hash: file_hash_info.content_hash,
            file_size: file_hash_info.size,
            modified_time: file_hash_info.modified,
            file_type,
            is_accessible: true,
            created_time: None,
            permissions: None,
        }
    }
}

/// Builder for FileInfo with validation
///
/// Provides a fluent interface for constructing FileInfo instances
/// with proper validation of all fields.
#[derive(Debug, Clone)]
pub struct FileInfoBuilder {
    path: Option<PathBuf>,
    relative_path: Option<String>,
    content_hash: Option<FileHash>,
    file_size: Option<u64>,
    modified_time: Option<SystemTime>,
    file_type: Option<FileType>,
    is_accessible: Option<bool>,
    created_time: Option<SystemTime>,
    permissions: Option<FilePermissions>,
}

impl FileInfoBuilder {
    /// Create a new FileInfoBuilder
    pub fn new() -> Self {
        Self {
            path: None,
            relative_path: None,
            content_hash: None,
            file_size: None,
            modified_time: None,
            file_type: None,
            is_accessible: Some(true), // Default to accessible
            created_time: None,
            permissions: None,
        }
    }

    /// Set the absolute path to the file
    pub fn path(mut self, path: PathBuf) -> Self {
        self.path = Some(path);
        self
    }

    /// Set the relative path from the monitored root
    pub fn relative_path(mut self, relative_path: String) -> Self {
        self.relative_path = Some(relative_path);
        self
    }

    /// Set the content hash
    pub fn content_hash(mut self, content_hash: FileHash) -> Self {
        self.content_hash = Some(content_hash);
        self
    }

    /// Set the file size in bytes
    pub fn file_size(mut self, file_size: u64) -> Self {
        self.file_size = Some(file_size);
        self
    }

    /// Set the last modification time
    pub fn modified_time(mut self, modified_time: SystemTime) -> Self {
        self.modified_time = Some(modified_time);
        self
    }

    /// Set the file type
    pub fn file_type(mut self, file_type: FileType) -> Self {
        self.file_type = Some(file_type);
        self
    }

    /// Set whether the file is accessible
    pub fn is_accessible(mut self, is_accessible: bool) -> Self {
        self.is_accessible = Some(is_accessible);
        self
    }

    /// Set the optional creation time
    pub fn created_time(mut self, created_time: SystemTime) -> Self {
        self.created_time = Some(created_time);
        self
    }

    /// Set the optional file permissions
    pub fn permissions(mut self, permissions: FilePermissions) -> Self {
        self.permissions = Some(permissions);
        self
    }

    /// Build the FileInfo after validation
    ///
    /// # Returns
    ///
    /// Ok(FileInfo) if all required fields are present and valid,
    /// Err(Error) if validation fails
    pub fn build(self) -> Result<FileInfo, Error> {
        let path = self.path.ok_or_else(|| Error::ValidationError {
            field: "path".to_string(),
            message: "Path is required".to_string(),
        })?;

        let relative_path = self.relative_path.ok_or_else(|| Error::ValidationError {
            field: "relative_path".to_string(),
            message: "Relative path is required".to_string(),
        })?;

        let content_hash = self.content_hash.unwrap_or_else(FileHash::zero);
        let file_size = self.file_size.unwrap_or(0);
        let modified_time = self.modified_time.unwrap_or_else(SystemTime::now);
        let file_type = self.file_type.unwrap_or_else(|| FileType::from_path(&path));

        Ok(FileInfo {
            path,
            relative_path,
            content_hash,
            file_size,
            modified_time,
            file_type,
            is_accessible: self.is_accessible.unwrap_or(true),
            created_time: self.created_time,
            permissions: self.permissions,
        })
    }
}

impl Default for FileInfoBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Categorized file types for processing decisions
///
/// This enum provides a high-level categorization of file types to help
/// the file watching system make appropriate processing decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FileType {
    /// Markdown files (.md, .markdown)
    Markdown,
    /// Plain text files (.txt, .text)
    Text,
    /// Source code files (.rs, .js, .py, etc.)
    Code,
    /// Configuration files (.yaml, .json, .toml, etc.)
    Config,
    /// Note files (.pdf, .doc, .odt, etc.)
    Note,
    /// Image files (.png, .jpg, .svg, etc.)
    Image,
    /// Audio files (.mp3, .wav, .ogg, etc.)
    Audio,
    /// Video files (.mp4, .avi, .mov, etc.)
    Video,
    /// Archive files (.zip, .tar, .gz, etc.)
    Archive,
    /// Binary files (executables, compiled libraries, etc.)
    Binary,
    /// Unknown file type
    Unknown,
}

impl FileType {
    /// Determine file type from path extension
    ///
    /// This method examines the file extension to determine the appropriate
    /// file type category. It's case-insensitive and handles common extensions.
    ///
    /// # Arguments
    ///
    /// * `path` - The file path to examine
    ///
    /// # Returns
    ///
    /// The determined FileType
    ///
    /// # Examples
    ///
    /// // TODO: Add example once API stabilizes
    pub fn from_path(path: &Path) -> Self {
        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_lowercase());

        match extension.as_deref() {
            Some("md") | Some("markdown") => FileType::Markdown,
            Some("txt") | Some("text") => FileType::Text,
            Some("rs") | Some("py") | Some("js") | Some("ts") | Some("jsx") | Some("tsx")
            | Some("go") | Some("java") | Some("c") | Some("cpp") | Some("h") | Some("hpp")
            | Some("cs") | Some("php") | Some("rb") | Some("swift") | Some("kt")
            | Some("scala") | Some("clj") | Some("hs") | Some("ml") | Some("sh") => FileType::Code,
            Some("yaml") | Some("yml") | Some("json") | Some("toml") | Some("ini")
            | Some("cfg") | Some("conf") | Some("xml") | Some("plist") => FileType::Config,
            Some("pdf") | Some("doc") | Some("docx") | Some("odt") | Some("rtf") => FileType::Note,
            Some("png") | Some("jpg") | Some("jpeg") | Some("gif") | Some("svg") | Some("bmp")
            | Some("webp") | Some("ico") => FileType::Image,
            Some("mp3") | Some("wav") | Some("ogg") | Some("flac") | Some("aac") => FileType::Audio,
            Some("mp4") | Some("avi") | Some("mov") | Some("wmv") | Some("flv") | Some("webm") => {
                FileType::Video
            }
            Some("zip") | Some("tar") | Some("gz") | Some("bz2") | Some("xz") | Some("7z")
            | Some("rar") | Some("deb") | Some("rpm") => FileType::Archive,
            Some("exe") | Some("dll") | Some("so") | Some("dylib") | Some("bin") | Some("app") => {
                FileType::Binary
            }
            _ => FileType::Unknown,
        }
    }

    /// Check if this file type should be processed for content
    ///
    /// Returns true for file types that contain extractable text content
    /// that should be indexed and processed.
    pub fn should_process_content(&self) -> bool {
        matches!(
            self,
            FileType::Markdown | FileType::Text | FileType::Code | FileType::Config
        )
    }

    /// Check if this file type should be watched for changes
    ///
    /// Returns true for file types that are meaningful to watch.
    /// Some file types (like temporary files or caches) might be ignored.
    pub fn should_watch(&self) -> bool {
        !matches!(
            self,
            FileType::Binary | FileType::Archive | FileType::Unknown
        )
    }

    /// Get the file type as a string
    pub fn as_str(&self) -> &'static str {
        match self {
            FileType::Markdown => "markdown",
            FileType::Text => "text",
            FileType::Code => "code",
            FileType::Config => "config",
            FileType::Note => "note",
            FileType::Image => "image",
            FileType::Audio => "audio",
            FileType::Video => "video",
            FileType::Archive => "archive",
            FileType::Binary => "binary",
            FileType::Unknown => "unknown",
        }
    }
}

impl std::fmt::Display for FileType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// File permissions metadata
///
/// Represents file permissions in a cross-platform way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FilePermissions {
    /// File is read-only
    ReadOnly,
    /// File is writable (platform-specific mode)
    Writable(u32),
    /// File is executable (Unix-specific)
    Executable(u32),
}

impl FilePermissions {
    /// Check if the file is read-only
    pub fn is_readonly(&self) -> bool {
        matches!(self, FilePermissions::ReadOnly)
    }

    /// Check if the file is writable
    pub fn is_writable(&self) -> bool {
        !matches!(self, FilePermissions::ReadOnly)
    }

    /// Check if the file is executable (Unix systems only)
    pub fn is_executable(&self) -> bool {
        matches!(self, FilePermissions::Executable(_))
    }

    /// Get the permission mode (Unix systems only)
    pub fn mode(&self) -> Option<u32> {
        match self {
            FilePermissions::Writable(mode) | FilePermissions::Executable(mode) => Some(*mode),
            FilePermissions::ReadOnly => None,
        }
    }
}

impl std::fmt::Display for FilePermissions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FilePermissions::ReadOnly => write!(f, "read-only"),
            FilePermissions::Writable(mode) => write!(f, "writable({:#o})", mode),
            FilePermissions::Executable(mode) => write!(f, "executable({:#o})", mode),
        }
    }
}

/// Configuration for file scanning operations
///
/// This struct provides configuration options for how files are scanned
/// and processed by the file watching system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanConfig {
    /// Maximum file size to process (in bytes)
    pub max_file_size: u64,

    /// File types to include in scanning
    pub include_types: Vec<FileType>,

    /// File types to exclude from scanning
    pub exclude_types: Vec<FileType>,

    /// File patterns to exclude (glob patterns)
    pub exclude_patterns: Vec<String>,

    /// Whether to follow symbolic links
    pub follow_symlinks: bool,

    /// Whether to calculate content hashes for all files
    pub calculate_hashes: bool,

    /// Maximum depth for directory traversal
    pub max_depth: Option<usize>,

    /// Whether to ignore hidden files and directories
    pub ignore_hidden: bool,
}

impl Default for ScanConfig {
    fn default() -> Self {
        Self {
            max_file_size: 10 * 1024 * 1024, // 10MB
            include_types: vec![
                FileType::Markdown,
                FileType::Text,
                FileType::Code,
                FileType::Config,
            ],
            exclude_types: vec![FileType::Binary, FileType::Archive],
            exclude_patterns: vec![
                "*.tmp".to_string(),
                "*.cache".to_string(),
                ".git/**".to_string(),
                ".crucible/**".to_string(), // SurrealDB database directory
                ".obsidian/**".to_string(), // Obsidian config directory
                ".trash/**".to_string(),    // Obsidian trash
                "node_modules/**".to_string(), // Match any depth in node_modules
                "target/**".to_string(),
            ],
            follow_symlinks: false,
            calculate_hashes: true,
            max_depth: Some(50),
            ignore_hidden: true,
        }
    }
}

impl ScanConfig {
    /// Create a new ScanConfig with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if a file type should be included in scanning
    pub fn should_include_type(&self, file_type: FileType) -> bool {
        // Check exclusions first
        if self.exclude_types.contains(&file_type) {
            return false;
        }

        // If include_types is empty, include all non-excluded types
        if self.include_types.is_empty() {
            return true;
        }

        // Check if the type is explicitly included
        self.include_types.contains(&file_type)
    }

    /// Check if a file should be included based on size
    pub fn should_include_size(&self, file_size: u64) -> bool {
        file_size <= self.max_file_size
    }

    /// Check if a path matches any exclude patterns
    pub fn matches_exclude_pattern(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy();

        for pattern in &self.exclude_patterns {
            if let Ok(glob_pattern) = glob::Pattern::new(pattern) {
                if glob_pattern.matches(&path_str) {
                    return true;
                }
            }
        }

        false
    }

    /// Create a configuration that includes all file types
    pub fn include_all() -> Self {
        let mut config = Self::default();
        config.include_types.clear();
        config.exclude_types.clear();
        config
    }

    /// Create a configuration for markdown files only
    pub fn markdown_only() -> Self {
        let mut config = Self::default();
        config.include_types = vec![FileType::Markdown];
        config
    }

    /// Create a configuration for code files only
    pub fn code_only() -> Self {
        let mut config = Self::default();
        config.include_types = vec![FileType::Code];
        config
    }
}

/// Result of a file scanning operation
///
/// Contains comprehensive information about the results of scanning
/// a directory tree for files.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScanResult {
    /// Files that were discovered and successfully processed
    pub discovered_files: Vec<FileInfo>,

    /// Paths that were skipped and why
    pub skipped_paths: Vec<SkipReason>,

    /// Errors that occurred during scanning
    pub scan_errors: Vec<ScanError>,

    /// Total number of files considered
    pub total_considered: usize,

    /// Number of files successfully processed
    pub successful_files: usize,

    /// Number of files skipped
    pub skipped_files: usize,

    /// Duration of the scan operation
    pub scan_duration: std::time::Duration,

    /// Size of all successfully processed files
    pub total_size: u64,
}

impl ScanResult {
    /// Create a new empty ScanResult
    pub fn new() -> Self {
        Self {
            discovered_files: Vec::new(),
            skipped_paths: Vec::new(),
            scan_errors: Vec::new(),
            total_considered: 0,
            successful_files: 0,
            skipped_files: 0,
            scan_duration: std::time::Duration::from_secs(0),
            total_size: 0,
        }
    }

    /// Check if the scan was successful
    pub fn is_successful(&self) -> bool {
        self.scan_errors.is_empty() || self.successful_files > 0
    }

    /// Get the success rate as a percentage
    pub fn success_rate(&self) -> f64 {
        if self.total_considered == 0 {
            1.0
        } else {
            self.successful_files as f64 / self.total_considered as f64
        }
    }

    /// Get files by type
    pub fn files_by_type(&self, file_type: FileType) -> Vec<&FileInfo> {
        self.discovered_files
            .iter()
            .filter(|file| file.file_type() == file_type)
            .collect()
    }

    /// Get markdown files
    pub fn markdown_files(&self) -> Vec<&FileInfo> {
        self.files_by_type(FileType::Markdown)
    }

    /// Get code files
    pub fn code_files(&self) -> Vec<&FileInfo> {
        self.files_by_type(FileType::Code)
    }

    /// Get summary statistics
    pub fn summary(&self) -> ScanSummary {
        ScanSummary {
            total_files: self.total_considered,
            successful_files: self.successful_files,
            skipped_files: self.skipped_files,
            error_count: self.scan_errors.len(),
            scan_duration: self.scan_duration,
            total_size: self.total_size,
            success_rate: self.success_rate(),
        }
    }
}

impl Default for ScanResult {
    fn default() -> Self {
        Self::new()
    }
}

/// Summary statistics for a scan operation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScanSummary {
    pub total_files: usize,
    pub successful_files: usize,
    pub skipped_files: usize,
    pub error_count: usize,
    pub scan_duration: std::time::Duration,
    pub total_size: u64,
    pub success_rate: f64,
}

/// Reason why a file or directory was skipped during scanning
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkipReason {
    pub path: PathBuf,
    pub reason: SkipType,
}

/// Types of skip reasons
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SkipType {
    /// File type is excluded by configuration
    ExcludedType(FileType),
    /// File size exceeds maximum limit
    TooLarge(u64),
    /// Path matches exclude pattern
    ExcludedPattern(String),
    /// Hidden file or directory (when ignore_hidden is true)
    HiddenFile,
    /// Symbolic link (when follow_symlinks is false)
    Symlink,
    /// Maximum directory depth exceeded
    DepthLimit,
    /// File is not accessible
    NotAccessible(String),
}

/// Error that occurred during scanning
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScanError {
    pub path: PathBuf,
    pub error_type: ScanErrorType,
    pub message: String,
}

/// Types of scan errors
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ScanErrorType {
    /// I/O error accessing file or directory
    IoError,
    /// Error calculating file hash
    HashError,
    /// Invalid path or file name
    InvalidPath,
    /// Permission denied
    PermissionDenied,
    /// File system error
    FileSystemError,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::SystemTime;

    // --- FileType tests ---

    #[test]
    fn file_type_from_markdown_extensions() {
        assert_eq!(
            FileType::from_path(Path::new("note.md")),
            FileType::Markdown
        );
        assert_eq!(
            FileType::from_path(Path::new("note.markdown")),
            FileType::Markdown
        );
        assert_eq!(
            FileType::from_path(Path::new("NOTE.MD")),
            FileType::Markdown
        );
    }

    #[test]
    fn file_type_from_code_extensions() {
        let code_files = [
            "main.rs",
            "app.py",
            "index.js",
            "types.ts",
            "Component.tsx",
            "main.go",
            "App.java",
        ];
        for f in code_files {
            assert_eq!(
                FileType::from_path(Path::new(f)),
                FileType::Code,
                "failed for {f}"
            );
        }
    }

    #[test]
    fn file_type_from_config_extensions() {
        let config_files = ["config.yaml", "data.json", "Cargo.toml", "settings.ini"];
        for f in config_files {
            assert_eq!(
                FileType::from_path(Path::new(f)),
                FileType::Config,
                "failed for {f}"
            );
        }
    }

    #[test]
    fn file_type_from_media_extensions() {
        assert_eq!(FileType::from_path(Path::new("photo.png")), FileType::Image);
        assert_eq!(FileType::from_path(Path::new("song.mp3")), FileType::Audio);
        assert_eq!(FileType::from_path(Path::new("clip.mp4")), FileType::Video);
    }

    #[test]
    fn file_type_from_binary_and_archive() {
        assert_eq!(FileType::from_path(Path::new("app.exe")), FileType::Binary);
        assert_eq!(FileType::from_path(Path::new("lib.so")), FileType::Binary);
        assert_eq!(
            FileType::from_path(Path::new("data.zip")),
            FileType::Archive
        );
        assert_eq!(
            FileType::from_path(Path::new("backup.tar")),
            FileType::Archive
        );
    }

    #[test]
    fn file_type_unknown_extension() {
        assert_eq!(
            FileType::from_path(Path::new("mystery.xyz123")),
            FileType::Unknown
        );
    }

    #[test]
    fn file_type_no_extension() {
        assert_eq!(
            FileType::from_path(Path::new("Makefile")),
            FileType::Unknown
        );
    }

    #[test]
    fn file_type_should_process_content() {
        assert!(FileType::Markdown.should_process_content());
        assert!(FileType::Text.should_process_content());
        assert!(FileType::Code.should_process_content());
        assert!(FileType::Config.should_process_content());
        assert!(!FileType::Binary.should_process_content());
        assert!(!FileType::Image.should_process_content());
    }

    #[test]
    fn file_type_should_watch() {
        assert!(FileType::Markdown.should_watch());
        assert!(FileType::Code.should_watch());
        assert!(!FileType::Binary.should_watch());
        assert!(!FileType::Archive.should_watch());
        assert!(!FileType::Unknown.should_watch());
    }

    #[test]
    fn file_type_as_str_and_display() {
        assert_eq!(FileType::Markdown.as_str(), "markdown");
        assert_eq!(FileType::Code.as_str(), "code");
        assert_eq!(format!("{}", FileType::Binary), "binary");
    }

    // --- FilePermissions tests ---

    #[test]
    fn permissions_readonly() {
        let perm = FilePermissions::ReadOnly;
        assert!(perm.is_readonly());
        assert!(!perm.is_writable());
        assert!(!perm.is_executable());
        assert_eq!(perm.mode(), None);
    }

    #[test]
    fn permissions_writable() {
        let perm = FilePermissions::Writable(0o644);
        assert!(!perm.is_readonly());
        assert!(perm.is_writable());
        assert!(!perm.is_executable());
        assert_eq!(perm.mode(), Some(0o644));
    }

    #[test]
    fn permissions_executable() {
        let perm = FilePermissions::Executable(0o755);
        assert!(!perm.is_readonly());
        assert!(perm.is_writable());
        assert!(perm.is_executable());
        assert_eq!(perm.mode(), Some(0o755));
    }

    #[test]
    fn permissions_display() {
        assert_eq!(format!("{}", FilePermissions::ReadOnly), "read-only");
        assert!(format!("{}", FilePermissions::Writable(0o644)).contains("writable"));
        assert!(format!("{}", FilePermissions::Executable(0o755)).contains("executable"));
    }

    // --- ScanConfig tests ---

    #[test]
    fn scan_config_default_includes_text_types() {
        let config = ScanConfig::default();
        assert!(config.should_include_type(FileType::Markdown));
        assert!(config.should_include_type(FileType::Text));
        assert!(config.should_include_type(FileType::Code));
        assert!(config.should_include_type(FileType::Config));
    }

    #[test]
    fn scan_config_default_excludes_binary() {
        let config = ScanConfig::default();
        assert!(!config.should_include_type(FileType::Binary));
        assert!(!config.should_include_type(FileType::Archive));
    }

    #[test]
    fn scan_config_should_include_size() {
        let config = ScanConfig::default();
        assert!(config.should_include_size(100));
        assert!(config.should_include_size(config.max_file_size));
        assert!(!config.should_include_size(config.max_file_size + 1));
    }

    #[test]
    fn scan_config_matches_exclude_pattern() {
        let config = ScanConfig::default();
        assert!(config.matches_exclude_pattern(Path::new("file.tmp")));
        assert!(config.matches_exclude_pattern(Path::new("data.cache")));
        assert!(!config.matches_exclude_pattern(Path::new("note.md")));
    }

    #[test]
    fn scan_config_include_all() {
        let config = ScanConfig::include_all();
        assert!(config.should_include_type(FileType::Binary));
        assert!(config.should_include_type(FileType::Image));
        assert!(config.should_include_type(FileType::Unknown));
    }

    #[test]
    fn scan_config_markdown_only() {
        let config = ScanConfig::markdown_only();
        assert!(config.should_include_type(FileType::Markdown));
        assert!(!config.should_include_type(FileType::Code));
        assert!(!config.should_include_type(FileType::Text));
    }

    #[test]
    fn scan_config_code_only() {
        let config = ScanConfig::code_only();
        assert!(config.should_include_type(FileType::Code));
        assert!(!config.should_include_type(FileType::Markdown));
    }

    // --- FileInfo builder tests ---

    #[test]
    fn builder_creates_valid_file_info() {
        let now = SystemTime::now();
        let info = FileInfo::builder()
            .path(PathBuf::from("/root/note.md"))
            .relative_path("note.md".to_string())
            .file_size(42)
            .modified_time(now)
            .file_type(FileType::Markdown)
            .build()
            .unwrap();

        assert_eq!(info.path(), Path::new("/root/note.md"));
        assert_eq!(info.relative_path(), "note.md");
        assert_eq!(info.file_size(), 42);
        assert_eq!(info.file_type(), FileType::Markdown);
        assert!(info.is_accessible());
        assert!(info.has_zero_hash());
    }

    #[test]
    fn builder_requires_path() {
        let result = FileInfo::builder()
            .relative_path("note.md".to_string())
            .build();
        assert!(result.is_err());
    }

    #[test]
    fn builder_requires_relative_path() {
        let result = FileInfo::builder()
            .path(PathBuf::from("/root/note.md"))
            .build();
        assert!(result.is_err());
    }

    #[test]
    fn builder_optional_fields() {
        let now = SystemTime::now();
        let info = FileInfo::builder()
            .path(PathBuf::from("/root/script.sh"))
            .relative_path("script.sh".to_string())
            .is_accessible(false)
            .created_time(now)
            .permissions(FilePermissions::Executable(0o755))
            .build()
            .unwrap();

        assert!(!info.is_accessible());
        assert!(info.created_time().is_some());
        assert_eq!(
            info.permissions(),
            Some(&FilePermissions::Executable(0o755))
        );
    }

    // --- FileInfo method tests ---

    #[test]
    fn file_info_type_checks() {
        let md = FileInfo::new(
            PathBuf::from("/note.md"),
            "note.md".to_string(),
            FileHash::zero(),
            100,
            SystemTime::now(),
            FileType::Markdown,
        );
        assert!(md.is_markdown());
        assert!(md.is_text());
        assert!(!md.is_code());
        assert!(!md.is_binary());

        let rs = FileInfo::new(
            PathBuf::from("/main.rs"),
            "main.rs".to_string(),
            FileHash::zero(),
            200,
            SystemTime::now(),
            FileType::Code,
        );
        assert!(!rs.is_markdown());
        assert!(rs.is_text());
        assert!(rs.is_code());
    }

    #[test]
    fn file_info_content_matches() {
        let hash = FileHash::new([1; 32]);
        let a = FileInfo::new(
            PathBuf::from("/a.md"),
            "a.md".to_string(),
            hash,
            100,
            SystemTime::now(),
            FileType::Markdown,
        );
        let b = FileInfo::new(
            PathBuf::from("/b.md"),
            "b.md".to_string(),
            hash,
            200,
            SystemTime::now(),
            FileType::Markdown,
        );
        assert!(a.content_matches(&b));
    }

    #[test]
    fn file_info_metadata_matches() {
        let time = SystemTime::now();
        let a = FileInfo::new(
            PathBuf::from("/a.md"),
            "a.md".to_string(),
            FileHash::zero(),
            100,
            time,
            FileType::Markdown,
        );
        let b = FileInfo::new(
            PathBuf::from("/b.md"),
            "b.md".to_string(),
            FileHash::new([1; 32]),
            100,
            time,
            FileType::Markdown,
        );
        assert!(a.metadata_matches(&b));
    }

    #[test]
    fn file_info_update_content_hash() {
        let mut info = FileInfo::with_zero_hash(
            PathBuf::from("/note.md"),
            "note.md".to_string(),
            100,
            SystemTime::now(),
            FileType::Markdown,
        );
        assert!(info.has_zero_hash());

        let new_hash = FileHash::new([42; 32]);
        info.update_content_hash(new_hash);
        assert!(!info.has_zero_hash());
        assert_eq!(info.content_hash(), new_hash);
    }

    #[test]
    fn file_info_content_hash_hex() {
        let info = FileInfo::with_zero_hash(
            PathBuf::from("/note.md"),
            "note.md".to_string(),
            100,
            SystemTime::now(),
            FileType::Markdown,
        );
        let hex = info.content_hash_hex();
        assert_eq!(hex.len(), 64);
        assert!(hex.chars().all(|c| c.is_ascii_hexdigit()));
    }

    // --- ScanResult tests ---

    #[test]
    fn scan_result_new_is_empty() {
        let result = ScanResult::new();
        assert!(result.discovered_files.is_empty());
        assert!(result.scan_errors.is_empty());
        assert_eq!(result.successful_files, 0);
        assert!(result.is_successful());
    }

    #[test]
    fn scan_result_success_rate_empty() {
        let result = ScanResult::new();
        assert_eq!(result.success_rate(), 1.0);
    }

    #[test]
    fn scan_result_success_rate_with_files() {
        let mut result = ScanResult::new();
        result.total_considered = 10;
        result.successful_files = 8;
        assert!((result.success_rate() - 0.8).abs() < f64::EPSILON);
    }

    #[test]
    fn scan_result_summary() {
        let mut result = ScanResult::new();
        result.total_considered = 10;
        result.successful_files = 8;
        result.skipped_files = 2;
        let summary = result.summary();
        assert_eq!(summary.total_files, 10);
        assert_eq!(summary.successful_files, 8);
        assert_eq!(summary.skipped_files, 2);
    }
}
