//! CRUD operation tests for `NoteTools`.

use super::super::{
    CreateNoteParams, DeleteNoteParams, NoteTools, ReadMetadataParams, ReadNoteParams,
    UpdateNoteParams,
};
use super::create_name_resolution_kiln;
use rmcp::handler::server::wrapper::Parameters;
use tempfile::TempDir;

#[test]
fn test_note_tools_creation() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();

    let note_tools = NoteTools::new(kiln_path);
    assert_eq!(note_tools.kiln_path, temp_dir.path().to_string_lossy());
}

#[tokio::test]
async fn test_create_note() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();

    let note_tools = NoteTools::new(kiln_path);

    let result = note_tools
        .create_note(Parameters(CreateNoteParams {
            path: "test.md".to_string(),
            content: "# Test Note\n\nThis is a test note.".to_string(),
            frontmatter: None,
        }))
        .await;

    assert!(result.is_ok());

    let call_result = result.unwrap();
    assert!(!call_result.content.is_empty());

    if let Some(content) = call_result.content.first() {
        if let Some(raw_text) = content.as_text() {
            let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();
            assert_eq!(parsed["path"], "test.md");
            assert_eq!(parsed["status"], "created");
        }
    }
}

#[tokio::test]
async fn test_create_and_read_note() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();

    let note_tools = NoteTools::new(kiln_path);
    let content = "# Test Note\n\nThis is a test note.";

    // Create note
    let create_result = note_tools
        .create_note(Parameters(CreateNoteParams {
            path: "test.md".to_string(),
            content: content.to_string(),
            frontmatter: None,
        }))
        .await;
    assert!(create_result.is_ok());

    // Read note
    let read_result = note_tools
        .read_note(Parameters(ReadNoteParams {
            path: "test.md".to_string(),
            start_line: None,
            end_line: None,
        }))
        .await;
    assert!(read_result.is_ok());

    let call_result = read_result.unwrap();

    // Verify the response structure and content
    if let Some(response_content) = call_result.content.first() {
        if let Some(raw_text) = response_content.as_text() {
            let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();
            assert_eq!(parsed["path"], "test.md");
            assert_eq!(parsed["content"], content);
            assert_eq!(parsed["total_lines"], 3);
        }
    }
}

#[tokio::test]
async fn test_read_nonexistent_note() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();

    let note_tools = NoteTools::new(kiln_path);

    let result = note_tools
        .read_note(Parameters(ReadNoteParams {
            path: "nonexistent.md".to_string(),
            start_line: None,
            end_line: None,
        }))
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_create_note_without_md_suffix() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();

    let note_tools = NoteTools::new(kiln_path.clone());

    let result = note_tools
        .create_note(Parameters(CreateNoteParams {
            path: "wikilink".to_string(),
            content: "# Wiki\n".to_string(),
            frontmatter: None,
        }))
        .await;
    assert!(result.is_ok());

    let call_result = result.unwrap();
    if let Some(content) = call_result.content.first() {
        if let Some(raw_text) = content.as_text() {
            let parsed: serde_json::Value =
                serde_json::from_str(&raw_text.text).expect("Valid JSON response");
            assert_eq!(parsed["path"], "wikilink.md");
        }
    }
    assert!(temp_dir.path().join("wikilink.md").exists());
}

#[tokio::test]
async fn test_read_note_without_md_suffix() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();

    let note_tools = NoteTools::new(kiln_path.clone());
    let note_path = temp_dir.path().join("wikilink.md");
    std::fs::write(&note_path, "content").unwrap();

    let result = note_tools
        .read_note(Parameters(ReadNoteParams {
            path: "wikilink".to_string(),
            start_line: None,
            end_line: None,
        }))
        .await;
    assert!(result.is_ok());

    let call_result = result.unwrap();
    if let Some(content) = call_result.content.first() {
        if let Some(raw_text) = content.as_text() {
            let parsed: serde_json::Value =
                serde_json::from_str(&raw_text.text).expect("Valid JSON response");
            assert_eq!(parsed["path"], "wikilink.md");
            assert_eq!(parsed["content"], "content");
        }
    }
}

