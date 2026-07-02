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
        let data = value
            .get("data")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        msgs.extend(session_event_to_chat_msgs(event_type, &data));
    }
    msgs
}

pub(crate) struct StoryRuntime {
    app: OilChatApp,
    vt: Vt100TestRuntime,
    frames: Vec<String>,
    /// Ring buffer of the most recently rendered frames, dumped on panic so a
    /// failing story shows the states leading up to the failure (the image
    /// sequence VS Code's harness captures on failure).
    recent_frames: std::collections::VecDeque<String>,
    width: u16,
    height: u16,
}

impl StoryRuntime {
    /// How many trailing frames to keep for the on-panic dump.
    const RECENT_FRAME_CAP: usize = 5;

    pub(crate) fn new(width: u16, height: u16) -> Self {
        Self {
            app: OilChatApp::init(),
            vt: Vt100TestRuntime::new(width, height),
            frames: Vec::new(),
            recent_frames: std::collections::VecDeque::with_capacity(Self::RECENT_FRAME_CAP),
            width,
            height,
        }
    }

    /// Record a rendered frame into the on-panic ring buffer.
    fn remember(&mut self, frame: &str) {
        if self.recent_frames.len() == Self::RECENT_FRAME_CAP {
            self.recent_frames.pop_front();
        }
        self.recent_frames.push_back(frame.to_string());
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
        self.app
            .update(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)))
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
        self.app.update(Event::Key(KeyEvent::new(
            KeyCode::Char(c),
            KeyModifiers::CONTROL,
        )))
    }

    /// Render the current state into a fresh terminal and record it under
    /// `label`. Each captured frame is an independent clean render, so the
    /// joined [`sequence`](Self::sequence) is a faithful step-by-step
    /// "image sequence" free of cross-frame vt100 bleed.
    pub(crate) fn capture(&mut self, label: &str) -> &mut Self {
        let mut vt = Vt100TestRuntime::new(self.width, self.height);
        vt.render_frame(&mut self.app);
        let contents = vt.screen_contents();
        self.remember(&contents);
        self.frames.push(format!("─── {label} ───\n{contents}"));
        self
    }

    /// Render and return the current visible screen (plain text).
    pub(crate) fn screen(&mut self) -> String {
        self.vt.render_frame(&mut self.app);
        let contents = self.vt.screen_contents();
        self.remember(&contents);
        contents
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

    /// Feed a single `Tick` event (advances spinner/animation state).
    pub(crate) fn tick(&mut self) -> Action<ChatAppMsg> {
        self.app.update(Event::Tick)
    }

    /// Pump `Tick`s until the rendered frame stops changing — two consecutive
    /// identical frames — or `max_ticks` is exhausted. Returns the final frame.
    ///
    /// This runtime is synchronous: a frame only changes because a `Tick`
    /// advanced spinner/animation state, so the tick budget is the
    /// deterministic analog of a wall-clock timeout. Prefer this over asserting
    /// a single post-message frame when the UI animates (spinners, streaming
    /// indicators).
    pub(crate) fn settle(&mut self, max_ticks: usize) -> String {
        let mut prev = self.fresh_screen();
        self.remember(&prev);
        for _ in 0..max_ticks {
            self.tick();
            let next = self.fresh_screen();
            self.remember(&next);
            if next == prev {
                return next;
            }
            prev = next;
        }
        prev
    }

    /// Pump `Tick`s until `pred` holds for the rendered frame, or panic after
    /// `max_ticks` with the last frame in the message (an eventual-state
    /// assertion). On success, pump one more tick and warn on stderr if the
    /// frame changed — a cheap indeterminism check (a settled UI should not
    /// keep mutating).
    pub(crate) fn expect_frame(
        &mut self,
        pred: impl Fn(&str) -> bool,
        max_ticks: usize,
    ) -> String {
        let mut last = self.fresh_screen();
        self.remember(&last);
        let mut ticks = 0;
        while !pred(&last) {
            assert!(
                ticks < max_ticks,
                "expect_frame: predicate never held within {max_ticks} ticks.\n\
                 Last frame:\n{last}"
            );
            self.tick();
            last = self.fresh_screen();
            self.remember(&last);
            ticks += 1;
        }
        // Indeterminism check: a satisfied, settled frame should be stable.
        self.tick();
        let after = self.fresh_screen();
        if after != last {
            self.remember(&after);
            eprintln!(
                "WARNING [expect_frame]: the frame changed on the tick after the \
                 predicate held (matched at tick {ticks}) — possible \
                 nondeterministic render."
            );
        }
        last
    }

    /// Print the current frame as a paste-ready `insta` inline-snapshot body:
    /// copy the block between the markers into `assert_snapshot!(frame, @r#"…"#)`.
    /// An authoring aid — never leave a call to this in a committed test.
    // Called ad hoc while authoring a snapshot; intentionally has no committed
    // call site.
    #[allow(dead_code)]
    pub(crate) fn print_expectation(&mut self) {
        let frame = self.fresh_screen();
        eprintln!("--- paste into insta inline snapshot: assert_snapshot!(x, @r#\" ---");
        eprintln!("{frame}");
        eprintln!("--- \"#) end snapshot body ---");
    }
}

impl Drop for StoryRuntime {
    fn drop(&mut self) {
        if std::thread::panicking() && !self.recent_frames.is_empty() {
            let n = self.recent_frames.len();
            eprintln!("\n=== StoryRuntime: last {n} frame(s) before panic ===");
            for (i, frame) in self.recent_frames.iter().enumerate() {
                eprintln!("--- frame -{} ---\n{frame}", n - 1 - i);
            }
            eprintln!("=== end frame dump ===");
        }
    }
}
