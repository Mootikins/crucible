//! `AgentHandle` that no-ops everything — used for pure-display replay.
//!
//! Every method of [`AgentHandle`] is explicitly implemented to prevent
//! future trait additions from silently delegating to defaults that might
//! contact the daemon or surface `NotSupported` errors as user-visible
//! notifications during replay.
//!
//! Reference checklist (from `crucible_core::traits::chat::AgentHandle`):
//!
//! ```ignore
//! fn send_message_stream(&mut self, message: String) -> BoxStream<'static, ChatResult<ChatChunk>>;
//! async fn send_message_fire_and_forget(&mut self, message: String) -> ChatResult<()>;
//! fn continue_with_tool_results(&mut self, tool_calls: Vec<ChatToolCall>, tool_results: Vec<ChatToolResult>) -> BoxStream<'static, ChatResult<ChatChunk>>;
//! async fn send_message(&mut self, message: &str) -> ChatResult<ChatResponse>;
//! fn is_connected(&self) -> bool;
//! fn supports_streaming(&self) -> bool;
//! async fn on_commands_update(&mut self, commands: Vec<CommandDescriptor>) -> ChatResult<()>;
//! fn get_modes(&self) -> Option<&SessionModeState>;
//! fn get_mode_id(&self) -> &str;
//! async fn set_mode_str(&mut self, mode_id: &str) -> ChatResult<()>;
//! fn get_commands(&self) -> &[AvailableCommand];
//! async fn clear_history(&mut self);
//! async fn switch_model(&mut self, model_id: &str) -> ChatResult<()>;
//! fn current_model(&self) -> Option<&str>;
//! fn available_models(&self) -> Vec<String>;
//! async fn fetch_available_models(&mut self) -> Vec<String>;
//! async fn set_thinking_budget(&mut self, budget: i64) -> ChatResult<()>;
//! fn get_thinking_budget(&self) -> Option<i64>;
//! async fn set_system_prompt(&mut self, prompt: &str) -> ChatResult<()>;
//! fn get_system_prompt(&self) -> Option<String>;
//! async fn undo(&mut self, count: usize) -> ChatResult<Vec<UndoSummary>>;
//! fn can_undo(&self) -> bool;
//! fn undo_depth(&self) -> usize;
//! async fn cancel(&self) -> ChatResult<()>;
//! async fn set_temperature(&mut self, temperature: f64) -> ChatResult<()>;
//! fn get_temperature(&self) -> Option<f64>;
//! async fn set_max_tokens(&mut self, max_tokens: Option<u32>) -> ChatResult<()>;
//! fn get_max_tokens(&self) -> Option<u32>;
//! async fn set_max_iterations(&mut self, max_iterations: Option<u32>) -> ChatResult<()>;
//! fn get_max_iterations(&self) -> Option<u32>;
//! async fn set_execution_timeout(&mut self, timeout_secs: Option<u64>) -> ChatResult<()>;
//! fn get_execution_timeout(&self) -> Option<u64>;
//! async fn set_context_budget(&mut self, budget: Option<usize>) -> ChatResult<()>;
//! fn get_context_budget(&self) -> Option<usize>;
//! async fn set_context_strategy(&mut self, strategy: ContextStrategy) -> ChatResult<()>;
//! fn get_context_strategy(&self) -> ContextStrategy;
//! async fn set_context_window(&mut self, window: Option<usize>) -> ChatResult<()>;
//! fn get_context_window(&self) -> Option<usize>;
//! async fn set_output_validation(&mut self, validation: OutputValidation) -> ChatResult<()>;
//! fn get_output_validation(&self) -> &OutputValidation;
//! async fn set_validation_retries(&mut self, retries: u32) -> ChatResult<()>;
//! fn get_validation_retries(&self) -> u32;
//! async fn set_precognition_results(&mut self, count: usize) -> ChatResult<()>;
//! fn get_precognition_results(&self) -> usize;
//! async fn interaction_respond(&mut self, request_id: String, response: InteractionResponse) -> ChatResult<()>;
//! fn take_interaction_receiver(&mut self) -> Option<UnboundedReceiver<InteractionEvent>>;
//! fn session_id(&self) -> Option<&str>;
//! ```

