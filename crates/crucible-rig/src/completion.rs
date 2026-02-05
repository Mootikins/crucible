//! CompletionBackend implementation for Rig
//!
//! This module provides the adapter between Crucible's `CompletionBackend` trait
//! and Rig's completion models.
//!
//! ## Design
//!
//! Since Rig uses generics for different model types, we use a trait object
//! approach with a wrapper that erases the concrete model type.

use async_trait::async_trait;
use crucible_core::traits::{
    BackendCompletionChunk, BackendCompletionRequest, BackendError, BackendResult,
    CompletionBackend, ContextMessage, MessageRole, ToolCall,
};
use futures::stream::BoxStream;
use futures::StreamExt;
use rig::completion::CompletionModel;
use rig::message::{AssistantContent, Message, ToolFunction};
use rig::OneOrMany;

/// Rig-based completion backend
///
/// Wraps a Rig `CompletionModel` and implements `CompletionBackend` for
/// integration with Crucible's context management.
pub struct RigCompletionBackend<M>
where
    M: CompletionModel + Send + Sync + Clone + 'static,
{
    model: M,
    model_name: String,
    provider_name: String,
}

impl<M> RigCompletionBackend<M>
where
    M: CompletionModel + Send + Sync + Clone + 'static,
{
    /// Create a new Rig completion backend
    pub fn new(model: M, model_name: impl Into<String>, provider_name: impl Into<String>) -> Self {
        Self {
            model,
            model_name: model_name.into(),
            provider_name: provider_name.into(),
        }
    }
}

/// Convert a ContextMessage to Rig's Message type
fn context_to_rig_message(msg: &ContextMessage) -> Option<Message> {
    match msg.role {
        MessageRole::User => Some(Message::user(&msg.content)),
        MessageRole::Assistant => {
            // Handle assistant messages with or without tool calls
            if msg.metadata.tool_calls.is_empty() {
                Some(Message::assistant(&msg.content))
            } else {
                // Convert tool calls to Rig format
                let tool_calls: Vec<AssistantContent> = msg
                    .metadata
                    .tool_calls
                    .iter()
                    .map(|tc| {
                        AssistantContent::ToolCall(rig::message::ToolCall::new(
                            tc.id.clone(),
                            ToolFunction {
                                name: tc.function.name.clone(),
                                arguments: serde_json::from_str(&tc.function.arguments)
                                    .unwrap_or_default(),
                            },
                        ))
                    })
                    .collect();

                // Build content with optional text + tool calls
                let mut content: Vec<AssistantContent> = Vec::new();
                if !msg.content.is_empty() {
                    content.push(AssistantContent::text(&msg.content));
                }
                content.extend(tool_calls);

                Some(Message::Assistant {
                    id: None,
                    content: OneOrMany::many(content).unwrap_or_else(|_| {
                        // Fallback to just text if tool_calls were empty somehow
                        OneOrMany::one(AssistantContent::text(&msg.content))
                    }),
                })
            }
        }
        MessageRole::Tool => {
            // Tool result messages - in Rig these are User messages with ToolResult content
            // Can't create tool result without ID, so skip if missing
            msg.metadata
                .tool_call_id
                .as_ref()
                .map(|tool_call_id| Message::tool_result(tool_call_id.clone(), &msg.content))
        }
        MessageRole::System => {
            // System messages are handled via preamble, not chat history
            None
        }
        MessageRole::Function => {
            // Legacy function role, skip
            None
        }
    }
}

