/// Message bus middleware integrating transport and context tracking
///
/// Provides unified message routing with automatic metadata extraction and storage.

use crate::bus::EntityExtractor;
use crate::context::{AgentId, MessageMetadata, MessageMetadataStore};
use crate::protocol::{MessageEnvelope, SystemEvent, TypedMessage};
use crate::transport::{AgentHandle, LocalAgentBus, Result};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Message bus that routes messages and tracks metadata
///
/// Combines LocalAgentBus (transport) with MessageMetadataStore (context tracking).
pub struct MessageBus {
    /// Transport layer
    transport: LocalAgentBus,

    /// Context metadata store (shared, thread-safe)
    store: Arc<RwLock<MessageMetadataStore>>,

    /// Entity extractor
    extractor: EntityExtractor,
}

impl MessageBus {
    /// Create a new message bus
    pub fn new() -> Self {
        Self {
            transport: LocalAgentBus::new(),
            store: Arc::new(RwLock::new(MessageMetadataStore::new())),
            extractor: EntityExtractor::new(),
        }
    }

    /// Register a new agent
    pub fn register_agent(&mut self, agent_id: AgentId) -> Result<AgentHandle> {
        self.transport.register_agent(agent_id)
    }

    /// Unregister an agent
    pub fn unregister_agent(&mut self, agent_id: &AgentId) -> Result<()> {
        self.transport.unregister_agent(agent_id)
    }

    /// Send a message with automatic metadata tracking
    ///
    /// Extracts entities, creates metadata, and stores in context before sending.
    pub async fn send(&mut self, to: &AgentId, envelope: MessageEnvelope) -> Result<()> {
        // Extract message content as text for entity extraction
        let content_text = self.extract_text_from_message(&envelope.content);

        // Extract entities from content
        let entity_names = self.extractor.extract(&content_text);

        // Get or create entity IDs
        let mut store = self.store.write().await;
        let entity_ids: Vec<_> = entity_names
            .iter()
            .map(|name| store.get_or_create_entity(name))
            .collect();

        // Estimate token count (char count / 4)
        let token_count = (content_text.len() / 4) as u32;

        // Create metadata
        let metadata = MessageMetadata {
            message_id: envelope.message_id,
            agent_id: envelope.from.clone(),
            timestamp: envelope.timestamp,
            token_count,
            entity_ids,
            reference_count: 0,
            access_count: 0,
            parent_id: None, // TODO: Track parent messages
        };

        // Store metadata
        store.insert(metadata);

        // Release write lock before sending
        drop(store);

        // Send via transport
        self.transport.send(to, envelope)
    }

    /// Receive next message for an agent
    ///
    /// Automatically increments access count in metadata.
    pub async fn recv(&mut self, from: &AgentId) -> Result<MessageEnvelope> {
        let envelope = self.transport.recv(from).await?;

        // Increment access count
        let mut store = self.store.write().await;
        store.increment_access_count(envelope.message_id);

        Ok(envelope)
    }

    /// Broadcast a system event
    pub fn broadcast(&self, event: SystemEvent) -> Result<()> {
        self.transport.broadcast(event)
    }

    /// Get a shared reference to the metadata store
    pub fn store(&self) -> Arc<RwLock<MessageMetadataStore>> {
        Arc::clone(&self.store)
    }

    /// Get count of registered agents
    pub fn agent_count(&self) -> usize {
        self.transport.agent_count()
    }

    /// Check if an agent is registered
    pub fn is_registered(&self, agent_id: &AgentId) -> bool {
        self.transport.is_registered(agent_id)
    }

    /// Get a handle for an agent
    pub fn get_handle(&self, agent_id: &AgentId) -> Result<AgentHandle> {
        self.transport.get_handle(agent_id)
    }

    /// Allocate next message ID
    pub fn next_message_id(&mut self) -> u64 {
        self.transport.next_message_id()
    }

