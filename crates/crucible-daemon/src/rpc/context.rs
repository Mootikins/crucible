//! RPC context holding shared state for handlers

use crate::agent_manager::AgentManager;
use crate::kiln_manager::KilnManager;
use crate::protocol::SessionEventMessage;
use crate::session_manager::SessionManager;
use crate::subscription::SubscriptionManager;
use std::sync::Arc;
use tokio::sync::broadcast;

pub struct RpcContext {
    pub kiln: Arc<KilnManager>,
    pub sessions: Arc<SessionManager>,
    pub agents: Arc<AgentManager>,
    pub subscriptions: Arc<SubscriptionManager>,
    pub event_tx: broadcast::Sender<SessionEventMessage>,
    pub shutdown_tx: broadcast::Sender<()>,
}

impl RpcContext {
    pub fn new(
        kiln: Arc<KilnManager>,
        sessions: Arc<SessionManager>,
        agents: Arc<AgentManager>,
        subscriptions: Arc<SubscriptionManager>,
        event_tx: broadcast::Sender<SessionEventMessage>,
        shutdown_tx: broadcast::Sender<()>,
    ) -> Self {
        Self {
            kiln,
            sessions,
            agents,
            subscriptions,
            event_tx,
            shutdown_tx,
        }
    }
}
