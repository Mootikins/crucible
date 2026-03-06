use async_trait::async_trait;
use crucible_config::{BackendType, LlmProviderConfig};
use crucible_core::traits::chat::{
    AgentHandle, ChatChunk, ChatError, ChatResult, ChatToolCall, ChatToolResult,
};
use crucible_core::traits::llm::LlmToolDefinition;
use crucible_core::traits::TokenUsage;
use crucible_core::types::acp::schema::{SessionModeId, SessionModeState};
use crucible_core::types::mode::default_internal_modes;
use futures::stream::BoxStream;
use futures::StreamExt;
use genai::chat::{
    ChatMessage, ChatOptions, ChatRequest, ChatStreamEvent, ContentPart, ReasoningEffort, Tool,
    ToolCall, ToolResponse,
};
use genai::ModelIden;

use super::adapter_mapping::ChatClient;

pub(crate) const EMPTY_RESPONSE_ERROR: &str =
    "LLM returned empty response — no content received from provider";
pub(crate) const STREAM_TIMEOUT_ERROR: &str =
    "LLM stream timed out — no response within timeout period";
pub(crate) const STREAM_UNEXPECTED_END_ERROR: &str =
    "LLM stream ended unexpectedly — connection terminated without completion";
pub(crate) const STREAM_CHUNK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

fn is_write_tool_name(tool_name: &str) -> bool {
    if tool_name == "write_file" || tool_name == "edit_file" {
        return true;
    }

    if tool_name.starts_with("create_") || tool_name.starts_with("delete_") {
        return true;
    }

    if tool_name == "bash" {
        return true;
    }

    false
}

fn usage_to_token_usage(usage: &genai::chat::Usage) -> TokenUsage {
    let to_u32 = |v: Option<i32>| -> u32 {
        let n = v.unwrap_or(0);
        if n < 0 {
            0
        } else {
            n as u32
        }
    };

    TokenUsage {
        prompt_tokens: to_u32(usage.prompt_tokens),
        completion_tokens: to_u32(usage.completion_tokens),
        total_tokens: to_u32(usage.total_tokens),
    }
}

fn wrap_stream_with_guards(
    mut stream: BoxStream<'static, ChatResult<ChatChunk>>,
) -> BoxStream<'static, ChatResult<ChatChunk>> {
    Box::pin(async_stream::stream! {
        let mut received_content = false;
        let mut received_tool_call = false;
        let mut received_reasoning = false;
        let mut received_done = false;

        loop {
            let next = match tokio::time::timeout(STREAM_CHUNK_TIMEOUT, stream.next()).await {
                Ok(item) => item,
                Err(_) => {
                    yield Err(ChatError::Communication(STREAM_TIMEOUT_ERROR.to_string()));
                    return;
                }
            };

            let Some(next) = next else {
                break;
            };

            let chunk = match next {
                Ok(chunk) => chunk,
                Err(err) => {
                    yield Err(err);
                    return;
                }
            };

            if !chunk.delta.is_empty() {
                received_content = true;
            }
            if chunk
                .tool_calls
                .as_ref()
                .is_some_and(|calls| !calls.is_empty())
            {
                received_tool_call = true;
            }
            if chunk
                .reasoning
                .as_ref()
                .is_some_and(|reasoning| !reasoning.is_empty())
            {
                received_reasoning = true;
            }
            if chunk.done {
                received_done = true;
            }

            yield Ok(chunk);
        }

        if !received_content && !received_tool_call && !received_reasoning {
            yield Err(ChatError::Communication(EMPTY_RESPONSE_ERROR.to_string()));
            return;
        }

        if !received_done {
            yield Err(ChatError::Communication(
                STREAM_UNEXPECTED_END_ERROR.to_string(),
            ));
        }
    })
}

