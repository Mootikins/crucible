//! Tests for the filesystem RPC surface.
//!
//! Split from `mod.rs` for the 1000-line module budget; the plain-text kind
//! pushed the combined file over it.

use super::*;
use std::fs;

/// Build a hermetic `ProjectManager` (temp `projects.json`, never the
/// developer's real `~/.crucible`) with `project_dir` registered. Returns
/// the manager and the canonical registered root.
fn registered_pm(storage_root: &Path, project_dir: &Path) -> (Arc<ProjectManager>, PathBuf) {
    let pm = Arc::new(ProjectManager::new(storage_root.join("projects.json")));
    let project = pm.register(project_dir).expect("register project");
    (pm, project.path)
}

#[test]
fn lists_nested_dirs_and_files_dirs_first() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = tempfile::TempDir::new().unwrap();
    let proj = tmp.path();
    fs::create_dir(proj.join("src")).unwrap();
    fs::create_dir(proj.join("assets")).unwrap();
    fs::write(proj.join("README.md"), "hi").unwrap();
    fs::write(proj.join("Cargo.toml"), "[package]").unwrap();
    fs::write(proj.join("src").join("main.rs"), "fn main() {}").unwrap();

    let (pm, root) = registered_pm(store.path(), proj);

    // Top level.
    let entries = list_dir(&pm, &root, "", false, false).unwrap().entries;
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    // Dirs first (case-insensitive name), then files (case-insensitive).
    assert_eq!(names, vec!["assets", "src", "Cargo.toml", "README.md"]);
    assert!(entries.iter().find(|e| e.name == "src").unwrap().is_dir);
    assert!(
        !entries
            .iter()
            .find(|e| e.name == "README.md")
            .unwrap()
            .is_dir
    );
    // status is always null in Phase 1.
    assert!(entries.iter().all(|e| e.status.is_none()));

    // One level down via rel_path.
    let sub = list_dir(&pm, &root, "src", false, false).unwrap().entries;
    assert_eq!(sub.len(), 1);
    assert_eq!(sub[0].name, "main.rs");
    assert_eq!(sub[0].rel_path, "src/main.rs");
}

#[test]
fn gitignored_file_hidden_by_default_shown_with_show_ignored() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = tempfile::TempDir::new().unwrap();
    let proj = tmp.path();
    fs::write(proj.join(".gitignore"), "ignored.txt\n").unwrap();
    fs::write(proj.join("ignored.txt"), "secret").unwrap();
    fs::write(proj.join("kept.txt"), "ok").unwrap();

    let (pm, root) = registered_pm(store.path(), proj);

    let hidden = list_dir(&pm, &root, "", false, false).unwrap().entries;
    let hidden_names: Vec<&str> = hidden.iter().map(|e| e.name.as_str()).collect();
    assert!(hidden_names.contains(&"kept.txt"));
    assert!(!hidden_names.contains(&"ignored.txt"));
    // `.gitignore` is itself a dotfile → also hidden by default.
    assert!(!hidden_names.contains(&".gitignore"));

    // show_ignored alone reveals gitignored entries but NOT dotfiles.
    let shown = list_dir(&pm, &root, "", true, false).unwrap().entries;
    let shown_names: Vec<&str> = shown.iter().map(|e| e.name.as_str()).collect();
    assert!(shown_names.contains(&"ignored.txt"));
    assert!(!shown_names.contains(&".gitignore"));

    // Both axes on: dotfiles too.
    let all = list_dir(&pm, &root, "", true, true).unwrap().entries;
    assert!(all.iter().any(|e| e.name == ".gitignore"));
}

#[test]
fn dotfile_hidden_by_default() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = tempfile::TempDir::new().unwrap();
    let proj = tmp.path();
    // Not gitignored — hidden purely because it is a dotfile.
    fs::write(proj.join(".env"), "SECRET=1").unwrap();
    fs::write(proj.join("visible.txt"), "ok").unwrap();

    let (pm, root) = registered_pm(store.path(), proj);

    let hidden = list_dir(&pm, &root, "", false, false).unwrap().entries;
    assert!(hidden.iter().all(|e| e.name != ".env"));
    assert!(hidden.iter().any(|e| e.name == "visible.txt"));

    // show_hidden alone reveals the dotfile (no gitignore involvement).
    let shown = list_dir(&pm, &root, "", false, true).unwrap().entries;
    assert!(shown.iter().any(|e| e.name == ".env"));
}

