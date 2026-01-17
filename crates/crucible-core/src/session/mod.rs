//! Session types for Crucible.
//!
//! A session is a continuous sequence of agent actions in a workspace.
//! Sessions are the fundamental unit of agent interaction in Crucible.
//!
//! # Key Concepts
//!
//! - **Session**: A sequence of agent actions, stored in a kiln
//! - **Workspace**: Where the agent operates (file I/O happens here)
//! - **Kiln**: Where the session is stored (logs, artifacts, history)
//! - **Connected Kilns**: Additional knowledge stores the session can query
//!
//! # Example
//!
//! ```ignore
//! use crucible_core::session::{Session, SessionType, SessionState};
//! use std::path::PathBuf;
//!
//! let session = Session::new(
//!     SessionType::Chat,
//!     PathBuf::from("/home/user/notes"),  // kiln
//! )
//! .with_workspace(PathBuf::from("/home/user/project"))
//! .with_connected_kiln(PathBuf::from("/home/user/reference"));
//! ```

mod types;

pub use types::{Session, SessionAgent, SessionState, SessionSummary, SessionType};
