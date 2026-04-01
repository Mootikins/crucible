//! Message dispatch handlers for OilChatApp.
//!
//! TODO(rewrite): Phase 5 — reimplement with new Container vec.
//! Currently stubbed to make the crate compile.

use crate::tui::oil::app::Action;

use super::messages::ChatAppMsg;
use super::model_state::ModelListState;
use super::OilChatApp;

impl OilChatApp {
    /// Handle streaming events (TextDelta, ThinkingDelta, ToolCall, etc.)
    /// TODO(rewrite): Phase 5 — wire to new container state mutations
    pub(super) fn handle_stream_msg(&mut self, msg: ChatAppMsg) -> Action<ChatAppMsg> {
        match msg {
            ChatAppMsg::StreamComplete | ChatAppMsg::StreamCancelled => {
                tracing::debug!("[stub] stream ended");
            }
            ChatAppMsg::ContextUsage { used, total } => {
                self.context_used = used;
                self.context_total = total;
            }
            ChatAppMsg::Error(err) => {
                self.add_notification(crucible_core::types::Notification::warning(err));
            }
            _ => {
                // TODO(rewrite): TextDelta, ThinkingDelta, ToolCall, ToolResult*, etc.
                tracing::trace!("[stub] stream msg ignored: {:?}", msg.category());
            }
        }
        Action::Continue
    }

    /// Handle config messages (SwitchModel, Set*, ModelsLoaded, etc.)
    pub(super) fn handle_config_msg(&mut self, msg: ChatAppMsg) -> Action<ChatAppMsg> {
        match msg {
            ChatAppMsg::SwitchModel(model) => {
                self.model = model;
            }
            ChatAppMsg::ModelsLoaded(models) => {
                self.available_models = models;
                self.model_list_state = ModelListState::Loaded;
            }
            ChatAppMsg::ModelsFetchFailed(err) => {
                self.model_list_state = ModelListState::Failed(err);
            }
            ChatAppMsg::McpStatusLoaded(servers) => {
                self.mcp_servers = servers;
            }
            ChatAppMsg::PluginStatusLoaded(entries) => {
                self.plugin_status = entries;
            }
            _ => {
                tracing::trace!("[stub] config msg: {:?}", msg.category());
            }
        }
        Action::Continue
    }

    /// Handle delegation events (Subagent*, Delegation*)
    /// TODO(rewrite): Phase 5 — wire to new container state
    pub(super) fn handle_delegation_msg(&mut self, msg: ChatAppMsg) -> Action<ChatAppMsg> {
        tracing::trace!("[stub] delegation msg: {:?}", msg.category());
        Action::Continue
    }

    /// Handle UI messages (ClearHistory, ToggleMessages, Status, etc.)
    pub(super) fn handle_ui_msg(&mut self, msg: ChatAppMsg) -> Action<ChatAppMsg> {
        match msg {
            ChatAppMsg::ClearHistory => {
                self.reset_session();
            }
            ChatAppMsg::ToggleMessages => {
                self.toggle_messages();
            }
            ChatAppMsg::Status(status) => {
                self.status = status;
            }
            ChatAppMsg::ModeChanged(mode) => {
                self.mode = super::state::ChatMode::parse(&mode);
            }
            _ => {
                tracing::trace!("[stub] ui msg: {:?}", msg.category());
            }
        }
        Action::Continue
    }
}
