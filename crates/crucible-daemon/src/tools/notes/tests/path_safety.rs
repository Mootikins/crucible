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
