use anyhow::Result;
use async_trait::async_trait;
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use std::pin::Pin;

pub mod ollama;

/// A chat message in the conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

impl Message {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: content.into(),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: content.into(),
        }
    }

    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".to_string(),
            content: content.into(),
        }
    }
}

/// Backend trait for LLM providers
/// Each implementation translates AgentCard parameters to provider-specific protocols
#[async_trait]
pub trait Backend: Send + Sync {
    /// Send a chat request and get the complete response
    async fn chat(&self, messages: Vec<Message>, params: &ChatParams) -> Result<String>;

    /// Send a chat request and stream the response token by token
    async fn chat_stream(
        &self,
        messages: Vec<Message>,
        params: &ChatParams,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>>;

    /// List available models (optional, returns empty vec if not supported)
    async fn list_models(&self) -> Result<Vec<Model>> {
        Ok(vec![])
    }
}

/// Parameters for chat completion
#[derive(Debug, Clone)]
pub struct ChatParams {
    pub model: String,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
}

/// Model information from provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model {
    pub name: String,
    pub size: Option<u64>,
    pub digest: Option<String>,
    pub modified_at: Option<String>,
    pub details: Option<ModelDetails>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelDetails {
    pub format: Option<String>,
    pub family: Option<String>,
    pub parameter_size: Option<String>,
    pub quantization_level: Option<String>,
}
