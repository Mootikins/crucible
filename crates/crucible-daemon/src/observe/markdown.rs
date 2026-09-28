//! The markdown export of a session: its transcript as a document.
//!
//! The export reads the transcript that the daemon folds once
//! ([`crucible_core::transcript`]), as the TUI and the web client do. It has
//! no rules of its own about where a part of a turn goes.

use crucible_core::transcript::{
    DelegationStatus, ItemBody, Notice, ToolStatus, Transcript, TranscriptItem,
};
use crucible_core::turn::TurnStatus;
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

/// Render a transcript to markdown.
pub fn render_to_markdown(transcript: &Transcript, options: &RenderOptions) -> String {
    let mut render = Render {
        output: String::new(),
        options,
        answering: None,
    };
    for item in &transcript.items {
        render.item(item);
    }
    render.output
}

struct Render<'a> {
    output: String,
    options: &'a RenderOptions,
    /// The turn whose `## Assistant` heading is already written. The segments
    /// of one answer and the tools between them go under one heading.
    answering: Option<&'a str>,
}

impl<'a> Render<'a> {
    fn item(&mut self, item: &'a TranscriptItem) {
        let shown = match &item.body {
            ItemBody::ToolCard { .. } => self.options.include_tools,
            _ => true,
        };
        if !shown {
            return;
        }
        if self.options.include_timestamps {
            if let Some(ts) = item.timestamp {
                writeln!(self.output, "<!-- {} -->", ts.format("%H:%M:%S")).unwrap();
            }
        }
        match &item.body {
            ItemBody::UserTurn {
                content,
                origin,
                precognition,
            } => {
                self.answering = None;
                self.user(origin.as_ref().and_then(|o| o.plugin()), content);
                if let Some(p) = precognition {
                    self.system(&format!(
                        "Context injected: {} note(s) for \"{}\"",
                        p.notes_count, p.query_summary
                    ));
                }
            }
            ItemBody::AssistantSegment {
                text,
                thinking,
                usage,
                model,
                ..
            } => {
                if self.answering != item.turn_id.as_deref() || item.turn_id.is_none() {
                    self.heading(model.as_deref());
                    self.answering = item.turn_id.as_deref();
                }
                if !thinking.is_empty() {
                    self.callout("[!thinking]- Thinking", thinking);
                }
                if !text.is_empty() {
                    writeln!(self.output, "{}\n", self.cut(text)).unwrap();
                }
                if let Some(usage) = usage.filter(|_| self.options.include_tokens) {
                    if let (Some(prompt), Some(completion)) =
                        (usage.prompt_tokens, usage.completion_tokens)
                    {
                        write!(self.output, "*Tokens: {prompt} in, {completion} out").unwrap();
                        // Cache accounting only when the provider reported it:
                        // "0 cached" and "not measured" are different claims.
                        if let Some(cached) = usage.cache_read_tokens {
                            write!(self.output, ", {cached} cached").unwrap();
                        }
                        writeln!(self.output, "*\n").unwrap();
                    }
                }
            }
            ItemBody::ToolCard {
                call_id,
                name,
                args,
                status,
                result,
                error,
                ..
            } => {
                writeln!(self.output, "### Tool: `{name}` (id: {call_id})\n").unwrap();
                let args = serde_json::to_string_pretty(args).unwrap_or_else(|_| args.to_string());
                writeln!(self.output, "```json\n{args}\n```\n").unwrap();
                match status {
                    ToolStatus::Complete => {
                        let result = result.as_deref().unwrap_or_default();
                        writeln!(self.output, "#### Result (id: {call_id})\n").unwrap();
                        writeln!(self.output, "```\n{}\n```\n", self.cut(result)).unwrap();
                    }
                    ToolStatus::Failed => {
                        let error = error.as_deref().unwrap_or_default();
                        writeln!(self.output, "#### Result (id: {call_id}) - ERROR\n").unwrap();
                        writeln!(self.output, "```\n{error}\n```\n").unwrap();
                    }
                    // The call has no answer to show.
                    ToolStatus::Running | ToolStatus::Incomplete => {}
                }
            }
            ItemBody::Delegation {
                delegation_id,
                prompt,
                status,
                outcome,
                ..
            } => {
                writeln!(self.output, "### Subagent (`{delegation_id}`)\n").unwrap();
                writeln!(self.output, "> **Task**: {}\n", self.cut(prompt)).unwrap();
                let label = match status {
                    DelegationStatus::Running => None,
                    DelegationStatus::Complete => Some("Result"),
                    DelegationStatus::Failed => Some("Error"),
                };
                if let (Some(label), Some(outcome)) = (label, outcome) {
                    writeln!(self.output, "> **{label}**: {}\n", self.cut(outcome)).unwrap();
                }
            }
            ItemBody::InjectedContext {
                role,
                content,
                kind,
                source,
                ..
            } => match role.as_str() {
                "user" => {
                    self.answering = None;
                    let plugin = source
                        .as_deref()
                        .filter(|_| kind.as_deref() == Some("plugin"));
                    self.user(plugin, content);
                }
                "assistant" => {
                    self.heading(None);
                    self.answering = None;
                    writeln!(self.output, "{}\n", self.cut(content)).unwrap();
                }
                _ => self.system(content),
            },
            ItemBody::Notice { notice } => match notice {
                Notice::ContextCleared { plugin } => match plugin {
                    Some(plugin) => {
                        writeln!(self.output, "---\n\nContext cleared by {plugin}\n").unwrap()
                    }
                    None => writeln!(self.output, "---\n\nContext cleared\n").unwrap(),
                },
                Notice::StopReason { text, .. } => {
                    writeln!(self.output, "> **Notice:** {text}\n").unwrap();
                }
                Notice::TurnFailed { status, error } => {
                    let what = match status {
                        TurnStatus::TimedOut => "The turn timed out",
                        _ => "The turn failed",
                    };
                    match error {
                        Some(error) => writeln!(self.output, "> **Error:** {what}: {error}\n"),
                        None => writeln!(self.output, "> **Error:** {what}\n"),
                    }
                    .unwrap();
                }
            },
        }
    }

