//! Mock constrained generation provider for testing
//!
//! This module provides mock implementations of the `Provider` and `CanConstrainGeneration`
//! traits for use in unit and integration tests. It allows testing grammar-constrained
//! generation without requiring a real llama.cpp model.
//!
//! ## Usage
//!
//! ```rust
//! use crucible_llm::constrained_mock::MockConstrainedProvider;
//! use crucible_core::traits::provider::{CanConstrainGeneration, ConstrainedRequest};
//!
//! let provider = MockConstrainedProvider::new();
//!
//! // Configure a specific response for a grammar
//! provider.set_response("l0_l1_tools", r#"read(path="/src/main.rs")"#);
//!
//! // Or configure based on prompt content
//! provider.set_prompt_response("read a file", r#"read(path="README.md")"#);
//! ```

use async_trait::async_trait;
use crucible_config::BackendType;
use crucible_core::traits::llm::{LlmResult, ProviderCapabilities};
use crucible_core::traits::provider::{
    CanConstrainGeneration, ConstrainedRequest, ConstrainedResponse, ExtendedCapabilities,
    Provider, SchemaFormat,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Record of a constrained generation call
#[derive(Debug, Clone)]
pub struct ConstrainedCall {
    /// The prompt that was sent
    pub prompt: String,
    /// The schema/grammar content
    pub schema: String,
    /// The schema format used
    pub format: SchemaFormat,
    /// Max tokens requested
    pub max_tokens: Option<u32>,
    /// Temperature used
    pub temperature: Option<f32>,
}

/// Mock provider for testing constrained generation
///
/// Provides deterministic responses based on configured mappings. Supports
/// mapping by grammar name or prompt content.
pub struct MockConstrainedProvider {
    /// Provider name
    name: String,
    /// Supported schema formats
    supported_formats: Vec<SchemaFormat>,
    /// Responses keyed by grammar name (extracted from schema content)
    grammar_responses: Arc<Mutex<HashMap<String, String>>>,
    /// Responses keyed by prompt substring
    prompt_responses: Arc<Mutex<HashMap<String, String>>>,
    /// Default response when no mapping matches
    default_response: String,
    /// Call history for verification
    call_history: Arc<Mutex<Vec<ConstrainedCall>>>,
    /// Whether to simulate truncation
    simulate_truncation: Arc<Mutex<bool>>,
}

impl MockConstrainedProvider {
    /// Create a new mock provider with default settings
    pub fn new() -> Self {
        Self {
            name: "mock-constrained".to_string(),
            supported_formats: vec![SchemaFormat::Gbnf, SchemaFormat::JsonSchema],
            grammar_responses: Arc::new(Mutex::new(HashMap::new())),
            prompt_responses: Arc::new(Mutex::new(HashMap::new())),
            default_response: r#"read(path="default.txt")"#.to_string(),
            call_history: Arc::new(Mutex::new(Vec::new())),
            simulate_truncation: Arc::new(Mutex::new(false)),
        }
    }

    /// Create a mock that only supports GBNF
    pub fn gbnf_only() -> Self {
        Self {
            supported_formats: vec![SchemaFormat::Gbnf],
            ..Self::new()
        }
    }

    /// Create a mock that only supports JSON Schema
    pub fn json_schema_only() -> Self {
        Self {
            supported_formats: vec![SchemaFormat::JsonSchema],
            ..Self::new()
        }
    }

    /// Set a response for a specific grammar name
    ///
    /// The grammar name is matched if found anywhere in the schema content.
    ///
    /// # Example
    ///
    /// ```rust
    /// use crucible_llm::constrained_mock::MockConstrainedProvider;
    ///
    /// let provider = MockConstrainedProvider::new();
    /// provider.set_response("l0_l1_tools", r#"read(path="/src/main.rs")"#);
    /// ```
    pub fn set_response(&self, grammar_name: &str, response: &str) {
        let mut responses = self.grammar_responses.lock().unwrap();
        responses.insert(grammar_name.to_string(), response.to_string());
    }

    /// Set a response based on prompt content
    ///
    /// The prompt key is matched if found as a substring in the prompt.
    ///
    /// # Example
    ///
    /// ```rust
    /// use crucible_llm::constrained_mock::MockConstrainedProvider;
    ///
    /// let provider = MockConstrainedProvider::new();
    /// provider.set_prompt_response("read a file", r#"read(path="README.md")"#);
    /// ```
    pub fn set_prompt_response(&self, prompt_key: &str, response: &str) {
        let mut responses = self.prompt_responses.lock().unwrap();
        responses.insert(prompt_key.to_string(), response.to_string());
    }

    /// Set the default response when no mapping matches
    pub fn set_default_response(&self, response: &str) {
        let mut grammar_responses = self.grammar_responses.lock().unwrap();
        // Store under a special key that we check as fallback
        grammar_responses.insert("__default__".to_string(), response.to_string());
    }

    /// Configure the mock to simulate truncation
    pub fn simulate_truncation(&self, truncate: bool) {
        let mut truncation = self.simulate_truncation.lock().unwrap();
        *truncation = truncate;
    }

    /// Get the call history for verification
    pub fn call_history(&self) -> Vec<ConstrainedCall> {
        self.call_history.lock().unwrap().clone()
    }

    /// Clear the call history
    pub fn clear_history(&self) {
        self.call_history.lock().unwrap().clear();
    }

    /// Get the last call made to the provider
    pub fn last_call(&self) -> Option<ConstrainedCall> {
        self.call_history.lock().unwrap().last().cloned()
    }

    /// Record a call in the history
    fn record_call(&self, request: &ConstrainedRequest) {
        let mut history = self.call_history.lock().unwrap();
        history.push(ConstrainedCall {
            prompt: request.prompt.clone(),
            schema: request.schema.clone(),
            format: request.format,
            max_tokens: request.max_tokens,
            temperature: request.temperature,
        });
    }

    /// Find a response for the given request
    fn find_response(&self, request: &ConstrainedRequest) -> String {
        // First, check prompt-based responses
        {
            let prompt_responses = self.prompt_responses.lock().unwrap();
            for (key, response) in prompt_responses.iter() {
                if request.prompt.to_lowercase().contains(&key.to_lowercase()) {
                    return response.clone();
                }
            }
        }

        // Then, check grammar-based responses
        {
            let grammar_responses = self.grammar_responses.lock().unwrap();
            for (key, response) in grammar_responses.iter() {
                if key != "__default__" && request.schema.contains(key) {
                    return response.clone();
                }
            }

            // Check for default override
            if let Some(default) = grammar_responses.get("__default__") {
                return default.clone();
            }
        }

        // Fall back to hardcoded default
        self.default_response.clone()
    }
}

impl Default for MockConstrainedProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for MockConstrainedProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MockConstrainedProvider")
            .field("name", &self.name)
            .field("supported_formats", &self.supported_formats)
            .finish()
    }
}

