//! Daemon-side implementation of [`crucible_lua::DaemonGrammarApi`].
//!
//! Bridges `cru.grammar.{set,clear,get}_session_grammar` Lua calls to
//! [`AgentManager`]'s grammar surface. Keeping the bridge separate from
//! [`crate::session_bridge::DaemonSessionBridge`] mirrors how
//! [`crate::team_bridge`] (the team patterns) is split out — each Lua
//! module owns its own trait + bridge, so upgrades stay independent.

use crate::agent_manager::{AgentError, AgentManager};
use crate::protocol::SessionEventMessage;
use crucible_core::traits::chat::ChatError;
use crucible_core::types::Grammar;
use crucible_lua::DaemonGrammarApi;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::broadcast;

type BoxFut<T> = Pin<Box<dyn Future<Output = Result<T, String>> + Send>>;

pub struct DaemonGrammarBridge {
    agent_manager: Arc<AgentManager>,
    event_tx: broadcast::Sender<SessionEventMessage>,
}

impl DaemonGrammarBridge {
    pub fn new(
        agent_manager: Arc<AgentManager>,
        event_tx: broadcast::Sender<SessionEventMessage>,
    ) -> Self {
        Self {
            agent_manager,
            event_tx,
        }
    }
}

impl DaemonGrammarApi for DaemonGrammarBridge {
    fn set_session_grammar(&self, session_id: String, grammar: Grammar) -> BoxFut<()> {
        let am = Arc::clone(&self.agent_manager);
        let tx = self.event_tx.clone();
        Box::pin(async move {
            am.set_grammar(&session_id, grammar, Some(&tx))
                .await
                .map_err(format_grammar_error)
        })
    }

    fn clear_session_grammar(&self, session_id: String) -> BoxFut<()> {
        let am = Arc::clone(&self.agent_manager);
        let tx = self.event_tx.clone();
        Box::pin(async move {
            am.clear_grammar(&session_id, Some(&tx))
                .await
                .map_err(format_grammar_error)
        })
    }

    fn get_session_grammar(&self, session_id: String) -> BoxFut<Option<Grammar>> {
        let am = Arc::clone(&self.agent_manager);
        Box::pin(async move { am.get_grammar(&session_id).map_err(format_grammar_error) })
    }
}

/// Stringify `AgentError` for the Lua side. We surface `NotSupported`
/// verbatim — Wave 2 Item 5 specifies the message must carry the backend
/// name so plugin authors immediately know why the call failed.
fn format_grammar_error(e: AgentError) -> String {
    match e {
        AgentError::NotSupported(msg)
        | AgentError::Chat(ChatError::NotSupported(msg))
        | AgentError::InvalidConfig(msg) => msg,
        other => other.to_string(),
    }
}
