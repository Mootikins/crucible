use serde::{Deserialize, Serialize};
use tauri::Emitter;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct DocumentCreatedEvent {
    pub document_id: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct DocumentUpdatedEvent {
    pub document_id: String,
    pub changes: serde_json::Value,
}

#[allow(dead_code)]
pub fn emit_document_created(app_handle: &tauri::AppHandle, document_id: String, title: String) {
    let event = DocumentCreatedEvent { document_id, title };

    app_handle
        .emit("document-created", &event)
        .expect("Failed to emit document-created event");
}

#[allow(dead_code)]
pub fn emit_document_updated(
    app_handle: &tauri::AppHandle,
    document_id: String,
    changes: serde_json::Value,
) {
    let event = DocumentUpdatedEvent {
        document_id,
        changes,
    };

    app_handle
        .emit("document-updated", &event)
        .expect("Failed to emit document-updated event");
}
