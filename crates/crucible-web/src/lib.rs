//! # Crucible Web - Browser Interface for ACP Agent Chat
//!
//! This crate provides a web-based chat interface for Crucible's ACP agents.
//! It uses Axum for HTTP/SSE and tokio channels for internal communication.
//!
//! ## Architecture
//!
//! ```text
//! Browser ←─SSE─→ Axum Handler ←─broadcast─→ ChatService ←─ACP─→ Agent
//! ```
//!
//! ## Usage
//!
//! ```rust,ignore
//! use crucible_web::{WebConfig, start_server};
//!
//! let config = WebConfig::default();
//! start_server(config).await?;
//! ```

pub mod routes;
pub mod server;
pub mod services;

mod assets;
mod error;
mod events;

pub use error::{WebError, Result};
pub use events::ChatEvent;
pub use server::{WebConfig, start_server};
