use std::cell::Cell;

use crate::tui::oil::components::NotificationArea;
use crate::tui::oil::config::RuntimeConfig;
use crate::tui::oil::containers::ContainerList;
use crate::tui::oil::event::InputBuffer;

use super::{
    ChatMode, MessageQueueState, ModelListState, OilChatApp, PermissionState, PopupState,
    PrecognitionState, ShellHistoryState,
};

impl Default for OilChatApp {
    fn default() -> Self {
        Self {
            // Viewport Projection
            container_list: ContainerList::new(),
            mode: ChatMode::Normal,
            model: String::new(),
            status: String::new(),
            context_used: 0,
            context_total: 0,
            current_provider: "local".to_string(),
            mcp_servers: Vec::new(),
            plugin_status: Vec::new(),
            available_models: Vec::new(),

            model_list_state: ModelListState::NotLoaded,

            // UI Chrome
            input: InputBuffer::new(),
            popup: PopupState::default(),
            notification_area: NotificationArea::new(),
            interaction_modal: None,
            shell_modal: None,
            spinner_epoch: std::time::Instant::now(),
            needs_full_redraw: false,
            show_thinking: true,
            precognition: PrecognitionState::default(),
            terminal_size: Cell::new((80, 24)),
            permission: PermissionState::default(),
            message_queue: MessageQueueState::default(),
            attached_context: Vec::new(),
            drop_stream_deltas: false,
            pending_delegate_supersessions: std::collections::HashSet::new(),

            // I/O / Lifecycle
            on_submit: None,
            session_dir: None,
            shell_history: ShellHistoryState::default(),
            runtime_config: RuntimeConfig::empty(),
            workspace_files: Vec::new(),
            kiln_notes: Vec::new(),
            slash_commands: crate::commands::chat::known_slash_commands(),
            statusline_config: Some(
                crucible_lua::get_statusline_config()
                    .unwrap_or_else(crucible_lua::statusline::StatuslineConfig::builtin_default),
            ),
        }
    }
}
