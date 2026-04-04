//! Oil-style declarative TUI framework
//!
//! A component-based TUI built on crucible-oil primitives.
//! Components are structs with `view()` → Node. The framework renders
//! the Node tree via Taffy layout + line-level diffing.
//!
//! ## Architecture
//!
//! ```text
//! State -> view() -> Node tree -> FramePlanner -> Terminal
//!   ^                                  |
//!   |                                  v
//!   +---- update() <---- Event <--- Input
//! ```

mod agent_selection;
mod app;
pub mod chat_app;
pub mod containers;
pub mod chat_runner;
pub mod commands;
pub mod component;
pub mod components;
mod composer;
pub mod config;
pub mod diff;
mod event;
pub mod lua_bridge;
pub mod lua_view;
pub mod markdown;
mod render_state;
mod runner;
mod test_harness;
pub mod theme;
pub mod utils;
mod viewport_cache;

pub use agent_selection::AgentSelection;
pub use app::{Action, App, ViewContext};
pub use chat_app::{ChatAppMsg, ChatMode, McpServerDisplay, OilChatApp, PluginStatusEntry, Role};
pub use chat_runner::OilChatRunner;
pub use component::{Component, ComponentHarness};
pub use components::{InputArea, InputMode, StatusBar, INPUT_MAX_CONTENT_LINES};
pub use composer::{pad_popup_region, ComposerConfig};
pub use event::{Event, InputAction, InputBuffer};
pub use lua_view::{LuaView, ViewAction};
pub use render_state::RenderState;
pub use runner::{run_sync, OilRunner};
pub use test_harness::AppHarness;
pub use containers::{
    Container, ContainerContent, ContainerKind, ContainerList, ContainerState,
};
pub use viewport_cache::CachedMessage;

// Re-export commonly used crucible-oil types
pub use crucible_oil::focus::*;
pub use crucible_oil::layout::*;
pub use crucible_oil::node::*;
pub use crucible_oil::overlay::{composite_overlays, Overlay, OverlayAnchor};
pub use crucible_oil::planning::{FramePlan, FramePlanner, FrameSnapshot, FrameTrace};
pub use crucible_oil::render::*;
pub use crucible_oil::runtime::TestRuntime;
pub use crucible_oil::style::*;
pub use crucible_oil::terminal::Terminal;
pub use crucible_oil::viewport::{clamp_lines_bottom, clamp_lines_top, ensure_min_height, pad_lines_to};

#[cfg(test)]
mod tests;
