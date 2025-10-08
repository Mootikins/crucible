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
