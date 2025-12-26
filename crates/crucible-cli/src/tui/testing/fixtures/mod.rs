//! Test fixtures for TUI testing
//!
//! Provides reusable data for tests. Fixtures are plain functions returning data,
//! designed for composition and reuse across multiple test assertions.

pub mod events;
pub mod registries;
pub mod sessions;

pub use events::*;
pub use registries::*;
pub use sessions::*;
