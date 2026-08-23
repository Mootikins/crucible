pub mod oil;

pub use oil::{
    clamp_lines_bottom, clamp_lines_top, composite_overlays, ensure_min_height, pad_lines_to,
    Action, AgentSelection, ChatAppMsg, Component, Event, FramePlan, FramePlanner, FrameSnapshot,
    InputAction, InputBuffer, InputMode, McpServerDisplay, OilChatApp, OilChatRunner, Overlay,
    OverlayAnchor, PluginStatusEntry, RenderState, StatusBar, Terminal, ViewContext,
    INPUT_MAX_CONTENT_LINES,
};
#[cfg(any(test, feature = "test-utils"))]
pub use oil::{AppHarness, ComponentHarness, TestRuntime};