#[async_trait]
impl<M> CompletionBackend for RigCompletionBackend<M>
where
    M: CompletionModel + Send + Sync + Clone + 'static,
    M::StreamingResponse: Clone + Send + Sync + Unpin,
{
    fn complete_stream(
        &self,
        request: BackendCompletionRequest,
    ) -> BoxStream<'static, BackendResult<BackendCompletionChunk>> {
        let model = self.model.clone();
        let preamble = request.system_prompt.clone();
        let messages = request.messages.clone();

        Box::pin(async_stream::stream! {
            // Convert messages to Rig format
            let rig_messages: Vec<Message> = messages
                .iter()
                .filter_map(context_to_rig_message)
                .collect();

            // Get the last user message as the prompt
            let prompt = messages
                .iter()
                .rev()
                .find(|m| matches!(m.role, MessageRole::User))
                .map(|m| m.content.clone())
                .unwrap_or_default();

            // Build chat history (all messages except the last user message)
            let history: Vec<Message> = rig_messages
                .into_iter()
                .rev()
                .skip(1) // Skip last user message (it's the prompt)
                .rev()
                .collect();

            // Build the streaming request using CompletionRequestBuilder
            let builder = model
                .completion_request(&prompt)
                .preamble(preamble);

            // Add chat history
            let builder = history.into_iter().fold(builder, |b, msg| b.message(msg));

            // Execute the streaming request
            let stream_result = builder.stream().await;

            let mut stream = match stream_result {
                Ok(s) => s,
                Err(e) => {
                    yield Err(BackendError::Provider(e.to_string()));
                    return;
                }
            };

            while let Some(result) = stream.next().await {
                match result {
                    Ok(content) => {
                        use rig::streaming::StreamedAssistantContent;
                        match content {
                            StreamedAssistantContent::Text(text) => {
                                yield Ok(BackendCompletionChunk::text(text.text));
                            }
                            StreamedAssistantContent::ToolCall { tool_call: tc, .. } => {
                                let tool_call = ToolCall {
                                    id: tc.id.clone(),
                                    r#type: "function".to_string(),
                                    function: crucible_core::traits::FunctionCall {
                                        name: tc.function.name.clone(),
                                        arguments: serde_json::to_string(&tc.function.arguments)
                                            .unwrap_or_default(),
                                    },
                                };
                                yield Ok(BackendCompletionChunk::tool_call(tool_call));
                            }
                            _ => {
                                // Ignore other content types for now (reasoning, images)
                            }
                        }
                    }
                    Err(e) => {
                        yield Err(BackendError::Provider(e.to_string()));
                    }
                }
            }

            // Final done chunk
            yield Ok(BackendCompletionChunk::finished(None));
        })
    }

    fn provider_name(&self) -> &str {
        &self.provider_name
    }

    fn model_name(&self) -> &str {
        &self.model_name
    }

    async fn health_check(&self) -> BackendResult<bool> {
        // For now, just return true - we could add a ping mechanism later
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::traits::FunctionCall;

    // --- context_to_rig_message tests ---

    #[test]
    fn test_context_to_rig_message_user() {
        let msg = ContextMessage::user("Hello, world!");
        let rig_msg = context_to_rig_message(&msg);
        assert!(rig_msg.is_some());
    }

    #[test]
    fn test_context_to_rig_message_assistant() {
        let msg = ContextMessage::assistant("Hi there!");
        let rig_msg = context_to_rig_message(&msg);
        assert!(rig_msg.is_some());
    }

    #[test]
    fn test_context_to_rig_message_assistant_with_tool_calls() {
        let mut msg = ContextMessage::assistant("Let me check that.");
        msg.metadata.tool_calls = vec![ToolCall {
            id: "call_123".to_string(),
            r#type: "function".to_string(),
            function: FunctionCall {
                name: "read_file".to_string(),
                arguments: r#"{"path": "test.txt"}"#.to_string(),
            },
        }];

        let rig_msg = context_to_rig_message(&msg);
        assert!(rig_msg.is_some());
    }

    #[test]
    fn test_context_to_rig_message_assistant_empty_content_with_tool_calls() {
        // Assistant can have empty text but tool calls
        let mut msg = ContextMessage::assistant("");
        msg.metadata.tool_calls = vec![ToolCall {
            id: "call_456".to_string(),
            r#type: "function".to_string(),
            function: FunctionCall {
                name: "bash".to_string(),
                arguments: r#"{"command": "ls"}"#.to_string(),
            },
        }];

        let rig_msg = context_to_rig_message(&msg);
        assert!(rig_msg.is_some());
    }

    #[test]
    fn test_context_to_rig_message_system_skipped() {
        let msg = ContextMessage::system("You are helpful");
        let rig_msg = context_to_rig_message(&msg);
        // System messages are handled via preamble, not converted
        assert!(rig_msg.is_none());
    }

    #[test]
    fn test_context_to_rig_message_tool_with_id() {
        let msg = ContextMessage::tool_result("call_123", "File contents here");

        let rig_msg = context_to_rig_message(&msg);
        assert!(rig_msg.is_some());
    }

    #[test]
    fn test_context_to_rig_message_tool_without_id_skipped() {
        // Tool result without call ID cannot be converted
        // Create a Tool message manually without the ID
        let msg = ContextMessage {
            role: MessageRole::Tool,
            content: "File contents here".to_string(),
            metadata: Default::default(), // no tool_call_id
        };
        let rig_msg = context_to_rig_message(&msg);
        assert!(rig_msg.is_none());
    }

    #[test]
    fn test_context_to_rig_message_function_skipped() {
        // Legacy function role should be skipped
        let msg = ContextMessage {
            role: MessageRole::Function,
            content: "legacy function".to_string(),
            metadata: Default::default(),
        };
        let rig_msg = context_to_rig_message(&msg);
        assert!(rig_msg.is_none());
    }

    // --- RigCompletionBackend tests ---

    use rig::client::{CompletionClient, Nothing};
    use rig::providers::ollama;

    fn test_client() -> ollama::Client {
        ollama::Client::builder().api_key(Nothing).build().unwrap()
    }

    #[test]
    fn test_backend_provider_and_model_name() {
        let client = test_client();
        let model = client.completion_model("llama3.2");
        let backend = RigCompletionBackend::new(model, "llama3.2", "ollama");

        assert_eq!(backend.provider_name(), "ollama");
        assert_eq!(backend.model_name(), "llama3.2");
    }

    #[tokio::test]
    async fn test_backend_health_check() {
        let client = test_client();
        let model = client.completion_model("llama3.2");
        let backend = RigCompletionBackend::new(model, "llama3.2", "ollama");

        // Health check just returns true for now
        let result = backend.health_check().await;
        assert!(result.is_ok());
        assert!(result.unwrap());
    }
}
