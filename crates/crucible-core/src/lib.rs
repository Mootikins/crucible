pub mod canvas;
pub mod crdt;
pub mod document;
pub mod properties;

pub use canvas::{CanvasEdge, CanvasNode};
pub use document::{DocumentNode, ViewportState};
pub use properties::{PropertyMap, PropertyValue};

#[derive(Debug, thiserror::Error)]
pub enum CrucibleError {
    #[error("Document not found: {0}")]
    DocumentNotFound(uuid::Uuid),

    #[error("Invalid operation: {0}")]
    InvalidOperation(String),

    #[error("CRDT error: {0}")]
    CrdtError(String),
}

pub type Result<T> = std::result::Result<T, CrucibleError>;
