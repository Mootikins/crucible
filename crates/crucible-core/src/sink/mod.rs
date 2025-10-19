//! Output sink infrastructure for the parsing pipeline
//!
//! This module provides traits and implementations for distributing parsed documents
//! to multiple destinations (database, logger, etc.) with backpressure handling
//! and fault isolation.

pub mod traits;
pub mod error;
pub mod circuit_breaker;

pub use traits::{OutputSink, SinkHealth};
pub use error::{SinkError, SinkResult};
pub use circuit_breaker::{CircuitBreaker, CircuitState, CircuitBreakerConfig};
