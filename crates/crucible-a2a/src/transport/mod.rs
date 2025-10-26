pub mod error;
/// Transport layer for agent communication
///
/// Concrete implementations for local and remote agent messaging.
pub mod local;

pub use error::TransportError;
pub use local::{AgentHandle, LocalAgentBus};

pub type Result<T> = std::result::Result<T, TransportError>;
