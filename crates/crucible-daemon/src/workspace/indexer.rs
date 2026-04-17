//! Pure filesystem indexers for session setup.
//!
//! Two functions moved verbatim from `crucible-cli/src/chat/session.rs` as part
//! of Task 1.2c of the "daemon owns setup" refactor. They do only filesystem
//! I/O (no daemon state), so `session.create`'s setup task can invoke them
//! inside `tokio::task::spawn_blocking`.
//!
//! The CLI copies remain until Task 1.3 deletes them.

use std::path::Path;
use std::process::Command;

use walkdir::WalkDir;

/// List files under `root`, respecting `.gitignore` when possible.
///
/// Prefers `git ls-files` (cached + untracked, exclude-standard). Falls back
/// to a `walkdir` traversal that skips dotfiles. Returns at most `MAX_ENTRIES`
/// paths, sorted and de-duplicated, with forward-slash separators.
pub fn index_workspace_files(root: &Path) -> Vec<String> {
    const MAX_ENTRIES: usize = 2000;
    // Try git ls-files to respect gitignore
    if let Ok(output) = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["ls-files", "--cached", "--others", "--exclude-standard"])
        .output()
    {
        if output.status.success() {
            if let Ok(text) = String::from_utf8(output.stdout) {
                let mut files: Vec<String> = text
                    .lines()
                    .filter(|l| !l.trim().is_empty())
                    .take(MAX_ENTRIES)
                    .map(|s| s.replace('\\', "/"))
                    .collect();
                files.sort();
                files.dedup();
                return files;
            }
        }
    }

    let mut files = Vec::new();
    for entry in WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| !is_hidden_entry(e))
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        if files.len() >= MAX_ENTRIES {
            break;
        }
        if let Ok(rel) = entry.path().strip_prefix(root) {
            if let Some(path_str) = rel.to_str() {
                files.push(path_str.replace('\\', "/"));
            }
        }
    }
    files.sort();
    files.dedup();
    files
}

/// List markdown notes under `kiln_root`, each prefixed with `note:`.
///
/// Skips dotfiles and non-`.md` extensions. Returns at most `MAX_ENTRIES`
/// paths, sorted and de-duplicated, with forward-slash separators.
pub fn index_kiln_notes(kiln_root: &Path) -> Vec<String> {
    const MAX_ENTRIES: usize = 2000;
    if !kiln_root.exists() {
        return Vec::new();
    }
    let mut notes = Vec::new();
    for entry in WalkDir::new(kiln_root)
        .into_iter()
        .filter_entry(|e| !is_hidden_entry(e))
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        if let Some(ext) = entry.path().extension() {
            if ext != "md" {
                continue;
            }
        } else {
            continue;
        }
        if notes.len() >= MAX_ENTRIES {
            break;
        }
        if let Ok(rel) = entry.path().strip_prefix(kiln_root) {
            if let Some(path_str) = rel.to_str() {
                notes.push(format!("note:{}", path_str.replace('\\', "/")));
            }
        }
    }
    notes.sort();
    notes.dedup();
    notes
}

fn is_hidden_entry(entry: &walkdir::DirEntry) -> bool {
    // Don't filter the root directory (depth 0)
    // Only check non-root entries for hidden names
    entry.depth() > 0 && entry.file_name().to_string_lossy().starts_with('.')
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Create a tempdir that is NOT a git repo so `git ls-files` fails and we
    /// exercise the walkdir fallback path deterministically.
    fn non_git_tempdir() -> TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        // Ensure no ambient .git picks it up: the tempdir root is fresh, so
        // `git -C <dir> ls-files` will fail with "not a git repository".
        dir
    }

    #[test]
    fn index_workspace_files_lists_regular_files_and_skips_hidden() {
        let dir = non_git_tempdir();
        let root = dir.path();
        fs::write(root.join("README.md"), "hello").unwrap();
        fs::write(root.join("main.rs"), "fn main() {}").unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/lib.rs"), "// lib").unwrap();
        // Hidden file/dir — should be skipped by the walkdir fallback.
        fs::write(root.join(".secret"), "nope").unwrap();
        fs::create_dir_all(root.join(".hidden")).unwrap();
        fs::write(root.join(".hidden/inside"), "nope").unwrap();

        let files = index_workspace_files(root);

        // Either git ls-files worked (unlikely in a non-git tempdir) or the
        // walkdir fallback ran. Either way we must see the three tracked-style
        // files and no dotfiles.
        assert!(files.contains(&"README.md".to_string()), "got {files:?}");
        assert!(files.contains(&"main.rs".to_string()), "got {files:?}");
        assert!(files.contains(&"src/lib.rs".to_string()), "got {files:?}");
        assert!(
            !files.iter().any(|f| f.starts_with('.') || f.contains("/.")),
            "dotfiles leaked through: {files:?}"
        );
        // Sorted + deduped.
        let mut sorted = files.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(files, sorted);
    }

    #[test]
    fn index_kiln_notes_returns_only_markdown_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("a.md"), "# a").unwrap();
        fs::write(root.join("b.md"), "# b").unwrap();
        fs::write(root.join("not-a-note.txt"), "plaintext").unwrap();
        fs::write(root.join("no-extension"), "bare").unwrap();
        fs::create_dir_all(root.join("sub")).unwrap();
        fs::write(root.join("sub/c.md"), "# c").unwrap();

        let notes = index_kiln_notes(root);

        assert_eq!(
            notes,
            vec![
                "note:a.md".to_string(),
                "note:b.md".to_string(),
                "note:sub/c.md".to_string(),
            ]
        );
    }

    #[test]
    fn index_kiln_notes_returns_empty_for_missing_root() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("does-not-exist");
        assert!(index_kiln_notes(&missing).is_empty());
    }

    #[test]
    fn index_workspace_files_caps_at_max_entries() {
        let dir = non_git_tempdir();
        let root = dir.path();
        // Write slightly more than MAX_ENTRIES files so the cap engages on
        // the walkdir fallback. We cannot control git-vs-walkdir, but either
        // branch applies the same 2000 cap.
        for i in 0..2005 {
            fs::write(root.join(format!("f{i:05}.txt")), "x").unwrap();
        }

        let files = index_workspace_files(root);
        assert!(
            files.len() <= 2000,
            "indexer returned {} entries; expected <= 2000",
            files.len()
        );
    }

    #[test]
    fn index_kiln_notes_caps_at_max_entries() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for i in 0..2005 {
            fs::write(root.join(format!("n{i:05}.md")), "x").unwrap();
        }

        let notes = index_kiln_notes(root);
        assert_eq!(notes.len(), 2000);
    }
}
