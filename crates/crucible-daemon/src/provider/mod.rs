//! Provider module for genai integration
//!
//! Maps Crucible's `BackendType` to genai's `AdapterKind` and provides
//! utilities for building genai clients with proper authentication and
//! service target resolution.

pub mod adapter_mapping;
pub mod genai_handle;
pub mod model_listing;
pub mod tool_bridge;
pub mod copilot;

pub use adapter_mapping::{backend_to_adapter, build_genai_client, build_model_iden};
