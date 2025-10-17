use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasNode {
    pub id: Uuid,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub content: String,
    pub properties: PropertyMap,
}

impl CanvasNode {
    pub fn new(x: f64, y: f64, content: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            x,
            y,
            width: 200.0,
            height: 100.0,
            content,
            properties: PropertyMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasEdge {
    pub id: Uuid,
    pub from: Uuid,
    pub to: Uuid,
    pub label: Option<String>,
    pub properties: PropertyMap,
}

impl CanvasEdge {
    pub fn new(from: Uuid, to: Uuid) -> Self {
        Self {
            id: Uuid::new_v4(),
            from,
            to,
            label: None,
            properties: PropertyMap::new(),
        }
    }
}

use crate::properties::PropertyMap;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_canvas_node_new() {
        let node = CanvasNode::new(100.0, 200.0, "Test Content".to_string());

        assert_eq!(node.x, 100.0);
        assert_eq!(node.y, 200.0);
        assert_eq!(node.width, 200.0);
        assert_eq!(node.height, 100.0);
        assert_eq!(node.content, "Test Content");
        assert!(node.properties.is_empty());
    }

    #[test]
    fn test_canvas_node_uuid_unique() {
        let node1 = CanvasNode::new(0.0, 0.0, "Node 1".to_string());
        let node2 = CanvasNode::new(0.0, 0.0, "Node 2".to_string());

        assert_ne!(node1.id, node2.id);
    }

    #[test]
    fn test_canvas_node_serialization() {
        let node = CanvasNode::new(50.0, 75.0, "Content".to_string());
        let json = serde_json::to_string(&node).unwrap();

        let deserialized: CanvasNode = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, node.id);
        assert_eq!(deserialized.x, node.x);
        assert_eq!(deserialized.y, node.y);
        assert_eq!(deserialized.content, node.content);
    }

    #[test]
    fn test_canvas_edge_new() {
        let from_id = Uuid::new_v4();
        let to_id = Uuid::new_v4();
        let edge = CanvasEdge::new(from_id, to_id);

        assert_eq!(edge.from, from_id);
        assert_eq!(edge.to, to_id);
        assert!(edge.label.is_none());
        assert!(edge.properties.is_empty());
    }

    #[test]
    fn test_canvas_edge_uuid_unique() {
        let from_id = Uuid::new_v4();
        let to_id = Uuid::new_v4();
        let edge1 = CanvasEdge::new(from_id, to_id);
        let edge2 = CanvasEdge::new(from_id, to_id);

        assert_ne!(edge1.id, edge2.id);
    }

    #[test]
    fn test_canvas_edge_serialization() {
        let from_id = Uuid::new_v4();
        let to_id = Uuid::new_v4();
        let edge = CanvasEdge::new(from_id, to_id);
        let json = serde_json::to_string(&edge).unwrap();

        let deserialized: CanvasEdge = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, edge.id);
        assert_eq!(deserialized.from, edge.from);
        assert_eq!(deserialized.to, edge.to);
    }
}
