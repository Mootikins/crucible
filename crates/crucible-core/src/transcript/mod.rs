//! The transcript of a session: what a client draws, folded once.
//!
//! The wire carries events ([`SessionEventMessage`]). A client draws turns,
//! answer segments and tool cards. The decision that turns the first into the
//! second is the *fold*, and it lives here, once. The TUI, the web client and
//! `cru acp` each folded the events themselves, and they disagreed: about
//! where narration goes after a resume, about a tool call that never ended,
//! about reasoning that a provider sends twice.
//!
//! [`TranscriptFold`] reads the events of one session in order. For each
//! event it gives the [`TranscriptOp`]s that change the transcript, and
//! [`TranscriptFold::snapshot`] gives the whole [`Transcript`]. A client that
//! holds a snapshot applies each later op with [`Transcript::apply`], so the
//! snapshot and the live stream give one result.
//!
//! The fold keeps only what a person sees. Interactions (permission and
//! question cards) are not transcript items: they belong to the moment, and
//! each client shows the open one on its own.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error_utils::strip_tool_error_prefix;
use crate::protocol::session_events::{
    JobPayload, SessionEventPayload, SettingsPayload, SetupPayload, ToolResultBody, TurnPayload,
};
use crate::protocol::SessionEventMessage;
use crate::traits::chat::PrecognitionNoteInfo;
use crate::turn::{StopReason, TurnOrigin, TurnStatus};
use crate::types::CanonicalToolCall;

#[cfg(test)]
mod tests;

/// The folded transcript of one session.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Transcript {
    /// The `seq` of the last event in the fold. A client drops each later op
    /// whose event `seq` is not above it.
    pub as_of_seq: u64,
    pub items: Vec<TranscriptItem>,
}

