//! One-shot topic-title completion.
//!
//! Lives behind the provider seam (architecture gate: `genai` types stay in
//! `provider/` + `agent_factory.rs`). The agent manager builds the client
//! via `agent_factory::build_chat_client_for_agent` and hands it here.

use genai::chat::{ChatMessage, ChatOptions, ChatRequest};

const TITLE_SYSTEM_PROMPT: &str = "You name conversations. Given the opening exchange of a \
    session, reply with a short descriptive title (3 to 7 words) capturing its topic. Reply \
    with the title only - no quotes, no trailing punctuation, no explanations.";

/// Max characters of each message fed to the title prompt.
const TITLE_CONTEXT_CLIP: usize = 1500;

pub(crate) async fn generate_title_via_backend(
    client: &genai::Client,
    model: &str,
    user_msg: &str,
    assistant_msg: Option<&str>,
) -> Result<String, String> {
    let mut exchange = format!("User: {}", clip_chars(user_msg, TITLE_CONTEXT_CLIP));
    if let Some(assistant) = assistant_msg {
        exchange.push_str("\n\nAssistant: ");
        exchange.push_str(&clip_chars(assistant, TITLE_CONTEXT_CLIP));
    }

    let request = ChatRequest::new(vec![
        ChatMessage::system(TITLE_SYSTEM_PROMPT),
        ChatMessage::user(exchange),
    ]);
    let options = ChatOptions::default().with_capture_content(true);
    let resp = client
        .exec_chat(model, request, Some(&options))
        .await
        .map_err(|e| format!("title call failed: {e}"))?;
    Ok(sanitize_title(&resp.content.texts().join("")))
}

fn clip_chars(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

/// Normalize a model's title reply: first non-empty line, quotes and
/// "Title:" scaffolding stripped, whitespace collapsed, length-capped.
fn sanitize_title(raw: &str) -> String {
    let first_line = raw
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    let mut title = first_line
        .trim_matches(|c: char| matches!(c, '"' | '\'' | '\u{201c}' | '\u{201d}' | '`'))
        .trim()
        .to_string();
    for prefix in ["Title:", "title:", "TITLE:"] {
        if let Some(stripped) = title.strip_prefix(prefix) {
            title = stripped.trim().to_string();
        }
    }
    while title.ends_with('.') {
        title.pop();
    }
    let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
    if title.chars().count() > 80 {
        let clipped: String = title.chars().take(77).collect();
        format!("{}...", clipped.trim_end())
    } else {
        title
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_strips_quotes_and_trailing_period() {
        assert_eq!(
            sanitize_title("\"Fixing the auth flow.\""),
            "Fixing the auth flow"
        );
    }

    #[test]
    fn sanitize_strips_title_prefix_and_takes_first_line() {
        assert_eq!(
            sanitize_title("Title: Session archiving sweep\n\nExplanation follows"),
            "Session archiving sweep"
        );
    }

    #[test]
    fn sanitize_collapses_whitespace_and_caps_length() {
        let long = "word ".repeat(40);
        let result = sanitize_title(&long);
        assert!(result.chars().count() <= 80);
        assert!(result.ends_with("..."));
    }

    #[test]
    fn sanitize_empty_input_yields_empty() {
        assert_eq!(sanitize_title("   \n  "), "");
    }
}
