//! JSONL to Markdown rendering for session export

use crate::events::LogEvent;
use std::fmt::Write;

/// Options for markdown rendering
#[derive(Debug, Clone)]
pub struct RenderOptions {
    /// Include timestamps in output
    pub include_timestamps: bool,
    /// Include token usage stats
    pub include_tokens: bool,
    /// Include tool call details
    pub include_tools: bool,
    /// Maximum content length before truncation (0 = no limit)
    pub max_content_length: usize,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            include_timestamps: false,
            include_tokens: true,
            include_tools: true,
            max_content_length: 0,
        }
    }
}

/// Render a sequence of events to markdown
pub fn render_to_markdown(events: &[LogEvent], options: &RenderOptions) -> String {
    let mut output = String::new();

    for event in events {
        render_event(&mut output, event, options);
    }

    output
}

fn render_event(output: &mut String, event: &LogEvent, options: &RenderOptions) {
    match event {
        LogEvent::System { ts, content } => {
            if options.include_timestamps {
                writeln!(output, "<!-- system: {} -->", ts.format("%H:%M:%S")).unwrap();
            }
            writeln!(output, "<details><summary>System Prompt</summary>\n").unwrap();
            writeln!(output, "{}", truncate(content, options.max_content_length)).unwrap();
            writeln!(output, "\n</details>\n").unwrap();
        }

        LogEvent::User { ts, content } => {
            if options.include_timestamps {
                writeln!(output, "<!-- {} -->", ts.format("%H:%M:%S")).unwrap();
            }
            writeln!(output, "## User\n").unwrap();
            writeln!(output, "{}\n", truncate(content, options.max_content_length)).unwrap();
        }

        LogEvent::Assistant {
            ts,
            content,
            model,
            tokens,
        } => {
            if options.include_timestamps {
                writeln!(output, "<!-- {} -->", ts.format("%H:%M:%S")).unwrap();
            }

            let mut header = "## Assistant".to_string();
            if let Some(model) = model {
                write!(header, " ({model})").unwrap();
            }
            writeln!(output, "{header}\n").unwrap();

            writeln!(output, "{}\n", truncate(content, options.max_content_length)).unwrap();

            if options.include_tokens {
                if let Some(tokens) = tokens {
                    writeln!(
                        output,
                        "*Tokens: {} in, {} out*\n",
                        tokens.input, tokens.output
                    )
                    .unwrap();
                }
            }
        }

        LogEvent::ToolCall {
            ts,
            id,
            name,
            args,
        } => {
            if !options.include_tools {
                return;
            }

            if options.include_timestamps {
                writeln!(output, "<!-- {} -->", ts.format("%H:%M:%S")).unwrap();
            }
            writeln!(output, "### Tool: `{name}` (id: {id})\n").unwrap();
            writeln!(output, "```json").unwrap();
            writeln!(
                output,
                "{}",
                serde_json::to_string_pretty(args).unwrap_or_else(|_| args.to_string())
            )
            .unwrap();
            writeln!(output, "```\n").unwrap();
        }

        LogEvent::ToolResult {
            ts,
            id,
            result,
            truncated,
            error,
        } => {
            if !options.include_tools {
                return;
            }

            if options.include_timestamps {
                writeln!(output, "<!-- {} -->", ts.format("%H:%M:%S")).unwrap();
            }

            if let Some(err) = error {
                writeln!(output, "#### Result (id: {id}) - ERROR\n").unwrap();
                writeln!(output, "```").unwrap();
                writeln!(output, "{err}").unwrap();
                writeln!(output, "```\n").unwrap();
            } else {
                let truncated_marker = if *truncated { " (truncated)" } else { "" };
                writeln!(output, "#### Result (id: {id}){truncated_marker}\n").unwrap();
                writeln!(output, "```").unwrap();
                writeln!(output, "{}", truncate(result, options.max_content_length)).unwrap();
                writeln!(output, "```\n").unwrap();
            }
        }

        LogEvent::Error {
            ts,
            message,
            recoverable,
        } => {
            if options.include_timestamps {
                writeln!(output, "<!-- {} -->", ts.format("%H:%M:%S")).unwrap();
            }
            let severity = if *recoverable { "Warning" } else { "Error" };
            writeln!(output, "> **{severity}:** {message}\n").unwrap();
        }
    }
}