    /// Extract text content from a TypedMessage for entity extraction
    fn extract_text_from_message(&self, message: &TypedMessage) -> String {
        match message {
            TypedMessage::TaskAssignment { description, .. } => description.clone(),

            TypedMessage::StatusUpdate { message, .. } => {
                message.clone().unwrap_or_default()
            }

            TypedMessage::CoordinationRequest { context, .. } => {
                // Extract text from context values
                serde_json::to_string(context).unwrap_or_default()
            }

            TypedMessage::CoordinationResponse { response_data, .. } => {
                serde_json::to_string(response_data).unwrap_or_default()
            }

            TypedMessage::ContextShare { summary, .. } => {
                summary.clone().unwrap_or_default()
            }

            TypedMessage::PruneRequest { strategy_hint, .. } => {
                strategy_hint.clone().unwrap_or_default()
            }

            TypedMessage::PruneComplete { summary, .. } => {
                summary.clone().unwrap_or_default()
            }

            // These messages don't have extractable text content
            TypedMessage::CapabilityQuery { .. }
            | TypedMessage::CapabilityAdvertisement { .. } => String::new(),
        }
    }
}

impl Default for MessageBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::messages::{Priority, PrivacyLevel, TaskRequirements, TaskStatus};

    fn create_task_assignment(task_id: &str, description: &str) -> TypedMessage {
        TypedMessage::TaskAssignment {
            task_id: task_id.to_string(),
            description: description.to_string(),
            requirements: TaskRequirements {
                required_tools: vec![],
                max_tokens: Some(4000),
                priority: Priority::Normal,
                privacy_level: PrivacyLevel::Internal,
            },
            deadline: None,
        }
    }

    fn create_envelope(
        message_id: u64,
        from: &str,
        to: Option<&str>,
        content: TypedMessage,
    ) -> MessageEnvelope {
        MessageEnvelope {
            message_id,
            from: from.to_string(),
            to: to.map(|s| s.to_string()),
            timestamp: 1697500000,
            content,
        }
    }

    #[tokio::test]
    async fn test_send_and_track_metadata() {
        let mut bus = MessageBus::new();
        bus.register_agent("agent_1".to_string()).unwrap();
        bus.register_agent("agent_2".to_string()).unwrap();

        let content = create_task_assignment(
            "task_001",
            "Work on #project-alpha using src/main.rs",
        );
        let envelope = create_envelope(1, "agent_1", Some("agent_2"), content);

        bus.send(&"agent_2".to_string(), envelope).await.unwrap();

        // Check metadata was stored
        let store = bus.store.read().await;
        let metadata = store.get(1).unwrap();

        assert_eq!(metadata.message_id, 1);
        assert_eq!(metadata.agent_id, "agent_1");
        assert!(metadata.token_count > 0);

        // Check entities were extracted
        assert!(!metadata.entity_ids.is_empty());

        // Verify specific entities
        let project_entity = store.get_entity_id("#project-alpha");
        let file_entity = store.get_entity_id("src/main.rs");

        assert!(project_entity.is_some());
        assert!(file_entity.is_some());
        assert!(metadata.entity_ids.contains(&project_entity.unwrap()));
        assert!(metadata.entity_ids.contains(&file_entity.unwrap()));
    }

    #[tokio::test]
    async fn test_recv_increments_access_count() {
        let mut bus = MessageBus::new();
        bus.register_agent("agent_1".to_string()).unwrap();

        let content = create_task_assignment("task_001", "Simple task");
        let envelope = create_envelope(1, "sender", Some("agent_1"), content);

        // Send via handle (bypass bus tracking for this test)
        let handle = bus.get_handle(&"agent_1".to_string()).unwrap();
        handle.send(envelope).unwrap();

        // Manually add metadata
        {
            let mut store = bus.store.write().await;
            store.insert(MessageMetadata {
                message_id: 1,
                agent_id: "sender".to_string(),
                timestamp: 1697500000,
                token_count: 10,
                entity_ids: vec![],
                reference_count: 0,
                access_count: 0,
                parent_id: None,
            });
        }

        // Receive message
        bus.recv(&"agent_1".to_string()).await.unwrap();

        // Check access count increased
        let store = bus.store.read().await;
        let metadata = store.get(1).unwrap();
        assert_eq!(metadata.access_count, 1);
    }

    #[tokio::test]
    async fn test_entity_extraction_from_status_update() {
        let mut bus = MessageBus::new();
        bus.register_agent("agent_1".to_string()).unwrap();
        bus.register_agent("agent_2".to_string()).unwrap();

        let content = TypedMessage::StatusUpdate {
            task_id: "task_001".to_string(),
            progress: 0.5,
            status: TaskStatus::InProgress,
            artifacts: vec![],
            message: Some("Updated @backend-agent about ProjectDelta progress".to_string()),
        };

        let envelope = create_envelope(1, "agent_1", Some("agent_2"), content);

        bus.send(&"agent_2".to_string(), envelope).await.unwrap();

        // Check entities were extracted from the status message
        let store = bus.store.read().await;
        let metadata = store.get(1).unwrap();

        assert!(!metadata.entity_ids.is_empty());

        // Verify specific entities
        let backend_entity = store.get_entity_id("@backend-agent");
        let project_entity = store.get_entity_id("ProjectDelta");

        assert!(backend_entity.is_some());
        assert!(project_entity.is_some());
    }

    #[tokio::test]
    async fn test_shared_store_access() {
        let bus = MessageBus::new();

        let store_ref = bus.store();
        let mut store = store_ref.write().await;

        let entity_id = store.get_or_create_entity("#test-entity");
        assert_eq!(entity_id, 1);

        drop(store);

        // Can get another reference
        let store2 = bus.store();
        let store_read = store2.read().await;
        let retrieved_id = store_read.get_entity_id("#test-entity");
        assert_eq!(retrieved_id, Some(1));
    }

    #[tokio::test]
    async fn test_multiple_messages_with_shared_entities() {
        let mut bus = MessageBus::new();
        bus.register_agent("agent_1".to_string()).unwrap();
        bus.register_agent("agent_2".to_string()).unwrap();

        // Send first message mentioning #project-alpha
        let content1 = create_task_assignment("task_001", "Starting #project-alpha work");
        let envelope1 = create_envelope(1, "agent_1", Some("agent_2"), content1);
        bus.send(&"agent_2".to_string(), envelope1).await.unwrap();

        // Send second message also mentioning #project-alpha
        let content2 = create_task_assignment("task_002", "Continue #project-alpha development");
        let envelope2 = create_envelope(2, "agent_1", Some("agent_2"), content2);
        bus.send(&"agent_2".to_string(), envelope2).await.unwrap();

        // Both messages should share the same entity ID
        let store = bus.store.read().await;
        let metadata1 = store.get(1).unwrap();
        let metadata2 = store.get(2).unwrap();

        let project_entity_id = store.get_entity_id("#project-alpha").unwrap();

        assert!(metadata1.entity_ids.contains(&project_entity_id));
        assert!(metadata2.entity_ids.contains(&project_entity_id));

        // Check entity index shows both messages
        let messages_with_entity = store.get_by_entity(project_entity_id);
        assert_eq!(messages_with_entity.len(), 2);
    }

    #[tokio::test]
    async fn test_token_count_estimation() {
        let mut bus = MessageBus::new();
        bus.register_agent("agent_1".to_string()).unwrap();
        bus.register_agent("agent_2".to_string()).unwrap();

        // Create a message with known length
        let description = "a".repeat(400); // 400 characters
        let content = create_task_assignment("task_001", &description);
        let envelope = create_envelope(1, "agent_1", Some("agent_2"), content);

        bus.send(&"agent_2".to_string(), envelope).await.unwrap();

        // Check token count (should be ~100 tokens for 400 chars)
        let store = bus.store.read().await;
        let metadata = store.get(1).unwrap();

        assert_eq!(metadata.token_count, 100); // 400 / 4 = 100
    }
}
