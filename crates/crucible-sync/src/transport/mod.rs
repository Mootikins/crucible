//! Transport layer for CRDT synchronization
//!
//! This module provides different transport implementations for
//! sending and receiving CRDT updates between peers.

pub mod memory;
pub mod traits;

// pub mod websocket; // Disabled until implementation is complete

// Re-export transport traits and basic implementations
pub use memory::MemoryTransport;
pub use traits::Transport;
