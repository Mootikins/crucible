/// Core types for A2A context management
///
/// This module defines the fundamental types for tracking messages, entities,
/// and agent interactions across multi-agent conversations.
use fixedbitset::FixedBitSet;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Simple u64 message identifier
///
/// Sequential allocation per agent. Can upgrade to Uuid for distributed systems.
pub type MessageId = u64;

/// Human-readable agent identifier
///
/// Matches agent markdown file names (e.g., "researcher_001", "sony-backend")
pub type AgentId = String;

/// Internal entity identifier
///
/// Bidirectional mapping to entity names for fast lookup and comparison.
pub type EntityId = u64;

/// Metadata for a single message in the conversation
///
/// Lightweight structure for fast lookups and importance scoring.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessageMetadata {
    pub message_id: MessageId,
    pub agent_id: AgentId,
    pub timestamp: i64,   // Unix timestamp
    pub token_count: u32, // Approximate: len / 4
    pub entity_ids: Vec<EntityId>,
    pub reference_count: usize, // How many messages reference this
    pub access_count: u32,      // How often accessed
    pub parent_id: Option<MessageId>,
}

/// Per-agent context window with message tracking
///
/// Uses BitSet for O(1) pin checks and maintains entity index for entity-focused strategies.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextWindow {
    pub agent_id: AgentId,
    pub messages: Vec<MessageId>,
    pub total_tokens: usize,
    #[serde(skip)] // FixedBitSet doesn't implement Serialize
    pub pinned_messages: FixedBitSet,
    pub entity_coverage: HashMap<EntityId, Vec<MessageId>>,
}

/// Decision output from pruning strategies
///
/// Indicates which messages to keep/prune and optional summarization request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PruningDecision {
    #[serde(skip)] // FixedBitSet doesn't implement Serialize
    pub keep_messages: FixedBitSet,
    pub pruned_messages: Vec<MessageId>,
    pub reason_codes: HashMap<MessageId, PruneReason>,
    pub summary_needed: Option<SummaryRequest>,
}

/// Request for message summarization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummaryRequest {
    pub messages: Vec<MessageId>,
    pub max_tokens: usize,
}

/// Reason why a message was pruned
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PruneReason {
    LowAttention(f32),
    BeyondWindow,
    Redundant { similar_to: MessageId },
    LowReferenceCount,
    EntityNoLongerRelevant,
    AgentNotSubscribed,
    Custom(String), // From Rune strategies
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_metadata_creation() {
        let metadata = MessageMetadata {
            message_id: 1,
            agent_id: "test_agent".to_string(),
            timestamp: 1697500000,
            token_count: 100,
            entity_ids: vec![1, 2, 3],
            reference_count: 0,
            access_count: 0,
            parent_id: None,
        };

        assert_eq!(metadata.message_id, 1);
        assert_eq!(metadata.agent_id, "test_agent");
        assert_eq!(metadata.token_count, 100);
        assert_eq!(metadata.entity_ids.len(), 3);
    }

    #[test]
    fn test_message_metadata_with_parent() {
        let metadata = MessageMetadata {
            message_id: 2,
            agent_id: "test_agent".to_string(),
            timestamp: 1697500001,
            token_count: 50,
            entity_ids: vec![],
            reference_count: 0,
            access_count: 0,
            parent_id: Some(1),
        };

        assert_eq!(metadata.parent_id, Some(1));
    }

    #[test]
    fn test_context_window_creation() {
        let window = ContextWindow {
            agent_id: "agent_1".to_string(),
            messages: vec![1, 2, 3],
            total_tokens: 300,
            pinned_messages: FixedBitSet::with_capacity(10),
            entity_coverage: HashMap::new(),
        };

        assert_eq!(window.agent_id, "agent_1");
        assert_eq!(window.messages.len(), 3);
        assert_eq!(window.total_tokens, 300);
    }

    #[test]
    fn test_context_window_pinning() {
        let mut window = ContextWindow {
            agent_id: "agent_1".to_string(),
            messages: vec![1, 2, 3],
            total_tokens: 300,
            pinned_messages: FixedBitSet::with_capacity(10),
            entity_coverage: HashMap::new(),
        };

        // Pin message at index 1
        window.pinned_messages.insert(1);

        assert!(window.pinned_messages.contains(1));
        assert!(!window.pinned_messages.contains(0));
        assert!(!window.pinned_messages.contains(2));
    }

    #[test]
    fn test_context_window_entity_coverage() {
        let mut entity_coverage = HashMap::new();
        entity_coverage.insert(1, vec![1, 2]);
        entity_coverage.insert(2, vec![2, 3]);

        let window = ContextWindow {
            agent_id: "agent_1".to_string(),
            messages: vec![1, 2, 3],
            total_tokens: 300,
            pinned_messages: FixedBitSet::with_capacity(10),
            entity_coverage,
        };

        assert_eq!(window.entity_coverage.get(&1), Some(&vec![1, 2]));
        assert_eq!(window.entity_coverage.get(&2), Some(&vec![2, 3]));
        assert_eq!(window.entity_coverage.len(), 2);
    }

    #[test]
    fn test_pruning_decision_creation() {
        let mut keep_messages = FixedBitSet::with_capacity(5);
        keep_messages.insert(0);
        keep_messages.insert(2);

        let decision = PruningDecision {
            keep_messages,
            pruned_messages: vec![1, 3, 4],
            reason_codes: HashMap::new(),
            summary_needed: None,
        };

        assert_eq!(decision.pruned_messages.len(), 3);
        assert!(decision.keep_messages.contains(0));
        assert!(decision.keep_messages.contains(2));
    }

    #[test]
    fn test_prune_reason_variants() {
        let reasons = vec![
            PruneReason::LowAttention(0.3),
            PruneReason::BeyondWindow,
            PruneReason::Redundant { similar_to: 5 },
            PruneReason::LowReferenceCount,
            PruneReason::EntityNoLongerRelevant,
            PruneReason::AgentNotSubscribed,
            PruneReason::Custom("custom reason".to_string()),
        ];

        assert_eq!(reasons.len(), 7);

        if let PruneReason::LowAttention(score) = reasons[0] {
            assert_eq!(score, 0.3);
        } else {
            panic!("Expected LowAttention variant");
        }
    }

    #[test]
    fn test_summary_request() {
        let summary = SummaryRequest {
            messages: vec![1, 2, 3, 4],
            max_tokens: 500,
        };

        assert_eq!(summary.messages.len(), 4);
        assert_eq!(summary.max_tokens, 500);
    }

    #[test]
    fn test_message_metadata_serialization() {
        let metadata = MessageMetadata {
            message_id: 1,
            agent_id: "test".to_string(),
            timestamp: 1697500000,
            token_count: 100,
            entity_ids: vec![1, 2],
            reference_count: 5,
            access_count: 10,
            parent_id: None,
        };

        let json = serde_json::to_string(&metadata).unwrap();
        let deserialized: MessageMetadata = serde_json::from_str(&json).unwrap();

        assert_eq!(metadata, deserialized);
    }
}
