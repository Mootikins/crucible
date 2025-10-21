//! # Event System for Centralized Daemon Coordination
//!
//! This module provides a comprehensive event system for coordinating services
//! through the central daemon. It defines event types, routing logic, and
//! error handling mechanisms.

pub mod core;
pub mod routing;
pub mod service_events;
pub mod errors;

// Re-export main components
pub use core::*;
pub use routing::*;
pub use service_events::*;
pub use errors::*;