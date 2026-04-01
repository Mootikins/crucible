mod diff_preview;
mod drawer;
mod input_area;
mod input_component;
mod interaction_modal;
mod message_list;
mod notification_area;
mod notification_component;
mod popup_component;
mod popup_overlay;
mod shell_modal;
mod shell_render;
pub(crate) mod status_bar;
pub(crate) mod status_component;
mod subagent_render;
pub(crate) mod thinking_component;
pub(crate) mod tool_render;

pub use crucible_oil::components::DrawerKind;
#[allow(unused_imports)] // WIP: render_diff_preview not yet used
pub(crate) use diff_preview::render_diff_preview;
pub use drawer::Drawer;
pub use input_area::{InputArea, InputMode, INPUT_MAX_CONTENT_LINES};
pub use input_component::InputComponent;
pub use interaction_modal::{
    InteractionModal, InteractionModalMsg, InteractionModalOutput, InteractionMode,
};
#[allow(unused_imports)] // WIP: MessageList not yet used
pub(crate) use message_list::MessageList;
pub use message_list::render_user_prompt;
pub use thinking_component::ThinkingComponent;
pub use notification_area::NotificationArea;
pub use notification_component::{NotificationComponent, NotificationEntry};
pub use popup_component::PopupComponent;
pub use popup_overlay::{
    popup_item, popup_item_with_desc, PopupOverlay, FOCUS_POPUP, POPUP_MAX_VISIBLE,
};
pub use shell_modal::{ShellHistoryItem, ShellModal, ShellModalMsg, ShellModalOutput, ShellStatus};
pub use shell_render::render_shell_execution;
pub use status_bar::{NotificationToastKind, StatusBar};
pub use status_component::StatusComponent;
pub use subagent_render::render_subagent;
pub use tool_render::{
    format_streaming_output, format_tool_args, format_tool_result, render_tool_call,
    render_tool_call_with_frame, summarize_tool_result,
};
