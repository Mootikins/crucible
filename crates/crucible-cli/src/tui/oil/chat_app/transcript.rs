//! The TUI draws the transcript that the daemon folds.
//!
//! The daemon folds the events of a session once
//! ([`crucible_core::transcript`]) and sends the ops of the fold with each
//! live event. This module turns an op into a change of the container list.
//! It decides nothing about turns, segments or tool status: those come in the
//! items. It decides only how a terminal draws them — tool cards of one
//! stretch share a group, and a slow tool splits into two rows.

use std::collections::VecDeque;
use std::sync::Arc;

use crucible_core::transcript::{
    DelegationStatus, ItemBody, Notice, TextField, ToolStatus, Transcript, TranscriptItem,
    TranscriptOp,
};
use crucible_core::turn::TurnOrigin;

use super::message_handlers::{parse_tool_source, precognition_notice};
use super::OilChatApp;
use crate::tui::oil::components::thinking_component::ThinkingComponent;
use crate::tui::oil::containers::ChatNode;
use crate::tui::oil::viewport_cache::{CachedSubagent, CachedToolCall};

impl OilChatApp {
    /// Draw a whole transcript, for a resumed session.
    pub(super) fn load_transcript(&mut self, transcript: Transcript) {
        self.transcript_as_of = transcript.as_of_seq;
        for item in transcript.items {
            self.upsert_item(item);
        }
        // A snapshot holds no open turn that this TUI streams.
        self.container_list.cancel_streaming();
    }

    /// Apply the ops of one event. An op from before the snapshot is already
    /// in it.
    pub(super) fn apply_transcript(&mut self, seq: Option<u64>, ops: Vec<TranscriptOp>) {
        if seq.is_some_and(|seq| seq <= self.transcript_as_of) {
            return;
        }
        for op in ops {
            match op {
                TranscriptOp::Upsert { item, .. } => self.upsert_item(*item),
                TranscriptOp::Append {
                    id, field, text, ..
                } => self.append_item_text(&id, field, &text),
            }
        }
    }

