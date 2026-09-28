//! Fake transcripts for the full-screen prototype. They need no daemon.
//!
//! The tests, the frame-time measurement and the `fullscreen_demo` example
//! use them. They are test data, so the library has them only in a test
//! build, and the `cru` binary does not carry them. The example includes
//! this file with `#[path]`, and it gives the file the `crate::tui` path.

use crate::tui::oil::chat_app::{ChatAppMsg, OilChatApp};

/// A user question for exchange `i`.
pub fn user_text(i: usize) -> String {
    format!(
        "Question {i}: explain how the renderer keeps the scroll position when the \
         terminal width changes, and what it copies when a selection crosses a wrapped line."
    )
}

/// An assistant answer for exchange `i`: about 40 source lines of
/// markdown with paragraphs that wrap, a list, a code block, CJK text and a
/// ZWJ emoji.
pub fn assistant_text(i: usize) -> String {
    let mut s = format!("## Answer {i}\n\n");
    for p in 0..4 {
        s.push_str(&format!(
            "Paragraph {p} of answer {i}. The full-screen mode owns every row of the \
             screen, so it can rewrite one row by its address. It keeps the rows of the \
             last frame and writes only the rows that differ, inside one synchronized \
             update. A row that the transcript wraps is still one logical line, and a \
             copy joins the wrapped rows again with the exact source text between them.\n\n"
        ));
    }
    for item in 0..6 {
        s.push_str(&format!(
            "- Item {item}: a list entry with enough words to fill part of the row.\n"
        ));
    }
    s.push_str("\n```rust\n");
    for line in 0..12 {
        s.push_str(&format!(
            "fn step_{line}(input: &str) -> usize {{ input.len() * {line} }}\n"
        ));
    }
    s.push_str("```\n\n");
    s.push_str(
        "Wide text: 日本語のテキストと中文字符 and a family 👨\u{200d}👩\u{200d}👧 emoji.\n",
    );
    s
}

/// Add one finished exchange to `app`. The turn id comes from `i`, so a
/// feed for each exchange gives each turn its own items.
pub fn push_exchange(app: &mut OilChatApp, i: usize) {
    let mut feed = EventFeed::default();
    app.on_message(ChatAppMsg::UserMessage(user_text(i)));
    let events = [
        (
            "user_message",
            serde_json::json!({ "message_id": format!("exchange-{i}"), "content": user_text(i) }),
        ),
        (
            "text_delta",
            serde_json::json!({ "content": assistant_text(i) }),
        ),
        (
            "message_complete",
            serde_json::json!({ "full_response": "" }),
        ),
    ];
    for (name, data) in events {
        for msg in feed.msgs(name, data) {
            app.on_message(msg);
        }
    }
}

/// An app that holds `exchanges` finished exchanges.
pub fn app_with_exchanges(exchanges: usize) -> OilChatApp {
    let mut app = OilChatApp::default();
    for i in 0..exchanges {
        push_exchange(&mut app, i);
    }
    app
}

/// The pieces of a streamed answer, a few words at a time, as a provider
/// sends them.
pub fn stream_deltas(i: usize) -> Vec<String> {
    let text = assistant_text(i);
    let words: Vec<&str> = text.split_inclusive(' ').collect();
    words.chunks(3).map(|chunk| chunk.concat()).collect()
}

/// The path of a live session, for fake events: the daemon's event bus
/// folds each event and puts the ops on it, and the TUI consumer turns the
/// event into messages. A raw event carries no ops, so the caller needs the
/// fold. One feed keeps one fold, as one session does.
#[derive(Default)]
pub struct EventFeed {
    fold: crucible_core::transcript::TranscriptFold,
    stream: crate::tui::oil::chat_runner::SessionEventStream,
}

impl EventFeed {
    /// The app messages of the event `name` with `data`.
    pub fn msgs(&mut self, name: &str, data: serde_json::Value) -> Vec<ChatAppMsg> {
        self.event(crucible_core::protocol::SessionEventMessage::new(
            "test", name, data,
        ))
    }

    /// The app messages of `event`.
    pub fn event(
        &mut self,
        mut event: crucible_core::protocol::SessionEventMessage,
    ) -> Vec<ChatAppMsg> {
        event.transcript = self.fold.apply(&event);
        crate::tui::oil::chat_runner::event_msgs(&mut self.stream, &event)
    }
}
