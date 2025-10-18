/// Local agent communication via Tokio channels
///
/// Provides in-process message passing between agents with zero network overhead.

use crate::context::types::AgentId;
use crate::protocol::{MessageEnvelope, SystemEvent};
use crate::transport::{Result, TransportError};
use std::collections::HashMap;
use tokio::sync::{broadcast, mpsc};

/// Local agent message bus using Tokio channels
///
/// Provides efficient in-process communication between agents.
pub struct LocalAgentBus {
    /// Per-agent mailboxes
    agents: HashMap<AgentId, AgentMailbox>,

    /// Broadcast channel for system events
    broadcast_tx: broadcast::Sender<SystemEvent>,

    /// Next message ID to allocate
    next_message_id: u64,
}

/// Agent mailbox with send/receive channels
struct AgentMailbox {
    /// Receiver for inbound messages (owned by mailbox)
    rx: mpsc::UnboundedReceiver<MessageEnvelope>,

    /// Sender handle (cloneable for distribution)
    tx: mpsc::UnboundedSender<MessageEnvelope>,
}

/// Handle for sending messages to a specific agent
///
/// Cloneable and can be shared across tasks.
#[derive(Clone)]
pub struct AgentHandle {
    pub agent_id: AgentId,
    tx: mpsc::UnboundedSender<MessageEnvelope>,
}

impl AgentHandle {
    /// Send a message to this agent
    pub fn send(&self, message: MessageEnvelope) -> Result<()> {
        self.tx
            .send(message)
            .map_err(|_| TransportError::ChannelClosed {
                agent_id: self.agent_id.clone(),
            })
    }
}

impl LocalAgentBus {
    /// Create a new local agent bus
    pub fn new() -> Self {
        let (broadcast_tx, _) = broadcast::channel(100);

        Self {
            agents: HashMap::new(),
            broadcast_tx,
            next_message_id: 1,
        }
    }

    /// Register a new agent and return a handle
    ///
    /// Returns an error if the agent is already registered.
    pub fn register_agent(&mut self, agent_id: AgentId) -> Result<AgentHandle> {
        if self.agents.contains_key(&agent_id) {
            return Err(TransportError::AlreadyRegistered {
                agent_id: agent_id.clone(),
            });
        }

        let (tx, rx) = mpsc::unbounded_channel();
        let mailbox = AgentMailbox {
            rx,
            tx: tx.clone(),
        };

        self.agents.insert(agent_id.clone(), mailbox);

        Ok(AgentHandle { agent_id, tx })
    }

    /// Unregister an agent
    ///
    /// Removes the agent's mailbox and closes its channels.
    pub fn unregister_agent(&mut self, agent_id: &AgentId) -> Result<()> {
        self.agents
            .remove(agent_id)
            .ok_or(TransportError::AgentNotFound {
                agent_id: agent_id.clone(),
            })?;

        Ok(())
    }

    /// Send a message to a specific agent
    pub fn send(&self, to: &AgentId, mut envelope: MessageEnvelope) -> Result<()> {
        let mailbox = self.agents.get(to).ok_or(TransportError::AgentNotFound {
            agent_id: to.clone(),
        })?;

        // Ensure routing metadata is set
        envelope.to = Some(to.clone());

        mailbox
            .tx
            .send(envelope)
            .map_err(|_| TransportError::ChannelClosed {
                agent_id: to.clone(),
            })
    }

    /// Send a message using an agent handle
    pub fn send_via_handle(&self, handle: &AgentHandle, envelope: MessageEnvelope) -> Result<()> {
        self.send(&handle.agent_id, envelope)
    }

    /// Receive the next message for an agent
    ///
    /// Blocks until a message arrives. Returns an error if the channel is closed.
    pub async fn recv(&mut self, from: &AgentId) -> Result<MessageEnvelope> {
        let mailbox = self
            .agents
            .get_mut(from)
            .ok_or(TransportError::AgentNotFound {
                agent_id: from.clone(),
            })?;

        mailbox
            .rx
            .recv()
            .await
            .ok_or(TransportError::ChannelClosed {
                agent_id: from.clone(),
            })
    }

    /// Broadcast a system event to all agents
    pub fn broadcast(&self, event: SystemEvent) -> Result<()> {
        self.broadcast_tx
            .send(event)
            .map_err(|e| TransportError::BroadcastFailed(e.to_string()))?;

        Ok(())
    }

    /// Subscribe to system events
    pub fn subscribe_events(&self) -> broadcast::Receiver<SystemEvent> {
        self.broadcast_tx.subscribe()
    }

    /// Allocate a new unique message ID
    pub fn next_message_id(&mut self) -> u64 {
        let id = self.next_message_id;
        self.next_message_id += 1;
        id
    }

    /// Get a handle for an agent (if registered)
    pub fn get_handle(&self, agent_id: &AgentId) -> Result<AgentHandle> {
        let mailbox = self.agents.get(agent_id).ok_or(TransportError::AgentNotFound {
            agent_id: agent_id.clone(),
        })?;

        Ok(AgentHandle {
            agent_id: agent_id.clone(),
            tx: mailbox.tx.clone(),
        })
    }

    /// Get count of registered agents
    pub fn agent_count(&self) -> usize {
        self.agents.len()
    }

    /// Check if an agent is registered
    pub fn is_registered(&self, agent_id: &AgentId) -> bool {
        self.agents.contains_key(agent_id)
    }
}