#[async_trait]
impl Provider for MockConstrainedProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn backend_type(&self) -> BackendType {
        BackendType::Mock
    }

    fn endpoint(&self) -> Option<&str> {
        None
    }

    fn capabilities(&self) -> ExtendedCapabilities {
        ExtendedCapabilities {
            llm: ProviderCapabilities {
                text_completion: true,
                chat_completion: true,
                streaming: false,
                function_calling: false,
                tool_use: true,
                vision: false,
                audio: false,
                max_batch_size: Some(1),
                input_formats: vec!["text".to_string()],
                output_formats: vec!["text".to_string()],
            },
            embeddings: false,
            embeddings_batch: false,
            embedding_dimensions: None,
            max_batch_size: None,
        }
    }

    async fn health_check(&self) -> LlmResult<bool> {
        Ok(true)
    }
}

#[async_trait]
impl CanConstrainGeneration for MockConstrainedProvider {
    fn supported_formats(&self) -> Vec<SchemaFormat> {
        self.supported_formats.clone()
    }

    async fn generate_constrained(
        &self,
        request: ConstrainedRequest,
    ) -> LlmResult<ConstrainedResponse> {
        // Validate format is supported
        if !self.supports_format(request.format) {
            return Err(crucible_core::traits::llm::LlmError::ConfigError(format!(
                "MockConstrainedProvider does not support {:?} format",
                request.format
            )));
        }

        // Record the call
        self.record_call(&request);

        // Find and return the appropriate response
        let text = self.find_response(&request);
        let tokens = text.split_whitespace().count() as u32;
        let truncated = *self.simulate_truncation.lock().unwrap();

        Ok(ConstrainedResponse {
            text,
            tokens,
            truncated,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::types::grammar::presets;

    #[tokio::test]
    async fn test_mock_provider_default_response() {
        let provider = MockConstrainedProvider::new();

        let request = ConstrainedRequest::gbnf("Do something", "root ::= \"test\"");

        let response = provider.generate_constrained(request).await.unwrap();

        assert_eq!(response.text, r#"read(path="default.txt")"#);
        assert!(!response.truncated);
    }

    #[tokio::test]
    async fn test_mock_provider_grammar_based_response() {
        let provider = MockConstrainedProvider::new();
        // Match on content that actually appears in the l0_l1_tools grammar
        provider.set_response(
            "read | write | edit | ls | git | rg",
            r#"git(args="status")"#,
        );

        let grammar = presets::l0_l1_tools();
        let request = ConstrainedRequest::gbnf("Show git status", grammar.as_str());

        let response = provider.generate_constrained(request).await.unwrap();

        assert_eq!(response.text, r#"git(args="status")"#);
    }

    #[tokio::test]
    async fn test_mock_provider_prompt_based_response() {
        let provider = MockConstrainedProvider::new();
        provider.set_prompt_response("read", r#"read(path="src/lib.rs")"#);

        let request = ConstrainedRequest::gbnf(
            "Read the main library file",
            presets::l0_l1_tools().as_str(),
        );

        let response = provider.generate_constrained(request).await.unwrap();

        assert_eq!(response.text, r#"read(path="src/lib.rs")"#);
    }

    #[tokio::test]
    async fn test_mock_provider_call_history() {
        let provider = MockConstrainedProvider::new();

        let request1 = ConstrainedRequest::gbnf("First prompt", "grammar1");
        let _ = provider.generate_constrained(request1).await;

        let request2 = ConstrainedRequest::gbnf("Second prompt", "grammar2");
        let _ = provider.generate_constrained(request2).await;

        let history = provider.call_history();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].prompt, "First prompt");
        assert_eq!(history[1].prompt, "Second prompt");
    }

    #[tokio::test]
    async fn test_mock_provider_format_validation() {
        let provider = MockConstrainedProvider::gbnf_only();

        // GBNF should work
        let gbnf_request = ConstrainedRequest::gbnf("Test", "grammar");
        assert!(provider.generate_constrained(gbnf_request).await.is_ok());

        // JSON Schema should fail
        let json_request = ConstrainedRequest::json_schema("Test", "{}");
        assert!(provider.generate_constrained(json_request).await.is_err());
    }

    #[tokio::test]
    async fn test_mock_provider_truncation_simulation() {
        let provider = MockConstrainedProvider::new();
        provider.simulate_truncation(true);

        let request = ConstrainedRequest::gbnf("Test", "grammar");
        let response = provider.generate_constrained(request).await.unwrap();

        assert!(response.truncated);
    }

    #[tokio::test]
    async fn test_mock_provider_supports_format() {
        let provider = MockConstrainedProvider::new();

        assert!(provider.supports_format(SchemaFormat::Gbnf));
        assert!(provider.supports_format(SchemaFormat::JsonSchema));
        assert!(!provider.supports_format(SchemaFormat::Regex));
    }

    #[tokio::test]
    async fn test_mock_provider_health_check() {
        let provider = MockConstrainedProvider::new();
        assert!(provider.health_check().await.unwrap());
    }

    #[test]
    fn test_mock_provider_base_trait() {
        let provider = MockConstrainedProvider::new();

        assert_eq!(provider.name(), "mock-constrained");
        assert_eq!(provider.backend_type(), BackendType::Mock);
        assert!(provider.endpoint().is_none());

        let caps = provider.capabilities();
        assert!(caps.llm.tool_use);
        assert!(!caps.embeddings);
    }

    #[tokio::test]
    async fn test_mock_provider_with_yes_no_grammar() {
        let provider = MockConstrainedProvider::new();
        // Match on content that appears in the yes_no grammar: "yes" | "no"
        provider.set_response(r#""yes" | "no""#, "yes");

        let grammar = presets::yes_no();
        let request = ConstrainedRequest::gbnf("Should I proceed?", grammar.as_str());

        let response = provider.generate_constrained(request).await.unwrap();
        assert_eq!(response.text, "yes");
    }
}
