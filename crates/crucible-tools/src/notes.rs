//! Note CRUD operations tools
//!
//! This module provides simple filesystem-based note CRUD tools.

use rmcp::{tool, tool_router, model::CallToolResult};
use rmcp::handler::server::wrapper::Parameters;
use serde::Deserialize;
use schemars::JsonSchema;

#[derive(Clone)]
pub struct NoteTools {
    kiln_path: String,
}

/// Parameters for creating a note
#[derive(Deserialize, JsonSchema)]
struct CreateNoteParams {
    path: String,
    content: String,
}

/// Parameters for reading a note
#[derive(Deserialize, JsonSchema)]
struct ReadNoteParams {
    path: String,
}

/// Parameters for updating a note
#[derive(Deserialize, JsonSchema)]
struct UpdateNoteParams {
    path: String,
    content: String,
}

/// Parameters for deleting a note
#[derive(Deserialize, JsonSchema)]
struct DeleteNoteParams {
    path: String,
}

/// Parameters for listing notes
#[derive(Deserialize, JsonSchema)]
struct ListNotesParams {
    folder: Option<String>,
}

impl NoteTools {
    pub fn new(kiln_path: String) -> Self {
        Self { kiln_path }
    }
}

#[tool_router]
impl NoteTools {
    #[tool(description = "Create a new note in the kiln")]
    async fn create_note(
        &self,
        params: Parameters<CreateNoteParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let params = params.0;
        let path = params.path;
        let content = params.content;

        let full_path = std::path::Path::new(&self.kiln_path).join(&path);

        std::fs::write(&full_path, &content)
            .map_err(|e| rmcp::ErrorData::internal_error(format!("Failed to write file: {e}"), None))?;

        // TODO: Notify parser for reprocessing

        Ok(CallToolResult::success(vec![
            rmcp::model::Content::json(serde_json::json!({
                "path": path,
                "full_path": full_path.to_string_lossy(),
                "status": "created"
            }))?
        ]))
    }

    #[tool(description = "Read an existing note")]
    async fn read_note(
        &self,
        params: Parameters<ReadNoteParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let params = params.0;
        let path = params.path;
        let full_path = std::path::Path::new(&self.kiln_path).join(&path);

        if !full_path.exists() {
            return Err(rmcp::ErrorData::invalid_params(format!("File not found: {}", path), None));
        }

        let content = std::fs::read_to_string(&full_path)
            .map_err(|e| rmcp::ErrorData::internal_error(format!("Failed to read file: {e}"), None))?;

        Ok(CallToolResult::success(vec![
            rmcp::model::Content::json(serde_json::json!({
                "path": path,
                "full_path": full_path.to_string_lossy(),
                "content": content
            }))?
        ]))
    }

    #[tool(description = "Update an existing note")]
    async fn update_note(
        &self,
        params: Parameters<UpdateNoteParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let params = params.0;
        let path = params.path;
        let content = params.content;
        let full_path = std::path::Path::new(&self.kiln_path).join(&path);

        if !full_path.exists() {
            return Err(rmcp::ErrorData::invalid_params(format!("File not found: {}", path), None));
        }

        std::fs::write(&full_path, &content)
            .map_err(|e| rmcp::ErrorData::internal_error(format!("Failed to update file: {e}"), None))?;

        // TODO: Notify parser for reprocessing

        Ok(CallToolResult::success(vec![
            rmcp::model::Content::json(serde_json::json!({
                "path": path,
                "full_path": full_path.to_string_lossy(),
                "status": "updated"
            }))?
        ]))
    }

    #[tool(description = "Delete a note from the kiln")]
    async fn delete_note(
        &self,
        params: Parameters<DeleteNoteParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let params = params.0;
        let path = params.path;
        let full_path = std::path::Path::new(&self.kiln_path).join(&path);

        if !full_path.exists() {
            return Err(rmcp::ErrorData::invalid_params(format!("File not found: {}", path), None));
        }

        std::fs::remove_file(&full_path)
            .map_err(|e| rmcp::ErrorData::internal_error(format!("Failed to delete file: {e}"), None))?;

        // TODO: Notify parser for reprocessing

        Ok(CallToolResult::success(vec![
            rmcp::model::Content::json(serde_json::json!({
                "path": path,
                "full_path": full_path.to_string_lossy(),
                "status": "deleted"
            }))?
        ]))
    }

    #[tool(description = "List notes in a directory")]
    async fn list_notes(
        &self,
        params: Parameters<ListNotesParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let params = params.0;
        let folder = params.folder;
        let search_path = if let Some(ref folder) = folder {
            std::path::Path::new(&self.kiln_path).join(folder)
        } else {
            std::path::Path::new(&self.kiln_path).to_path_buf()
        };

        let mut notes = Vec::new();

        if search_path.exists() && search_path.is_dir() {
            for entry in std::fs::read_dir(&search_path)
                .map_err(|e| rmcp::ErrorData::internal_error(format!("Failed to read directory: {e}"), None))?
            {
                let entry = entry.map_err(|e| rmcp::ErrorData::internal_error(format!("Failed to read entry: {e}"), None))?;
                let path = entry.path();

                if path.extension().map_or(false, |ext| ext == "md") {
                    if let Ok(relative_path) = path.strip_prefix(&self.kiln_path) {
                        let metadata = entry.metadata().ok();
                        notes.push(serde_json::json!({
                            "path": relative_path.to_string_lossy(),
                            "full_path": path.to_string_lossy(),
                            "size": metadata.as_ref().map(|m| m.len()).unwrap_or(0),
                            "modified": metadata.as_ref().and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs().to_string())
                        }));
                    }
                }
            }
        }

