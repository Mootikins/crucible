//! Common utilities and shared components for Crucible CLI

pub mod kiln_processor;
pub mod tool_manager;

pub use kiln_processor::{KilnProcessor, ProcessingResult};
pub use tool_manager::{ToolRegistry, CrucibleToolManager, ToolDefinition};
