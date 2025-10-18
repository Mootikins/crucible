/// Transport layer for agent communication
///
/// Concrete implementations for local and remote agent messaging.

pub mod local;
pub mod error;

pub use local::{LocalAgentBus, AgentHandle};
pub use error::TransportError;

pub type Result<T> = std::result::Result<T, TransportError>;