use async_trait::async_trait;
use futures::stream::{self, BoxStream, StreamExt};

use crucible_core::interaction::{InteractionEvent, InteractionResponse};
use crucible_core::session::{ContextStrategy, OutputValidation};
use crucible_core::traits::chat::{
    AgentHandle, ChatChunk, ChatResponse, ChatResult, ChatToolCall, ChatToolResult,
    CommandDescriptor,
};
use crucible_core::types::acp::schema::{AvailableCommand, SessionModeState};
use crucible_core::types::UndoSummary;
use tokio::sync::mpsc;

/// No-op [`AgentHandle`] used for pure-display replay.
///
/// Owns a synthetic session id (for [`AgentHandle::session_id`]) and a
/// dropped-sender interaction receiver, so consumers that wait on
/// interactions see `None` immediately rather than hanging forever.
pub struct NoopAgentHandle {
    session_id: String,
    interaction_rx: Option<mpsc::UnboundedReceiver<InteractionEvent>>,
    mode_id: String,
}

impl NoopAgentHandle {
    pub fn new(session_id: String) -> Self {
        // Dropped-sender channel: the first `.recv().await` on the receiver
        // yields `None`, so consumers never block.
        let (_tx, rx) = mpsc::unbounded_channel::<InteractionEvent>();
        Self {
            session_id,
            interaction_rx: Some(rx),
            mode_id: "normal".to_string(),
        }
    }
}