impl Default for LocalAgentBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::TypedMessage;

    fn create_test_envelope(
        from: &str,
        to: Option<&str>,
        message_id: u64,
    ) -> MessageEnvelope {
        MessageEnvelope {
            message_id,
            from: from.to_string(),
            to: to.map(|s| s.to_string()),
            timestamp: 1697500000,
            content: TypedMessage::StatusUpdate {
                task_id: "test_task".to_string(),
                progress: 0.5,
                status: crate::protocol::messages::TaskStatus::InProgress,
                artifacts: vec![],
                message: Some("Test message".to_string()),
            },
        }
    }

    #[tokio::test]
    async fn test_register_agent() {
        let mut bus = LocalAgentBus::new();
        let handle = bus.register_agent("agent_1".to_string()).unwrap();

        assert_eq!(handle.agent_id, "agent_1");
        assert_eq!(bus.agent_count(), 1);
        assert!(bus.is_registered(&"agent_1".to_string()));
    }

    #[tokio::test]
    async fn test_duplicate_registration_fails() {
        let mut bus = LocalAgentBus::new();
        bus.register_agent("agent_1".to_string()).unwrap();

        let result = bus.register_agent("agent_1".to_string());
        assert!(matches!(
            result,
            Err(TransportError::AlreadyRegistered { .. })
        ));
    }

    #[tokio::test]
    async fn test_send_and_receive() {
        let mut bus = LocalAgentBus::new();
        bus.register_agent("agent_1".to_string()).unwrap();
        bus.register_agent("agent_2".to_string()).unwrap();

        let envelope = create_test_envelope("agent_1", Some("agent_2"), 1);

        bus.send(&"agent_2".to_string(), envelope.clone()).unwrap();

        let received = bus.recv(&"agent_2".to_string()).await.unwrap();
        assert_eq!(received.message_id, 1);
        assert_eq!(received.from, "agent_1");
        assert_eq!(received.to, Some("agent_2".to_string()));
    }

    #[tokio::test]
    async fn test_send_via_handle() {
        let mut bus = LocalAgentBus::new();
        let handle = bus.register_agent("agent_1".to_string()).unwrap();

        let envelope = create_test_envelope("sender", Some("agent_1"), 1);

        handle.send(envelope.clone()).unwrap();

        let received = bus.recv(&"agent_1".to_string()).await.unwrap();
        assert_eq!(received.message_id, 1);
    }

    #[tokio::test]
    async fn test_send_to_nonexistent_agent() {
        let bus = LocalAgentBus::new();
        let envelope = create_test_envelope("agent_1", Some("agent_999"), 1);

        let result = bus.send(&"agent_999".to_string(), envelope);
        assert!(matches!(result, Err(TransportError::AgentNotFound { .. })));
    }

    #[tokio::test]
    async fn test_unregister_agent() {
        let mut bus = LocalAgentBus::new();
        bus.register_agent("agent_1".to_string()).unwrap();

        assert_eq!(bus.agent_count(), 1);

        bus.unregister_agent(&"agent_1".to_string()).unwrap();

        assert_eq!(bus.agent_count(), 0);
        assert!(!bus.is_registered(&"agent_1".to_string()));
    }

    #[tokio::test]
    async fn test_broadcast_event() {
        let mut bus = LocalAgentBus::new();
        let mut rx1 = bus.subscribe_events();
        let mut rx2 = bus.subscribe_events();

        let event = SystemEvent::AgentJoined {
            agent_id: "agent_1".to_string(),
            capabilities: vec!["test".to_string()],
            timestamp: 1697500000,
        };

        bus.broadcast(event.clone()).unwrap();

        let received1 = rx1.recv().await.unwrap();
        let received2 = rx2.recv().await.unwrap();

        match (received1, received2) {
            (
                SystemEvent::AgentJoined { agent_id: id1, .. },
                SystemEvent::AgentJoined { agent_id: id2, .. },
            ) => {
                assert_eq!(id1, "agent_1");
                assert_eq!(id2, "agent_1");
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[tokio::test]
    async fn test_message_id_allocation() {
        let mut bus = LocalAgentBus::new();

        let id1 = bus.next_message_id();
        let id2 = bus.next_message_id();
        let id3 = bus.next_message_id();

        assert_eq!(id1, 1);
        assert_eq!(id2, 2);
        assert_eq!(id3, 3);
    }

    #[tokio::test]
    async fn test_get_handle() {
        let mut bus = LocalAgentBus::new();
        bus.register_agent("agent_1".to_string()).unwrap();

        let handle = bus.get_handle(&"agent_1".to_string()).unwrap();
        assert_eq!(handle.agent_id, "agent_1");

        // Can send via retrieved handle
        let envelope = create_test_envelope("sender", Some("agent_1"), 1);
        handle.send(envelope).unwrap();

        let received = bus.recv(&"agent_1".to_string()).await.unwrap();
        assert_eq!(received.message_id, 1);
    }

    #[tokio::test]
    async fn test_multiple_messages_queued() {
        let mut bus = LocalAgentBus::new();
        bus.register_agent("agent_1".to_string()).unwrap();

        // Send multiple messages
        for i in 1..=5 {
            let envelope = create_test_envelope("sender", Some("agent_1"), i);
            bus.send(&"agent_1".to_string(), envelope).unwrap();
        }

        // Receive all in order
        for i in 1..=5 {
            let received = bus.recv(&"agent_1".to_string()).await.unwrap();
            assert_eq!(received.message_id, i);
        }
    }
}
