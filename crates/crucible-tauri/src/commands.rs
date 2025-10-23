use crucible_core::DocumentNode;
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
    query: String,
) -> std::result::Result<Vec<serde_json::Value>, String> {
    // MCP functionality has been removed - please use crucible-services instead
    Err("Search functionality has been moved to crucible-services".to_string())
}

#[tauri::command]
pub async fn get_document(path: String) -> std::result::Result<serde_json::Value, String> {
    // MCP functionality has been removed - please use crucible-services instead
    Err("Document retrieval functionality has been moved to crucible-services".to_string())
}

#[tauri::command]
pub async fn create_document(
    request: CreateDocumentRequest,
) -> std::result::Result<DocumentNode, String> {
    let document = DocumentNode::new(request.title.clone(), request.content.clone());

    // MCP functionality has been removed - document creation is now handled by crucible-services
    // For now, just return the document without persisting
    Ok(document)
}

#[tauri::command]
pub async fn update_document(path: String, content: String) -> std::result::Result<(), String> {
    // MCP functionality has been removed - please use crucible-services instead
    Err("Document update functionality has been moved to crucible-services".to_string())
}

#[tauri::command]
pub async fn delete_document(path: String) -> std::result::Result<(), String> {
    // TODO: Implement document deletion
    Err("Not implemented".to_string())
}

#[tauri::command]
pub async fn list_documents() -> std::result::Result<Vec<String>, String> {
    // MCP functionality has been removed - please use crucible-services instead
    Err("Document listing functionality has been moved to crucible-services".to_string())
}

#[tauri::command]
pub async fn search_by_tags(tags: Vec<String>) -> std::result::Result<Vec<String>, String> {
    // MCP functionality has been removed - please use crucible-services instead
    Err("Tag search functionality has been moved to crucible-services".to_string())
}

#[tauri::command]
pub async fn search_by_properties(
    properties: HashMap<String, serde_json::Value>,
) -> std::result::Result<Vec<String>, String> {
    // MCP functionality has been removed - please use crucible-services instead
    Err("Property search functionality has been moved to crucible-services".to_string())
}

#[tauri::command]
pub async fn semantic_search(
    query: String,
    top_k: u32,
) -> std::result::Result<Vec<serde_json::Value>, String> {
    // MCP functionality has been removed - please use crucible-services instead
    Err("Semantic search functionality has been moved to crucible-services".to_string())
}

#[tauri::command]
pub async fn index_vault(force: bool) -> std::result::Result<serde_json::Value, String> {
    // MCP functionality has been removed - please use crucible-services instead
    Err("Vault indexing functionality has been moved to crucible-services".to_string())
}

#[tauri::command]
pub async fn get_note_metadata(path: String) -> std::result::Result<serde_json::Value, String> {
    // MCP functionality has been removed - please use crucible-services instead
    Err("Note metadata functionality has been moved to crucible-services".to_string())
}

#[tauri::command]
pub async fn update_note_properties(
    path: String,
    properties: HashMap<String, serde_json::Value>,
) -> std::result::Result<(), String> {
    // MCP functionality has been removed - please use crucible-services instead
    Err("Note property update functionality has been moved to crucible-services".to_string())
}
