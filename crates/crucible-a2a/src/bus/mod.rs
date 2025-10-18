/// Message bus middleware integrating transport and context tracking
///
/// Provides a unified interface that routes messages while maintaining metadata.

pub mod message_bus;
pub mod entity_extractor;

pub use message_bus::MessageBus;
pub use entity_extractor::EntityExtractor;
