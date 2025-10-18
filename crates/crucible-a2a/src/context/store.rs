/// Message metadata storage and indexing
///
/// Provides efficient storage and lookup for message metadata with entity tracking.

use crate::context::types::{AgentId, EntityId, MessageId, MessageMetadata};
use std::collections::HashMap;

/// Storage and indexing for message metadata
///
/// Provides fast lookups by message ID, agent, and entity.
pub struct MessageMetadataStore {
    /// All message metadata by ID
    messages: HashMap<MessageId, MessageMetadata>,

    /// Index: agent_id -> message_ids
    agent_index: HashMap<AgentId, Vec<MessageId>>,

    /// Index: entity_id -> message_ids
    entity_index: HashMap<EntityId, Vec<MessageId>>,

    /// Bidirectional entity mapping
    entity_names: HashMap<EntityId, String>,
    entity_ids: HashMap<String, EntityId>,

    /// Next entity ID to allocate
    next_entity_id: EntityId,
}

impl MessageMetadataStore {
    /// Create a new empty store
    pub fn new() -> Self {
        Self {
            messages: HashMap::new(),
            agent_index: HashMap::new(),
            entity_index: HashMap::new(),
            entity_names: HashMap::new(),
            entity_ids: HashMap::new(),
            next_entity_id: 1,
        }
    }

    /// Insert or update message metadata
    pub fn insert(&mut self, metadata: MessageMetadata) {
        let message_id = metadata.message_id;
        let agent_id = metadata.agent_id.clone();
        let entity_ids = metadata.entity_ids.clone();

        // Update agent index
        self.agent_index
            .entry(agent_id)
            .or_insert_with(Vec::new)
            .push(message_id);

        // Update entity index
        for entity_id in entity_ids {
            self.entity_index
                .entry(entity_id)
                .or_insert_with(Vec::new)
                .push(message_id);
        }

        // Store metadata
        self.messages.insert(message_id, metadata);
    }

    /// Get metadata by message ID
    pub fn get(&self, message_id: MessageId) -> Option<&MessageMetadata> {
        self.messages.get(&message_id)
    }

    /// Get mutable metadata by message ID
    pub fn get_mut(&mut self, message_id: MessageId) -> Option<&mut MessageMetadata> {
        self.messages.get_mut(&message_id)
    }

