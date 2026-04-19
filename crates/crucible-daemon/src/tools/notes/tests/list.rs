//! List-operation tests for `NoteTools`.

use super::super::{CreateNoteParams, ListNotesParams, NoteTools};
use rmcp::handler::server::wrapper::Parameters;
use tempfile::TempDir;

#[tokio::test]
async fn test_list_notes_empty() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();

    let note_tools = NoteTools::new(kiln_path.clone());

    let result = note_tools
        .list_notes(Parameters(ListNotesParams {
            folder: None,
            include_frontmatter: false,
            recursive: true,
        }))
        .await;
    assert!(result.is_ok());

    let call_result = result.unwrap();
    if let Some(content) = call_result.content.first() {
        if let Some(raw_text) = content.as_text() {
            let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();
            assert_eq!(parsed["notes"].as_array().unwrap().len(), 0);
            assert_eq!(parsed["count"], 0);
        }
    }
}

#[tokio::test]
async fn test_list_notes_with_files() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();

    let note_tools = NoteTools::new(kiln_path);

    // Create some test files
    note_tools
        .create_note(Parameters(CreateNoteParams {
            path: "test1.md".to_string(),
            content: "content1".to_string(),
            frontmatter: None,
        }))
        .await
        .unwrap();
    note_tools
        .create_note(Parameters(CreateNoteParams {
            path: "test2.md".to_string(),
            content: "content2".to_string(),
            frontmatter: None,
        }))
        .await
        .unwrap();

    // Create a non-md file (should be ignored)
    std::fs::write(temp_dir.path().join("ignore.txt"), "ignore").unwrap();

    let result = note_tools
        .list_notes(Parameters(ListNotesParams {
            folder: None,
            include_frontmatter: false,
            recursive: true,
        }))
        .await;
    assert!(result.is_ok());

    let call_result = result.unwrap();
    if let Some(content) = call_result.content.first() {
        if let Some(raw_text) = content.as_text() {
            let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();
            let notes = parsed["notes"].as_array().unwrap();
            assert_eq!(notes.len(), 2); // Should only find .md files
            assert_eq!(parsed["count"], 2);

            // Check that all notes have required fields
            for note in notes {
                assert!(note["path"].is_string());
                assert!(note["size"].is_number());
            }
        }
    }
}

#[tokio::test]
async fn test_list_notes_with_frontmatter() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();
    let note_tools = NoteTools::new(kiln_path);

    // Create notes with frontmatter
    note_tools
        .create_note(Parameters(CreateNoteParams {
            path: "note1.md".to_string(),
            content: "---\ntitle: Note 1\nstatus: draft\n---\n\nContent".to_string(),
            frontmatter: None,
        }))
        .await
        .unwrap();

    note_tools
        .create_note(Parameters(CreateNoteParams {
            path: "note2.md".to_string(),
            content: "---\ntitle: Note 2\nstatus: published\n---\n\nContent".to_string(),
            frontmatter: None,
        }))
        .await
        .unwrap();

    // List with frontmatter
    let result = note_tools
        .list_notes(Parameters(ListNotesParams {
            folder: None,
            include_frontmatter: true,
            recursive: true,
        }))
        .await
        .unwrap();

    if let Some(response_content) = result.content.first() {
        if let Some(raw_text) = response_content.as_text() {
            let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();
            let notes = parsed["notes"].as_array().unwrap();
            assert_eq!(notes.len(), 2);

            // Check that frontmatter is included
            for note in notes {
                assert!(note["frontmatter"].is_object());
                assert!(note["frontmatter"]["title"].is_string());
                assert!(note["word_count"].is_number());
            }
        }
    }
}

#[tokio::test]
async fn test_list_notes_non_recursive() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();
    let note_tools = NoteTools::new(kiln_path);

    // Create root note
    note_tools
        .create_note(Parameters(CreateNoteParams {
            path: "root.md".to_string(),
            content: "Root note".to_string(),
            frontmatter: None,
        }))
        .await
        .unwrap();

    // Create subfolder with note
    std::fs::create_dir(temp_dir.path().join("subfolder")).unwrap();
    note_tools
        .create_note(Parameters(CreateNoteParams {
            path: "subfolder/nested.md".to_string(),
            content: "Nested note".to_string(),
            frontmatter: None,
        }))
        .await
        .unwrap();

    // List non-recursively (should only find root.md)
    let result = note_tools
        .list_notes(Parameters(ListNotesParams {
            folder: None,
            include_frontmatter: false,
            recursive: false,
        }))
        .await
        .unwrap();

    if let Some(response_content) = result.content.first() {
        if let Some(raw_text) = response_content.as_text() {
            let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();
            assert_eq!(parsed["count"], 1);
            assert_eq!(parsed["recursive"], false);
        }
    }
}
