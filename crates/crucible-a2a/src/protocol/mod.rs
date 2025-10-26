pub mod events;
/// A2A protocol message types and events
///
/// Defines typed messages for agent-to-agent communication with compile-time safety.
pub mod messages;

pub use events::SystemEvent;
pub use messages::{MessageEnvelope, TypedMessage};