    fn upsert_item(&mut self, item: TranscriptItem) {
        let TranscriptItem { id, body, .. } = item;
        match body {
            ItemBody::UserTurn {
                content,
                origin,
                precognition,
            } => {
                if self.container_list.item_node(&id).is_none() {
                    let node = match origin {
                        None | Some(TurnOrigin::User) => {
                            if !self.container_list.claim_user_message(&id) {
                                self.container_list
                                    .push_item(&id, ChatNode::UserMessage { text: content });
                            }
                            None
                        }
                        Some(TurnOrigin::Relay(relay)) => Some(ChatNode::UserMessage {
                            text: format!("via {relay}\n{content}"),
                        }),
                        Some(TurnOrigin::Plugin(plugin)) => Some(ChatNode::SystemMessage {
                            text: format!("↻ {plugin}\n{content}"),
                        }),
                    };
                    if let Some(node) = node {
                        self.container_list.push_item(&id, node);
                    }
                }
                // The notes that grounded the answer show once, under the turn.
                if let Some(precognition) = precognition.filter(|p| p.notes_count > 0) {
                    let notes_id = format!("{id}-precognition");
                    if self.container_list.item_node(&notes_id).is_none() {
                        self.container_list.push_item(
                            &notes_id,
                            ChatNode::SystemMessage {
                                text: precognition_notice(
                                    precognition.notes_count,
                                    &precognition.notes,
                                ),
                            },
                        );
                    }
                }
            }
            ItemBody::AssistantSegment {
                text,
                thinking,
                streaming,
                ..
            } => {
                if streaming {
                    self.container_list.mark_turn_active();
                }
                let thinking = if thinking.is_empty() {
                    Vec::new()
                } else {
                    vec![ThinkingComponent::new(thinking)]
                };
                match self.container_list.item_node(&id) {
                    Some(index) => self.container_list.update_node(index, |node| {
                        if let ChatNode::AssistantResponse {
                            text: node_text,
                            thinking: node_thinking,
                            complete,
                        } = node
                        {
                            *node_text = text;
                            *node_thinking = thinking;
                            *complete = !streaming;
                        }
                    }),
                    None => self.container_list.push_item(
                        &id,
                        ChatNode::AssistantResponse {
                            text,
                            thinking,
                            complete: !streaming,
                        },
                    ),
                }
            }
            ItemBody::ToolCard {
                call_id,
                name,
                args,
                source,
                auto_approved,
                display,
                status,
                result,
                error,
                ..
            } => {
                self.container_list.mark_turn_active();
                let args = if args.is_null() {
                    String::new()
                } else {
                    args.to_string()
                };
                let render = display.as_ref().and_then(|d| d.render.clone());
                let diffs = display
                    .as_ref()
                    .map(|d| d.diffs.clone())
                    .unwrap_or_default();
                if self.container_list.item_node(&id).is_none() {
                    // The card title is the canonical tool name. A recording
                    // without a display falls back to the name on the call.
                    let title = display
                        .as_ref()
                        .map(|d| d.tool.clone())
                        .filter(|t| !t.is_empty())
                        .unwrap_or(name);
                    self.container_list.add_tool_call(CachedToolCall {
                        id: format!("tool-{title}-{call_id}"),
                        name: Arc::from(title.as_str()),
                        args: Arc::from(args.as_str()),
                        call_id: Some(call_id.clone()),
                        output_tail: VecDeque::new(),
                        output_path: None,
                        output_total_bytes: 0,
                        error: None,
                        started_at: self.frame_time(),
                        complete: false,
                        superseded: false,
                        description: None,
                        source: source.as_deref().and_then(parse_tool_source),
                        render: render.clone().map(Arc::new),
                        diffs: diffs.clone(),
                        auto_approved: auto_approved.clone(),
                        backgrounded: false,
                    });
                    self.container_list.mark_tool_item(&id);
                }
                let finished = !matches!(status, ToolStatus::Running);
                self.container_list
                    .update_tool_by_call_id(&call_id, |tool| {
                        if !args.is_empty() {
                            tool.set_args(&args);
                        }
                        if let Some(render) = render {
                            tool.render = Some(Arc::new(render));
                        }
                        if !diffs.is_empty() {
                            tool.set_diffs(diffs);
                        }
                        if auto_approved.is_some() {
                            tool.auto_approved = auto_approved;
                        }
                        if tool.complete || !finished {
                            return;
                        }
                        match status {
                            ToolStatus::Complete => {
                                if let Some(result) = &result {
                                    tool.append_output(result);
                                }
                                tool.mark_complete();
                            }
                            ToolStatus::Failed => tool.set_error(error.clone().unwrap_or_default()),
                            ToolStatus::Incomplete => {
                                tool.set_error("the tool did not complete".to_string())
                            }
                            ToolStatus::Running => {}
                        }
                    });
                // A tool that outran the split threshold writes its finish row.
                if finished {
                    let now = self.frame_time();
                    self.container_list
                        .finish_background_tool("", Some(&call_id), now);
                }
            }
            ItemBody::Delegation {
                delegation_id,
                prompt,
                target_agent,
                status,
                outcome,
            } => {
                if self.container_list.item_node(&id).is_none() {
                    let mut agent = CachedSubagent::new(
                        &delegation_id,
                        prompt,
                        "delegation",
                        self.frame_time(),
                    );
                    agent.target_agent = target_agent;
                    self.container_list
                        .push_item(&id, ChatNode::SubagentTask { agent });
                }
                let outcome = outcome.unwrap_or_default();
                match status {
                    DelegationStatus::Running => {}
                    DelegationStatus::Complete => self
                        .container_list
                        .update_agent_task(&delegation_id, |a| a.mark_completed(&outcome)),
                    DelegationStatus::Failed => self
                        .container_list
                        .update_agent_task(&delegation_id, |a| a.mark_failed(&outcome)),
                }
            }
            // Injected context goes to the model, not to the screen.
            ItemBody::InjectedContext { .. } => {}
            ItemBody::Notice { notice } => {
                if self.container_list.item_node(&id).is_some() {
                    return;
                }
                let text = match notice {
                    Notice::ContextCleared { plugin: Some(name) } => {
                        format!("── ↻ {name} cleared the context ──")
                    }
                    Notice::ContextCleared { plugin: None } => "── Context cleared ──".to_string(),
                    Notice::StopReason { text, .. } => text,
                    // A live failure also shows as an error notification;
                    // the line keeps it in the transcript.
                    Notice::TurnFailed { status, error } => match error {
                        Some(error) => format!("The turn {}: {error}", turn_status_word(status)),
                        None => format!("The turn {}.", turn_status_word(status)),
                    },
                };
                self.container_list
                    .push_item(&id, ChatNode::SystemMessage { text });
            }
        }
    }

    fn append_item_text(&mut self, id: &str, field: TextField, text: &str) {
        let Some(index) = self.container_list.item_node(id) else {
            tracing::warn!(item = %id, "an append for an item this TUI does not have");
            return;
        };
        self.container_list.mark_turn_active();
        self.container_list.update_node(index, |node| {
            if let ChatNode::AssistantResponse {
                text: node_text,
                thinking,
                ..
            } = node
            {
                match field {
                    TextField::Text => node_text.push_str(text),
                    TextField::Thinking => match thinking.last_mut() {
                        Some(block) => block.append(text),
                        None => thinking.push(ThinkingComponent::new(text.to_string())),
                    },
                }
            }
        });
    }
}

fn turn_status_word(status: crucible_core::turn::TurnStatus) -> &'static str {
    use crucible_core::turn::TurnStatus;
    match status {
        TurnStatus::Failed => "failed",
        TurnStatus::TimedOut => "timed out",
        TurnStatus::Completed => "completed",
        TurnStatus::Cancelled | TurnStatus::HandlerCancelled => "was cancelled",
    }
}