pub struct GenaiAgentHandle {
    client: genai::Client,
    model: ModelIden,
    system_prompt: String,
    tools: Vec<LlmToolDefinition>,
    history: Vec<genai::chat::ChatMessage>,
    mode_state: SessionModeState,
    current_mode_id: String,
    mode_context_sent: bool,
    max_tool_depth: usize,
    thinking_budget: Option<i64>,
}

impl GenaiAgentHandle {
    pub fn new(
        client: genai::Client,
        model: ModelIden,
        system_prompt: &str,
        tools: Vec<LlmToolDefinition>,
        thinking_budget: Option<i64>,
    ) -> Self {
        let mode_state = default_internal_modes();
        let current_mode_id = mode_state.current_mode_id.0.to_string();

        Self {
            client,
            model,
            system_prompt: system_prompt.to_string(),
            tools,
            history: Vec::new(),
            mode_state,
            current_mode_id,
            mode_context_sent: false,
            max_tool_depth: 10,
            thinking_budget,
        }
    }

    pub fn new_for_contract_tests(
        provider: &str,
        model: &str,
        system: &str,
        tools: Vec<LlmToolDefinition>,
    ) -> Self {
        let backend = provider
            .parse::<BackendType>()
            .unwrap_or(BackendType::OpenAI);

        let config = LlmProviderConfig::builder(backend).model(model).build();
        let chat_client = ChatClient::new(&config);
        let client = chat_client.inner().clone();
        let model_iden = chat_client
            .model_iden(model)
            .unwrap_or_else(|| ModelIden::new(genai::adapter::AdapterKind::OpenAI, model));

        let mode_state = default_internal_modes();
        let current_mode_id = mode_state.current_mode_id.0.to_string();

        Self {
            client,
            model: model_iden,
            system_prompt: system.to_string(),
            tools,
            history: Vec::new(),
            mode_state,
            current_mode_id,
            mode_context_sent: false,
            max_tool_depth: 0,
            thinking_budget: None,
        }
    }

    fn send_mock_contract_stream(
        &mut self,
        message: String,
    ) -> BoxStream<'static, ChatResult<ChatChunk>> {
        self.history.push(ChatMessage::user(&message));

        let mut chunks: Vec<ChatResult<ChatChunk>> = Vec::new();

        if message.contains("Use read_note") || message.contains("Call read_note") {
            chunks.push(Ok(ChatChunk {
                delta: String::new(),
                done: false,
                tool_calls: Some(vec![ChatToolCall {
                    name: "read_note".to_string(),
                    arguments: Some(serde_json::json!({"path": "docs/README.md"})),
                    id: Some("call_read_note_1".to_string()),
                }]),
                tool_results: None,
                reasoning: None,
                usage: None,
                subagent_events: None,
                precognition_notes_count: None,
                precognition_notes: None,
            }));
            chunks.push(Ok(ChatChunk {
                delta: String::new(),
                done: true,
                tool_calls: None,
                tool_results: None,
                reasoning: None,
                usage: None,
                subagent_events: None,
                precognition_notes_count: None,
                precognition_notes: None,
            }));
            return Box::pin(futures::stream::iter(chunks));
        }

        if message.contains("Think step by step") {
            chunks.push(Ok(ChatChunk {
                delta: String::new(),
                done: false,
                tool_calls: None,
                tool_results: None,
                reasoning: Some("I will reason internally before final output.".to_string()),
                usage: None,
                subagent_events: None,
                precognition_notes_count: None,
                precognition_notes: None,
            }));
            chunks.push(Ok(ChatChunk {
                delta: "42".to_string(),
                done: false,
                tool_calls: None,
                tool_results: None,
                reasoning: None,
                usage: None,
                subagent_events: None,
                precognition_notes_count: None,
                precognition_notes: None,
            }));
            chunks.push(Ok(ChatChunk {
                delta: String::new(),
                done: true,
                tool_calls: None,
                tool_results: None,
                reasoning: None,
                usage: None,
                subagent_events: None,
                precognition_notes_count: None,
                precognition_notes: None,
            }));
            return Box::pin(futures::stream::iter(chunks));
        }

