//! Kiln path validation layer.
//!
//! Shared between `cru init` and the `cru chat` first-run wizard.
//! Validates proposed kiln paths with tiered warnings:
//! - Hard blocks: prevent init entirely
//! - Strong warnings: default deny, user must confirm
//! - Mild warnings: default allow, inform user
//! - Info: no confirmation needed

use std::path::{Path, PathBuf};

/// Severity of a validation result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationSeverity {
    /// Cannot create kiln here. Init is blocked.
    HardBlock,
    /// Strongly discouraged. Default answer is No — user must explicitly confirm.
    StrongWarning,
    /// Mildly discouraged. Default answer is Yes — just inform the user.
    MildWarning,
    /// Informational. No confirmation needed.
    Info,
}

/// A single validation finding.
#[derive(Debug, Clone)]
pub struct ValidationFinding {
    pub severity: ValidationSeverity,
    pub message: String,
    /// Optional suggestion for what the user should do instead.
    pub suggestion: Option<String>,
}

/// Overall result of path validation.
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// Path that was validated (canonicalized if possible).
    pub path: PathBuf,
    /// All findings, ordered by severity (hard blocks first).
    pub findings: Vec<ValidationFinding>,
    /// Whether the path exists on disk.
    pub path_exists: bool,
    /// Whether the path already has a `.crucible/` directory.
    pub is_existing_kiln: bool,
    /// Number of `.md` files found if the path exists (0 if doesn't exist).
    pub markdown_file_count: usize,
}

impl ValidationResult {
    /// Returns true if any finding is a hard block.
    pub fn is_blocked(&self) -> bool {
        self.findings
            .iter()
            .any(|f| f.severity == ValidationSeverity::HardBlock)
    }

    /// Returns true if there are strong warnings that need user confirmation.
    pub fn has_strong_warnings(&self) -> bool {
        self.findings
            .iter()
            .any(|f| f.severity == ValidationSeverity::StrongWarning)
    }

    /// Returns true if there are mild warnings to inform the user about.
    pub fn has_mild_warnings(&self) -> bool {
        self.findings
            .iter()
            .any(|f| f.severity == ValidationSeverity::MildWarning)
    }

    /// Returns true if there are info findings.
    pub fn has_info(&self) -> bool {
        self.findings
            .iter()
            .any(|f| f.severity == ValidationSeverity::Info)
    }

    /// Returns findings of a specific severity.
    pub fn findings_by_severity(&self, severity: ValidationSeverity) -> Vec<&ValidationFinding> {
        self.findings
            .iter()
            .filter(|f| f.severity == severity)
            .collect()
    }

    /// Returns true if validation passed with no blocks or warnings requiring confirmation.
    pub fn is_clean(&self) -> bool {
        !self.is_blocked() && !self.has_strong_warnings()
    }
}

/// Expand tilde in a path string to the user's home directory.
pub fn expand_tilde(path: &str) -> PathBuf {
    if path.starts_with("~/") || path == "~" {
        if let Some(home) = dirs::home_dir() {
            if path == "~" {
                return home;
            }
            return home.join(&path[2..]);
        }
    }
    PathBuf::from(path)
}

