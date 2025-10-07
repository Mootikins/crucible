use crucible_core::{DocumentNode, Result};
use serde::{Deserialize, Serialize};
use tauri::State;

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
pub async fn create_document(
    request: CreateDocumentRequest,
) -> std::result::Result<DocumentNode, String> {
    let document = DocumentNode::new(request.title, request.content);
    Ok(document)
}

#[tauri::command]
pub async fn get_document(id: String) -> std::result::Result<DocumentNode, String> {
    // TODO: Implement document retrieval from database
    Err(format!("Document not found: {}", id))
}

