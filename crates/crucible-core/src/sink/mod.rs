//! Output sink infrastructure for the parsing pipeline
//!
//! This module provides traits and implementations for distributing parsed documents
//! to multiple destinations (database, logger, etc.) with backpressure handling
//! and fault isolation.

pub mod circuit_breaker;
pub mod error;
pub mod traits;

pub use circuit_breaker::{CircuitBreaker, CircuitBreakerConfig, CircuitState};
pub use error::{SinkError, SinkResult};
pub use traits::{OutputSink, SinkHealth};