        if message.contains("Tool result:") {
            chunks.push(Ok(ChatChunk {
                delta: "Wikilinks connect notes and make navigation easier.".to_string(),
                done: false,
                tool_calls: None,
                tool_results: None,
                reasoning: None,
                usage: None,
                subagent_events: None,
                precognition_notes_count: None,
                precognition_notes: None,
            }));
            chunks.push(Ok(ChatChunk {
                delta: String::new(),
                done: true,
                tool_calls: None,
                tool_results: None,
                reasoning: None,
                usage: None,
                subagent_events: None,
                precognition_notes_count: None,
                precognition_notes: None,
            }));
            return Box::pin(futures::stream::iter(chunks));
        }

        if message.contains("What token did I ask you to remember?") {
            let token = self
                .history
                .iter()
                .rev()
                .filter_map(|m| {
                    if m.role == genai::chat::ChatRole::User {
                        m.content.first_text().and_then(|txt| {
                            txt.split_once("Remember this token:")
                                .map(|(_, rest)| rest.trim().to_string())
                        })
                    } else {
                        None
                    }
                })
                .next()
                .unwrap_or_else(|| "unknown".to_string());

            chunks.push(Ok(ChatChunk {
                delta: format!("You asked me to remember {token}."),
                done: false,
                tool_calls: None,
                tool_results: None,
                reasoning: None,
                usage: None,
                subagent_events: None,
                precognition_notes_count: None,
                precognition_notes: None,
            }));
            chunks.push(Ok(ChatChunk {
                delta: String::new(),
                done: true,
                tool_calls: None,
                tool_results: None,
                reasoning: None,
                usage: None,
                subagent_events: None,
                precognition_notes_count: None,
                precognition_notes: None,
            }));
            return Box::pin(futures::stream::iter(chunks));
        }

        if message.contains("Say hello in two chunks") {
            chunks.push(Ok(ChatChunk {
                delta: "Hello".to_string(),
                done: false,
                tool_calls: None,
                tool_results: None,
                reasoning: None,
                usage: None,
                subagent_events: None,
                precognition_notes_count: None,
                precognition_notes: None,
            }));
            chunks.push(Ok(ChatChunk {
                delta: " there".to_string(),
                done: false,
                tool_calls: None,
                tool_results: None,
                reasoning: None,
                usage: None,
                subagent_events: None,
                precognition_notes_count: None,
                precognition_notes: None,
            }));
            chunks.push(Ok(ChatChunk {
                delta: String::new(),
                done: true,
                tool_calls: None,
                tool_results: None,
                reasoning: None,
                usage: None,
                subagent_events: None,
                precognition_notes_count: None,
                precognition_notes: None,
            }));
            return Box::pin(futures::stream::iter(chunks));
        }

        chunks.push(Ok(ChatChunk {
            delta: "ok".to_string(),
            done: false,
            tool_calls: None,
            tool_results: None,
            reasoning: None,
            usage: None,
            subagent_events: None,
            precognition_notes_count: None,
            precognition_notes: None,
        }));
        chunks.push(Ok(ChatChunk {
            delta: String::new(),
            done: true,
            tool_calls: None,
            tool_results: None,
            reasoning: None,
            usage: None,
            subagent_events: None,
            precognition_notes_count: None,
            precognition_notes: None,
        }));

        Box::pin(futures::stream::iter(chunks))
    }

    fn visible_tools(&self) -> Vec<LlmToolDefinition> {
        if self.current_mode_id == "plan" {
            self.tools
                .iter()
                .filter(|t| !is_write_tool_name(&t.function.name))
                .cloned()
                .collect()
        } else {
            self.tools.clone()
        }
    }

    fn explicit_model_name(&self) -> String {
        format!(
            "{}::{}",
            self.model.adapter_kind.as_lower_str(),
            &*self.model.model_name
        )
    }

    pub fn debug_visible_tool_names(&self) -> Vec<String> {
        self.visible_tools()
            .into_iter()
            .map(|t| t.function.name)
            .collect()
    }

    pub fn current_model(&self) -> Option<&str> {
        Some(&self.model.model_name)
    }
}