#[tokio::test]
async fn test_read_note_resolves_subdirectory_note_by_name() {
    let kiln = create_name_resolution_kiln();
    let note_tools = NoteTools::new(kiln.path().to_string_lossy().to_string());

    let result = note_tools
        .read_note(Parameters(ReadNoteParams {
            path: "Plugin User Stories".to_string(),
            start_line: None,
            end_line: None,
        }))
        .await;

    assert!(result.is_ok());
    let call_result = result.unwrap();
    let parsed: serde_json::Value =
        serde_json::from_str(&call_result.content.first().unwrap().as_text().unwrap().text)
            .unwrap();
    assert_eq!(
        parsed["content"],
        "# Plugin User Stories\n\nSubdirectory note"
    );
}

#[tokio::test]
async fn test_read_note_with_explicit_subdirectory_path() {
    let kiln = create_name_resolution_kiln();
    let note_tools = NoteTools::new(kiln.path().to_string_lossy().to_string());

    let result = note_tools
        .read_note(Parameters(ReadNoteParams {
            path: "Meta/Plugin User Stories".to_string(),
            start_line: None,
            end_line: None,
        }))
        .await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn test_read_note_resolves_root_note_by_name() {
    let kiln = create_name_resolution_kiln();
    let note_tools = NoteTools::new(kiln.path().to_string_lossy().to_string());

    let result = note_tools
        .read_note(Parameters(ReadNoteParams {
            path: "README".to_string(),
            start_line: None,
            end_line: None,
        }))
        .await;

    assert!(result.is_ok());
    let call_result = result.unwrap();
    let parsed: serde_json::Value =
        serde_json::from_str(&call_result.content.first().unwrap().as_text().unwrap().text)
            .unwrap();
    assert_eq!(parsed["content"], "# README\n\nRoot note");
}

#[tokio::test]
async fn test_read_note_reports_clear_not_found_for_name_lookup() {
    let kiln = create_name_resolution_kiln();
    let note_tools = NoteTools::new(kiln.path().to_string_lossy().to_string());

    let result = note_tools
        .read_note(Parameters(ReadNoteParams {
            path: "Nonexistent Note".to_string(),
            start_line: None,
            end_line: None,
        }))
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.message.contains("File not found: Nonexistent Note.md"));
}

#[tokio::test]
async fn test_read_note_resolves_subdirectory_note_by_name_with_md_suffix() {
    let kiln = create_name_resolution_kiln();
    let note_tools = NoteTools::new(kiln.path().to_string_lossy().to_string());

    let result = note_tools
        .read_note(Parameters(ReadNoteParams {
            path: "Plugin User Stories.md".to_string(),
            start_line: None,
            end_line: None,
        }))
        .await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn test_update_note() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();

    let note_tools = NoteTools::new(kiln_path);
    let initial_content = "# Initial Content";
    let updated_content = "# Updated Content\n\nWith more text.";

    // Create note first
    note_tools
        .create_note(Parameters(CreateNoteParams {
            path: "update.md".to_string(),
            content: initial_content.to_string(),
            frontmatter: None,
        }))
        .await
        .unwrap();

    // Update note
    let result = note_tools
        .update_note(Parameters(UpdateNoteParams {
            path: "update.md".to_string(),
            content: Some(updated_content.to_string()),
            frontmatter: None,
        }))
        .await;
    assert!(result.is_ok());

    // Verify update response structure
    if let Some(content) = result.unwrap().content.first() {
        if let Some(raw_text) = content.as_text() {
            let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();
            assert_eq!(parsed["path"], "update.md");
            assert_eq!(parsed["status"], "updated");
        }
    }

    // Verify file content
    let read_result = note_tools
        .read_note(Parameters(ReadNoteParams {
            path: "update.md".to_string(),
            start_line: None,
            end_line: None,
        }))
        .await
        .unwrap();
    if let Some(content) = read_result.content.first() {
        if let Some(raw_text) = content.as_text() {
            let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();
            assert_eq!(parsed["content"], updated_content);
        }
    }
}

