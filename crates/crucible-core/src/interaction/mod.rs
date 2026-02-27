//! Interaction protocol primitives for agent-user communication.
//!
//! This module defines request/response types for structured interactions between
//! agents and users. These primitives are renderer-agnostic and can be used by
//! TUI, web, or other frontends.
//!
//! # Request Types
//!
//! - [`AskRequest`] - Questions with optional choices (single/multi-select)
//! - [`PermRequest`] - Permission requests with token-based pattern building
//! - [`EditRequest`] - Artifact editing with format hints
//! - [`ShowRequest`] - Display content (no response needed)
//!
//! # Example
//!
//! ```
//! use crucible_core::interaction::{AskRequest, AskResponse, PermRequest, PermissionScope};
//!
//! // Create a question with choices
//! let ask = AskRequest::new("Which option?")
//!     .choices(["Option A", "Option B", "Option C"])
//!     .allow_other();
//!
//! // Create a permission request
//! let perm = PermRequest::bash(["npm", "install", "lodash"]);
//! assert_eq!(perm.pattern_at(2), "npm install *");
//! ```

mod ask;
mod edit;
mod permission;
mod types;

pub use ask::*;
pub use edit::*;
pub use permission::*;
pub use types::*;
