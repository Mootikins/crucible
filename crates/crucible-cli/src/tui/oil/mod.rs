//! Oil-style declarative TUI framework
//!
//! A minimal Elm/Dioxus-inspired UI framework for terminal applications.
//! Key concepts:
//!
//! - **Declarative**: Build UI as a tree of nodes, framework handles rendering
//! - **Scrollback-native**: `Static` nodes graduate to terminal scrollback
//! - **Composable**: Nest nodes freely, layout is automatic
//!
//! ## Architecture
//!
//! ```text
//! State -> view() -> Node tree -> Runtime -> Terminal
//!   ^                                |
//!   |                                v
//!   +---- update() <---- Event <--- Input
//! ```
//!
//! ## Example
//!
//! ```rust,ignore
//! use crucible_cli::tui::oil::*;
//!
//! fn view(messages: &[String], input: &str) -> Node {
//!     col([
//!         // Graduated to scrollback when complete
//!         messages.iter().enumerate().map(|(i, msg)| {
//!             scrollback(format!("msg-{i}"), [text(msg)])
//!         }),
//!         // Stays in viewport
//!         text_input(input, input.len()),
//!     ])
//! }
//! ```

mod agent_selection;
mod ansi;
mod app;
mod cell_grid;
pub mod chat_app;
pub mod chat_container;
pub mod chat_runner;
pub mod commands;
pub mod component;
pub mod components;
mod decrypt;
mod composer;
pub mod config;
pub mod diff;
mod event;
pub mod example;
mod focus;
pub mod graduation;
mod layout;
pub mod lua_bridge;
pub mod lua_view;
pub mod markdown;
mod node;
mod output;
mod overlay;
pub mod planning;
mod render;
pub mod render_state;
mod runner;
mod runtime;
mod style;
mod terminal;
mod test_harness;
pub mod theme;
pub mod utils;
mod viewport;
mod viewport_cache;

pub use agent_selection::AgentSelection;
pub use app::{Action, App, ViewContext};
pub use chat_app::{
    ChatAppMsg, ChatItem, ChatMode, McpServerDisplay, OilChatApp, PluginStatusEntry, Role,
};
pub use chat_container::{ChatContainer, ContainerList, ThinkingBlock};
pub use chat_runner::OilChatRunner;
pub use component::{Component, ComponentHarness};
pub use components::{InputArea, InputMode, StatusBar, INPUT_MAX_CONTENT_LINES};
pub use composer::{pad_popup_region, ComposerConfig};
pub use event::{Event, InputAction, InputBuffer};
pub use focus::*;
pub use graduation::{GraduatedContent, GraduationState};
pub use layout::*;
pub use lua_view::{LuaView, ViewAction};
pub use node::*;
pub use overlay::{composite_overlays, Overlay, OverlayAnchor};
pub use planning::{FramePlan, FramePlanner, FrameSnapshot, FrameTrace};
pub use render::*;
pub use render_state::RenderState;
pub use runner::{run_sync, OilRunner};
pub use runtime::TestRuntime;
pub use style::*;
pub use terminal::Terminal;
pub use test_harness::AppHarness;
pub use theme::ThemeTokens;
pub use viewport::{clamp_lines_bottom, clamp_lines_top, ensure_min_height, pad_lines_to};
pub use viewport_cache::CachedMessage;
pub use crucible_oil::decrypt::{decrypt_text, DecryptConfig, CIPHER_CHARS};

#[cfg(test)]
mod tests;