#[tokio::test]
async fn test_update_nonexistent_note() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();

    let note_tools = NoteTools::new(kiln_path);

    let result = note_tools
        .update_note(Parameters(UpdateNoteParams {
            path: "nonexistent.md".to_string(),
            content: Some("content".to_string()),
            frontmatter: None,
        }))
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_delete_note() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();

    let note_tools = NoteTools::new(kiln_path);

    // Create note first
    note_tools
        .create_note(Parameters(CreateNoteParams {
            path: "delete.md".to_string(),
            content: "content".to_string(),
            frontmatter: None,
        }))
        .await
        .unwrap();

    // Delete note
    let result = note_tools
        .delete_note(Parameters(DeleteNoteParams {
            path: "delete.md".to_string(),
        }))
        .await;
    assert!(result.is_ok());

    // Verify delete response structure
    if let Some(content) = result.unwrap().content.first() {
        if let Some(raw_text) = content.as_text() {
            let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();
            assert_eq!(parsed["path"], "delete.md");
            assert_eq!(parsed["status"], "deleted");
        }
    }

    // Verify deletion
    let read_result = note_tools
        .read_note(Parameters(ReadNoteParams {
            path: "delete.md".to_string(),
            start_line: None,
            end_line: None,
        }))
        .await;
    assert!(read_result.is_err());
}

// ===== Phase 2: New tests for read_metadata and line range support =====

#[tokio::test]
async fn test_read_metadata_with_frontmatter() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();
    let note_tools = NoteTools::new(kiln_path);

    // Create note with frontmatter
    let content = "---\ntitle: Test Note\ntags: [test, important]\nstatus: draft\n---\n\n# Test Note\n\nSome content here.";
    note_tools
        .create_note(Parameters(CreateNoteParams {
            path: "test.md".to_string(),
            content: content.to_string(),
            frontmatter: None,
        }))
        .await
        .unwrap();

    // Read metadata
    let result = note_tools
        .read_metadata(Parameters(ReadMetadataParams {
            path: "test.md".to_string(),
        }))
        .await;
    assert!(result.is_ok());

    let call_result = result.unwrap();
    if let Some(response_content) = call_result.content.first() {
        if let Some(raw_text) = response_content.as_text() {
            let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();

            // Check frontmatter
            assert_eq!(parsed["frontmatter"]["title"], "Test Note");
            assert_eq!(parsed["frontmatter"]["status"], "draft");
            assert_eq!(parsed["frontmatter"]["tags"].as_array().unwrap().len(), 2);

            // Check stats
            assert!(parsed["stats"]["word_count"].as_u64().unwrap() > 0);
            assert!(parsed["stats"]["heading_count"].as_u64().unwrap() == 1);
            assert!(parsed["modified"].is_number());
        }
    }
}

#[tokio::test]
async fn test_read_metadata_without_frontmatter() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();
    let note_tools = NoteTools::new(kiln_path);

    // Create note without frontmatter
    let content = "# Test Note\n\nJust content, no frontmatter.";
    note_tools
        .create_note(Parameters(CreateNoteParams {
            path: "test.md".to_string(),
            content: content.to_string(),
            frontmatter: None,
        }))
        .await
        .unwrap();

    // Read metadata
    let result = note_tools
        .read_metadata(Parameters(ReadMetadataParams {
            path: "test.md".to_string(),
        }))
        .await;
    assert!(result.is_ok());

    let call_result = result.unwrap();
    if let Some(response_content) = call_result.content.first() {
        if let Some(raw_text) = response_content.as_text() {
            let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();

            // Frontmatter should be empty object
            assert_eq!(parsed["frontmatter"], serde_json::json!({}));

            // Stats should still be present
            assert!(parsed["stats"]["word_count"].as_u64().unwrap() > 0);
        }
    }
}

