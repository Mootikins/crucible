//! Kiln path validation for `cru init`.
//!
//! `cru init` is the only caller (`commands/init.rs:54`); the `cru chat`
//! first-run auto-create path does not validate at all, so treat "shared
//! validation layer" as aspirational.
//!
//! Severity ranks how bad a location is, not what init does about it. Only
//! [`ValidationSeverity::HardBlock`] changes control flow — the "strong warning
//! means default deny, ask for confirmation" gate the tier names imply was
//! never built, so warnings print and init proceeds. Findings are also
//! informational only in the exit-code sense: a git repo or a temp directory
//! must stay exit 0, because scripts init into both.

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
    /// All findings, ordered by severity (hard blocks first).
    pub findings: Vec<ValidationFinding>,
    /// Number of markdown notes found if the path exists (0 if it doesn't).
    pub markdown_file_count: usize,
}

impl ValidationResult {
    /// Returns true if any finding is a hard block.
    pub fn is_blocked(&self) -> bool {
        self.findings
            .iter()
            .any(|f| f.severity == ValidationSeverity::HardBlock)
    }

    /// Returns findings of a specific severity.
    pub fn findings_by_severity(&self, severity: ValidationSeverity) -> Vec<&ValidationFinding> {
        self.findings
            .iter()
            .filter(|f| f.severity == severity)
            .collect()
    }
}

/// Expand tilde in a path string to the user's home directory.
///
/// Delegates to the daemon's expander so the CLI and the kiln registry agree
/// on what `~/vault` means; a second implementation is a second answer.
pub fn expand_tilde(path: &str) -> PathBuf {
    crucible_daemon::project_manager::resolve_registration_root(path, dirs::home_dir().as_deref())
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

    // Reported as a count rather than a finding: `cru init` prints it from
    // `markdown_file_count` in its success block, which is the better moment —
    // after the kiln exists.
    let markdown_file_count = if path_exists {
        count_markdown_files(&resolved)
    } else {
        0
    };

    // Sort by severity (hard blocks first)
    findings.sort_by_key(|f| match f.severity {
        ValidationSeverity::HardBlock => 0,
        ValidationSeverity::StrongWarning => 1,
        ValidationSeverity::MildWarning => 2,
    });

    ValidationResult {
        findings,
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

/// Returns true if the path is a well-known temp root directory (/tmp, /var/tmp, TMPDIR).
/// A `.crucible` dir at these roots is always a daemon artifact, never an intentional kiln.
/// Subdirectories are NOT excluded — tests and users may create kilns there.
fn is_temp_root(path: &Path) -> bool {
    let path_str = path.to_string_lossy();
    path_str == "/tmp"
        || path_str == "/var/tmp"
        || std::env::var("TMPDIR")
            .ok()
            .map(|t| path_str == *t)
            .unwrap_or(false)
}

/// Walk up from `path` looking for `.crucible/` in ancestors.
/// Returns the directory containing `.crucible/` if found.
/// Skips temp root directories (/tmp, /var/tmp, TMPDIR) since `.crucible` there is
/// always a daemon artifact, never an intentional kiln.
fn find_parent_kiln(path: &Path) -> Option<PathBuf> {
    let mut current = path.parent();
    while let Some(dir) = current {
        if is_temp_root(dir) {
            current = dir.parent();
            continue;
        }
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

/// Count markdown notes in a directory (non-recursive, top level only to keep
/// it fast).
///
/// Uses the canonical predicate rather than an inline `ext == "md"` so this
/// agrees with what the indexer will actually pick up — `.markdown` and
/// uppercase extensions included.
fn count_markdown_files(path: &Path) -> usize {
    std::fs::read_dir(path)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| crucible_core::kiln::is_note_file(&e.path()))
                .count()
        })
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// The predicate `cru init` acts on: at least one default-deny warning.
    fn has_strong_warning(result: &ValidationResult) -> bool {
        !result
            .findings_by_severity(ValidationSeverity::StrongWarning)
            .is_empty()
    }

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
        assert!(has_strong_warning(&result));
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
        assert!(has_strong_warning(&result));
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
        assert!(has_strong_warning(&result));
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
    }

    /// The count promises what the indexer will pick up, so it goes through the
    /// canonical predicate: `.markdown` and an uppercase `.MD` are notes, and
    /// counting only lowercase `.md` under-reported a synced vault.
    #[test]
    fn test_validate_markdown_count_matches_the_indexable_extensions() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("lower.md"), "# a").unwrap();
        std::fs::write(tmp.path().join("Upper.MD"), "# b").unwrap();
        std::fs::write(tmp.path().join("long.markdown"), "# c").unwrap();
        std::fs::write(tmp.path().join("board.canvas"), "{}").unwrap();
        std::fs::write(tmp.path().join("image.png"), "").unwrap();
        let result = validate_kiln_path(tmp.path());
        assert_eq!(result.markdown_file_count, 3);
    }

    /// The cloud-sync message alone only says where you are; the suggestion is
    /// the whole actionable content, and `cru init` prints it under `Note:`.
    #[test]
    fn test_validate_cloud_sync_folder_carries_its_suggestion() {
        let tmp = TempDir::new().unwrap();
        let kiln_path = tmp.path().join("Dropbox").join("notes");
        std::fs::create_dir_all(&kiln_path).unwrap();
        let result = validate_kiln_path(&kiln_path);

        let mild = result.findings_by_severity(ValidationSeverity::MildWarning);
        let cloud: Vec<_> = mild
            .iter()
            .filter(|f| f.message.contains("cloud sync folder"))
            .collect();
        assert_eq!(cloud.len(), 1, "expected one cloud-sync finding: {mild:?}");
        assert_eq!(
            cloud[0].suggestion.as_deref(),
            Some("Markdown notes sync fine, but the database may have conflicts.")
        );
    }

    #[test]
    fn test_validate_home_directory() {
        if let Some(home) = dirs::home_dir() {
            let result = validate_kiln_path(&home);
            assert!(has_strong_warning(&result));
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
    }
}
