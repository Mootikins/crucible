//! Shared driver for headless TUI user-story tests.
//!
//! Wraps an [`OilChatApp`] and a [`Vt100TestRuntime`] so a story can be
//! scripted as a sequence of daemon messages / key events, capturing a
//! real rendered frame after each step. The captured frames join into a
//! single string (with per-frame separators) suitable for an insta
//! snapshot or for `contains`-style assertions.
//!
//! This is the deterministic "image sequence" the story doc refers to:
//! every frame comes from the real render path (render_frame → raw
//! terminal bytes → vt100 parser), not a re-implemented view.

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::tui::oil::app::{Action, App};
use crate::tui::oil::chat_app::{ChatAppMsg, OilChatApp};
use crate::tui::oil::chat_runner::session_event_to_chat_msgs;
use crate::tui::oil::event::Event;

use super::super::vt100_runtime::Vt100TestRuntime;

/// Resolve `assets/fixtures/<name>` relative to the workspace root.
pub(crate) fn fixture_path(name: &str) -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    std::path::Path::new(manifest_dir)
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .join("assets/fixtures")
        .join(name)
}

/// Parse a JSONL session recording into the `ChatAppMsg` stream the TUI
/// would receive on replay. Only the daemon's replayable session events
/// are mapped (via the production `session_event_to_chat_msgs`); header,
/// footer, and interaction/undo control events are ignored — those are
/// injected directly by the test, matching how the live runner delivers
/// them over separate channels.
pub(crate) fn load_fixture(name: &str) -> Vec<ChatAppMsg> {
    let path = fixture_path(name);
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read fixture {}: {e}", path.display()));

    let mut msgs = Vec::new();
    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let value: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if value.get("version").is_some() || value.get("ended_at").is_some() {
            continue;
        }
        let Some(event_type) = value.get("event").and_then(|v| v.as_str()) else {
            continue;
        };
        let data = value.get("data").cloned().unwrap_or(serde_json::Value::Null);
        msgs.extend(session_event_to_chat_msgs(event_type, &data));
    }
    msgs
}

pub(crate) struct StoryRuntime {
    app: OilChatApp,
    vt: Vt100TestRuntime,
    frames: Vec<String>,
    width: u16,
    height: u16,
}

impl StoryRuntime {
    pub(crate) fn new(width: u16, height: u16) -> Self {
        Self {
            app: OilChatApp::init(),
            vt: Vt100TestRuntime::new(width, height),
            frames: Vec::new(),
            width,
            height,
        }
    }

    pub(crate) fn app(&mut self) -> &mut OilChatApp {
        &mut self.app
    }

    /// Feed a daemon → TUI message through the real `on_message` path.
    pub(crate) fn send(&mut self, msg: ChatAppMsg) -> &mut Self {
        self.app.on_message(msg);
        self
    }

    /// Pump every message from a JSONL fixture through `on_message`.
    pub(crate) fn pump_fixture(&mut self, name: &str) -> &mut Self {
        for msg in load_fixture(name) {
            self.app.on_message(msg);
        }
        self
    }

    /// Send a key event and return the resulting action (for asserting
    /// daemon-bound `Action::Send`s such as permission responses).
    pub(crate) fn key(&mut self, code: KeyCode) -> Action<ChatAppMsg> {
        self.app.update(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)))
    }

    /// Type a string one `Char` key at a time (drives autocomplete + input).
    pub(crate) fn text(&mut self, s: &str) -> &mut Self {
        for c in s.chars() {
            self.key(KeyCode::Char(c));
        }
        self
    }

    /// Press Enter and return the resulting action.
    pub(crate) fn enter(&mut self) -> Action<ChatAppMsg> {
        self.key(KeyCode::Enter)
    }

    /// Send a Ctrl+`c` key event (e.g. Ctrl+J to insert a newline).
    pub(crate) fn key_ctrl(&mut self, c: char) -> Action<ChatAppMsg> {
        self.app
            .update(Event::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)))
    }

    /// Render the current state into a fresh terminal and record it under
    /// `label`. Each captured frame is an independent clean render, so the
    /// joined [`sequence`](Self::sequence) is a faithful step-by-step
    /// "image sequence" free of cross-frame vt100 bleed.
    pub(crate) fn capture(&mut self, label: &str) -> &mut Self {
        let mut vt = Vt100TestRuntime::new(self.width, self.height);
        vt.render_frame(&mut self.app);
        self.frames
            .push(format!("─── {label} ───\n{}", vt.screen_contents()));
        self
    }

    /// Render and return the current visible screen (plain text).
    pub(crate) fn screen(&mut self) -> String {
        self.vt.render_frame(&mut self.app);
        self.vt.screen_contents()
    }

    /// Render the *current* app state into a brand-new terminal and return
    /// its screen. Unlike `screen()`, this drops any accumulated vt100
    /// state, so it reflects only what the live container list renders now
    /// — the right tool for asserting that reverted/cleared content is
    /// gone (a real terminal's full-redraw does the same).
    pub(crate) fn fresh_screen(&mut self) -> String {
        let mut vt = Vt100TestRuntime::new(self.width, self.height);
        vt.render_frame(&mut self.app);
        vt.screen_contents()
    }

    /// The full captured frame sequence, joined with blank-line separators.
    pub(crate) fn sequence(&self) -> String {
        self.frames.join("\n\n")
    }
}
