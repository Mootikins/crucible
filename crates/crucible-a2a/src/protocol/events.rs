/// System events for agent lifecycle and coordination
///
/// Broadcast events for discovery, health, and system-level notifications.

use crate::context::types::AgentId;
use serde::{Deserialize, Serialize};

/// System-level events broadcast to all agents
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event_type", rename_all = "snake_case")]
pub enum SystemEvent {
    /// Agent joined the system
    AgentJoined {
        agent_id: AgentId,
        capabilities: Vec<String>,
        timestamp: i64,
    },

    /// Agent left the system
    AgentLeft {
        agent_id: AgentId,
        reason: Option<String>,
        timestamp: i64,
    },

    /// Agent health check heartbeat
    Heartbeat {
        agent_id: AgentId,
        load_factor: f32,
        timestamp: i64,
    },

    /// System-wide context pruning initiated
    GlobalPruneInitiated {
        target_token_reduction: usize,
        coordinator: AgentId,
        timestamp: i64,
    },

    /// Emergency shutdown signal
    Shutdown {
        reason: String,
        timestamp: i64,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_joined_event() {
        let event = SystemEvent::AgentJoined {
            agent_id: "agent_1".to_string(),
            capabilities: vec!["analysis".to_string(), "coding".to_string()],
            timestamp: 1697500000,
        };

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: SystemEvent = serde_json::from_str(&json).unwrap();

        match deserialized {
            SystemEvent::AgentJoined { agent_id, capabilities, .. } => {
                assert_eq!(agent_id, "agent_1");
                assert_eq!(capabilities.len(), 2);
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[test]
    fn test_heartbeat_event() {
        let event = SystemEvent::Heartbeat {
            agent_id: "agent_2".to_string(),
            load_factor: 0.7,
            timestamp: 1697500001,
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("heartbeat"));
    }

    #[test]
    fn test_shutdown_event() {
        let event = SystemEvent::Shutdown {
            reason: "Maintenance window".to_string(),
            timestamp: 1697500002,
        };

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: SystemEvent = serde_json::from_str(&json).unwrap();

        match deserialized {
            SystemEvent::Shutdown { reason, .. } => {
                assert_eq!(reason, "Maintenance window");
            }
            _ => panic!("Wrong event type"),
        }
    }
}
