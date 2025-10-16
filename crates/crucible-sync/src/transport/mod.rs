//! Transport layer for CRDT synchronization
//!
//! This module provides different transport implementations for
//! sending and receiving CRDT updates between peers.

pub mod traits;
pub mod memory;

#[cfg(feature = "websocket")]
pub mod websocket;

// Re-export transport traits and basic implementations
pub use traits::Transport;
pub use memory::MemoryTransport;