#[test]
fn git_dir_never_listed_even_with_both_flags() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = tempfile::TempDir::new().unwrap();
    let proj = tmp.path();
    fs::create_dir_all(proj.join(".git")).unwrap();
    fs::write(proj.join(".git").join("HEAD"), "ref: x").unwrap();
    fs::write(proj.join("visible.txt"), "ok").unwrap();

    let (pm, root) = registered_pm(store.path(), proj);
    let all = list_dir(&pm, &root, "", true, true).unwrap().entries;
    assert!(all.iter().all(|e| e.name != ".git"));
    assert!(all.iter().any(|e| e.name == "visible.txt"));
}

#[test]
fn symlink_escaping_root_is_excluded() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = tempfile::TempDir::new().unwrap();
    let outside = tempfile::TempDir::new().unwrap();
    let proj = tmp.path();
    let secret = outside.path().join("secret.txt");
    fs::write(&secret, "top secret").unwrap();
    fs::write(proj.join("inside.txt"), "ok").unwrap();

    #[cfg(unix)]
    std::os::unix::fs::symlink(&secret, proj.join("escape.txt")).unwrap();

    let (pm, root) = registered_pm(store.path(), proj);
    let entries = list_dir(&pm, &root, "", false, false).unwrap().entries;
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"inside.txt"));
    // The escaping symlink must never surface.
    assert!(!names.contains(&"escape.txt"));
}

#[test]
fn intra_project_symlink_is_listed() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = tempfile::TempDir::new().unwrap();
    let proj = tmp.path();
    fs::write(proj.join("target.txt"), "data").unwrap();

    #[cfg(unix)]
    std::os::unix::fs::symlink(proj.join("target.txt"), proj.join("link.txt")).unwrap();

    let (pm, root) = registered_pm(store.path(), proj);
    let entries = list_dir(&pm, &root, "", false, false).unwrap().entries;
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    #[cfg(unix)]
    assert!(names.contains(&"link.txt"));
    assert!(names.contains(&"target.txt"));
}

#[test]
fn rejects_parent_traversal() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = tempfile::TempDir::new().unwrap();
    let proj = tmp.path();
    fs::create_dir(proj.join("src")).unwrap();
    let (pm, root) = registered_pm(store.path(), proj);

    assert!(matches!(
        list_dir(&pm, &root, "../", false, false),
        Err(FsListError::Escape)
    ));
    assert!(matches!(
        list_dir(&pm, &root, "src/../..", false, false),
        Err(FsListError::Escape)
    ));
}

#[test]
fn rejects_absolute_rel_path() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = tempfile::TempDir::new().unwrap();
    let proj = tmp.path();
    let (pm, root) = registered_pm(store.path(), proj);

    assert!(matches!(
        list_dir(&pm, &root, "/etc", false, false),
        Err(FsListError::Escape)
    ));
}

#[test]
fn unregistered_root_is_rejected() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = tempfile::TempDir::new().unwrap();
    // A ProjectManager with nothing registered.
    let pm = Arc::new(ProjectManager::new(store.path().join("projects.json")));
    assert!(matches!(
        list_dir(&pm, tmp.path(), "", false, false),
        Err(FsListError::NotRegistered)
    ));
}

#[test]
fn not_a_directory_is_rejected() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = tempfile::TempDir::new().unwrap();
    let proj = tmp.path();
    fs::write(proj.join("file.txt"), "x").unwrap();
    let (pm, root) = registered_pm(store.path(), proj);

    assert!(matches!(
        list_dir(&pm, &root, "file.txt", false, false),
        Err(FsListError::NotADir)
    ));
}

// ── move_within ────────────────────────────────────────────────────

