//! Regression tests for `cru chat --set` startup overrides.
//!
//! `initial_sets` daemon-bound overrides used to be sent down the UI
//! message channel, where only the reducer runs — the daemon RPC arm in
//! `process_action` was never reached, so `--set context_strategy=truncate`
//! (and every other daemon-scoped key) was silently inert, and
//! `--set model=X` updated the status bar without switching the model.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use crucible_core::events::EventRing;
use crucible_core::traits::chat::{AgentHandle, ChatError, ChatResult, SessionKnobs};
use crucible_oil::terminal::Terminal;
use tokio::sync::mpsc;

use crate::chat::bridge::AgentEventBridge;
use crate::tui::oil::chat_app::OilChatApp;
use crate::tui::oil::chat_runner::OilChatRunner;
use crate::tui::oil::commands::{SetEffect, SetRpcAction};

struct RpcCountingAgent {
    context_strategy_calls: AtomicUsize,
    switch_model_calls: AtomicUsize,
}

impl RpcCountingAgent {
    fn new() -> Self {
        Self {
            context_strategy_calls: AtomicUsize::new(0),
            switch_model_calls: AtomicUsize::new(0),
        }
    }
}

crucible_core::impl_noop_agent!(RpcCountingAgent);

#[async_trait]
impl AgentHandle for RpcCountingAgent {
    async fn send_message_fire_and_forget(&mut self, _message: String) -> ChatResult<()> {
        Ok(())
    }

    async fn clear_history(&mut self) -> ChatResult<()> {
        Ok(())
    }
    fn get_mode_id(&self) -> &str {
        "ask"
    }
    async fn set_mode_str(&mut self, _mode_id: &str) -> ChatResult<()> {
        Ok(())
    }
}

/// The two startup knobs count. The rest is the empty answer.
#[async_trait::async_trait]
impl SessionKnobs for RpcCountingAgent {
    fn get_system_prompt(&self) -> Option<String> {
        None
    }

    async fn switch_model(&mut self, _model_id: &str) -> ChatResult<()> {
        self.switch_model_calls.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    fn current_model(&self) -> Option<&str> {
        None
    }

    async fn fetch_available_models(&mut self) -> Vec<String> {
        Vec::new()
    }

    async fn fetch_available_modes(&mut self) -> Vec<String> {
        Vec::new()
    }

    async fn set_context_strategy(
        &mut self,
        _strategy: crucible_core::session::ContextStrategy,
    ) -> ChatResult<()> {
        self.context_strategy_calls.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    fn get_context_strategy(&self) -> crucible_core::session::ContextStrategy {
        crucible_core::session::ContextStrategy::default()
    }

    async fn set_precognition(&mut self, _enabled: bool) -> ChatResult<()> {
        Err(ChatError::NotSupported("set_precognition".into()))
    }

    fn get_precognition(&self) -> bool {
        true
    }
}

#[tokio::test]
async fn startup_set_overrides_reach_the_daemon_rpc() {
    let mut runner =
        OilChatRunner::with_terminal(Terminal::with_size(80, 24)).with_initial_sets(vec![
            SetEffect::DaemonRpc(SetRpcAction::SetContextStrategy("truncate".into())),
            SetEffect::DaemonRpc(SetRpcAction::SwitchModel("gpt-4o".into())),
        ]);

    let mut agent = RpcCountingAgent::new();
    let mut app = OilChatApp::default();
    let bridge = AgentEventBridge::new(Arc::new(EventRing::new(16)));
    let (msg_tx, _msg_rx) = mpsc::unbounded_channel();
    let mut background_tasks = Vec::new();

    runner
        .apply_initial_sets(
            &mut app,
            &mut agent,
            &bridge,
            &msg_tx,
            &mut background_tasks,
        )
        .await
        .expect("apply_initial_sets should not fail");

    assert_eq!(
        agent.context_strategy_calls.load(Ordering::Relaxed),
        1,
        "--set context_strategy must invoke the daemon RPC, not just the reducer"
    );
    assert_eq!(
        agent.switch_model_calls.load(Ordering::Relaxed),
        1,
        "--set model must actually switch the model, not only update the status bar"
    );
    assert_eq!(
        app.current_model(),
        "gpt-4o",
        "the reducer half must also run so the status bar reflects the override"
    );

    OilChatRunner::abort_background_tasks(&mut background_tasks);
}
