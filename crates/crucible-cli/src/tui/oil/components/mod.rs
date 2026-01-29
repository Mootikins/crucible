mod diff_preview;
mod drawer;
mod input_area;
mod interaction_modal;
mod message_list;
mod notification_area;
mod popup_overlay;
mod shell_modal;
mod shell_render;
pub(crate) mod status_bar;
mod subagent_render;
pub(crate) mod tool_render;

pub(crate) use diff_preview::render_diff_preview;
pub use drawer::{Drawer, DrawerKind};
pub use input_area::{InputArea, InputMode, INPUT_MAX_CONTENT_LINES};
pub use interaction_modal::{
    InteractionModal, InteractionModalMsg, InteractionModalOutput, InteractionMode,
};
pub(crate) use message_list::MessageList;
pub use message_list::{render_thinking_block, render_user_prompt};
pub use notification_area::NotificationArea;
pub use popup_overlay::{
    popup_item, popup_item_with_desc, PopupOverlay, FOCUS_POPUP, POPUP_MAX_VISIBLE,
};
pub use shell_modal::{ShellHistoryItem, ShellModal, ShellModalMsg, ShellModalOutput, ShellStatus};
pub use shell_render::render_shell_execution;
pub use status_bar::{NotificationToastKind, StatusBar};
pub use subagent_render::render_subagent;
pub use tool_render::{
    format_streaming_output, format_tool_args, format_tool_result, render_tool_call,
    render_tool_call_with_frame, summarize_tool_result,
};
