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

    // Indexed but not markdown: a `.txt` is full-text searchable, so an agent
    // that can search it should be able to list it too.
    std::fs::write(temp_dir.path().join("scratch.txt"), "scratch").unwrap();

    // A genuine asset stays out — nothing indexes it, so nothing lists it.
    std::fs::write(temp_dir.path().join("diagram.png"), "not text").unwrap();

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
            // Two notes plus the `.txt`; the `.png` is an asset and excluded.
            assert_eq!(notes.len(), 3);
            assert_eq!(parsed["count"], 3);

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

#[tokio::test]
async fn test_list_notes_filters_by_folder() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();
    let note_tools = NoteTools::new(kiln_path);

    std::fs::create_dir(temp_dir.path().join("projects")).unwrap();
    for path in ["root.md", "projects/rust.md", "projects/python.md"] {
        note_tools
            .create_note(Parameters(CreateNoteParams {
                path: path.to_string(),
                content: "Content".to_string(),
                frontmatter: None,
            }))
            .await
            .unwrap();
    }

    let result = note_tools
        .list_notes(Parameters(ListNotesParams {
            folder: Some("projects".to_string()),
            include_frontmatter: false,
            recursive: true,
        }))
        .await
        .unwrap();

    let raw_text = result.content.first().unwrap().as_text().unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();
    assert_eq!(parsed["count"], 2);
    assert_eq!(parsed["folder"], "projects");
}