/// Validate a proposed kiln path.
///
/// Checks for bad locations, existing content, and other potential issues.
/// Returns a `ValidationResult` with all findings sorted by severity.
pub fn validate_kiln_path(path: &Path) -> ValidationResult {
    let mut findings = Vec::new();

    // Resolve the path (expand symlinks if it exists, otherwise use as-is)
    let resolved = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let path_exists = resolved.exists();
    let is_existing_kiln = resolved.join(".crucible").is_dir();

    // --- Hard blocks ---

    // Filesystem root
    if is_filesystem_root(&resolved) {
        findings.push(ValidationFinding {
            severity: ValidationSeverity::HardBlock,
            message: "Cannot create kiln at filesystem root.".to_string(),
            suggestion: Some("Choose a subdirectory instead.".to_string()),
        });
    }

    // Nested kiln (inside an existing kiln)
    if let Some(parent_kiln) = find_parent_kiln(&resolved) {
        // Only block if it's a DIFFERENT kiln (not re-init of the same one)
        if parent_kiln != resolved {
            findings.push(ValidationFinding {
                severity: ValidationSeverity::HardBlock,
                message: format!(
                    "Cannot create kiln inside another kiln at {}.",
                    parent_kiln.display()
                ),
                suggestion: Some(format!(
                    "Use the existing kiln at {} instead.",
                    parent_kiln.display()
                )),
            });
        }
    }

    // --- Strong warnings (default No) ---

    // Inside a git repository
    if find_ancestor_file(&resolved, ".git").is_some() {
        findings.push(ValidationFinding {
            severity: ValidationSeverity::StrongWarning,
            message: "This is inside a git repository.".to_string(),
            suggestion: Some(
                "Kilns work best as standalone directories for notes, not inside source code projects."
                    .to_string(),
            ),
        });
    }

    // Contains build system markers (source code project)
    let build_markers = [
        "Cargo.toml",
        "package.json",
        "go.mod",
        "pyproject.toml",
        "Makefile",
        "CMakeLists.txt",
        "pom.xml",
        "build.gradle",
    ];
    if let Some(marker) = find_ancestor_files(&resolved, &build_markers) {
        // Only warn if NOT already warned about git
        let already_warned_git = findings
            .iter()
            .any(|f| f.message.contains("git repository"));
        if !already_warned_git {
            findings.push(ValidationFinding {
                severity: ValidationSeverity::StrongWarning,
                message: format!("This looks like a source code project (found {}).", marker),
                suggestion: Some(
                    "Kilns work best as standalone directories for notes.".to_string(),
                ),
            });
        }
    }

    // Home directory root
    if is_home_directory(&resolved) {
        findings.push(ValidationFinding {
            severity: ValidationSeverity::StrongWarning,
            message: "This is your home directory.".to_string(),
            suggestion: Some(
                "Most users prefer a subdirectory like ~/crucible or ~/notes.".to_string(),
            ),
        });
    }

    // Temp directory
    if is_temp_directory(&resolved) {
        findings.push(ValidationFinding {
            severity: ValidationSeverity::StrongWarning,
            message: "This is a temporary directory.".to_string(),
            suggestion: Some("Files here may be deleted on reboot.".to_string()),
        });
    }

    // --- Mild warnings (default Yes) ---

    // Cloud sync folder
    if is_cloud_sync_folder(&resolved) {
        findings.push(ValidationFinding {
            severity: ValidationSeverity::MildWarning,
            message: "This is inside a cloud sync folder.".to_string(),
            suggestion: Some(
                "Markdown notes sync fine, but the database may have conflicts.".to_string(),
            ),
        });
    }

    // --- Info (no confirmation needed) ---

    // Re-init of existing kiln
    if is_existing_kiln {
        findings.push(ValidationFinding {
            severity: ValidationSeverity::Info,
            message: "A kiln already exists here.".to_string(),
            suggestion: Some("Using the existing kiln. No changes made.".to_string()),
        });
    }

    // Non-existent path
    if !path_exists {
        findings.push(ValidationFinding {
            severity: ValidationSeverity::Info,
            message: "This directory doesn't exist yet.".to_string(),
            suggestion: None,
        });
    }

    // Count markdown files if path exists
    let markdown_file_count = if path_exists {
        count_markdown_files(&resolved)
    } else {
        0
    };

    if markdown_file_count > 0 {
        findings.push(ValidationFinding {
            severity: ValidationSeverity::Info,
            message: format!(
                "Found {} markdown file(s). Crucible will index these but won't modify them.",
                markdown_file_count
            ),
            suggestion: None,
        });
    }

    // Sort by severity (hard blocks first)
    findings.sort_by_key(|f| match f.severity {
        ValidationSeverity::HardBlock => 0,
        ValidationSeverity::StrongWarning => 1,
        ValidationSeverity::MildWarning => 2,
        ValidationSeverity::Info => 3,
    });

    ValidationResult {
        path: resolved,
        findings,
        path_exists,
        is_existing_kiln,
        markdown_file_count,
    }
}

