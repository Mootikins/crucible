/// Typed message protocol for agent-to-agent communication
///
/// Provides compile-time type safety for inter-agent messages.
use crate::context::types::{AgentId, EntityId, MessageId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Message envelope wrapping typed content with routing metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageEnvelope {
    /// Unique message identifier
    pub message_id: MessageId,

    /// Source agent
    pub from: AgentId,

    /// Target agent (None = broadcast)
    pub to: Option<AgentId>,

    /// Unix timestamp
    pub timestamp: i64,

    /// Typed message content
    pub content: TypedMessage,
}

/// Typed message variants for agent communication
///
/// Each variant has structured fields for type safety and versioning.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TypedMessage {
    /// Assign a task to an agent
    TaskAssignment {
        task_id: String,
        description: String,
        requirements: TaskRequirements,
        deadline: Option<i64>,
    },

    /// Report task progress
    StatusUpdate {
        task_id: String,
        progress: f32,
        status: TaskStatus,
        artifacts: Vec<Artifact>,
        message: Option<String>,
    },

    /// Request coordination from another agent
    CoordinationRequest {
        request_id: String,
        request_type: CoordinationType,
        context: HashMap<String, serde_json::Value>,
    },

    /// Response to coordination request
    CoordinationResponse {
        request_id: String,
        accepted: bool,
        response_data: serde_json::Value,
    },

    /// Discover agent capabilities
    CapabilityQuery {
        query_id: String,
        required_capabilities: Vec<String>,
    },

    /// Advertise agent capabilities
    CapabilityAdvertisement {
        query_id: Option<String>,
        capabilities: Vec<Capability>,
        load_factor: f32,
    },

    /// Share context information
    ContextShare {
        entity_ids: Vec<EntityId>,
        message_ids: Vec<MessageId>,
        summary: Option<String>,
    },

    /// Request context pruning
    PruneRequest {
        target_tokens: usize,
        preserve_entities: Vec<EntityId>,
        strategy_hint: Option<String>,
    },

    /// Notify of pruning completion
    PruneComplete {
        pruned_count: usize,
        remaining_tokens: usize,
        summary: Option<String>,
    },
}

/// Task requirements specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRequirements {
    pub required_tools: Vec<String>,
    pub max_tokens: Option<usize>,
    pub priority: Priority,
    pub privacy_level: PrivacyLevel,
}

/// Task execution status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    InProgress,
    Blocked,
    Completed,
    Failed,
    Canceled,
}

/// Task priority level
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    Low,
    Normal,
    High,
    Critical,
}

/// Privacy level for task execution
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyLevel {
    Public,
    Internal,
    Confidential,
    Restricted,
}

/// Task artifact (output)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub artifact_type: ArtifactType,
    pub name: String,
    pub content: ArtifactContent,
    pub metadata: HashMap<String, String>,
}

/// Type of artifact produced
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactType {
    Text,
    Code,
    Data,
    Image,
    Document,
}

/// Artifact content (can be inline or reference)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ArtifactContent {
    Inline(String),
    Reference { uri: String, size_bytes: usize },
}

/// Coordination type for multi-agent workflows
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CoordinationType {
    DataExchange,
    TaskDelegation,
    ConsensusRequest,
    ResourceSharing,
    SynchronizationPoint,
}

/// Agent capability descriptor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capability {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub parameters: HashMap<String, serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_assignment_serialization() {
        let msg = TypedMessage::TaskAssignment {
            task_id: "task_001".to_string(),
            description: "Analyze data".to_string(),
            requirements: TaskRequirements {
                required_tools: vec!["python".to_string()],
                max_tokens: Some(4000),
                priority: Priority::High,
                privacy_level: PrivacyLevel::Internal,
            },
            deadline: Some(1697500000),
        };

        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: TypedMessage = serde_json::from_str(&json).unwrap();

        match deserialized {
            TypedMessage::TaskAssignment { task_id, .. } => {
                assert_eq!(task_id, "task_001");
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_message_envelope_serialization() {
        let envelope = MessageEnvelope {
            message_id: 1,
            from: "agent_1".to_string(),
            to: Some("agent_2".to_string()),
            timestamp: 1697500000,
            content: TypedMessage::StatusUpdate {
                task_id: "task_001".to_string(),
                progress: 0.5,
                status: TaskStatus::InProgress,
                artifacts: vec![],
                message: Some("Halfway done".to_string()),
            },
        };

        let json = serde_json::to_string(&envelope).unwrap();
        let deserialized: MessageEnvelope = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.message_id, 1);
        assert_eq!(deserialized.from, "agent_1");
        assert_eq!(deserialized.to, Some("agent_2".to_string()));
    }

    #[test]
    fn test_capability_advertisement() {
        let msg = TypedMessage::CapabilityAdvertisement {
            query_id: Some("query_1".to_string()),
            capabilities: vec![Capability {
                name: "code_analysis".to_string(),
                version: "1.0.0".to_string(),
                description: Some("Analyze code quality".to_string()),
                parameters: HashMap::new(),
            }],
            load_factor: 0.3,
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("capability_advertisement"));
    }

    #[test]
    fn test_coordination_request() {
        let mut context = HashMap::new();
        context.insert(
            "shared_data".to_string(),
            serde_json::json!({"key": "value"}),
        );

        let msg = TypedMessage::CoordinationRequest {
            request_id: "coord_001".to_string(),
            request_type: CoordinationType::DataExchange,
            context,
        };

        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: TypedMessage = serde_json::from_str(&json).unwrap();

        match deserialized {
            TypedMessage::CoordinationRequest { request_type, .. } => {
                assert_eq!(request_type, CoordinationType::DataExchange);
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_priority_ordering() {
        assert!(Priority::Critical > Priority::High);
        assert!(Priority::High > Priority::Normal);
        assert!(Priority::Normal > Priority::Low);
    }

    #[test]
    fn test_task_status_variants() {
        let statuses = vec![
            TaskStatus::Pending,
            TaskStatus::InProgress,
            TaskStatus::Blocked,
            TaskStatus::Completed,
            TaskStatus::Failed,
            TaskStatus::Canceled,
        ];

        assert_eq!(statuses.len(), 6);
    }

    #[test]
    fn test_artifact_inline_content() {
        let artifact = Artifact {
            artifact_type: ArtifactType::Code,
            name: "solution.py".to_string(),
            content: ArtifactContent::Inline("def solve(): pass".to_string()),
            metadata: HashMap::new(),
        };

        let json = serde_json::to_string(&artifact).unwrap();
        let deserialized: Artifact = serde_json::from_str(&json).unwrap();

        match deserialized.content {
            ArtifactContent::Inline(code) => assert!(code.contains("solve")),
            _ => panic!("Expected inline content"),
        }
    }

    #[test]
    fn test_artifact_reference_content() {
        let artifact = Artifact {
            artifact_type: ArtifactType::Image,
            name: "diagram.png".to_string(),
            content: ArtifactContent::Reference {
                uri: "file:///tmp/diagram.png".to_string(),
                size_bytes: 1024,
            },
            metadata: HashMap::new(),
        };

        let json = serde_json::to_string(&artifact).unwrap();
        assert!(json.contains("uri"));
        assert!(json.contains("size_bytes"));
    }
}
