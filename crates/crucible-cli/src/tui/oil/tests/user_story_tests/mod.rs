//! Headless user-story test suites for the chat TUI.
//!
//! One file per story group from `docs/Meta/TUI User Stories.md`. Every
//! test drives a real [`OilChatApp`](crate::tui::oil::chat_app::OilChatApp)
//! through the [`Vt100TestRuntime`](super::vt100_runtime::Vt100TestRuntime)
//! frame-capture path via the shared [`support::StoryRuntime`] driver, so
//! the assertions are made against real rendered frames (deterministic,
//! no PTY dependence).
//!
//! Dispatch-logic matrices that need `pub(super)` handler access live
//! inline in the relevant `chat_app` submodule instead (autocomplete
//! matrix in `chat_app/autocomplete.rs`; `:set`/slash/undo dispatch in
//! `chat_app/command_handling.rs`). These files cover the render / state
//! / event-stream half of each story.

mod support;

mod notification_tests; // US-701 / US-702
mod paste_tests; // US-106
mod permission_tests; // US-401
mod scroll_tests; // US-801
mod shell_tests; // US-601 / US-602
mod subagent_mcp_tests; // US-302 / US-303
mod undo_tests; // US-902