/// Canonicalized tempdir root (macOS /var → /private/var etc.); the
/// handler always passes a canonical base, so tests must too.
fn canon_root(tmp: &tempfile::TempDir) -> PathBuf {
    tmp.path().canonicalize().unwrap()
}

#[test]
fn move_file_into_subdir() {
    let tmp = tempfile::TempDir::new().unwrap();
    let base = canon_root(&tmp);
    fs::create_dir(base.join("notes")).unwrap();
    fs::write(base.join("a.md"), "x").unwrap();

    move_within(&base, "a.md", "notes/a.md").unwrap();
    assert!(!base.join("a.md").exists());
    assert_eq!(fs::read_to_string(base.join("notes/a.md")).unwrap(), "x");
}

#[test]
fn move_renames_a_directory_with_contents() {
    let tmp = tempfile::TempDir::new().unwrap();
    let base = canon_root(&tmp);
    fs::create_dir_all(base.join("src/deep")).unwrap();
    fs::write(base.join("src/deep/f.rs"), "fn").unwrap();
    fs::create_dir(base.join("lib")).unwrap();

    move_within(&base, "src", "lib/src").unwrap();
    assert!(base.join("lib/src/deep/f.rs").exists());
    assert!(!base.join("src").exists());
}

#[test]
fn move_rejects_escapes_in_either_path() {
    let tmp = tempfile::TempDir::new().unwrap();
    let base = canon_root(&tmp);
    fs::write(base.join("a.md"), "x").unwrap();

    for (from, to) in [
        ("../a.md", "b.md"),
        ("a.md", "../b.md"),
        ("/etc/passwd", "b.md"),
        ("a.md", "/tmp/b.md"),
        ("", "b.md"),
        ("a.md", ""),
    ] {
        assert!(
            matches!(move_within(&base, from, to), Err(FsMoveError::Escape)),
            "expected Escape for ({from:?}, {to:?})"
        );
    }
    assert!(base.join("a.md").exists());
}

#[test]
fn move_rejects_overwrite_and_self_move() {
    let tmp = tempfile::TempDir::new().unwrap();
    let base = canon_root(&tmp);
    fs::write(base.join("a.md"), "a").unwrap();
    fs::write(base.join("b.md"), "b").unwrap();

    assert!(matches!(
        move_within(&base, "a.md", "b.md"),
        Err(FsMoveError::DestinationExists)
    ));
    // A no-op move (same path) is also an existing destination.
    assert!(matches!(
        move_within(&base, "a.md", "a.md"),
        Err(FsMoveError::DestinationExists)
    ));
    assert_eq!(fs::read_to_string(base.join("b.md")).unwrap(), "b");
}

#[test]
fn move_rejects_dir_into_itself() {
    let tmp = tempfile::TempDir::new().unwrap();
    let base = canon_root(&tmp);
    fs::create_dir_all(base.join("dir/sub")).unwrap();

    assert!(matches!(
        move_within(&base, "dir", "dir/sub/dir"),
        Err(FsMoveError::IntoSelf)
    ));
}

#[test]
fn move_distinguishes_missing_source_and_bad_destination() {
    let tmp = tempfile::TempDir::new().unwrap();
    let base = canon_root(&tmp);
    fs::write(base.join("a.md"), "x").unwrap();

    assert!(matches!(
        move_within(&base, "ghost.md", "a2.md"),
        Err(FsMoveError::SourceMissing)
    ));
    assert!(matches!(
        move_within(&base, "a.md", "no-such-dir/a.md"),
        Err(FsMoveError::BadDestination)
    ));
}

#[cfg(unix)]
#[test]
fn move_rejects_symlink_parent_escape() {
    let tmp = tempfile::TempDir::new().unwrap();
    let outside = tempfile::TempDir::new().unwrap();
    let base = canon_root(&tmp);
    fs::write(base.join("a.md"), "x").unwrap();
    std::os::unix::fs::symlink(outside.path(), base.join("evil")).unwrap();

    // Destination parent canonicalizes outside the root → Escape, and the
    // outside dir stays untouched.
    assert!(matches!(
        move_within(&base, "a.md", "evil/a.md"),
        Err(FsMoveError::Escape)
    ));
    assert!(outside.path().read_dir().unwrap().next().is_none());
}

