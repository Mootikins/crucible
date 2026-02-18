use std::path::Path;
use std::time::Duration;

use async_trait::async_trait;
use crucible_core::traits::chat::{
    AgentHandle, ChatChunk, ChatError, ChatResult, ChatToolCall, ChatToolResult,
};
use futures::stream::BoxStream;
use futures::StreamExt;

use super::recording::{DemoEvent, RecordingReader, TimestampedEvent};

pub struct ReplayAgentHandle {
    events: Vec<TimestampedEvent>,
    cursor: usize,
    replay_speed: f64,
}

impl ReplayAgentHandle {
    pub fn from_file(path: &Path, replay_speed: f64) -> anyhow::Result<Self> {
        let mut reader = RecordingReader::open(path)?;
        let _ = reader.read_header()?;
        let events = reader.events().collect::<Result<Vec<_>, _>>()?;

        Ok(Self {
            events,
            cursor: 0,
            replay_speed,
        })
    }

    fn map_event(event: DemoEvent) -> Option<ChatResult<ChatChunk>> {
        match event {
            DemoEvent::TextDelta { delta } => Some(Ok(ChatChunk {
                delta,
                done: false,
                tool_calls: None,
                tool_results: None,
                reasoning: None,
                usage: None,
                subagent_events: None,
            })),
            DemoEvent::ThinkingDelta { delta } => Some(Ok(ChatChunk {
                delta: String::new(),
                done: false,
                tool_calls: None,
                tool_results: None,
                reasoning: Some(delta),
                usage: None,
                subagent_events: None,
            })),
            DemoEvent::ToolCall { name, args, call_id } => {
                let arguments = if args.is_empty() {
                    None
                } else {
                    Some(
                        serde_json::from_str::<serde_json::Value>(&args)
                            .unwrap_or_else(|_| serde_json::Value::String(args)),
                    )
                };

                Some(Ok(ChatChunk {
                    delta: String::new(),
                    done: false,
                    tool_calls: Some(vec![ChatToolCall {
                        name,
                        arguments,
                        id: Some(call_id.unwrap_or_default()),
                    }]),
                    tool_results: None,
                    reasoning: None,
                    usage: None,
                    subagent_events: None,
                }))
            }
            DemoEvent::ToolResultDelta {
                name,
                delta,
                call_id,
            } => Some(Ok(ChatChunk {
                delta: String::new(),
                done: false,
                tool_calls: None,
                tool_results: Some(vec![ChatToolResult {
                    name,
                    result: delta,
                    error: None,
                    call_id: Some(call_id.unwrap_or_default()),
                }]),
                reasoning: None,
                usage: None,
                subagent_events: None,
            })),
            DemoEvent::ToolResultComplete { .. } => None,
            DemoEvent::ToolResultError {
                name,
                error,
                call_id,
            } => Some(Ok(ChatChunk {
                delta: String::new(),
                done: false,
                tool_calls: None,
                tool_results: Some(vec![ChatToolResult {
                    name,
                    result: String::new(),
                    error: Some(error),
                    call_id: Some(call_id.unwrap_or_default()),
                }]),
                reasoning: None,
                usage: None,
                subagent_events: None,
            })),
            DemoEvent::StreamComplete => Some(Ok(ChatChunk {
                delta: String::new(),
                done: true,
                tool_calls: None,
                tool_results: None,
                reasoning: None,
                usage: None,
                subagent_events: None,
            })),
            DemoEvent::Error { message } => Some(Err(ChatError::Communication(message))),
            _ => None,
        }
    }
}

#[async_trait]
impl AgentHandle for ReplayAgentHandle {
    fn send_message_stream(
        &mut self,
        _message: String,
    ) -> BoxStream<'static, ChatResult<ChatChunk>> {
        let start = self.cursor;
        let done_idx = self.events[start..]
            .iter()
            .position(|ev| matches!(ev.event, DemoEvent::StreamComplete))
            .map(|idx| start + idx);

        let (response_events, append_done_chunk) = match done_idx {
            Some(end) => {
                self.cursor = end + 1;
                (self.events[start..=end].to_vec(), false)
            }
            None => {
                self.cursor = self.events.len();
                (self.events[start..].to_vec(), true)
            }
        };

        let replay_speed = self.replay_speed;

        let mut pending = Vec::new();
        if response_events.is_empty() {
            pending.push((
                0,
                Some(Ok(ChatChunk {
                    delta: String::new(),
                    done: true,
                    tool_calls: None,
                    tool_results: None,
                    reasoning: None,
                    usage: None,
                    subagent_events: None,
                })),
            ));
        } else {
            let mut prev_ts = response_events[0].ts_ms;
            let mut saw_done = false;

            for timed_event in response_events {
                let ts_delta = timed_event.ts_ms.saturating_sub(prev_ts);
                prev_ts = timed_event.ts_ms;
                let mapped = Self::map_event(timed_event.event);

                if let Some(Ok(ref chunk)) = mapped {
                    if chunk.done {
                        saw_done = true;
                    }
                }

                pending.push((ts_delta, mapped));
            }

            if append_done_chunk && !saw_done {
                pending.push((
                    0,
                    Some(Ok(ChatChunk {
                        delta: String::new(),
                        done: true,
                        tool_calls: None,
                        tool_results: None,
                        reasoning: None,
                        usage: None,
                        subagent_events: None,
                    })),
                ));
            }
        }

        Box::pin(
            futures::stream::iter(pending)
                .then(move |(ts_delta, item)| async move {
                    if ts_delta > 0 {
                        let scaled = (ts_delta as f64 * replay_speed).round();
                        let sleep_ms = if scaled.is_sign_negative() {
                            0
                        } else {
                            scaled as u64
                        };
                        if sleep_ms > 0 {
                            tokio::time::sleep(Duration::from_millis(sleep_ms)).await;
                        }
                    }
                    item
                })
                .filter_map(|item| async move { item }),
        )
    }

    fn is_connected(&self) -> bool {
        true
    }

    async fn set_mode_str(&mut self, _mode_id: &str) -> ChatResult<()> {
        Ok(())
    }

    async fn clear_history(&mut self) {}

    async fn switch_model(&mut self, _model_id: &str) -> ChatResult<()> {
        Ok(())
    }

    async fn fetch_available_models(&mut self) -> Vec<String> {
        Vec::new()
    }

    async fn set_thinking_budget(&mut self, _budget: i64) -> ChatResult<()> {
        Ok(())
    }

    async fn cancel(&self) -> ChatResult<()> {
        Ok(())
    }

    async fn set_temperature(&mut self, _temperature: f64) -> ChatResult<()> {
        Ok(())
    }

    async fn set_max_tokens(&mut self, _max_tokens: Option<u32>) -> ChatResult<()> {
        Ok(())
    }

    async fn interaction_respond(
        &mut self,
        _request_id: String,
        _response: crucible_core::interaction::InteractionResponse,
    ) -> ChatResult<()> {
        Ok(())
    }

    fn take_interaction_receiver(
        &mut self,
    ) -> Option<tokio::sync::mpsc::UnboundedReceiver<crucible_core::interaction::InteractionEvent>>
    {
        None
    }
}