#[async_trait]
impl AgentHandle for NoopAgentHandle {
    fn send_message_stream(
        &mut self,
        _message: String,
    ) -> BoxStream<'static, ChatResult<ChatChunk>> {
        stream::empty().boxed()
    }

    async fn send_message_fire_and_forget(&mut self, _message: String) -> ChatResult<()> {
        Ok(())
    }

    fn continue_with_tool_results(
        &mut self,
        _tool_calls: Vec<ChatToolCall>,
        _tool_results: Vec<ChatToolResult>,
    ) -> BoxStream<'static, ChatResult<ChatChunk>> {
        stream::empty().boxed()
    }

    async fn send_message(&mut self, _message: &str) -> ChatResult<ChatResponse> {
        Ok(ChatResponse {
            content: String::new(),
            tool_calls: Vec::new(),
        })
    }

    fn is_connected(&self) -> bool {
        true
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    async fn on_commands_update(
        &mut self,
        _commands: Vec<CommandDescriptor>,
    ) -> ChatResult<()> {
        Ok(())
    }

    fn get_modes(&self) -> Option<&SessionModeState> {
        None
    }

    fn get_mode_id(&self) -> &str {
        &self.mode_id
    }

    async fn set_mode_str(&mut self, mode_id: &str) -> ChatResult<()> {
        self.mode_id = mode_id.to_string();
        Ok(())
    }

    fn get_commands(&self) -> &[AvailableCommand] {
        &[]
    }

    async fn clear_history(&mut self) {}

    async fn switch_model(&mut self, _model_id: &str) -> ChatResult<()> {
        Ok(())
    }

    fn current_model(&self) -> Option<&str> {
        None
    }

    fn available_models(&self) -> Vec<String> {
        Vec::new()
    }

    async fn fetch_available_models(&mut self) -> Vec<String> {
        Vec::new()
    }

    async fn set_thinking_budget(&mut self, _budget: i64) -> ChatResult<()> {
        Ok(())
    }

    fn get_thinking_budget(&self) -> Option<i64> {
        None
    }

    async fn set_system_prompt(&mut self, _prompt: &str) -> ChatResult<()> {
        Ok(())
    }

    fn get_system_prompt(&self) -> Option<String> {
        None
    }

    async fn undo(&mut self, _count: usize) -> ChatResult<Vec<UndoSummary>> {
        Ok(Vec::new())
    }

    fn can_undo(&self) -> bool {
        false
    }

    fn undo_depth(&self) -> usize {
        0
    }

    async fn cancel(&self) -> ChatResult<()> {
        Ok(())
    }

    async fn set_temperature(&mut self, _temperature: f64) -> ChatResult<()> {
        Ok(())
    }

    fn get_temperature(&self) -> Option<f64> {
        None
    }

    async fn set_max_tokens(&mut self, _max_tokens: Option<u32>) -> ChatResult<()> {
        Ok(())
    }

    fn get_max_tokens(&self) -> Option<u32> {
        None
    }

    async fn set_max_iterations(&mut self, _max_iterations: Option<u32>) -> ChatResult<()> {
        Ok(())
    }

    fn get_max_iterations(&self) -> Option<u32> {
        None
    }

    async fn set_execution_timeout(&mut self, _timeout_secs: Option<u64>) -> ChatResult<()> {
        Ok(())
    }

    fn get_execution_timeout(&self) -> Option<u64> {
        None
    }

    async fn set_context_budget(&mut self, _budget: Option<usize>) -> ChatResult<()> {
        Ok(())
    }

    fn get_context_budget(&self) -> Option<usize> {
        None
    }

    async fn set_context_strategy(&mut self, _strategy: ContextStrategy) -> ChatResult<()> {
        Ok(())
    }

    fn get_context_strategy(&self) -> ContextStrategy {
        ContextStrategy::default()
    }

    async fn set_context_window(&mut self, _window: Option<usize>) -> ChatResult<()> {
        Ok(())
    }

    fn get_context_window(&self) -> Option<usize> {
        None
    }

    async fn set_output_validation(&mut self, _validation: OutputValidation) -> ChatResult<()> {
        Ok(())
    }

    fn get_output_validation(&self) -> &OutputValidation {
        static NONE: OutputValidation = OutputValidation::None;
        &NONE
    }

    async fn set_validation_retries(&mut self, _retries: u32) -> ChatResult<()> {
        Ok(())
    }

    fn get_validation_retries(&self) -> u32 {
        0
    }

    async fn set_precognition_results(&mut self, _count: usize) -> ChatResult<()> {
        Ok(())
    }

    fn get_precognition_results(&self) -> usize {
        0
    }

    async fn interaction_respond(
        &mut self,
        _request_id: String,
        _response: InteractionResponse,
    ) -> ChatResult<()> {
        Ok(())
    }

    fn take_interaction_receiver(
        &mut self,
    ) -> Option<mpsc::UnboundedReceiver<InteractionEvent>> {
        self.interaction_rx.take()
    }

    fn session_id(&self) -> Option<&str> {
        Some(&self.session_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn noop_agent_session_id_returns_constructor_arg() {
        let agent = NoopAgentHandle::new("replay-session-42".into());
        assert_eq!(agent.session_id(), Some("replay-session-42"));
    }

    #[tokio::test]
    async fn noop_agent_send_message_stream_is_empty() {
        let mut agent = NoopAgentHandle::new("replay-test".into());
        let mut s = agent.send_message_stream("hi".into());
        assert!(s.next().await.is_none());
    }

    #[tokio::test]
    async fn noop_agent_continue_with_tool_results_stream_is_empty() {
        let mut agent = NoopAgentHandle::new("replay-test".into());
        let mut s = agent.continue_with_tool_results(Vec::new(), Vec::new());
        assert!(s.next().await.is_none());
    }

    #[tokio::test]
    async fn noop_agent_take_interaction_receiver_yields_none_on_recv() {
        let mut agent = NoopAgentHandle::new("replay-test".into());
        let mut rx = agent
            .take_interaction_receiver()
            .expect("receiver should be available on first take");
        // Sender was dropped immediately in `new`, so recv() resolves to None.
        assert!(rx.recv().await.is_none());
        // Subsequent takes return None.
        assert!(agent.take_interaction_receiver().is_none());
    }

    /// Exhaustively call every `AgentHandle` method on a `NoopAgentHandle`.
    /// Compile-time safety net: if a method is added to the trait, this test
    /// will keep compiling only because we rely on defaults — which is exactly
    /// the failure mode we want to avoid. Pair with manual review on trait
    /// changes.
    #[tokio::test]
    async fn noop_agent_all_methods_are_benign() {
        let mut agent = NoopAgentHandle::new("replay-all".into());

        // Streams
        {
            let mut s = agent.send_message_stream("hi".into());
            assert!(s.next().await.is_none());
        }
        assert!(agent.send_message_fire_and_forget("hi".into()).await.is_ok());
        {
            let mut s = agent.continue_with_tool_results(Vec::new(), Vec::new());
            assert!(s.next().await.is_none());
        }
        assert!(agent.send_message("hi").await.is_ok());

        // Connection / capability
        assert!(agent.is_connected());
        assert!(agent.supports_streaming());

        // Commands / modes
        assert!(agent.on_commands_update(Vec::new()).await.is_ok());
        assert!(agent.get_modes().is_none());
        let _ = agent.get_mode_id();
        assert!(agent.set_mode_str("plan").await.is_ok());
        assert_eq!(agent.get_mode_id(), "plan");
        assert!(agent.get_commands().is_empty());

        // History / model
        agent.clear_history().await;
        assert!(agent.switch_model("any").await.is_ok());
        assert!(agent.current_model().is_none());
        assert!(agent.available_models().is_empty());
        assert!(agent.fetch_available_models().await.is_empty());

        // Thinking budget
        assert!(agent.set_thinking_budget(-1).await.is_ok());
        assert!(agent.get_thinking_budget().is_none());

        // System prompt
        assert!(agent.set_system_prompt("sys").await.is_ok());
        assert!(agent.get_system_prompt().is_none());

        // Undo
        assert!(agent.undo(1).await.is_ok());
        assert!(!agent.can_undo());
        assert_eq!(agent.undo_depth(), 0);

        // Cancel
        assert!(agent.cancel().await.is_ok());

        // Sampling params
        assert!(agent.set_temperature(0.5).await.is_ok());
        assert!(agent.get_temperature().is_none());
        assert!(agent.set_max_tokens(Some(1024)).await.is_ok());
        assert!(agent.get_max_tokens().is_none());
        assert!(agent.set_max_iterations(Some(5)).await.is_ok());
        assert!(agent.get_max_iterations().is_none());
        assert!(agent.set_execution_timeout(Some(30)).await.is_ok());
        assert!(agent.get_execution_timeout().is_none());

        // Context
        assert!(agent.set_context_budget(Some(4096)).await.is_ok());
        assert!(agent.get_context_budget().is_none());
        assert!(agent
            .set_context_strategy(ContextStrategy::default())
            .await
            .is_ok());
        let _ = agent.get_context_strategy();
        assert!(agent.set_context_window(Some(10)).await.is_ok());
        assert!(agent.get_context_window().is_none());

        // Output validation
        assert!(agent
            .set_output_validation(OutputValidation::None)
            .await
            .is_ok());
        let _ = agent.get_output_validation();
        assert!(agent.set_validation_retries(3).await.is_ok());
        let _ = agent.get_validation_retries();

        // Precognition
        assert!(agent.set_precognition_results(5).await.is_ok());
        let _ = agent.get_precognition_results();

        // Interaction
        assert!(agent
            .interaction_respond(
                "req-1".into(),
                InteractionResponse::Cancelled,
            )
            .await
            .is_ok());

        // Session id
        assert_eq!(agent.session_id(), Some("replay-all"));

        // take_interaction_receiver: first call returns Some, second None
        let rx = agent.take_interaction_receiver();
        assert!(rx.is_some());
        assert!(agent.take_interaction_receiver().is_none());
    }
}
