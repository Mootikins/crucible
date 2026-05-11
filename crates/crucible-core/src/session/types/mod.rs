//! Core session types.

mod agent;
mod config;
mod enums;
mod session;
mod summary;

#[cfg(test)]
mod tests;

pub use agent::SessionAgent;
pub use config::{validate_output, ContextStrategy, OutputValidation};
pub use enums::{EndReason, RecordingMode, SessionState, SessionType};
pub use session::Session;
pub use summary::SessionSummary;