// --- Helper functions ---

fn is_filesystem_root(path: &Path) -> bool {
    path == Path::new("/")
}

fn is_home_directory(path: &Path) -> bool {
    dirs::home_dir().map(|home| path == home).unwrap_or(false)
}

fn is_temp_directory(path: &Path) -> bool {
    let path_str = path.to_string_lossy();
    path_str.starts_with("/tmp")
        || path_str.starts_with("/var/tmp")
        || std::env::var("TMPDIR")
            .ok()
            .map(|t| path_str.starts_with(&t))
            .unwrap_or(false)
}

fn is_cloud_sync_folder(path: &Path) -> bool {
    let path_str = path.to_string_lossy().to_lowercase();
    path_str.contains("dropbox")
        || path_str.contains("onedrive")
        || path_str.contains("google drive")
        || path_str.contains("icloud")
}

/// Walk up from `path` looking for `.crucible/` in ancestors.
/// Returns the directory containing `.crucible/` if found.
fn find_parent_kiln(path: &Path) -> Option<PathBuf> {
    let mut current = path.parent();
    while let Some(dir) = current {
        if dir.join(".crucible").is_dir() {
            return Some(dir.to_path_buf());
        }
        current = dir.parent();
    }
    None
}

/// Walk up from `path` looking for a specific file/directory name in ancestors.
fn find_ancestor_file(path: &Path, name: &str) -> Option<PathBuf> {
    let mut current = Some(path);
    while let Some(dir) = current {
        let candidate = dir.join(name);
        if candidate.exists() {
            return Some(candidate);
        }
        current = dir.parent();
    }
    None
}

/// Walk up from `path` looking for any of the given file names.
/// Returns the first match found.
fn find_ancestor_files(path: &Path, names: &[&str]) -> Option<String> {
    let mut current = Some(path);
    while let Some(dir) = current {
        for name in names {
            if dir.join(name).exists() {
                return Some((*name).to_string());
            }
        }
        current = dir.parent();
    }
    None
}