    fn user(&mut self, plugin: Option<&str>, content: &str) {
        match plugin {
            Some(name) => writeln!(self.output, "## ↻ {name}\n").unwrap(),
            None => writeln!(self.output, "## User\n").unwrap(),
        }
        writeln!(self.output, "{}\n", self.cut(content)).unwrap();
    }

    fn heading(&mut self, model: Option<&str>) {
        match model {
            Some(model) => writeln!(self.output, "## Assistant ({model})\n").unwrap(),
            None => writeln!(self.output, "## Assistant\n").unwrap(),
        }
    }

    fn system(&mut self, content: &str) {
        self.callout("[!system]- System Prompt", content);
    }

    fn callout(&mut self, title: &str, content: &str) {
        writeln!(self.output, "> {title}").unwrap();
        for line in self.cut(content).lines() {
            writeln!(self.output, "> {line}").unwrap();
        }
        writeln!(self.output).unwrap();
    }

    /// Cut to `max_content_length` bytes. A limit of 0 means no limit.
    fn cut<'s>(&self, s: &'s str) -> &'s str {
        match self.options.max_content_length {
            0 => s,
            max => crucible_core::text::truncate_bytes(s, max),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::protocol::SessionEventMessage;
    use crucible_core::transcript::TranscriptFold;
    use serde_json::json;

    fn render(events: &[(&str, serde_json::Value)], options: &RenderOptions) -> String {
        let events: Vec<SessionEventMessage> = events
            .iter()
            .map(|(name, data)| {
                let mut e = SessionEventMessage::new("s", *name, data.clone());
                e.timestamp = Some("2026-09-01T10:11:12Z".parse().unwrap());
                e
            })
            .collect();
        render_to_markdown(&TranscriptFold::of_events(&events), options)
    }

    fn tool_turn() -> Vec<(&'static str, serde_json::Value)> {
        vec![
            (
                "model_switched",
                json!({"model_id": "m-1", "provider": "p"}),
            ),
            (
                "user_message",
                json!({"message_id": "t1", "content": "Hello"}),
            ),
            ("thinking", json!({"content": "look first"})),
            (
                "segment_complete",
                json!({"message_id": "t1", "index": 0, "content": "I will look."}),
            ),
            (
                "tool_call",
                json!({"call_id": "c1", "tool": "read_file", "args": {"path": "a"}}),
            ),
            (
                "tool_result",
                json!({"call_id": "c1", "tool": "read_file", "result": {"result": "fn main() {}"}}),
            ),
            (
                "message_complete",
                json!({"message_id": "t1", "full_response": "I will look.It is small.",
                       "prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15,
                       "cache_read_tokens": 3}),
            ),
        ]
    }

    /// The segments of one answer and the tool between them go under one
    /// heading that names the model.
    #[test]
    fn one_answer_has_one_heading_with_its_model() {
        let md = render(&tool_turn(), &RenderOptions::default());
        assert_eq!(md.matches("## Assistant (m-1)").count(), 1, "{md}");
        let order = [
            "## User",
            "look first",
            "I will look.",
            "### Tool: `read_file` (id: c1)",
            "#### Result (id: c1)",
            "fn main() {}",
            "It is small.",
            "*Tokens: 10 in, 5 out, 3 cached*",
        ];
        let at: Vec<usize> = order.iter().map(|s| md.find(s).expect(s)).collect();
        assert!(at.windows(2).all(|w| w[0] < w[1]), "{md}");
    }

    #[test]
    fn the_options_leave_out_tools_and_tokens_and_add_times() {
        let md = render(
            &tool_turn(),
            &RenderOptions {
                include_tools: false,
                include_tokens: false,
                include_timestamps: true,
                ..Default::default()
            },
        );
        assert!(!md.contains("### Tool"), "{md}");
        assert!(!md.contains("*Tokens"), "{md}");
        assert!(md.contains("<!-- 10:11:12 -->"), "{md}");
    }

    #[test]
    fn a_failed_tool_shows_its_error() {
        let md = render(
            &[
                ("user_message", json!({"message_id": "t1", "content": "go"})),
                (
                    "tool_call",
                    json!({"call_id": "c1", "tool": "bash", "args": {}}),
                ),
                (
                    "tool_result",
                    json!({"call_id": "c1", "tool": "bash", "result": {"error": "File not found"}}),
                ),
            ],
            &RenderOptions::default(),
        );
        assert!(md.contains("#### Result (id: c1) - ERROR"), "{md}");
        assert!(md.contains("File not found"), "{md}");
    }

    #[test]
    fn a_clear_and_a_failed_turn_are_notices() {
        let md = render(
            &[
                ("user_message", json!({"message_id": "t1", "content": "go"})),
                (
                    "turn_finished",
                    json!({"status": "failed", "error": "provider down"}),
                ),
                ("context_cleared", json!({"plugin": "goal"})),
            ],
            &RenderOptions::default(),
        );
        assert!(
            md.contains("> **Error:** The turn failed: provider down"),
            "{md}"
        );
        assert!(md.contains("---\n\nContext cleared by goal"), "{md}");
    }

    #[test]
    fn a_long_text_is_cut() {
        let md = render(
            &[(
                "user_message",
                json!({"message_id": "t1", "content": "a".repeat(100)}),
            )],
            &RenderOptions {
                max_content_length: 10,
                ..Default::default()
            },
        );
        assert!(md.contains(&"a".repeat(10)), "{md}");
        assert!(!md.contains(&"a".repeat(11)), "{md}");
    }
}