// ── mkdir_within / trash_within ────────────────────────────────────

#[test]
fn mkdir_creates_nested_and_rejects_escape_and_existing() {
    let tmp = tempfile::TempDir::new().unwrap();
    let base = canon_root(&tmp);

    mkdir_within(&base, "a/b/c").unwrap();
    assert!(base.join("a/b/c").is_dir());

    assert!(matches!(
        mkdir_within(&base, "a/b/c"),
        Err(FsMoveError::DestinationExists)
    ));
    for bad in ["../x", "/abs", "", "a/../x"] {
        assert!(
            matches!(mkdir_within(&base, bad), Err(FsMoveError::Escape)),
            "expected Escape for {bad:?}"
        );
    }
}

#[cfg(unix)]
#[test]
fn mkdir_rejects_symlinked_prefix_escape() {
    let tmp = tempfile::TempDir::new().unwrap();
    let outside = tempfile::TempDir::new().unwrap();
    let base = canon_root(&tmp);
    std::os::unix::fs::symlink(outside.path(), base.join("evil")).unwrap();

    assert!(matches!(
        mkdir_within(&base, "evil/new-dir"),
        Err(FsMoveError::Escape)
    ));
    assert!(outside.path().read_dir().unwrap().next().is_none());
}

#[test]
fn trash_moves_into_crucible_trash_without_overwrite() {
    let tmp = tempfile::TempDir::new().unwrap();
    let base = canon_root(&tmp);
    fs::write(base.join("a.md"), "one").unwrap();

    let rel1 = trash_within(&base, "a.md").unwrap();
    assert!(!base.join("a.md").exists());
    assert_eq!(fs::read_to_string(base.join(&rel1)).unwrap(), "one");

    // Same name trashed again in the same second must not collide.
    fs::write(base.join("a.md"), "two").unwrap();
    let rel2 = trash_within(&base, "a.md").unwrap();
    assert_ne!(rel1, rel2);
    assert_eq!(fs::read_to_string(base.join(&rel2)).unwrap(), "two");
}

#[test]
fn trash_refuses_dot_crucible_and_escapes() {
    let tmp = tempfile::TempDir::new().unwrap();
    let base = canon_root(&tmp);
    fs::create_dir_all(base.join(".crucible/trash")).unwrap();
    fs::write(base.join(".crucible/trash/x.md"), "x").unwrap();

    assert!(matches!(
        trash_within(&base, ".crucible/trash/x.md"),
        Err(FsMoveError::Escape)
    ));
    assert!(matches!(
        trash_within(&base, "../escape.md"),
        Err(FsMoveError::Escape)
    ));
    assert!(matches!(
        trash_within(&base, "ghost.md"),
        Err(FsMoveError::SourceMissing)
    ));
}

#[test]
fn trash_takes_whole_directories_and_collects_child_notes() {
    let tmp = tempfile::TempDir::new().unwrap();
    let base = canon_root(&tmp);
    fs::create_dir_all(base.join("dir/sub")).unwrap();
    fs::write(base.join("dir/a.md"), "a").unwrap();
    fs::write(base.join("dir/sub/b.md"), "b").unwrap();
    fs::write(base.join("dir/other.txt"), "t").unwrap();

    let mut notes = Vec::new();
    collect_indexed_files(&base.join("dir"), &mut notes);
    // Three, not two: the `.txt` has an index row of its own now, so
    // trashing the directory has to drop that row as well.
    assert_eq!(
        notes.len(),
        3,
        "every indexed file under the directory is collected: {notes:?}"
    );

    let rel = trash_within(&base, "dir").unwrap();
    assert!(!base.join("dir").exists());
    assert_eq!(
        fs::read_to_string(base.join(&rel).join("a.md")).unwrap(),
        "a"
    );
    assert_eq!(
        fs::read_to_string(base.join(&rel).join("sub/b.md")).unwrap(),
        "b"
    );
}

