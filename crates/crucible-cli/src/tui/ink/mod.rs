//! Ink-style declarative TUI framework
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
//! use crucible_cli::tui::ink::*;
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

mod app;
pub mod chat_app;
pub mod chat_runner;
mod event;
pub mod example;
mod layout;
pub mod markdown;
mod node;
mod output;
mod render;
mod runner;
mod runtime;
mod style;
mod taffy_layout;
mod terminal;

pub use app::*;
pub use chat_app::{ChatAppMsg, ChatMode, InkChatApp, Message, Role, ToolCallInfo};
pub use chat_runner::InkChatRunner;
pub use event::*;
pub use layout::*;
pub use node::*;
pub use render::*;
pub use runner::*;
pub use runtime::*;
pub use style::*;
pub use terminal::*;

#[cfg(test)]
mod tests;
