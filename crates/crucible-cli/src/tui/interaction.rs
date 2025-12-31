//! Declarative interaction protocol
//!
//! Defines request/response primitives for agent-user interaction.
//! Renderer-agnostic - can be rendered in TUI, web, or via FFI.
//!
//! Types are re-exported from `crucible_core::interaction` for shared
//! use across TUI, web, and other frontends.

// Re-export all interaction types from core
pub use crucible_core::interaction::{
    ArtifactFormat, AskRequest, AskResponse, EditRequest, EditResponse, InteractionRequest,
    InteractionResponse, PermAction, PermRequest, PermResponse, PermissionScope, ShowRequest,
};
