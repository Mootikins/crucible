//! Fake transcripts for the full-screen prototype: the frame-time
//! measurement, the tests and `cru chat --fullscreen-demo` use them. They
//! need no daemon.

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
    s.push_str("Wide text: 日本語のテキストと中文字符 and a family 👨\u{200d}👩\u{200d}👧 emoji.\n");
    s
}

/// Add one finished exchange to `app`.
pub fn push_exchange(app: &mut OilChatApp, i: usize) {
    app.on_message(ChatAppMsg::UserMessage(user_text(i)));
    app.on_message(ChatAppMsg::TextDelta(assistant_text(i)));
    app.on_message(ChatAppMsg::StreamComplete);
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