/// Count `.md` files in a directory (non-recursive, top level only to keep it fast).
fn count_markdown_files(path: &Path) -> usize {
    std::fs::read_dir(path)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().map(|ext| ext == "md").unwrap_or(false))
                .count()
        })
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_expand_tilde() {
        let expanded = expand_tilde("~/notes");
        // Should not start with ~ anymore
        assert!(!expanded.to_string_lossy().starts_with('~'));
        // Should end with /notes
        assert!(expanded.to_string_lossy().ends_with("notes"));
    }

    #[test]
    fn test_expand_tilde_no_tilde() {
        let expanded = expand_tilde("/absolute/path");
        assert_eq!(expanded, PathBuf::from("/absolute/path"));
    }

    #[test]
    fn test_validate_filesystem_root() {
        let result = validate_kiln_path(Path::new("/"));
        assert!(result.is_blocked());
        assert!(result.findings[0].message.contains("filesystem root"));
    }

    #[test]
    fn test_validate_temp_directory() {
        let result = validate_kiln_path(Path::new("/tmp/test-kiln"));
        assert!(result.has_strong_warnings());
        assert!(result
            .findings
            .iter()
            .any(|f| f.message.contains("temporary directory")));
    }

    #[test]
    fn test_validate_clean_path() {
        let tmp = TempDir::new().unwrap();
        let kiln_path = tmp.path().join("my-notes");
        let result = validate_kiln_path(&kiln_path);
        assert!(!result.is_blocked());
        assert!(!result.path_exists);
        let non_temp_strong: Vec<_> = result
            .findings_by_severity(ValidationSeverity::StrongWarning)
            .into_iter()
            .filter(|f| !f.message.contains("temporary directory"))
            .collect();
        assert!(
            non_temp_strong.is_empty(),
            "unexpected strong warnings: {:?}",
            non_temp_strong
        );
    }

    #[test]
    fn test_validate_existing_kiln() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join(".crucible")).unwrap();
        let result = validate_kiln_path(tmp.path());
        assert!(result.is_existing_kiln);
        assert!(result
            .findings
            .iter()
            .any(|f| f.message.contains("already exists")));
    }

    #[test]
    fn test_validate_nested_kiln_blocked() {
        let tmp = TempDir::new().unwrap();
        // Create a parent kiln
        std::fs::create_dir_all(tmp.path().join(".crucible")).unwrap();
        // Try to create a nested kiln
        let nested = tmp.path().join("sub");
        std::fs::create_dir_all(&nested).unwrap();
        let result = validate_kiln_path(&nested);
        assert!(result.is_blocked());
        assert!(result.findings[0].message.contains("inside another kiln"));
    }

    #[test]
    fn test_validate_git_repo_warning() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join(".git")).unwrap();
        let result = validate_kiln_path(tmp.path());
        assert!(result.has_strong_warnings());
        assert!(result
            .findings
            .iter()
            .any(|f| f.message.contains("git repository")));
    }

    #[test]
    fn test_validate_source_project_warning() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("Cargo.toml"), "[package]").unwrap();
        let result = validate_kiln_path(tmp.path());
        assert!(result.has_strong_warnings());
        assert!(result
            .findings
            .iter()
            .any(|f| f.message.contains("source code project")));
    }

    #[test]
    fn test_validate_git_repo_suppresses_build_marker_warning() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join(".git")).unwrap();
        std::fs::write(tmp.path().join("Cargo.toml"), "[package]").unwrap();
        let result = validate_kiln_path(tmp.path());
        let strong = result.findings_by_severity(ValidationSeverity::StrongWarning);
        // Expect: git + temp warnings. Build marker should be suppressed by git.
        let git_warnings: Vec<_> = strong
            .iter()
            .filter(|f| f.message.contains("git repository"))
            .collect();
        let build_warnings: Vec<_> = strong
            .iter()
            .filter(|f| f.message.contains("source code project"))
            .collect();
        assert_eq!(git_warnings.len(), 1, "expected exactly one git warning");
        assert_eq!(
            build_warnings.len(),
            0,
            "build marker should be suppressed by git warning"
        );
    }

    #[test]
    fn test_validate_markdown_count() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("note1.md"), "# Hello").unwrap();
        std::fs::write(tmp.path().join("note2.md"), "# World").unwrap();
        std::fs::write(tmp.path().join("readme.txt"), "not md").unwrap();
        let result = validate_kiln_path(tmp.path());
        assert_eq!(result.markdown_file_count, 2);
        assert!(result
            .findings
            .iter()
            .any(|f| f.message.contains("2 markdown file(s)")));
    }

    #[test]
    fn test_validate_home_directory() {
        if let Some(home) = dirs::home_dir() {
            let result = validate_kiln_path(&home);
            assert!(result.has_strong_warnings());
            assert!(result
                .findings
                .iter()
                .any(|f| f.message.contains("home directory")));
        }
    }

    #[test]
    fn test_reinit_existing_kiln_not_blocked() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join(".crucible")).unwrap();
        let result = validate_kiln_path(tmp.path());
        // Re-init should NOT be blocked — it's idempotent
        assert!(!result.is_blocked());
        assert!(result.is_existing_kiln);
    }

    #[test]
    fn test_validation_result_methods() {
        let tmp = TempDir::new().unwrap();
        let kiln_path = tmp.path().join("notes");
        let result = validate_kiln_path(&kiln_path);
        assert!(!result.is_blocked());
        assert!(result.has_info());
        assert!(!result.path_exists);
    }
}
