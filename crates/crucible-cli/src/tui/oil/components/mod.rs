mod diff_preview;
mod drawer;
mod input_area;
mod interaction_modal;
mod message_list;
mod notification_area;
mod popup_overlay;
mod shell_modal;
pub(crate) mod status_bar;

pub use diff_preview::render_diff_preview;
pub use drawer::{Drawer, DrawerKind};
pub use input_area::{InputArea, InputMode, INPUT_MAX_CONTENT_LINES};
pub use interaction_modal::{
    InteractionModal, InteractionModalMsg, InteractionModalOutput, InteractionMode,
};
pub use message_list::{
    format_output_tail, format_streaming_output, format_tool_args, format_tool_result,
    render_shell_execution, render_subagent, render_thinking_block, render_tool_call,
    render_tool_call_with_frame, render_user_prompt, summarize_tool_result, MessageList,
    ThinkingBlock,
};
pub use notification_area::NotificationArea;
pub use popup_overlay::{
    popup_item, popup_item_full, popup_item_with_desc, PopupOverlay, FOCUS_POPUP, POPUP_MAX_VISIBLE,
};
pub use shell_modal::{ShellHistoryItem, ShellModal, ShellModalMsg, ShellModalOutput, ShellStatus};
pub use status_bar::{NotificationToastKind, StatusBar};