#[tokio::test]
async fn test_read_note_line_range_full() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();
    let note_tools = NoteTools::new(kiln_path);

    let content = "line 1\nline 2\nline 3\nline 4\nline 5";
    note_tools
        .create_note(Parameters(CreateNoteParams {
            path: "test.md".to_string(),
            content: content.to_string(),
            frontmatter: None,
        }))
        .await
        .unwrap();

    // Read full file (no line range)
    let result = note_tools
        .read_note(Parameters(ReadNoteParams {
            path: "test.md".to_string(),
            start_line: None,
            end_line: None,
        }))
        .await
        .unwrap();

    if let Some(response_content) = result.content.first() {
        if let Some(raw_text) = response_content.as_text() {
            let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();
            assert_eq!(parsed["total_lines"], 5);
            assert_eq!(parsed["lines_returned"], 5);
            assert_eq!(parsed["content"], content);
        }
    }
}

#[tokio::test]
async fn test_read_note_first_n_lines() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();
    let note_tools = NoteTools::new(kiln_path);

    let content = "line 1\nline 2\nline 3\nline 4\nline 5";
    note_tools
        .create_note(Parameters(CreateNoteParams {
            path: "test.md".to_string(),
            content: content.to_string(),
            frontmatter: None,
        }))
        .await
        .unwrap();

    // Read first 3 lines
    let result = note_tools
        .read_note(Parameters(ReadNoteParams {
            path: "test.md".to_string(),
            start_line: None,
            end_line: Some(3),
        }))
        .await
        .unwrap();

    if let Some(response_content) = result.content.first() {
        if let Some(raw_text) = response_content.as_text() {
            let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();
            assert_eq!(parsed["total_lines"], 5);
            assert_eq!(parsed["lines_returned"], 3);
            assert_eq!(parsed["content"], "line 1\nline 2\nline 3");
        }
    }
}

#[tokio::test]
async fn test_read_note_line_range() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();
    let note_tools = NoteTools::new(kiln_path);

    let content = "line 1\nline 2\nline 3\nline 4\nline 5";
    note_tools
        .create_note(Parameters(CreateNoteParams {
            path: "test.md".to_string(),
            content: content.to_string(),
            frontmatter: None,
        }))
        .await
        .unwrap();

    // Read lines 2-4 (1-indexed)
    let result = note_tools
        .read_note(Parameters(ReadNoteParams {
            path: "test.md".to_string(),
            start_line: Some(2),
            end_line: Some(4),
        }))
        .await
        .unwrap();

    if let Some(response_content) = result.content.first() {
        if let Some(raw_text) = response_content.as_text() {
            let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();
            assert_eq!(parsed["total_lines"], 5);
            assert_eq!(parsed["lines_returned"], 3);
            assert_eq!(parsed["content"], "line 2\nline 3\nline 4");
        }
    }
}

#[tokio::test]
async fn test_read_note_from_start_line() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();
    let note_tools = NoteTools::new(kiln_path);

    let content = "line 1\nline 2\nline 3\nline 4\nline 5";
    note_tools
        .create_note(Parameters(CreateNoteParams {
            path: "test.md".to_string(),
            content: content.to_string(),
            frontmatter: None,
        }))
        .await
        .unwrap();

    // Read from line 3 to end
    let result = note_tools
        .read_note(Parameters(ReadNoteParams {
            path: "test.md".to_string(),
            start_line: Some(3),
            end_line: None,
        }))
        .await
        .unwrap();

    if let Some(response_content) = result.content.first() {
        if let Some(raw_text) = response_content.as_text() {
            let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();
            assert_eq!(parsed["total_lines"], 5);
            assert_eq!(parsed["lines_returned"], 3);
            assert_eq!(parsed["content"], "line 3\nline 4\nline 5");
        }
    }
}