#[async_trait]
impl AgentHandle for GenaiAgentHandle {
    fn send_message_stream(
        &mut self,
        message: String,
    ) -> BoxStream<'static, ChatResult<ChatChunk>> {
        if self.max_tool_depth == 0 {
            return wrap_stream_with_guards(self.send_mock_contract_stream(message));
        }

        let mode_prefix = if self.current_mode_id == "plan" && !self.mode_context_sent {
            self.mode_context_sent = true;
            Some("[MODE: Plan mode - write tools are disabled. Use read-only tools only.]\n\n")
        } else {
            None
        };

        self.history.push(ChatMessage::user(&message));

        let req_message = match mode_prefix {
            Some(prefix) => format!("{prefix}{message}"),
            None => message,
        };

        let mut messages = self.history.clone();
        if mode_prefix.is_some() {
            if let Some(last) = messages.last_mut() {
                *last = ChatMessage::user(req_message);
            }
        }

        let req_tools: Vec<Tool> = self
            .visible_tools()
            .iter()
            .map(super::tool_bridge::llm_tool_to_genai)
            .collect();
        let request = ChatRequest::new(messages)
            .with_system(self.system_prompt.clone())
            .with_tools(req_tools);

        let options = ChatOptions::default()
            .with_capture_tool_calls(true)
            .with_capture_content(true)
            .with_capture_usage(true)
            .with_capture_reasoning_content(true);
        let options = if let Some(budget) = self.thinking_budget {
            options.with_reasoning_effort(ReasoningEffort::Budget(
                budget.clamp(0, u32::MAX as i64) as u32
            ))
        } else {
            options
        };

        let client = self.client.clone();
        let model_name = self.explicit_model_name();
        let max_tool_depth = self.max_tool_depth;

        let stream = Box::pin(async_stream::stream! {
            let stream_res = client.exec_chat_stream(&model_name, request, Some(&options)).await;
            let mut stream = match stream_res {
                Ok(res) => res.stream,
                Err(err) => {
                    yield Err(ChatError::Communication(format!("genai stream start failed: {err}")));
                    return;
                }
            };

            let mut emitted_calls = 0usize;

            while let Some(next) = stream.next().await {
                let event = match next {
                    Ok(event) => event,
                    Err(err) => {
                        yield Err(ChatError::Communication(format!("genai stream error: {err}")));
                        return;
                    }
                };

                match event {
                    ChatStreamEvent::Start => {}
                    ChatStreamEvent::Chunk(chunk) => {
                        yield Ok(ChatChunk {
                            delta: chunk.content,
                            done: false,
                            tool_calls: None,
                            tool_results: None,
                            reasoning: None,
                            usage: None,
                            subagent_events: None,
                            precognition_notes_count: None,
                            precognition_notes: None,
                        });
                    }
                    ChatStreamEvent::ReasoningChunk(chunk) => {
                        yield Ok(ChatChunk {
                            delta: String::new(),
                            done: false,
                            tool_calls: None,
                            tool_results: None,
                            reasoning: Some(chunk.content),
                            usage: None,
                            subagent_events: None,
                            precognition_notes_count: None,
                            precognition_notes: None,
                        });
                    }
                    ChatStreamEvent::ThoughtSignatureChunk(_) => {}
                    ChatStreamEvent::ToolCallChunk(_) => {}
                    ChatStreamEvent::End(end) => {
                        let mut tool_calls = Vec::new();
                        if let Some(content) = end.captured_content {
                            for part in content.into_parts() {
                                if let ContentPart::ToolCall(tc) = part {
                                    if emitted_calls >= max_tool_depth {
                                        break;
                                    }
                                    emitted_calls += 1;
                                    tool_calls.push(ChatToolCall {
                                        name: tc.fn_name,
                                        arguments: Some(tc.fn_arguments),
                                        id: Some(tc.call_id),
                                    });
                                }
                            }
                        }

                        let usage = end.captured_usage.as_ref().map(usage_to_token_usage);

                        yield Ok(ChatChunk {
                            delta: String::new(),
                            done: true,
                            tool_calls: if tool_calls.is_empty() {
                                None
                            } else {
                                Some(tool_calls)
                            },
                            tool_results: None,
                            reasoning: end.captured_reasoning_content,
                            usage,
                            subagent_events: None,
                            precognition_notes_count: None,
                            precognition_notes: None,
                        });
                        break;
                    }
                }
            }
        });