fn truncate(s: &str, max_len: usize) -> &str {
    if max_len == 0 || s.len() <= max_len {
        s
    } else {
        // Find char boundary
        let mut end = max_len;
        while !s.is_char_boundary(end) && end > 0 {
            end -= 1;
        }
        &s[..end]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::TokenUsage;

    #[test]
    fn test_render_user_message() {
        let events = vec![LogEvent::user("Hello, world!")];
        let md = render_to_markdown(&events, &RenderOptions::default());

        assert!(md.contains("## User"));
        assert!(md.contains("Hello, world!"));
    }

    #[test]
    fn test_render_assistant_with_model() {
        let events = vec![LogEvent::assistant_with_model(
            "Hi there!",
            "claude-3-haiku",
            Some(TokenUsage {
                input: 10,
                output: 5,
            }),
        )];
        let md = render_to_markdown(&events, &RenderOptions::default());

        assert!(md.contains("## Assistant (claude-3-haiku)"));
        assert!(md.contains("Hi there!"));
        assert!(md.contains("*Tokens: 10 in, 5 out*"));
    }

    #[test]
    fn test_render_without_tokens() {
        let events = vec![LogEvent::assistant_with_model(
            "Hi!",
            "model",
            Some(TokenUsage {
                input: 10,
                output: 5,
            }),
        )];
        let md = render_to_markdown(
            &events,
            &RenderOptions {
                include_tokens: false,
                ..Default::default()
            },
        );

        assert!(!md.contains("Tokens"));
    }

    #[test]
    fn test_render_tool_call() {
        let events = vec![LogEvent::tool_call(
            "tc1",
            "read_file",
            serde_json::json!({"path": "test.rs"}),
        )];
        let md = render_to_markdown(&events, &RenderOptions::default());

        assert!(md.contains("### Tool: `read_file`"));
        assert!(md.contains("tc1"));
        assert!(md.contains("\"path\""));
    }

    #[test]
    fn test_render_tool_result() {
        let events = vec![LogEvent::tool_result("tc1", "fn main() {}")];
        let md = render_to_markdown(&events, &RenderOptions::default());

        assert!(md.contains("#### Result (id: tc1)"));
        assert!(md.contains("fn main()"));
    }

    #[test]
    fn test_render_tool_error() {
        let events = vec![LogEvent::tool_error("tc1", "File not found")];
        let md = render_to_markdown(&events, &RenderOptions::default());

        assert!(md.contains("ERROR"));
        assert!(md.contains("File not found"));
    }

    #[test]
    fn test_render_without_tools() {
        let events = vec![
            LogEvent::tool_call("tc1", "test", serde_json::json!({})),
            LogEvent::tool_result("tc1", "result"),
        ];
        let md = render_to_markdown(
            &events,
            &RenderOptions {
                include_tools: false,
                ..Default::default()
            },
        );

        assert!(!md.contains("Tool"));
        assert!(!md.contains("Result"));
    }

    #[test]
    fn test_render_system_prompt() {
        let events = vec![LogEvent::system("You are helpful")];
        let md = render_to_markdown(&events, &RenderOptions::default());

        assert!(md.contains("<details>"));
        assert!(md.contains("System Prompt"));
        assert!(md.contains("You are helpful"));
    }

    #[test]
    fn test_render_error_recoverable() {
        let events = vec![LogEvent::error("Rate limited", true)];
        let md = render_to_markdown(&events, &RenderOptions::default());

        assert!(md.contains("**Warning:**"));
        assert!(md.contains("Rate limited"));
    }

    #[test]
    fn test_render_error_fatal() {
        let events = vec![LogEvent::error("Connection lost", false)];
        let md = render_to_markdown(&events, &RenderOptions::default());

        assert!(md.contains("**Error:**"));
    }

    #[test]
    fn test_render_with_timestamps() {
        let events = vec![LogEvent::user("Hello")];
        let md = render_to_markdown(
            &events,
            &RenderOptions {
                include_timestamps: true,
                ..Default::default()
            },
        );

        assert!(md.contains("<!--"));
    }

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("hello", 0), "hello"); // no limit
        assert_eq!(truncate("hello", 10), "hello"); // under limit
        assert_eq!(truncate("hello", 3), "hel"); // at limit
    }

    #[test]
    fn test_full_conversation() {
        let events = vec![
            LogEvent::system("You are a helpful assistant."),
            LogEvent::user("What is 2+2?"),
            LogEvent::assistant_with_model(
                "2+2 equals 4.",
                "gpt-4",
                Some(TokenUsage {
                    input: 20,
                    output: 10,
                }),
            ),
        ];

        let md = render_to_markdown(&events, &RenderOptions::default());

        assert!(md.contains("System Prompt"));
        assert!(md.contains("## User"));
        assert!(md.contains("## Assistant (gpt-4)"));
        assert!(md.contains("2+2 equals 4"));
    }
}