/// One thing that a client draws.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TranscriptItem {
    /// Stable across the live stream and a fold of the stored log: the turn
    /// id for a user turn, `{turn}-seg-{n}` for an answer segment,
    /// `tool-{call_id}` for a tool card.
    pub id: String,
    /// The turn that the item belongs to, when it belongs to one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    /// The time of the event that made the item. For an answer segment, the
    /// time of the event that ended it: a stored log has no text deltas, so
    /// only the end has one time in the live stream and in the log.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<DateTime<Utc>>,
    #[serde(flatten)]
    pub body: ItemBody,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ItemBody {
    /// What a person, a plugin or a relay asked.
    UserTurn {
        content: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        origin: Option<TurnOrigin>,
        /// The notes that Precognition gave this turn, when it ran.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        precognition: Option<Precognition>,
    },
    /// One run of answer text, with the reasoning that came before it. A
    /// tool call ends a segment; the text after the tool is a new segment.
    AssistantSegment {
        index: usize,
        text: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        thinking: String,
        /// The segment can still grow.
        streaming: bool,
        /// The token use of the turn, on its last segment.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        usage: Option<TokenUsage>,
        /// The model that the session used when the segment started, from
        /// the last `session_initialized` or `model_switched` event.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<String>,
    },
    ToolCard {
        call_id: String,
        name: String,
        #[cfg_attr(feature = "openapi", schema(value_type = serde_json::Value))]
        args: Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        auto_approved: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        display: Option<Box<CanonicalToolCall>>,
        status: ToolStatus,
        /// The output, as text.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        result: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        /// The tool asked to end the turn.
        #[serde(default)]
        terminate: bool,
    },
    Delegation {
        delegation_id: String,
        prompt: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target_agent: Option<String>,
        status: DelegationStatus,
        /// The summary of a finished delegation, or the error of a failed one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        outcome: Option<String>,
    },
    /// Context that the session accepted for a turn. It is not a user turn.
    InjectedContext {
        role: String,
        content: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        kind: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        tags: Vec<String>,
    },
    Notice {
        notice: Notice,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ToolStatus {
    Running,
    Complete,
    Failed,
    /// The turn ended before the call answered.
    Incomplete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum DelegationStatus {
    Running,
    Complete,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Notice {
    /// The model context was cleared here. The transcript keeps what came
    /// before it.
    ContextCleared {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        plugin: Option<String>,
    },
    /// Why a reply stops early, in the words of [`StopReason::user_notice`].
    StopReason {
        #[cfg_attr(feature = "openapi", schema(value_type = String))]
        reason: StopReason,
        text: String,
    },
    /// The turn failed or ran out of time.
    TurnFailed {
        #[cfg_attr(feature = "openapi", schema(value_type = String))]
        status: TurnStatus,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Precognition {
    pub notes_count: usize,
    /// The query that the search ran with.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub query_summary: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<PrecognitionNoteInfo>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TokenUsage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_tokens: Option<u32>,
    /// The prompt tokens that the provider read from its cache, when it said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read_tokens: Option<u32>,
}

/// One change to a transcript.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum TranscriptOp {
    /// Add the item, or replace the item with its id.
    Upsert {
        item: Box<TranscriptItem>,
        /// For a new item: the id of the item it goes before. `None` puts it
        /// at the end.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        before: Option<String>,
    },
    /// Add text to a field of a segment. `at` is the length of the field
    /// before the text, so a client that missed an op sees the gap.
    Append {
        id: String,
        field: TextField,
        at: usize,
        text: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum TextField {
    Text,
    Thinking,
}

impl Transcript {
    /// Apply one op. `false` means the op does not fit this transcript: an
    /// `Append` whose `at` is not the length of the field, or one for an
    /// item that is not here. The client then reads a new snapshot.
    pub fn apply(&mut self, op: &TranscriptOp) -> bool {
        match op {
            TranscriptOp::Upsert { item, before } => {
                if let Some(slot) = self.items.iter_mut().find(|i| i.id == item.id) {
                    *slot = (**item).clone();
                    return true;
                }
                let at = before
                    .as_ref()
                    .and_then(|b| self.items.iter().position(|i| &i.id == b))
                    .unwrap_or(self.items.len());
                self.items.insert(at, (**item).clone());
                true
            }
            TranscriptOp::Append {
                id,
                field,
                at,
                text,
            } => {
                let Some(item) = self.items.iter_mut().find(|i| &i.id == id) else {
                    return false;
                };
                let ItemBody::AssistantSegment {
                    text: seg_text,
                    thinking,
                    ..
                } = &mut item.body
                else {
                    return false;
                };
                let target = match field {
                    TextField::Text => seg_text,
                    TextField::Thinking => thinking,
                };
                if target.len() != *at {
                    return false;
                }
                target.push_str(text);
                true
            }
        }
    }
}

/// A reasoning run needs this many deltas before a repeat of it counts as a
/// provider's end-of-stream replay. One thought repeated once is content.
const MIN_REPLAY_RUN_DELTAS: usize = 2;

/// The open turn: what the next text, tool or end belongs to.
#[derive(Debug, Default)]
struct OpenTurn {
    id: String,
    /// The segment that text and reasoning go into now.
    open_segment: Option<usize>,
    /// The index the next segment takes.
    next_segment: usize,
    /// The text of each closed segment, in order. The final text of the turn
    /// drops this prefix.
    closed_texts: Vec<String>,
    thinking_run: String,
    thinking_run_deltas: usize,
}

/// Folds the events of one session into its transcript.
#[derive(Debug, Default)]
pub struct TranscriptFold {
    transcript: Transcript,
    turn: Option<OpenTurn>,
    /// The number of turns seen, for a turn with no id in an old log.
    turns_seen: usize,
    /// The model of the session now.
    model: Option<String>,
    /// The time of the event that the fold reads now.
    now: Option<DateTime<Utc>>,
}

impl TranscriptFold {
    pub fn new() -> Self {
        Self::default()
    }

    /// The transcript as it stands.
    pub fn snapshot(&self) -> Transcript {
        self.transcript.clone()
    }

    /// A fold that already read `events`, in order. A live fold starts here,
    /// from the stored log, so that its next ops fit the snapshot a client
    /// reads from that log.
    pub fn from_events<'a>(events: impl IntoIterator<Item = &'a SessionEventMessage>) -> Self {
        let mut fold = Self::new();
        for event in events {
            fold.apply(event);
        }
        fold
    }

    /// Fold every event of `events`, in order.
    pub fn of_events<'a>(events: impl IntoIterator<Item = &'a SessionEventMessage>) -> Transcript {
        Self::from_events(events).snapshot()
    }

    /// Fold one event. The answer is the ops that the event caused, already
    /// applied to the fold's own transcript.
    pub fn apply(&mut self, event: &SessionEventMessage) -> Vec<TranscriptOp> {
        if let Some(seq) = event.seq {
            self.transcript.as_of_seq = self.transcript.as_of_seq.max(seq);
        }
        let Ok(payload) = event.payload() else {
            return Vec::new();
        };
        self.now = event.timestamp;
        let mut ops = Vec::new();
        match payload {
            SessionEventPayload::Turn(turn) => self.turn_event(turn, &mut ops),
            SessionEventPayload::Job(job) => self.job_event(job, &mut ops),
            // The model of the later answers. An empty name is no answer: the
            // setup task can run before the model resolves.
            SessionEventPayload::Setup(SetupPayload::SessionInitialized(setup)) => {
                self.set_model(setup.model);
            }
            SessionEventPayload::Settings(SettingsPayload::ModelSwitched { model_id, .. }) => {
                self.set_model(model_id);
            }
            // Other settings, setup, review, workflow and system events
            // change no transcript item.
            _ => {}
        }
        ops
    }

    fn set_model(&mut self, model: String) {
        if !model.is_empty() {
            self.model = Some(model);
        }
    }

    /// Apply `op` to the fold's own transcript at once, so a later read in
    /// the same event sees it, and record it for the caller.
    fn emit(&mut self, ops: &mut Vec<TranscriptOp>, op: TranscriptOp) {
        let fitted = self.transcript.apply(&op);
        debug_assert!(fitted, "the fold made an op that does not fit: {op:?}");
        ops.push(op);
    }

    fn turn_event(&mut self, turn: TurnPayload, ops: &mut Vec<TranscriptOp>) {
        match turn {
            TurnPayload::UserMessage {
                message_id,
                content,
                origin,
            } => {
                self.close_turn(None, ops);
                self.turns_seen += 1;
                let id = if message_id.is_empty() {
                    format!("turn-{}", self.turns_seen)
                } else {
                    message_id
                };
                self.emit(
                    ops,
                    upsert(TranscriptItem {
                        id: id.clone(),
                        turn_id: Some(id.clone()),
                        timestamp: self.now,
                        body: ItemBody::UserTurn {
                            content,
                            origin: origin.filter(|o| *o != TurnOrigin::User),
                            precognition: None,
                        },
                    }),
                );
                self.turn = Some(OpenTurn {
                    id,
                    ..OpenTurn::default()
                });
            }
            TurnPayload::TextDelta { content } => {
                if !content.is_empty() {
                    self.append(TextField::Text, content, ops);
                }
            }
            TurnPayload::Thinking { content } => {
                if content.is_empty() {
                    return;
                }
                let turn = self.open_turn();
                // A provider that streams its reasoning can also send the whole
                // block again at the end of the stream. The repeat is byte-exact;
                // anything else is new reasoning and stays.
                if turn.thinking_run_deltas >= MIN_REPLAY_RUN_DELTAS && content == turn.thinking_run
                {
                    turn.thinking_run.clear();
                    turn.thinking_run_deltas = 0;
                    return;
                }
                turn.thinking_run.push_str(&content);
                turn.thinking_run_deltas += 1;
                self.append(TextField::Thinking, content, ops);
            }
            TurnPayload::SegmentComplete { content, .. } => {
                self.close_segment(Some(content), ops);
            }
            TurnPayload::ToolCall {
                call_id,
                tool,
                args,
                source,
                display,
                auto_approved,
                ..
            } => {
                self.close_segment(None, ops);
                let turn_id = self.turn.as_ref().map(|t| t.id.clone());
                self.emit(
                    ops,
                    upsert(TranscriptItem {
                        id: tool_id(&call_id),
                        turn_id,
                        timestamp: self.now,
                        body: ItemBody::ToolCard {
                            call_id,
                            name: tool,
                            args,
                            source,
                            auto_approved,
                            display,
                            status: ToolStatus::Running,
                            result: None,
                            error: None,
                            terminate: false,
                        },
                    }),
                );
            }
            TurnPayload::ToolCallUpdate {
                call_id,
                args,
                display,
                auto_approved,
            } => {
                let Some(mut item) = self.item(&tool_id(&call_id)).cloned() else {
                    return;
                };
                if let ItemBody::ToolCard {
                    args: card_args,
                    display: card_display,
                    auto_approved: card_approved,
                    ..
                } = &mut item.body
                {
                    // An empty update carries nothing worth replacing the
                    // arguments with.
                    if !is_empty_value(&args) {
                        *card_args = args;
                    }
                    // A display without a render carries only diffs: the card
                    // keeps its render and takes the diffs.
                    match (display, card_display.as_mut()) {
                        (Some(next), Some(current)) if next.render.is_none() => {
                            current.diffs = next.diffs;
                        }
                        (Some(next), _) => *card_display = Some(next),
                        (None, _) => {}
                    }
                    if auto_approved.is_some() {
                        *card_approved = auto_approved;
                    }
                }
                self.emit(ops, upsert(item));
            }
            TurnPayload::ToolResult {
                call_id,
                result,
                terminate,
                ..
            } => {
                // A result belongs to the card of its call. A result whose call id
                // matches no call (an old ACP recorder wrote fresh ids) has no
                // card to go on.
                let Some(mut item) = self.item(&tool_id(&call_id)).cloned() else {
                    return;
                };
                if let ItemBody::ToolCard {
                    display,
                    status,
                    result: card_result,
                    error,
                    terminate: card_terminate,
                    ..
                } = &mut item.body
                {
                    *card_terminate = terminate;
                    match ToolResultBody::of(&result) {
                        Some(body) => {
                            if let (Some(render), Some(display)) = (body.render(), display.as_mut())
                            {
                                display.render = Some(render.clone());
                            }
                            match body {
                                ToolResultBody::Ok { result, .. } => {
                                    *status = ToolStatus::Complete;
                                    *card_result = Some(result_text(&result));
                                }
                                ToolResultBody::Err { error: text, .. } => {
                                    *status = ToolStatus::Failed;
                                    *error = Some(strip_tool_error_prefix(&text));
                                }
                            }
                        }
                        // A body of another shape is still an answer.
                        None => {
                            *status = ToolStatus::Complete;
                            *card_result = Some(result_text(&result));
                        }
                    }
                }
                self.emit(ops, upsert(item));
            }
            TurnPayload::MessageComplete {
                full_response,
                prompt_tokens,
                completion_tokens,
                total_tokens,
                cache_read_tokens,
                stop_reason,
                ..
            } => {
                let usage = (prompt_tokens.is_some()
                    || completion_tokens.is_some()
                    || total_tokens.is_some())
                .then_some(TokenUsage {
                    prompt_tokens,
                    completion_tokens,
                    total_tokens,
                    cache_read_tokens,
                });
                self.finish_answer(full_response, usage, ops);
                if let Some(reason) = stop_reason {
                    if let Some(text) = reason.user_notice() {
                        let turn_id = self.turn.as_ref().map(|t| t.id.clone());
                        self.emit(
                            ops,
                            upsert(TranscriptItem {
                                id: format!("{}-stop", turn_id.clone().unwrap_or_default()),
                                turn_id,
                                timestamp: self.now,
                                body: ItemBody::Notice {
                                    notice: Notice::StopReason {
                                        reason,
                                        text: text.to_string(),
                                    },
                                },
                            }),
                        );
                    }
                }
            }
            TurnPayload::TurnFinished { status, error, .. } => {
                self.close_turn(Some((status, error)), ops);
            }
            TurnPayload::ContextCleared { plugin } => {
                let id = format!("clear-{}", self.transcript.items.len());
                self.emit(
                    ops,
                    upsert(TranscriptItem {
                        id,
                        turn_id: None,
                        timestamp: self.now,
                        body: ItemBody::Notice {
                            notice: Notice::ContextCleared { plugin },
                        },
                    }),
                );
            }
            TurnPayload::ContextInjected {
                role,
                content,
                tags,
                kind,
                source,
                after_turn,
            } => {
                // The context goes before the first turn after its anchor. A
                // stored line can sit later in the log than that turn.
                let before = after_turn.and_then(|anchor| self.first_turn_after(&anchor));
                let id = format!("context-{}", self.transcript.items.len());
                self.emit(
                    ops,
                    TranscriptOp::Upsert {
                        item: Box::new(TranscriptItem {
                            id,
                            turn_id: None,
                            timestamp: self.now,
                            body: ItemBody::InjectedContext {
                                role,
                                content,
                                kind,
                                source,
                                tags,
                            },
                        }),
                        before,
                    },
                );
            }
            TurnPayload::PrecognitionComplete {
                notes_count,
                query_summary,
                notes,
            } => {
                let Some(turn_id) = self.turn.as_ref().map(|t| t.id.clone()) else {
                    return;
                };
                let Some(mut item) = self.item(&turn_id).cloned() else {
                    return;
                };
                if let ItemBody::UserTurn { precognition, .. } = &mut item.body {
                    *precognition = Some(Precognition {
                        notes_count,
                        query_summary,
                        notes,
                    });
                }
                self.emit(ops, upsert(item));
            }
            // Interactions are not transcript items, and the model call
            // summary is a plugin hook.
            TurnPayload::InteractionRequested { .. }
            | TurnPayload::InteractionCompleted { .. }
            | TurnPayload::PostLlmCall { .. } => {}
        }
    }

    fn job_event(&mut self, job: JobPayload, ops: &mut Vec<TranscriptOp>) {
        let turn_id = self.turn.as_ref().map(|t| t.id.clone());
        match job {
            JobPayload::DelegationSpawned {
                delegation_id,
                prompt,
                target_agent,
                ..
            } => self.emit(
                ops,
                upsert(TranscriptItem {
                    id: delegation_item_id(&delegation_id),
                    turn_id,
                    timestamp: self.now,
                    body: ItemBody::Delegation {
                        delegation_id,
                        prompt,
                        target_agent,
                        status: DelegationStatus::Running,
                        outcome: None,
                    },
                }),
            ),
            JobPayload::DelegationCompleted {
                delegation_id,
                result_summary,
                ..
            } => self.finish_delegation(
                &delegation_id,
                DelegationStatus::Complete,
                result_summary,
                ops,
            ),
            JobPayload::DelegationFailed {
                delegation_id,
                error,
                ..
            } => self.finish_delegation(&delegation_id, DelegationStatus::Failed, error, ops),
            _ => {}
        }
    }

    fn finish_delegation(
        &mut self,
        delegation_id: &str,
        next: DelegationStatus,
        text: String,
        ops: &mut Vec<TranscriptOp>,
    ) {
        let Some(mut item) = self.item(&delegation_item_id(delegation_id)).cloned() else {
            return;
        };
        if let ItemBody::Delegation {
            status, outcome, ..
        } = &mut item.body
        {
            *status = next;
            *outcome = (!text.is_empty()).then_some(text);
        }
        self.emit(ops, upsert(item));
    }

    /// The open turn. Events outside a turn (a recording that starts in
    /// the middle, a plugin turn with no echo) open one.
    fn open_turn(&mut self) -> &mut OpenTurn {
        if self.turn.is_none() {
            self.turns_seen += 1;
            self.turn = Some(OpenTurn {
                id: format!("turn-{}", self.turns_seen),
                ..OpenTurn::default()
            });
        }
        self.turn.as_mut().expect("the turn was just opened")
    }

    /// Add text to the open segment, and open one when there is none.
    fn append(&mut self, field: TextField, text: String, ops: &mut Vec<TranscriptOp>) {
        let id = self.segment_for_text(ops);
        let at = match self.item(&id).map(|i| &i.body) {
            Some(ItemBody::AssistantSegment {
                text: seg_text,
                thinking,
                ..
            }) => match field {
                TextField::Text => seg_text.len(),
                TextField::Thinking => thinking.len(),
            },
            _ => 0,
        };
        self.emit(
            ops,
            TranscriptOp::Append {
                id,
                field,
                at,
                text,
            },
        );
    }

    /// The id of the open segment. A new segment starts empty.
    fn segment_for_text(&mut self, ops: &mut Vec<TranscriptOp>) -> String {
        let turn = self.open_turn();
        if let Some(index) = turn.open_segment {
            return segment_id(&turn.id, index);
        }
        let index = turn.next_segment;
        turn.open_segment = Some(index);
        turn.next_segment = index + 1;
        let turn_id = turn.id.clone();
        let item = TranscriptItem {
            id: segment_id(&turn_id, index),
            turn_id: Some(turn_id),
            // The end of the segment gives its time.
            timestamp: None,
            body: ItemBody::AssistantSegment {
                index,
                text: String::new(),
                thinking: String::new(),
                streaming: true,
                usage: None,
                model: self.model.clone(),
            },
        };
        let id = item.id.clone();
        self.emit(ops, upsert(item));
        id
    }

    /// End the open segment. `segment_complete` gives its whole text; a tool
    /// call gives none. The fold numbers the segments itself, so a segment
    /// with reasoning and no text has an index too.
    fn close_segment(&mut self, content: Option<String>, ops: &mut Vec<TranscriptOp>) {
        let content = content.filter(|c| !c.is_empty());
        let open = self
            .turn
            .as_ref()
            .and_then(|t| t.open_segment.map(|index| segment_id(&t.id, index)));
        let id = match open {
            Some(id) => id,
            // A stored log has no text deltas, so the segment opens here.
            None if content.is_some() => self.segment_for_text(ops),
            None => return,
        };
        let Some(mut item) = self.item(&id).cloned() else {
            return;
        };
        if let ItemBody::AssistantSegment {
            text, streaming, ..
        } = &mut item.body
        {
            if let Some(content) = content {
                *text = content;
            }
            *streaming = false;
            item.timestamp = self.now;
            let turn = self.open_turn();
            turn.closed_texts.push(text.clone());
            turn.open_segment = None;
        }
        self.emit(ops, upsert(item));
    }

    /// The answer of the turn is complete. The whole text of the turn comes
    /// with it; the closed segments already hold its start.
    fn finish_answer(
        &mut self,
        full_response: String,
        usage: Option<TokenUsage>,
        ops: &mut Vec<TranscriptOp>,
    ) {
        let rest = {
            let turn = self.open_turn();
            strip_closed_prefix(&full_response, &turn.closed_texts).to_string()
        };
        let open = self.turn.as_ref().and_then(|t| t.open_segment);
        let id = match open {
            Some(index) => {
                let turn_id = self.turn.as_ref().map(|t| t.id.clone()).unwrap_or_default();
                segment_id(&turn_id, index)
            }
            None if !rest.trim().is_empty() || usage.is_some() => self.segment_for_text(ops),
            None => return,
        };
        if let Some(mut item) = self.item(&id).cloned() {
            if let ItemBody::AssistantSegment {
                text,
                streaming,
                usage: seg_usage,
                ..
            } = &mut item.body
            {
                // A live segment holds the streamed text; a stored log has no
                // deltas, so the text comes from the whole answer.
                if text.is_empty() {
                    *text = rest;
                }
                *streaming = false;
                *seg_usage = usage;
            }
            item.timestamp = self.now;
            self.emit(ops, upsert(item));
        }
        if let Some(turn) = self.turn.as_mut() {
            turn.open_segment = None;
        }
    }

    /// End the open turn. A tool call that did not answer is incomplete; a
    /// failed turn gets a notice.
    fn close_turn(
        &mut self,
        end: Option<(TurnStatus, Option<String>)>,
        ops: &mut Vec<TranscriptOp>,
    ) {
        let Some(turn) = self.turn.take() else {
            return;
        };
        let open_tools: Vec<TranscriptItem> = self
            .transcript
            .items
            .iter()
            .filter(|i| i.turn_id.as_deref() == Some(turn.id.as_str()))
            .filter(|i| {
                matches!(
                    i.body,
                    ItemBody::ToolCard {
                        status: ToolStatus::Running,
                        ..
                    } | ItemBody::AssistantSegment {
                        streaming: true,
                        ..
                    }
                )
            })
            .cloned()
            .collect();
        for mut item in open_tools {
            match &mut item.body {
                ItemBody::ToolCard { status, .. } => *status = ToolStatus::Incomplete,
                ItemBody::AssistantSegment { streaming, .. } => {
                    *streaming = false;
                    item.timestamp = self.now;
                }
                _ => {}
            }
            self.emit(ops, upsert(item));
        }
        if let Some((status, error)) = end {
            if matches!(status, TurnStatus::Failed | TurnStatus::TimedOut) {
                self.emit(
                    ops,
                    upsert(TranscriptItem {
                        id: format!("{}-failed", turn.id),
                        turn_id: Some(turn.id.clone()),
                        timestamp: self.now,
                        body: ItemBody::Notice {
                            notice: Notice::TurnFailed { status, error },
                        },
                    }),
                );
            }
        }
    }

    fn item(&self, id: &str) -> Option<&TranscriptItem> {
        self.transcript.items.iter().find(|i| i.id == id)
    }

    /// The id of the first user turn after the turn `anchor`.
    fn first_turn_after(&self, anchor: &str) -> Option<String> {
        let at = self.transcript.items.iter().position(|i| i.id == anchor)?;
        self.transcript.items[at + 1..]
            .iter()
            .find(|i| matches!(i.body, ItemBody::UserTurn { .. }))
            .map(|i| i.id.clone())
    }
}

fn upsert(item: TranscriptItem) -> TranscriptOp {
    TranscriptOp::Upsert {
        item: Box::new(item),
        before: None,
    }
}

fn segment_id(turn_id: &str, index: usize) -> String {
    format!("{turn_id}-seg-{index}")
}

fn tool_id(call_id: &str) -> String {
    format!("tool-{call_id}")
}

fn delegation_item_id(delegation_id: &str) -> String {
    format!("delegation-{delegation_id}")
}

fn is_empty_value(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::Object(map) => map.is_empty(),
        Value::Array(_) | Value::Bool(_) | Value::Number(_) | Value::String(_) => false,
    }
}

/// A tool result as text: a string as it is; an object with a readable
/// `output`, `content`, `text` or `message` field, that field; other JSON in
/// its compact form. Text that reads as an error loses the nested `Error: `
/// prefixes.
fn result_text(value: &Value) -> String {
    let readable = ["output", "content", "text", "message"]
        .iter()
        .find_map(|key| value.get(key).and_then(Value::as_str));
    let text = match (value, readable) {
        (Value::String(text), _) => text.clone(),
        (_, Some(text)) => text.to_string(),
        (other, None) => other.to_string(),
    };
    if text.starts_with("Error: ") {
        strip_tool_error_prefix(&text)
    } else {
        text
    }
}

/// `full` without the text of the closed segments at its start.
///
/// The two texts come from different accumulators, so a segment whose
/// trailing whitespace `full` does not repeat still matches. On any other
/// mismatch `full` stays whole: a wrong cut loses words.
fn strip_closed_prefix<'a>(full: &'a str, closed: &[String]) -> &'a str {
    let mut rest = full;
    for segment in closed {
        if let Some(after) = rest.strip_prefix(segment.as_str()) {
            rest = after;
            continue;
        }
        let trimmed = segment.trim_end();
        match rest.strip_prefix(trimmed) {
            Some(after) if !trimmed.is_empty() => rest = after,
            _ => return full,
        }
    }
    rest
}