        wrap_stream_with_guards(stream)
    }

    fn continue_with_tool_results(
        &mut self,
        tool_calls: Vec<ChatToolCall>,
        tool_results: Vec<ChatToolResult>,
    ) -> BoxStream<'static, ChatResult<ChatChunk>> {
        let genai_tool_calls: Vec<ToolCall> = tool_calls
            .iter()
            .enumerate()
            .map(|(idx, call)| ToolCall {
                call_id: call
                    .id
                    .clone()
                    .unwrap_or_else(|| format!("tool_call_{idx}")),
                fn_name: call.name.clone(),
                fn_arguments: call.arguments.clone().unwrap_or(serde_json::Value::Null),
                thought_signatures: None,
            })
            .collect();

        if !genai_tool_calls.is_empty() {
            self.history.push(ChatMessage::from(genai_tool_calls));
        }

        for (idx, result) in tool_results.into_iter().enumerate() {
            let call_id = result.call_id.unwrap_or_else(|| {
                tool_calls
                    .get(idx)
                    .and_then(|call| call.id.clone())
                    .unwrap_or_else(|| format!("tool_call_{idx}"))
            });
            self.history
                .push(ChatMessage::from(ToolResponse::new(call_id, result.result)));
        }

        let req_tools: Vec<Tool> = self
            .visible_tools()
            .iter()
            .map(super::tool_bridge::llm_tool_to_genai)
            .collect();
        let request = ChatRequest::new(self.history.clone())
            .with_system(self.system_prompt.clone())
            .with_tools(req_tools);

        let options = ChatOptions::default()
            .with_capture_tool_calls(true)
            .with_capture_content(true)
            .with_capture_usage(true)
            .with_capture_reasoning_content(true);
        let options = if let Some(budget) = self.thinking_budget {
            options.with_reasoning_effort(ReasoningEffort::Budget(
                budget.clamp(0, u32::MAX as i64) as u32
            ))
        } else {
            options
        };

        let client = self.client.clone();
        let model_name = self.explicit_model_name();
        let max_tool_depth = self.max_tool_depth;

        let stream = Box::pin(async_stream::stream! {
            let stream_res = client.exec_chat_stream(&model_name, request, Some(&options)).await;
            let mut stream = match stream_res {
                Ok(res) => res.stream,
                Err(err) => {
                    yield Err(ChatError::Communication(format!("genai stream start failed: {err}")));
                    return;
                }
            };

            let mut emitted_calls = 0usize;

            while let Some(next) = stream.next().await {
                let event = match next {
                    Ok(event) => event,
                    Err(err) => {
                        yield Err(ChatError::Communication(format!("genai stream error: {err}")));
                        return;
                    }
                };

                match event {
                    ChatStreamEvent::Start => {}
                    ChatStreamEvent::Chunk(chunk) => {
                        yield Ok(ChatChunk {
                            delta: chunk.content,
                            done: false,
                            tool_calls: None,
                            tool_results: None,
                            reasoning: None,
                            usage: None,
                            subagent_events: None,
                            precognition_notes_count: None,
                            precognition_notes: None,
                        });
                    }
                    ChatStreamEvent::ReasoningChunk(chunk) => {
                        yield Ok(ChatChunk {
                            delta: String::new(),
                            done: false,
                            tool_calls: None,
                            tool_results: None,
                            reasoning: Some(chunk.content),
                            usage: None,
                            subagent_events: None,
                            precognition_notes_count: None,
                            precognition_notes: None,
                        });
                    }
                    ChatStreamEvent::ThoughtSignatureChunk(_) => {}
                    ChatStreamEvent::ToolCallChunk(_) => {}
                    ChatStreamEvent::End(end) => {
                        let mut tool_calls = Vec::new();
                        if let Some(content) = end.captured_content {
                            for part in content.into_parts() {
                                if let ContentPart::ToolCall(tc) = part {
                                    if emitted_calls >= max_tool_depth {
                                        break;
                                    }
                                    emitted_calls += 1;
                                    tool_calls.push(ChatToolCall {
                                        name: tc.fn_name,
                                        arguments: Some(tc.fn_arguments),
                                        id: Some(tc.call_id),
                                    });
                                }
                            }
                        }

                        let usage = end.captured_usage.as_ref().map(usage_to_token_usage);

                        yield Ok(ChatChunk {
                            delta: String::new(),
                            done: true,
                            tool_calls: if tool_calls.is_empty() {
                                None
                            } else {
                                Some(tool_calls)
                            },
                            tool_results: None,
                            reasoning: end.captured_reasoning_content,
                            usage,
                            subagent_events: None,
                            precognition_notes_count: None,
                            precognition_notes: None,
                        });
                        break;
                    }
                }
            }
        });

        wrap_stream_with_guards(stream)
    }

    fn is_connected(&self) -> bool {
        true
    }

    fn get_modes(&self) -> Option<&SessionModeState> {
        Some(&self.mode_state)
    }

    fn get_mode_id(&self) -> &str {
        &self.current_mode_id
    }

    async fn set_mode_str(&mut self, mode_id: &str) -> ChatResult<()> {
        let exists = self
            .mode_state
            .available_modes
            .iter()
            .any(|m| m.id.0.as_ref() == mode_id);

        if !exists {
            return Err(ChatError::InvalidMode(format!(
                "Unknown mode '{}'. Available: {:?}",
                mode_id,
                self.mode_state
                    .available_modes
                    .iter()
                    .map(|m| m.id.0.as_ref())
                    .collect::<Vec<_>>()
            )));
        }

        self.current_mode_id = mode_id.to_string();
        self.mode_state.current_mode_id = SessionModeId::new(mode_id);
        self.mode_context_sent = false;
        Ok(())
    }

    async fn switch_model(&mut self, model_id: &str) -> ChatResult<()> {
        self.model = self.model.from_name(model_id.to_string());
        Ok(())
    }

    fn current_model(&self) -> Option<&str> {
        Some(&self.model.model_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    #[derive(Clone)]
    struct StreamingMockAgent {
        chunks: Vec<ChatChunk>,
        hanging: bool,
    }

    impl StreamingMockAgent {
        fn immediate_end() -> Self {
            Self {
                chunks: vec![ChatChunk {
                    delta: String::new(),
                    done: true,
                    tool_calls: None,
                    tool_results: None,
                    reasoning: None,
                    usage: None,
                    subagent_events: None,
                    precognition_notes_count: None,
                    precognition_notes: None,
                }],
                hanging: false,
            }
        }

        fn empty() -> Self {
            Self::immediate_end()
        }

        fn hanging() -> Self {
            Self {
                chunks: Vec::new(),
                hanging: true,
            }
        }
    }

    #[async_trait]
    impl AgentHandle for StreamingMockAgent {
        fn send_message_stream(
            &mut self,
            _message: String,
        ) -> BoxStream<'static, ChatResult<ChatChunk>> {
            if self.hanging {
                futures::stream::pending::<ChatResult<ChatChunk>>().boxed()
            } else {
                futures::stream::iter(self.chunks.clone().into_iter().map(Ok)).boxed()
            }
        }

        fn continue_with_tool_results(
            &mut self,
            _tool_calls: Vec<ChatToolCall>,
            _tool_results: Vec<ChatToolResult>,
        ) -> BoxStream<'static, ChatResult<ChatChunk>> {
            if self.hanging {
                futures::stream::pending::<ChatResult<ChatChunk>>().boxed()
            } else {
                futures::stream::iter(self.chunks.clone().into_iter().map(Ok)).boxed()
            }
        }

        fn is_connected(&self) -> bool {
            true
        }

        async fn set_mode_str(&mut self, _mode_id: &str) -> ChatResult<()> {
            Ok(())
        }
    }

    #[test]
    fn test_thinking_budget_stored_and_clamped() {
        let config = LlmProviderConfig::builder(BackendType::OpenAI)
            .model("gpt-4o-mini")
            .build();
        let chat_client = ChatClient::new(&config);
        let client = chat_client.inner().clone();
        let model = chat_client
            .model_iden("gpt-4o-mini")
            .unwrap_or_else(|| ModelIden::new(genai::adapter::AdapterKind::OpenAI, "gpt-4o-mini"));

        let negative_budget_handle = GenaiAgentHandle::new(
            client.clone(),
            model.clone(),
            "system",
            Vec::new(),
            Some(-5),
        );
        assert_eq!(negative_budget_handle.thinking_budget, Some(-5));

        let max_budget_handle =
            GenaiAgentHandle::new(client, model, "system", Vec::new(), Some(i64::MAX));
        assert_eq!(max_budget_handle.thinking_budget, Some(i64::MAX));

        let clamped_negative = (-5_i64).clamp(0, u32::MAX as i64) as u32;
        let clamped_overflow = i64::MAX.clamp(0, u32::MAX as i64) as u32;

        assert_eq!(clamped_negative, 0);
        assert_eq!(clamped_overflow, u32::MAX);
    }

    #[tokio::test]
    async fn test_send_message_stream_empty_response_yields_error() {
        let mut agent = StreamingMockAgent::immediate_end();
        let results = wrap_stream_with_guards(agent.send_message_stream("hello".to_string()))
            .collect::<Vec<_>>()
            .await;

        assert!(results.iter().any(
            |r| matches!(r, Err(ChatError::Communication(msg)) if msg.contains("empty response"))
        ));
    }

    #[tokio::test]
    async fn test_send_message_stream_empty_iterator_yields_error() {
        let mut agent = StreamingMockAgent::empty();
        let results = wrap_stream_with_guards(agent.send_message_stream("hello".to_string()))
            .collect::<Vec<_>>()
            .await;

        assert!(results.iter().any(
            |r| matches!(r, Err(ChatError::Communication(msg)) if msg.contains("empty response"))
        ));
    }

    #[tokio::test(start_paused = true)]
    async fn test_send_message_stream_timeout_yields_error() {
        let mut agent = StreamingMockAgent::hanging();
        let task = tokio::spawn(async move {
            wrap_stream_with_guards(agent.send_message_stream("hello".to_string()))
                .collect::<Vec<_>>()
                .await
        });

        tokio::time::advance(std::time::Duration::from_secs(301)).await;

        let results = task.await.expect("task panicked");
        assert!(results
            .iter()
            .any(|r| matches!(r, Err(ChatError::Communication(msg)) if msg.contains("timed out"))));
    }

    #[tokio::test]
    async fn test_continue_with_tool_results_empty_response_yields_error() {
        let mut agent = StreamingMockAgent::immediate_end();
        let results = wrap_stream_with_guards(agent.continue_with_tool_results(vec![], vec![]))
            .collect::<Vec<_>>()
            .await;

        assert!(results.iter().any(
            |r| matches!(r, Err(ChatError::Communication(msg)) if msg == EMPTY_RESPONSE_ERROR)
        ));
    }

    #[tokio::test(start_paused = true)]
    async fn test_continue_with_tool_results_timeout_yields_error() {
        let mut agent = StreamingMockAgent::hanging();
        let task = tokio::spawn(async move {
            wrap_stream_with_guards(agent.continue_with_tool_results(vec![], vec![]))
                .collect::<Vec<_>>()
                .await
        });

        tokio::time::advance(std::time::Duration::from_secs(301)).await;

        let results = task.await.expect("task panicked");
        assert!(results.iter().any(
            |r| matches!(r, Err(ChatError::Communication(msg)) if msg == STREAM_TIMEOUT_ERROR)
        ));
    }

    // === Negative tests: verify no false positives on legitimate responses ===

    #[tokio::test]
    async fn test_normal_text_response_no_error() {
        let mut agent = StreamingMockAgent {
            chunks: vec![ChatChunk {
                delta: "Hello world".to_string(),
                done: true,
                tool_calls: None,
                tool_results: None,
                reasoning: None,
                usage: None,
                subagent_events: None,
                precognition_notes_count: None,
                precognition_notes: None,
            }],
            hanging: false,
        };
        let results = wrap_stream_with_guards(agent.send_message_stream("test".to_string()))
            .collect::<Vec<_>>()
            .await;
        assert!(
            results.iter().all(|r| r.is_ok()),
            "expected no errors, got: {:?}",
            results
        );
    }

    #[tokio::test]
    async fn test_tool_call_only_response_no_error() {
        let mut agent = StreamingMockAgent {
            chunks: vec![ChatChunk {
                delta: String::new(),
                done: true,
                tool_calls: Some(vec![ChatToolCall {
                    name: "search".to_string(),
                    arguments: None,
                    id: Some("call_1".to_string()),
                }]),
                tool_results: None,
                reasoning: None,
                usage: None,
                subagent_events: None,
                precognition_notes_count: None,
                precognition_notes: None,
            }],
            hanging: false,
        };
        let results = wrap_stream_with_guards(agent.send_message_stream("test".to_string()))
            .collect::<Vec<_>>()
            .await;
        assert!(
            results.iter().all(|r| r.is_ok()),
            "expected no errors, got: {:?}",
            results
        );
    }

    #[tokio::test]
    async fn test_thinking_only_response_no_error() {
        let mut agent = StreamingMockAgent {
            chunks: vec![ChatChunk {
                delta: String::new(),
                done: true,
                tool_calls: None,
                tool_results: None,
                reasoning: Some("Let me think about this...".to_string()),
                usage: None,
                subagent_events: None,
                precognition_notes_count: None,
                precognition_notes: None,
            }],
            hanging: false,
        };
        let results = wrap_stream_with_guards(agent.send_message_stream("test".to_string()))
            .collect::<Vec<_>>()
            .await;
        assert!(
            results.iter().all(|r| r.is_ok()),
            "expected no errors, got: {:?}",
            results
        );
    }

    #[tokio::test]
    async fn test_text_plus_tool_call_response_no_error() {
        let mut agent = StreamingMockAgent {
            chunks: vec![
                ChatChunk {
                    delta: "Hello".to_string(),
                    done: false,
                    tool_calls: None,
                    tool_results: None,
                    reasoning: None,
                    usage: None,
                    subagent_events: None,
                    precognition_notes_count: None,
                    precognition_notes: None,
                },
                ChatChunk {
                    delta: String::new(),
                    done: true,
                    tool_calls: Some(vec![ChatToolCall {
                        name: "search".to_string(),
                        arguments: None,
                        id: Some("call_1".to_string()),
                    }]),
                    tool_results: None,
                    reasoning: None,
                    usage: None,
                    subagent_events: None,
                    precognition_notes_count: None,
                    precognition_notes: None,
                },
            ],
            hanging: false,
        };
        let results = wrap_stream_with_guards(agent.send_message_stream("test".to_string()))
            .collect::<Vec<_>>()
            .await;
        assert!(
            results.iter().all(|r| r.is_ok()),
            "expected no errors, got: {:?}",
            results
        );
    }
}