#[tokio::test]
async fn test_create_note_with_frontmatter() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();
    let note_tools = NoteTools::new(kiln_path.clone());

    let frontmatter = serde_json::json!({
        "title": "Test Note",
        "tags": ["test", "example"],
        "status": "draft"
    });

    let result = note_tools
        .create_note(Parameters(CreateNoteParams {
            path: "test_frontmatter.md".to_string(),
            content: "This is the content".to_string(),
            frontmatter: Some(frontmatter.clone()),
        }))
        .await
        .unwrap();

    if let Some(response_content) = result.content.first() {
        if let Some(raw_text) = response_content.as_text() {
            let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();
            assert_eq!(parsed["status"], "created");
        }
    }

    // Verify file content has frontmatter
    let file_path = temp_dir.path().join("test_frontmatter.md");
    let content = std::fs::read_to_string(file_path).unwrap();

    assert!(content.starts_with("---\n"));
    assert!(content.contains("title: Test Note"));
    assert!(content.contains("tags:"));
    assert!(content.contains("- test"));
    assert!(content.contains("- example"));
    assert!(content.contains("status: draft"));
    assert!(content.contains("---\nThis is the content"));
}

#[tokio::test]
async fn test_create_note_without_frontmatter() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();
    let note_tools = NoteTools::new(kiln_path.clone());

    let result = note_tools
        .create_note(Parameters(CreateNoteParams {
            path: "test_no_frontmatter.md".to_string(),
            content: "Just content".to_string(),
            frontmatter: None,
        }))
        .await
        .unwrap();

    if let Some(response_content) = result.content.first() {
        if let Some(raw_text) = response_content.as_text() {
            let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();
            assert_eq!(parsed["status"], "created");
        }
    }

    // Verify file content has NO frontmatter
    let file_path = temp_dir.path().join("test_no_frontmatter.md");
    let content = std::fs::read_to_string(file_path).unwrap();

    assert_eq!(content, "Just content");
}

#[tokio::test]
async fn test_update_note_content_only() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();
    let note_tools = NoteTools::new(kiln_path.clone());

    // Create initial note with frontmatter
    let initial_frontmatter = serde_json::json!({
        "title": "Original",
        "tags": ["original"]
    });

    note_tools
        .create_note(Parameters(CreateNoteParams {
            path: "update_test.md".to_string(),
            content: "Original content".to_string(),
            frontmatter: Some(initial_frontmatter),
        }))
        .await
        .unwrap();

    // Update content only (frontmatter should remain)
    let result = note_tools
        .update_note(Parameters(UpdateNoteParams {
            path: "update_test.md".to_string(),
            content: Some("New content".to_string()),
            frontmatter: None,
        }))
        .await
        .unwrap();

    if let Some(response_content) = result.content.first() {
        if let Some(raw_text) = response_content.as_text() {
            let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();
            assert_eq!(parsed["status"], "updated");
            assert_eq!(parsed["updated_fields"], serde_json::json!(["content"]));
        }
    }

    // Verify frontmatter preserved, content updated
    let file_path = temp_dir.path().join("update_test.md");
    let content = std::fs::read_to_string(file_path).unwrap();

    assert!(content.contains("title: Original"));
    assert!(content.contains("- original"));
    assert!(content.contains("---\nNew content"));
}

