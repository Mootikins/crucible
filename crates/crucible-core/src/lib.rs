pub mod document;
pub mod canvas;
pub mod properties;
pub mod crdt;

pub use document::{DocumentNode, ViewportState};
pub use canvas::{CanvasNode, CanvasEdge};
pub use properties::{PropertyMap, PropertyValue};

#[derive(Debug, thiserror::Error)]
pub enum CrucibleError {
    #[error("Document not found: {0}")]
    DocumentNotFound(uuid::Uuid),
    
    #[error("Invalid operation: {0}")]
    InvalidOperation(String),
    
    #[error("CRDT error: {0}")]
    CrdtError(#[from] yrs::Error),
}

pub type Result<T> = std::result::Result<T, CrucibleError>;