        Ok(CallToolResult::success(vec![
            rmcp::model::Content::json(serde_json::json!({
                "notes": notes,
                "folder": folder,
                "search_path": search_path.to_string_lossy()
            }))?
        ]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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

        let result = note_tools.create_note(
            "test.md".to_string(),
            "# Test Note\n\nThis is a test note.".to_string(),
        ).await;

        assert!(result.is_ok());

        let call_result = result.unwrap();
        assert!(!call_result.content.is_empty());

        // Check that the response is JSON with expected structure
        if let Some(content) = call_result.content.first() {
            if let Some(json_str) = content.as_text() {
                let parsed: serde_json::Value = serde_json::from_str(json_str).unwrap();
                assert_eq!(parsed["path"], "test.md");
                assert_eq!(parsed["status"], "created");
                assert!(parsed["full_path"].as_str().unwrap().contains("test.md"));
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
        let create_result = note_tools.create_note(
            "test.md".to_string(),
            content.to_string(),
        ).await;
        assert!(create_result.is_ok());

        // Read note
        let read_result = note_tools.read_note("test.md".to_string()).await;
        assert!(read_result.is_ok());

        let call_result = read_result.unwrap();

        // Verify the response structure and content
        if let Some(content) = call_result.content.first() {
            if let Some(json_str) = content.as_text() {
                let parsed: serde_json::Value = serde_json::from_str(json_str).unwrap();
                assert_eq!(parsed["path"], "test.md");
                assert_eq!(parsed["content"], content);
                assert!(parsed["full_path"].as_str().unwrap().contains("test.md"));
            }
        }
    }

    #[tokio::test]
    async fn test_read_nonexistent_note() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        let note_tools = NoteTools::new(kiln_path);

        let result = note_tools.read_note("nonexistent.md".to_string()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_update_note() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        let note_tools = NoteTools::new(kiln_path);
        let initial_content = "# Initial Content";
        let updated_content = "# Updated Content\n\nWith more text.";

        // Create note first
        note_tools.create_note("update.md".to_string(), initial_content.to_string()).await.unwrap();

        // Update note
        let result = note_tools.update_note("update.md".to_string(), updated_content.to_string()).await;
        assert!(result.is_ok());

        // Verify update response structure
        if let Some(content) = result.unwrap().content.first() {
            if let Some(json_str) = content.as_text() {
                let parsed: serde_json::Value = serde_json::from_str(json_str).unwrap();
                assert_eq!(parsed["path"], "update.md");
                assert_eq!(parsed["status"], "updated");
            }
        }

        // Verify file content
        let read_result = note_tools.read_note("update.md".to_string()).await.unwrap();
        if let Some(content) = read_result.content.first() {
            if let Some(json_str) = content.as_text() {
                let parsed: serde_json::Value = serde_json::from_str(json_str).unwrap();
                assert_eq!(parsed["content"], updated_content);
            }
        }
    }

    #[tokio::test]
    async fn test_update_nonexistent_note() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        let note_tools = NoteTools::new(kiln_path);

        let result = note_tools.update_note("nonexistent.md".to_string(), "content".to_string()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_delete_note() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        let note_tools = NoteTools::new(kiln_path);

        // Create note first
        note_tools.create_note("delete.md".to_string(), "content".to_string()).await.unwrap();

        // Delete note
        let result = note_tools.delete_note("delete.md".to_string()).await;
        assert!(result.is_ok());

        // Verify delete response structure
        if let Some(content) = result.unwrap().content.first() {
            if let Some(json_str) = content.as_text() {
                let parsed: serde_json::Value = serde_json::from_str(json_str).unwrap();
                assert_eq!(parsed["path"], "delete.md");
                assert_eq!(parsed["status"], "deleted");
            }
        }

        // Verify deletion
        let read_result = note_tools.read_note("delete.md".to_string()).await;
        assert!(read_result.is_err());
    }

    #[tokio::test]
    async fn test_list_notes_empty() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        let note_tools = NoteTools::new(kiln_path);

        let result = note_tools.list_notes(None).await;
        assert!(result.is_ok());

        let call_result = result.unwrap();
        if let Some(content) = call_result.content.first() {
            if let Some(json_str) = content.as_text() {
                let parsed: serde_json::Value = serde_json::from_str(json_str).unwrap();
                assert_eq!(parsed["notes"].as_array().unwrap().len(), 0);
                assert!(parsed["search_path"].as_str().unwrap().contains(&kiln_path));
            }
        }
    }

    #[tokio::test]
    async fn test_list_notes_with_files() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        let note_tools = NoteTools::new(kiln_path);

        // Create some test files
        note_tools.create_note("test1.md".to_string(), "content1".to_string()).await.unwrap();
        note_tools.create_note("test2.md".to_string(), "content2".to_string()).await.unwrap();

        // Create a non-md file (should be ignored)
        std::fs::write(temp_dir.path().join("ignore.txt"), "ignore").unwrap();

        let result = note_tools.list_notes(None).await;
        assert!(result.is_ok());

        let call_result = result.unwrap();
        if let Some(content) = call_result.content.first() {
            if let Some(json_str) = content.as_text() {
                let parsed: serde_json::Value = serde_json::from_str(json_str).unwrap();
                let notes = parsed["notes"].as_array().unwrap();
                assert_eq!(notes.len(), 2); // Should only find .md files

                // Check that all notes have required fields
                for note in notes {
                    assert!(note["path"].is_string());
                    assert!(note["full_path"].is_string());
                    assert!(note["size"].is_number());
                }
            }
        }
    }

    #[test]
    fn test_tool_router_creation() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        let note_tools = NoteTools::new(kiln_path);

        // This should compile and not panic - the tool_router macro generates the router
        let _router = note_tools.tool_router();
    }
}