#[tokio::test]
async fn test_update_note_frontmatter_only() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();
    let note_tools = NoteTools::new(kiln_path.clone());

    // Create initial note
    note_tools
        .create_note(Parameters(CreateNoteParams {
            path: "update_fm_test.md".to_string(),
            content: "Original content".to_string(),
            frontmatter: Some(serde_json::json!({"title": "Original"})),
        }))
        .await
        .unwrap();

    // Update frontmatter only (content should remain)
    let new_frontmatter = serde_json::json!({
        "title": "Updated",
        "tags": ["new"]
    });

    let result = note_tools
        .update_note(Parameters(UpdateNoteParams {
            path: "update_fm_test.md".to_string(),
            content: None,
            frontmatter: Some(new_frontmatter),
        }))
        .await
        .unwrap();

    if let Some(response_content) = result.content.first() {
        if let Some(raw_text) = response_content.as_text() {
            let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();
            assert_eq!(parsed["status"], "updated");
            assert_eq!(parsed["updated_fields"], serde_json::json!(["frontmatter"]));
        }
    }

    // Verify frontmatter updated, content preserved
    let file_path = temp_dir.path().join("update_fm_test.md");
    let content = std::fs::read_to_string(file_path).unwrap();

    assert!(content.contains("title: Updated"));
    assert!(content.contains("- new"));
    assert!(content.contains("---\nOriginal content"));
}

#[tokio::test]
async fn test_update_note_both_content_and_frontmatter() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();
    let note_tools = NoteTools::new(kiln_path.clone());

    // Create initial note
    note_tools
        .create_note(Parameters(CreateNoteParams {
            path: "update_both_test.md".to_string(),
            content: "Original content".to_string(),
            frontmatter: Some(serde_json::json!({"title": "Original"})),
        }))
        .await
        .unwrap();

    // Update both
    let result = note_tools
        .update_note(Parameters(UpdateNoteParams {
            path: "update_both_test.md".to_string(),
            content: Some("New content".to_string()),
            frontmatter: Some(serde_json::json!({"title": "New", "status": "published"})),
        }))
        .await
        .unwrap();

    if let Some(response_content) = result.content.first() {
        if let Some(raw_text) = response_content.as_text() {
            let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();
            assert_eq!(parsed["status"], "updated");
            // Should contain both fields
            let updated_fields = parsed["updated_fields"].as_array().unwrap();
            assert!(updated_fields.contains(&serde_json::json!("content")));
            assert!(updated_fields.contains(&serde_json::json!("frontmatter")));
        }
    }

    // Verify both updated
    let file_path = temp_dir.path().join("update_both_test.md");
    let content = std::fs::read_to_string(file_path).unwrap();

    assert!(content.contains("title: New"));
    assert!(content.contains("status: published"));
    assert!(content.contains("---\nNew content"));
}

#[tokio::test]
async fn test_update_note_remove_frontmatter() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();
    let note_tools = NoteTools::new(kiln_path.clone());

    // Create note with frontmatter
    note_tools
        .create_note(Parameters(CreateNoteParams {
            path: "remove_fm_test.md".to_string(),
            content: "Content".to_string(),
            frontmatter: Some(serde_json::json!({"title": "Test"})),
        }))
        .await
        .unwrap();

    // Update with content only and empty frontmatter to remove it
    let result = note_tools
        .update_note(Parameters(UpdateNoteParams {
            path: "remove_fm_test.md".to_string(),
            content: Some("Just content".to_string()),
            frontmatter: Some(serde_json::json!({})),
        }))
        .await
        .unwrap();

    if let Some(response_content) = result.content.first() {
        if let Some(raw_text) = response_content.as_text() {
            let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();
            assert_eq!(parsed["status"], "updated");
        }
    }

    // Verify frontmatter removed
    let file_path = temp_dir.path().join("remove_fm_test.md");
    let content = std::fs::read_to_string(file_path).unwrap();

    // Should be just content, no frontmatter block
    assert_eq!(content, "Just content");
}

#[test]
fn test_tool_router_creation() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();

    let _note_tools = NoteTools::new(kiln_path);

    // This should compile and not panic - the tool_router macro generates the router
    let _router = NoteTools::tool_router();
}
