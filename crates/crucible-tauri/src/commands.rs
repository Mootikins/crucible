use crucible_core::NoteNode;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateDocumentRequest {
    pub title: String,
    pub content: String,
}

#[tauri::command]
pub async fn greet(name: &str) -> std::result::Result<String, String> {
    Ok(format!("Hello, {}! You've been greeted from Rust!", name))
}

#[tauri::command]
pub async fn initialize_database() -> std::result::Result<(), String> {
    // MCP functionality has been removed - using crucible-services instead
    Ok(())
}

#[tauri::command]
pub async fn search_documents(
    _query: String,
) -> std::result::Result<Vec<serde_json::Value>, String> {
    // MCP functionality has been removed - please use crucible-services instead
    Err("Search functionality has been moved to crucible-services".to_string())
}

#[tauri::command]
pub async fn get_document(_path: String) -> std::result::Result<serde_json::Value, String> {
    // MCP functionality has been removed - please use crucible-services instead
    Err("Note retrieval functionality has been moved to crucible-services".to_string())
}

#[tauri::command]
pub async fn create_document(
    request: CreateDocumentRequest,
) -> std::result::Result<NoteNode, String> {
    let note = NoteNode::new(request.title.clone(), request.content.clone());

    // MCP functionality has been removed - note creation is now handled by crucible-services
    // For now, just return the note without persisting
    Ok(note)
}

#[tauri::command]
pub async fn update_document(_path: String, _content: String) -> std::result::Result<(), String> {
    // MCP functionality has been removed - please use crucible-services instead
    Err("Note update functionality has been moved to crucible-services".to_string())
}

#[tauri::command]
pub async fn delete_document(_path: String) -> std::result::Result<(), String> {
    // TODO: Implement note deletion
    Err("Not implemented".to_string())
}

#[tauri::command]
pub async fn list_documents() -> std::result::Result<Vec<String>, String> {
    // MCP functionality has been removed - please use crucible-services instead
    Err("Note listing functionality has been moved to crucible-services".to_string())
}

#[tauri::command]
pub async fn search_by_tags(_tags: Vec<String>) -> std::result::Result<Vec<String>, String> {
    // MCP functionality has been removed - please use crucible-services instead
    Err("Tag search functionality has been moved to crucible-services".to_string())
}

#[tauri::command]
pub async fn search_by_properties(
    _properties: HashMap<String, serde_json::Value>,
) -> std::result::Result<Vec<String>, String> {
    // MCP functionality has been removed - please use crucible-services instead
    Err("Property search functionality has been moved to crucible-services".to_string())
}

#[tauri::command]
pub async fn semantic_search(
    _query: String,
    _top_k: u32,
) -> std::result::Result<Vec<serde_json::Value>, String> {
    // MCP functionality has been removed - please use crucible-services instead
    Err("Semantic search functionality has been moved to crucible-services".to_string())
}

#[tauri::command]
pub async fn index_kiln(_force: bool) -> std::result::Result<serde_json::Value, String> {
    // MCP functionality has been removed - please use crucible-services instead
    Err("Kiln indexing functionality has been moved to crucible-services".to_string())
}

#[tauri::command]
pub async fn get_note_metadata(_path: String) -> std::result::Result<serde_json::Value, String> {
    // MCP functionality has been removed - please use crucible-services instead
    Err("Note metadata functionality has been moved to crucible-services".to_string())
}

#[tauri::command]
pub async fn update_note_properties(
    _path: String,
    _properties: HashMap<String, serde_json::Value>,
) -> std::result::Result<(), String> {
    // MCP functionality has been removed - please use crucible-services instead
    Err("Note property update functionality has been moved to crucible-services".to_string())
}
