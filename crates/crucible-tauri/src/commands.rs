use crucible_core::{DocumentNode, Result};
use serde::{Deserialize, Serialize};
use tauri::State;

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateDocumentRequest {
    pub title: String,
    pub content: String,
}

#[tauri::command]
pub async fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
pub async fn create_document(
    request: CreateDocumentRequest,
) -> Result<DocumentNode> {
    let document = DocumentNode::new(request.title, request.content);
    Ok(document)
}

#[tauri::command]
pub async fn get_document(id: String) -> Result<DocumentNode> {
    // TODO: Implement document retrieval from database
    Err(crucible_core::CrucibleError::DocumentNotFound(
        uuid::Uuid::parse_str(&id).unwrap_or_default()
    ))
}

