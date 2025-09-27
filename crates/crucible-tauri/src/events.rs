use serde::{Deserialize, Serialize};
use tauri::Manager;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentCreatedEvent {
    pub document_id: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentUpdatedEvent {
    pub document_id: String,
    pub changes: serde_json::Value,
}

pub fn emit_document_created(app_handle: &tauri::AppHandle, document_id: String, title: String) {
    let event = DocumentCreatedEvent {
        document_id,
        title,
    };
    
    app_handle.emit_all("document-created", &event)
        .expect("Failed to emit document-created event");
}

pub fn emit_document_updated(app_handle: &tauri::AppHandle, document_id: String, changes: serde_json::Value) {
    let event = DocumentUpdatedEvent {
        document_id,
        changes,
    };
    
    app_handle.emit_all("document-updated", &event)
        .expect("Failed to emit document-updated event");
}

