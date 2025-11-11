use crate::PropertyMap;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteNode {
    pub id: Uuid,
    pub title: String,
    pub content: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub parent_id: Option<Uuid>,
    pub children: Vec<Uuid>,
    pub properties: PropertyMap,
    pub collapsed: bool,
    pub position: i32,
}

impl NoteNode {
    pub fn new(title: String, content: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            title,
            content,
            created_at: now,
            updated_at: now,
            parent_id: None,
            children: Vec::new(),
            properties: PropertyMap::new(),
            collapsed: false,
            position: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewportState {
    pub zoom: f64,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Default for ViewportState {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            x: 0.0,
            y: 0.0,
            width: 800.0,
            height: 600.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_document_node_new() {
        let doc = NoteNode::new("Test Title".to_string(), "Test Content".to_string());

        assert_eq!(doc.title, "Test Title");
        assert_eq!(doc.content, "Test Content");
        assert!(doc.parent_id.is_none());
        assert!(doc.children.is_empty());
        assert!(doc.properties.is_empty());
        assert_eq!(doc.collapsed, false);
        assert_eq!(doc.position, 0);
        assert_eq!(doc.created_at, doc.updated_at);
    }

    #[test]
    fn test_document_node_uuid_unique() {
        let doc1 = NoteNode::new("Doc 1".to_string(), "Content 1".to_string());
        let doc2 = NoteNode::new("Doc 2".to_string(), "Content 2".to_string());

        assert_ne!(doc1.id, doc2.id);
    }

    #[test]
    fn test_document_node_serialization() {
        let doc = NoteNode::new("Test".to_string(), "Content".to_string());
        let json = serde_json::to_string(&doc).unwrap();

        let deserialized: NoteNode = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, doc.id);
        assert_eq!(deserialized.title, doc.title);
        assert_eq!(deserialized.content, doc.content);
    }

    #[test]
    fn test_viewport_state_default() {
        let viewport = ViewportState::default();

        assert_eq!(viewport.zoom, 1.0);
        assert_eq!(viewport.x, 0.0);
        assert_eq!(viewport.y, 0.0);
        assert_eq!(viewport.width, 800.0);
        assert_eq!(viewport.height, 600.0);
    }

    #[test]
    fn test_viewport_state_serialization() {
        let viewport = ViewportState {
            zoom: 2.0,
            x: 100.0,
            y: 200.0,
            width: 1920.0,
            height: 1080.0,
        };

        let json = serde_json::to_string(&viewport).unwrap();
        let deserialized: ViewportState = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.zoom, 2.0);
        assert_eq!(deserialized.x, 100.0);
        assert_eq!(deserialized.y, 200.0);
    }
}