    /// Get all messages for an agent
    pub fn get_by_agent(&self, agent_id: &AgentId) -> Vec<&MessageMetadata> {
        self.agent_index
            .get(agent_id)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.messages.get(id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get all messages containing an entity
    pub fn get_by_entity(&self, entity_id: EntityId) -> Vec<&MessageMetadata> {
        self.entity_index
            .get(&entity_id)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.messages.get(id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Increment reference count for a message
    pub fn increment_reference_count(&mut self, message_id: MessageId) {
        if let Some(metadata) = self.messages.get_mut(&message_id) {
            metadata.reference_count += 1;
        }
    }

    /// Increment access count for a message
    pub fn increment_access_count(&mut self, message_id: MessageId) {
        if let Some(metadata) = self.messages.get_mut(&message_id) {
            metadata.access_count += 1;
        }
    }

    /// Get or create entity ID for a name
    pub fn get_or_create_entity(&mut self, name: &str) -> EntityId {
        if let Some(&id) = self.entity_ids.get(name) {
            return id;
        }

        let id = self.next_entity_id;
        self.next_entity_id += 1;
        self.entity_names.insert(id, name.to_string());
        self.entity_ids.insert(name.to_string(), id);
        id
    }

    /// Get entity name by ID
    pub fn get_entity_name(&self, entity_id: EntityId) -> Option<&str> {
        self.entity_names.get(&entity_id).map(|s| s.as_str())
    }

    /// Get entity ID by name
    pub fn get_entity_id(&self, name: &str) -> Option<EntityId> {
        self.entity_ids.get(name).copied()
    }

    /// Total number of messages stored
    pub fn len(&self) -> usize {
        self.messages.len()
    }

    /// Check if store is empty
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }
}

impl Default for MessageMetadataStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_metadata(message_id: MessageId, agent_id: &str, entity_ids: Vec<EntityId>) -> MessageMetadata {
        MessageMetadata {
            message_id,
            agent_id: agent_id.to_string(),
            timestamp: 1697500000,
            token_count: 100,
            entity_ids,
            reference_count: 0,
            access_count: 0,
            parent_id: None,
        }
    }

    #[test]
    fn test_store_creation() {
        let store = MessageMetadataStore::new();
        assert_eq!(store.len(), 0);
        assert!(store.is_empty());
    }

    #[test]
    fn test_insert_and_get() {
        let mut store = MessageMetadataStore::new();
        let metadata = create_test_metadata(1, "agent_1", vec![]);

        store.insert(metadata.clone());

        assert_eq!(store.len(), 1);
        assert!(!store.is_empty());

        let retrieved = store.get(1).unwrap();
        assert_eq!(retrieved.message_id, 1);
        assert_eq!(retrieved.agent_id, "agent_1");
    }

    #[test]
    fn test_get_by_agent() {
        let mut store = MessageMetadataStore::new();

        store.insert(create_test_metadata(1, "agent_1", vec![]));
        store.insert(create_test_metadata(2, "agent_1", vec![]));
        store.insert(create_test_metadata(3, "agent_2", vec![]));

        let agent1_messages = store.get_by_agent(&"agent_1".to_string());
        assert_eq!(agent1_messages.len(), 2);

        let agent2_messages = store.get_by_agent(&"agent_2".to_string());
        assert_eq!(agent2_messages.len(), 1);
    }

    #[test]
    fn test_get_by_entity() {
        let mut store = MessageMetadataStore::new();

        store.insert(create_test_metadata(1, "agent_1", vec![1, 2]));
        store.insert(create_test_metadata(2, "agent_1", vec![2, 3]));
        store.insert(create_test_metadata(3, "agent_2", vec![1]));

        let entity1_messages = store.get_by_entity(1);
        assert_eq!(entity1_messages.len(), 2);

        let entity2_messages = store.get_by_entity(2);
        assert_eq!(entity2_messages.len(), 2);

        let entity3_messages = store.get_by_entity(3);
        assert_eq!(entity3_messages.len(), 1);
    }

    #[test]
    fn test_increment_reference_count() {
        let mut store = MessageMetadataStore::new();
        store.insert(create_test_metadata(1, "agent_1", vec![]));

        assert_eq!(store.get(1).unwrap().reference_count, 0);

        store.increment_reference_count(1);
        assert_eq!(store.get(1).unwrap().reference_count, 1);

        store.increment_reference_count(1);
        assert_eq!(store.get(1).unwrap().reference_count, 2);
    }

    #[test]
    fn test_increment_access_count() {
        let mut store = MessageMetadataStore::new();
        store.insert(create_test_metadata(1, "agent_1", vec![]));

        assert_eq!(store.get(1).unwrap().access_count, 0);

        store.increment_access_count(1);
        assert_eq!(store.get(1).unwrap().access_count, 1);

        store.increment_access_count(1);
        assert_eq!(store.get(1).unwrap().access_count, 2);
    }

    #[test]
    fn test_entity_mapping() {
        let mut store = MessageMetadataStore::new();

        let id1 = store.get_or_create_entity("project_alpha");
        let id2 = store.get_or_create_entity("project_beta");
        let id1_again = store.get_or_create_entity("project_alpha");

        assert_eq!(id1, id1_again);
        assert_ne!(id1, id2);

        assert_eq!(store.get_entity_name(id1), Some("project_alpha"));
        assert_eq!(store.get_entity_name(id2), Some("project_beta"));

        assert_eq!(store.get_entity_id("project_alpha"), Some(id1));
        assert_eq!(store.get_entity_id("project_beta"), Some(id2));
    }

    #[test]
    fn test_entity_extraction_workflow() {
        let mut store = MessageMetadataStore::new();

        // Simulate extracting entities from a message
        let entity1 = store.get_or_create_entity("#project");
        let entity2 = store.get_or_create_entity("src/main.rs");

        let metadata = create_test_metadata(1, "agent_1", vec![entity1, entity2]);
        store.insert(metadata);

        // Query by entity
        let messages_with_project = store.get_by_entity(entity1);
        assert_eq!(messages_with_project.len(), 1);
        assert_eq!(messages_with_project[0].message_id, 1);
    }

    #[test]
    fn test_update_metadata() {
        let mut store = MessageMetadataStore::new();
        store.insert(create_test_metadata(1, "agent_1", vec![]));

        {
            let metadata = store.get_mut(1).unwrap();
            metadata.token_count = 200;
        }

        assert_eq!(store.get(1).unwrap().token_count, 200);
    }

    #[test]
    fn test_nonexistent_message() {
        let store = MessageMetadataStore::new();
        assert!(store.get(999).is_none());
    }

    #[test]
    fn test_nonexistent_entity() {
        let store = MessageMetadataStore::new();
        assert_eq!(store.get_entity_name(999), None);
        assert_eq!(store.get_entity_id("nonexistent"), None);
    }
}