#[cfg(unix)]
#[test]
fn move_of_symlink_moves_the_link_not_its_target() {
    let tmp = tempfile::TempDir::new().unwrap();
    let outside = tempfile::TempDir::new().unwrap();
    let base = canon_root(&tmp);
    let target = outside.path().join("real.txt");
    fs::write(&target, "real").unwrap();
    std::os::unix::fs::symlink(&target, base.join("link.txt")).unwrap();
    fs::create_dir(base.join("sub")).unwrap();

    move_within(&base, "link.txt", "sub/link.txt").unwrap();
    // The link moved; the outside target is untouched.
    assert!(base
        .join("sub/link.txt")
        .symlink_metadata()
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(fs::read_to_string(&target).unwrap(), "real");
}

#[test]
fn a_directory_past_the_cap_is_truncated_and_says_so() {
    // `target/debug/deps` in this very repo is 1,472,409 entries — 381 MB and
    // 14.7 s in a single response before the cap, with the client rendering
    // every child unvirtualized.
    let tmp = tempfile::TempDir::new().unwrap();
    let proj = tmp.path().join("proj");
    let big = proj.join("many");
    std::fs::create_dir_all(&big).unwrap();
    for i in 0..(MAX_DIR_ENTRIES + 25) {
        std::fs::write(big.join(format!("f{i:05}.txt")), b"").unwrap();
    }
    let (pm, root) = registered_pm(tmp.path(), &proj);

    let listing = list_dir(&pm, &root, "many", true, false).unwrap();
    assert_eq!(listing.entries.len(), MAX_DIR_ENTRIES);
    assert!(
        listing.truncated,
        "a capped listing must be distinguishable from a complete one"
    );
}

#[test]
fn a_directory_within_the_cap_is_not_flagged_truncated() {
    // The flag drives a user-visible notice, so a false positive is as wrong as
    // a missing one.
    let tmp = tempfile::TempDir::new().unwrap();
    let proj = tmp.path().join("proj");
    std::fs::create_dir_all(proj.join("few")).unwrap();
    for i in 0..5 {
        std::fs::write(proj.join("few").join(format!("f{i}.txt")), b"").unwrap();
    }
    let (pm, root) = registered_pm(tmp.path(), &proj);

    let listing = list_dir(&pm, &root, "few", true, false).unwrap();
    assert_eq!(listing.entries.len(), 5);
    assert!(!listing.truncated);
}

/// The three fields of `FsPathRequest` reach the operation: a wrong `root`,
/// `kind` or `rel_path` cannot produce this directory.
#[tokio::test]
async fn fs_mkdir_reads_root_kind_and_rel_path() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = tempfile::TempDir::new().unwrap();
    let (pm, root) = registered_pm(store.path(), tmp.path());
    let km = Arc::new(KilnManager::new());

    let req = Request {
        jsonrpc: "2.0".to_string(),
        id: Some(crate::protocol::RequestId::Number(1)),
        method: "fs.mkdir".to_string(),
        params: serde_json::json!({
            "root": root.to_string_lossy(),
            "kind": "project",
            "rel_path": "notes/inbox",
        }),
    };
    let resp = handle_fs_mkdir(req, &pm, &km).await;

    assert!(resp.error.is_none(), "mkdir failed: {:?}", resp.error);
    assert!(root.join("notes").join("inbox").is_dir());
}

#[tokio::test]
async fn fs_mkdir_without_a_rel_path_is_invalid_params() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = tempfile::TempDir::new().unwrap();
    let (pm, root) = registered_pm(store.path(), tmp.path());
    let km = Arc::new(KilnManager::new());

    let req = Request {
        jsonrpc: "2.0".to_string(),
        id: Some(crate::protocol::RequestId::Number(1)),
        method: "fs.mkdir".to_string(),
        params: serde_json::json!({ "root": root.to_string_lossy(), "kind": "project" }),
    };
    let resp = handle_fs_mkdir(req, &pm, &km).await;

    let error = resp.error.expect("a request with no `rel_path` must fail");
    assert_eq!(error.code, crate::protocol::INVALID_PARAMS);
    assert!(
        error.message.contains("rel_path"),
        "the message must name the field: {}",
        error.message
    );
}
