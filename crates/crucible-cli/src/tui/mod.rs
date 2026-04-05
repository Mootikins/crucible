pub mod oil;

pub use oil::{
    clamp_lines_bottom, clamp_lines_top, composite_overlays, ensure_min_height, pad_lines_to,
    pad_popup_region, run_sync, Action, AgentSelection, App, AppHarness, CachedMessage, ChatAppMsg,
    ChatMode, Component, ComponentHarness, ComposerConfig, Event, FramePlan, FramePlanner,
    FrameSnapshot, InputAction, InputArea, InputBuffer, InputMode, LuaView,
    McpServerDisplay, OilChatApp, OilChatRunner, OilRunner, Overlay, OverlayAnchor,
    PluginStatusEntry, RenderState, Role, StatusBar, Terminal, TestRuntime, ViewAction, ViewContext,
    INPUT_MAX_CONTENT_LINES,
};
