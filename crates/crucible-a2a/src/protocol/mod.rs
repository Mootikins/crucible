/// A2A protocol message types and events
///
/// Defines typed messages for agent-to-agent communication with compile-time safety.

pub mod messages;
pub mod events;

pub use messages::{TypedMessage, MessageEnvelope};
pub use events::SystemEvent;
