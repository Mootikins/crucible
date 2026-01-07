//! Ask module for Rune
//!
//! Provides types for batched user questions with choices and "Other" text input.
//!
//! # Example
//!
//! ```rune
//! use crucible::ask::{AskBatch, AskQuestion, question, batch};
//!
//! // Create a question with choices
//! let q = question("Library", "Which library should we use?")
//!     .choice("Tokio (Recommended)")
//!     .choice("async-std")
//!     .choice("smol");
//!
//! // Create a batch of questions
//! let batch = batch()
//!     .question(q)
//!     .question(question("Style", "Code style?").choice("Tabs").choice("Spaces"));
//! ```

use crucible_core::events::SessionEvent;
use crucible_core::interaction::{
    AskBatch, AskBatchResponse, AskQuestion, InteractionRequest, InteractionResponse,
    QuestionAnswer,
};
use crucible_core::traits::completion_backend::{BackendCompletionRequest, CompletionBackend};
use crucible_core::traits::context_ops::ContextMessage;
use crucible_core::InteractionRegistry;
use rune::{Any, ContextError, Module};
use std::sync::{Arc, Mutex};

use crate::EventRing;

/// Create the ask module for Rune under crucible::ask namespace
pub fn ask_module() -> Result<Module, ContextError> {
    let mut module = Module::with_crate_item("crucible", ["ask"])?;

    // Register AskQuestion wrapper
    module.ty::<RuneAskQuestion>()?;
    module.function_meta(RuneAskQuestion::new)?;
    module.function_meta(RuneAskQuestion::choice)?;
    module.function_meta(RuneAskQuestion::multi_select)?;
    module.function_meta(RuneAskQuestion::header)?;
    module.function_meta(RuneAskQuestion::question_text)?;
    module.function_meta(RuneAskQuestion::choice_count)?;

    // Convenience function
    module.function_meta(question)?;

    // Register AskBatch wrapper
    module.ty::<RuneAskBatch>()?;
    module.function_meta(RuneAskBatch::new)?;
    module.function_meta(RuneAskBatch::question)?;
    module.function_meta(RuneAskBatch::id)?;
    module.function_meta(RuneAskBatch::question_count)?;

    // Convenience function
    module.function_meta(batch)?;

    // Register AskBatchResponse wrapper (for receiving responses)
    module.ty::<RuneAskBatchResponse>()?;
    module.function_meta(RuneAskBatchResponse::id)?;
    module.function_meta(RuneAskBatchResponse::is_cancelled)?;
    module.function_meta(RuneAskBatchResponse::answer_count)?;
    module.function_meta(RuneAskBatchResponse::get_answer)?;

    // Register QuestionAnswer wrapper
    module.ty::<RuneQuestionAnswer>()?;
    module.function_meta(RuneQuestionAnswer::selected_indices)?;
    module.function_meta(RuneQuestionAnswer::other_text)?;
    module.function_meta(RuneQuestionAnswer::has_other)?;

    Ok(module)
}

// =============================================================================
// RuneAskQuestion - Wrapper for AskQuestion
// =============================================================================

/// AskQuestion wrapper for Rune
///
/// Represents a single question with header, question text, and choices.
#[derive(Debug, Clone, Any)]
#[rune(item = ::crucible::ask, name = AskQuestion)]
pub struct RuneAskQuestion {
    inner: AskQuestion,
}

impl RuneAskQuestion {
    /// Create from core AskQuestion
    pub fn from_core(q: AskQuestion) -> Self {
        Self { inner: q }
    }

    /// Convert to core AskQuestion
    pub fn into_core(self) -> AskQuestion {
        self.inner
    }

    // === Rust implementation methods ===

    /// Create a new question (impl)
    pub fn new_impl(header: String, question: String) -> Self {
        Self {
            inner: AskQuestion::new(header, question),
        }
    }

    /// Add a choice (impl)
    pub fn choice_impl(mut self, choice: String) -> Self {
        self.inner = self.inner.choice(choice);
        self
    }

    /// Enable multi-select (impl)
    pub fn multi_select_impl(mut self) -> Self {
        self.inner = self.inner.multi_select();
        self
    }

    /// Get header (impl)
    pub fn header_impl(&self) -> String {
        self.inner.header.clone()
    }

    /// Get question text (impl)
    pub fn question_text_impl(&self) -> String {
        self.inner.question.clone()
    }

    /// Get choice count (impl)
    pub fn choice_count_impl(&self) -> usize {
        self.inner.choices.len()
    }

    // === Rune bindings ===

    /// Create a new question with header and question text
    #[rune::function(path = Self::new)]
    pub fn new(header: String, question: String) -> Self {
        Self::new_impl(header, question)
    }

    /// Add a choice option (builder pattern)
    #[rune::function(path = Self::choice)]
    pub fn choice(self, choice: String) -> Self {
        self.choice_impl(choice)
    }

    /// Enable multi-select mode (builder pattern)
    #[rune::function(path = Self::multi_select)]
    pub fn multi_select(self) -> Self {
        self.multi_select_impl()
    }

    /// Get the header
    #[rune::function(path = Self::header)]
    pub fn header(&self) -> String {
        self.header_impl()
    }

    /// Get the question text
    #[rune::function(path = Self::question_text)]
    pub fn question_text(&self) -> String {
        self.question_text_impl()
    }

    /// Get the number of choices
    #[rune::function(path = Self::choice_count)]
    pub fn choice_count(&self) -> usize {
        self.choice_count_impl()
    }
}

/// Convenience function to create a question (impl for Rust use)
pub fn question_impl(header: String, question_text: String) -> RuneAskQuestion {
    RuneAskQuestion::new_impl(header, question_text)
}

/// Convenience function to create a question
///
/// # Example
/// ```rune
/// use crucible::ask::question;
///
/// let q = question("Auth", "Which auth method?")
///     .choice("OAuth")
///     .choice("JWT");
/// ```
#[rune::function]
fn question(header: String, question_text: String) -> RuneAskQuestion {
    question_impl(header, question_text)
}

// =============================================================================
// RuneAskBatch - Wrapper for AskBatch
// =============================================================================

/// AskBatch wrapper for Rune
///
/// Represents a batch of questions to ask the user.
#[derive(Debug, Clone, Any)]
#[rune(item = ::crucible::ask, name = AskBatch)]
pub struct RuneAskBatch {
    inner: AskBatch,
}

impl RuneAskBatch {
    /// Create from core AskBatch
    pub fn from_core(b: AskBatch) -> Self {
        Self { inner: b }
    }

    /// Convert to core AskBatch
    pub fn into_core(self) -> AskBatch {
        self.inner
    }

    /// Get reference to inner
    pub fn as_core(&self) -> &AskBatch {
        &self.inner
    }

    // === Rust implementation methods ===

    /// Create a new empty batch (impl)
    pub fn new_impl() -> Self {
        Self {
            inner: AskBatch::new(),
        }
    }

    /// Add a question (impl)
    pub fn question_impl(mut self, q: RuneAskQuestion) -> Self {
        self.inner = self.inner.question(q.into_core());
        self
    }

    /// Get batch ID (impl)
    pub fn id_impl(&self) -> String {
        self.inner.id.to_string()
    }

    /// Get question count (impl)
    pub fn question_count_impl(&self) -> usize {
        self.inner.questions.len()
    }

    // === Rune bindings ===

    /// Create a new empty batch
    #[rune::function(path = Self::new)]
    pub fn new() -> Self {
        Self::new_impl()
    }

    /// Add a question to the batch (builder pattern)
    #[rune::function(path = Self::question)]
    pub fn question(self, q: RuneAskQuestion) -> Self {
        self.question_impl(q)
    }

    /// Get the batch ID as a string
    #[rune::function(path = Self::id)]
    pub fn id(&self) -> String {
        self.id_impl()
    }

    /// Get the number of questions
    #[rune::function(path = Self::question_count)]
    pub fn question_count(&self) -> usize {
        self.question_count_impl()
    }
}

/// Convenience function to create an empty batch (impl for Rust use)
pub fn batch_impl() -> RuneAskBatch {
    RuneAskBatch::new_impl()
}

/// Convenience function to create an empty batch
///
/// # Example
/// ```rune
/// use crucible::ask::{batch, question};
///
/// let b = batch()
///     .question(question("Q1", "First?").choice("A"))
///     .question(question("Q2", "Second?").choice("B"));
/// ```
#[rune::function]
fn batch() -> RuneAskBatch {
    batch_impl()
}

// =============================================================================
// AskContext - Holds registry and ring for ask_user function
// =============================================================================

/// Context for ask_user function execution.
///
/// Holds references to the interaction registry and event ring needed
/// to submit requests and wait for responses.
#[derive(Clone)]
pub struct AskContext {
    registry: Arc<Mutex<InteractionRegistry>>,
    ring: Arc<EventRing<SessionEvent>>,
}

impl AskContext {
    /// Create new context with registry and ring.
    pub fn new(
        registry: Arc<Mutex<InteractionRegistry>>,
        ring: Arc<EventRing<SessionEvent>>,
    ) -> Self {
        Self { registry, ring }
    }

    /// Submit an ask batch and wait for the response.
    ///
    /// This function:
    /// 1. Registers the batch ID with the registry (gets a receiver)
    /// 2. Pushes an InteractionRequested event to the ring
    /// 3. Blocks waiting for the response via the receiver
    pub fn ask_user(&self, batch: RuneAskBatch) -> Result<RuneAskBatchResponse, RuneAskError> {
        let core_batch = batch.into_core();
        let id = core_batch.id;

        // Register with the registry to get a receiver
        let rx = {
            let mut guard = self
                .registry
                .lock()
                .map_err(|e| RuneAskError::new(format!("Registry lock failed: {}", e)))?;
            guard.register(id)
        };

        // Push InteractionRequested event
        self.ring.push(SessionEvent::InteractionRequested {
            request_id: id.to_string(),
            request: InteractionRequest::AskBatch(core_batch),
        });

        // Wait for response (blocking)
        // Note: This blocks the current thread until the TUI completes the interaction
        let response = rx
            .blocking_recv()
            .map_err(|_| RuneAskError::new("Interaction was cancelled or dropped".to_string()))?;

        match response {
            InteractionResponse::AskBatch(batch_response) => {
                Ok(RuneAskBatchResponse::from_core(batch_response))
            }
            InteractionResponse::Cancelled => {
                Ok(RuneAskBatchResponse::from_core(AskBatchResponse::cancelled(id)))
            }
            _ => Err(RuneAskError::new(format!(
                "Unexpected response type: {:?}",
                response
            ))),
        }
    }
}

/// Error type for ask_user function (Rune-compatible).
#[derive(Debug, Clone, Any)]
#[rune(item = ::crucible::ask, name = AskError)]
pub struct RuneAskError {
    /// Error message
    #[rune(get)]
    pub message: String,
}

impl RuneAskError {
    /// Create a new error with message.
    pub fn new(message: String) -> Self {
        Self { message }
    }
}

impl std::fmt::Display for RuneAskError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

/// Create the ask module for Rune with context for ask_user function.
///
/// This version includes the `ask_user` function that can submit questions
/// to the TUI and wait for responses.
///
/// # Arguments
///
/// * `registry` - Shared interaction registry for request-response correlation
/// * `ring` - Event ring for pushing InteractionRequested events
///
/// # Example
///
/// ```rust,ignore
/// use crucible_rune::ask_module_with_context;
///
/// let registry = Arc::new(Mutex::new(InteractionRegistry::new()));
/// let ring = Arc::new(EventRing::new(1024));
///
/// let module = ask_module_with_context(registry, ring).unwrap();
/// ```
pub fn ask_module_with_context(
    registry: Arc<Mutex<InteractionRegistry>>,
    ring: Arc<EventRing<SessionEvent>>,
) -> Result<Module, ContextError> {
    let mut module = Module::with_crate_item("crucible", ["ask"])?;

    // Register types (same as ask_module)
    module.ty::<RuneAskQuestion>()?;
    module.function_meta(RuneAskQuestion::new)?;
    module.function_meta(RuneAskQuestion::choice)?;
    module.function_meta(RuneAskQuestion::multi_select)?;
    module.function_meta(RuneAskQuestion::header)?;
    module.function_meta(RuneAskQuestion::question_text)?;
    module.function_meta(RuneAskQuestion::choice_count)?;

    module.function_meta(question)?;

    module.ty::<RuneAskBatch>()?;
    module.function_meta(RuneAskBatch::new)?;
    module.function_meta(RuneAskBatch::question)?;
    module.function_meta(RuneAskBatch::id)?;
    module.function_meta(RuneAskBatch::question_count)?;

    module.function_meta(batch)?;

    module.ty::<RuneAskBatchResponse>()?;
    module.function_meta(RuneAskBatchResponse::id)?;
    module.function_meta(RuneAskBatchResponse::is_cancelled)?;
    module.function_meta(RuneAskBatchResponse::answer_count)?;
    module.function_meta(RuneAskBatchResponse::get_answer)?;

    module.ty::<RuneQuestionAnswer>()?;
    module.function_meta(RuneQuestionAnswer::selected_indices)?;
    module.function_meta(RuneQuestionAnswer::other_text)?;
    module.function_meta(RuneQuestionAnswer::has_other)?;

    // Register error type
    module.ty::<RuneAskError>()?;

    // Register ask_user function with context
    let ctx = AskContext::new(registry, ring);
    module
        .function("ask_user", move |batch: RuneAskBatch| ctx.ask_user(batch))
        .build()?;

    Ok(module)
}

// =============================================================================
// AgentAskContext - For asking questions to LLM agents
// =============================================================================

/// Context for asking questions to an LLM agent instead of a user.
///
/// This enables script-to-agent communication where the "user" answering
/// questions is another LLM.
#[derive(Clone)]
pub struct AgentAskContext {
    backend: Arc<dyn CompletionBackend>,
}

impl AgentAskContext {
    /// Create new context with a completion backend.
    pub fn new(backend: Arc<dyn CompletionBackend>) -> Self {
        Self { backend }
    }

    /// Submit a batch of questions to the LLM and get structured answers.
    ///
    /// This function:
    /// 1. Formats questions as a structured prompt
    /// 2. Sends to the LLM backend
    /// 3. Parses the response into AskBatchResponse
    pub fn ask_agent(&self, batch: RuneAskBatch) -> Result<RuneAskBatchResponse, RuneAskError> {
        // Use tokio runtime to run async code
        let rt = tokio::runtime::Handle::try_current()
            .map_err(|_| RuneAskError::new("No tokio runtime available".to_string()))?;

        let backend = self.backend.clone();
        let core_batch = batch.into_core();

        rt.block_on(async move {
            Self::ask_agent_async(&backend, core_batch).await
        })
        .map(RuneAskBatchResponse::from_core)
        .map_err(|e| RuneAskError::new(e))
    }

    /// Async implementation of ask_agent
    async fn ask_agent_async(
        backend: &Arc<dyn CompletionBackend>,
        batch: AskBatch,
    ) -> Result<AskBatchResponse, String> {
        let prompt = Self::format_batch_prompt(&batch);
        let system_prompt = Self::system_prompt();

        let request = BackendCompletionRequest::new(system_prompt, vec![ContextMessage::user(prompt)]);

        let response = backend
            .complete(request)
            .await
            .map_err(|e| format!("LLM completion failed: {}", e))?;

        Self::parse_response(&response.content, batch)
    }

    /// Format the batch as a structured prompt for the LLM.
    fn format_batch_prompt(batch: &AskBatch) -> String {
        let mut prompt = String::from("Please answer the following questions by selecting from the provided choices.\n\n");
        prompt.push_str("For each question, respond with ONLY the choice number (0-indexed) or \"other: <your text>\" if none fit.\n\n");

        for (i, q) in batch.questions.iter().enumerate() {
            prompt.push_str(&format!("Question {}: {} ({})\n", i + 1, q.question, q.header));
            for (j, choice) in q.choices.iter().enumerate() {
                prompt.push_str(&format!("  {}: {}\n", j, choice));
            }
            prompt.push('\n');
        }

        prompt.push_str("\nRespond in JSON format:\n");
        prompt.push_str("```json\n");
        prompt.push_str("{\n");
        prompt.push_str("  \"answers\": [\n");
        prompt.push_str("    {\"selected\": [0], \"other\": null},  // for first question\n");
        prompt.push_str("    {\"selected\": [], \"other\": \"custom answer\"}  // if using other\n");
        prompt.push_str("  ]\n");
        prompt.push_str("}\n");
        prompt.push_str("```\n");

        prompt
    }

    /// System prompt for the answering agent.
    fn system_prompt() -> String {
        String::from(
            "You are a helpful assistant answering multiple-choice questions. \
             Respond ONLY with valid JSON containing your answers. \
             Select the most appropriate choice for each question. \
             If no choice fits, use the 'other' field with your custom answer."
        )
    }

    /// Parse the LLM response into AskBatchResponse.
    fn parse_response(content: &str, batch: AskBatch) -> Result<AskBatchResponse, String> {
        // Try to extract JSON from the response (may be wrapped in markdown code blocks)
        let json_str = Self::extract_json(content)?;

        // Parse the JSON
        let parsed: serde_json::Value = serde_json::from_str(&json_str)
            .map_err(|e| format!("Invalid JSON response: {}", e))?;

        let answers = parsed
            .get("answers")
            .and_then(|a| a.as_array())
            .ok_or_else(|| "Response missing 'answers' array".to_string())?;

        let mut response = AskBatchResponse::new(batch.id);

        for (i, answer) in answers.iter().enumerate() {
            if i >= batch.questions.len() {
                break;
            }

            let selected: Vec<usize> = answer
                .get("selected")
                .and_then(|s| s.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_u64().map(|n| n as usize))
                        .collect()
                })
                .unwrap_or_default();

            let other: Option<String> = answer
                .get("other")
                .and_then(|o| {
                    if o.is_null() {
                        None
                    } else {
                        o.as_str().map(|s| s.to_string())
                    }
                });

            let qa = if let Some(text) = other {
                if selected.is_empty() {
                    QuestionAnswer::other(text)
                } else {
                    // Both selected and other - create struct directly
                    QuestionAnswer {
                        selected,
                        other: Some(text),
                    }
                }
            } else if !selected.is_empty() {
                QuestionAnswer::choices(selected)
            } else {
                QuestionAnswer::choice(0) // Default to first choice
            };

            response = response.answer(qa);
        }

        Ok(response)
    }

    /// Extract JSON from response text (handles markdown code blocks).
    fn extract_json(content: &str) -> Result<String, String> {
        // Try to find JSON in code blocks first
        if let Some(start) = content.find("```json") {
            let json_start = start + 7;
            if let Some(end) = content[json_start..].find("```") {
                return Ok(content[json_start..json_start + end].trim().to_string());
            }
        }

        // Try plain code blocks
        if let Some(start) = content.find("```") {
            let json_start = start + 3;
            // Skip optional language identifier on same line
            let json_start = content[json_start..]
                .find('\n')
                .map(|n| json_start + n + 1)
                .unwrap_or(json_start);
            if let Some(end) = content[json_start..].find("```") {
                return Ok(content[json_start..json_start + end].trim().to_string());
            }
        }

        // Try to find raw JSON object
        if let Some(start) = content.find('{') {
            if let Some(end) = content.rfind('}') {
                if end > start {
                    return Ok(content[start..=end].to_string());
                }
            }
        }

        Err("Could not find JSON in response".to_string())
    }
}

/// Create the ask module for Rune with an LLM agent backend.
///
/// This version includes the `ask_agent` function that sends questions
/// to an LLM instead of showing a user dialog.
///
/// # Arguments
///
/// * `backend` - Completion backend for LLM requests
///
/// # Example
///
/// ```rust,ignore
/// use crucible_rune::ask_module_with_agent;
///
/// let backend: Arc<dyn CompletionBackend> = /* create backend */;
/// let module = ask_module_with_agent(backend).unwrap();
/// ```
pub fn ask_module_with_agent(
    backend: Arc<dyn CompletionBackend>,
) -> Result<Module, ContextError> {
    let mut module = Module::with_crate_item("crucible", ["ask"])?;

    // Register types (same as ask_module)
    module.ty::<RuneAskQuestion>()?;
    module.function_meta(RuneAskQuestion::new)?;
    module.function_meta(RuneAskQuestion::choice)?;
    module.function_meta(RuneAskQuestion::multi_select)?;
    module.function_meta(RuneAskQuestion::header)?;
    module.function_meta(RuneAskQuestion::question_text)?;
    module.function_meta(RuneAskQuestion::choice_count)?;

    module.function_meta(question)?;

    module.ty::<RuneAskBatch>()?;
    module.function_meta(RuneAskBatch::new)?;
    module.function_meta(RuneAskBatch::question)?;
    module.function_meta(RuneAskBatch::id)?;
    module.function_meta(RuneAskBatch::question_count)?;

    module.function_meta(batch)?;

    module.ty::<RuneAskBatchResponse>()?;
    module.function_meta(RuneAskBatchResponse::id)?;
    module.function_meta(RuneAskBatchResponse::is_cancelled)?;
    module.function_meta(RuneAskBatchResponse::answer_count)?;
    module.function_meta(RuneAskBatchResponse::get_answer)?;

    module.ty::<RuneQuestionAnswer>()?;
    module.function_meta(RuneQuestionAnswer::selected_indices)?;
    module.function_meta(RuneQuestionAnswer::other_text)?;
    module.function_meta(RuneQuestionAnswer::has_other)?;

    // Register error type
    module.ty::<RuneAskError>()?;

    // Register ask_agent function with backend
    let ctx = AgentAskContext::new(backend);
    module
        .function("ask_agent", move |batch: RuneAskBatch| ctx.ask_agent(batch))
        .build()?;

    Ok(module)
}

// =============================================================================
// RuneAskBatchResponse - Wrapper for AskBatchResponse
// =============================================================================

/// AskBatchResponse wrapper for Rune
///
/// Represents the user's responses to a batch of questions.
#[derive(Debug, Clone, Any)]
#[rune(item = ::crucible::ask, name = AskBatchResponse)]
pub struct RuneAskBatchResponse {
    inner: AskBatchResponse,
}

impl RuneAskBatchResponse {
    /// Create from core AskBatchResponse
    pub fn from_core(r: AskBatchResponse) -> Self {
        Self { inner: r }
    }

    /// Convert to core AskBatchResponse
    pub fn into_core(self) -> AskBatchResponse {
        self.inner
    }

    // === Rust implementation methods ===

    /// Get batch ID (impl)
    pub fn id_impl(&self) -> String {
        self.inner.id.to_string()
    }

    /// Check if cancelled (impl)
    pub fn is_cancelled_impl(&self) -> bool {
        self.inner.cancelled
    }

    /// Get answer count (impl)
    pub fn answer_count_impl(&self) -> usize {
        self.inner.answers.len()
    }

    /// Get answer by index (impl)
    pub fn get_answer_impl(&self, index: usize) -> Option<RuneQuestionAnswer> {
        self.inner
            .answers
            .get(index)
            .cloned()
            .map(RuneQuestionAnswer::from_core)
    }

    // === Rune bindings ===

    /// Get the batch ID as a string
    #[rune::function(path = Self::id)]
    pub fn id(&self) -> String {
        self.id_impl()
    }

    /// Check if the batch was cancelled
    #[rune::function(path = Self::is_cancelled)]
    pub fn is_cancelled(&self) -> bool {
        self.is_cancelled_impl()
    }

    /// Get the number of answers
    #[rune::function(path = Self::answer_count)]
    pub fn answer_count(&self) -> usize {
        self.answer_count_impl()
    }

    /// Get an answer by index
    #[rune::function(path = Self::get_answer)]
    pub fn get_answer(&self, index: usize) -> Option<RuneQuestionAnswer> {
        self.get_answer_impl(index)
    }
}

// =============================================================================
// RuneQuestionAnswer - Wrapper for QuestionAnswer
// =============================================================================

/// QuestionAnswer wrapper for Rune
///
/// Represents the user's answer to a single question.
#[derive(Debug, Clone, Any)]
#[rune(item = ::crucible::ask, name = QuestionAnswer)]
pub struct RuneQuestionAnswer {
    inner: QuestionAnswer,
}

impl RuneQuestionAnswer {
    /// Create from core QuestionAnswer
    pub fn from_core(a: QuestionAnswer) -> Self {
        Self { inner: a }
    }

    /// Convert to core QuestionAnswer
    pub fn into_core(self) -> QuestionAnswer {
        self.inner
    }

    // === Rust implementation methods ===

    /// Get selected indices (impl)
    pub fn selected_indices_impl(&self) -> Vec<usize> {
        self.inner.selected.clone()
    }

    /// Get other text (impl)
    pub fn other_text_impl(&self) -> Option<String> {
        self.inner.other.clone()
    }

    /// Check if has other (impl)
    pub fn has_other_impl(&self) -> bool {
        self.inner.other.is_some()
    }

    // === Rune bindings ===

    /// Get the indices of selected choices
    #[rune::function(path = Self::selected_indices)]
    pub fn selected_indices(&self) -> Vec<usize> {
        self.selected_indices_impl()
    }

    /// Get the "Other" text input (if any)
    #[rune::function(path = Self::other_text)]
    pub fn other_text(&self) -> Option<String> {
        self.other_text_impl()
    }

    /// Check if "Other" was selected
    #[rune::function(path = Self::has_other)]
    pub fn has_other(&self) -> bool {
        self.has_other_impl()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ask_module_creation() {
        let module = ask_module();
        assert!(module.is_ok(), "Should create ask module");
    }

    #[test]
    fn test_ask_module_with_context_creation() {
        let registry = Arc::new(Mutex::new(InteractionRegistry::new()));
        let ring = Arc::new(crate::EventRing::new(64));
        let module = ask_module_with_context(registry, ring);
        assert!(module.is_ok(), "Should create ask module with context");
    }

    #[test]
    fn test_ask_question_new() {
        let q = RuneAskQuestion::new_impl("Header".to_string(), "Question?".to_string());
        assert_eq!(q.header_impl(), "Header");
        assert_eq!(q.question_text_impl(), "Question?");
        assert_eq!(q.choice_count_impl(), 0);
    }

    #[test]
    fn test_ask_question_with_choices() {
        let q = RuneAskQuestion::new_impl("Auth".to_string(), "Which method?".to_string())
            .choice_impl("OAuth".to_string())
            .choice_impl("JWT".to_string())
            .choice_impl("Basic".to_string());

        assert_eq!(q.choice_count_impl(), 3);
    }

    #[test]
    fn test_ask_batch_new() {
        let b = RuneAskBatch::new_impl();
        assert_eq!(b.question_count_impl(), 0);
        assert!(!b.id_impl().is_empty());
    }

    #[test]
    fn test_ask_batch_with_questions() {
        let q1 = RuneAskQuestion::new_impl("Q1".to_string(), "First?".to_string())
            .choice_impl("A".to_string());
        let q2 = RuneAskQuestion::new_impl("Q2".to_string(), "Second?".to_string())
            .choice_impl("B".to_string());

        let b = RuneAskBatch::new_impl().question_impl(q1).question_impl(q2);

        assert_eq!(b.question_count_impl(), 2);
    }

    #[test]
    fn test_ask_batch_response() {
        let mut response = AskBatchResponse::new(crucible_core::uuid::Uuid::new_v4());
        response.answers.push(QuestionAnswer::choice(0));
        response.answers.push(QuestionAnswer::other("Custom".to_string()));

        let rune_response = RuneAskBatchResponse::from_core(response);

        assert_eq!(rune_response.answer_count_impl(), 2);
        assert!(!rune_response.is_cancelled_impl());

        let a0 = rune_response.get_answer_impl(0).unwrap();
        assert_eq!(a0.selected_indices_impl(), vec![0]);
        assert!(!a0.has_other_impl());

        let a1 = rune_response.get_answer_impl(1).unwrap();
        assert_eq!(a1.other_text_impl(), Some("Custom".to_string()));
        assert!(a1.has_other_impl());
    }

    /// Test ask module from Rune script
    #[test]
    fn test_ask_from_rune() {
        use rune::termcolor::{ColorChoice, StandardStream};
        use rune::{Context, Diagnostics, Source, Sources, Vm};
        use std::sync::Arc;

        let mut context = Context::with_default_modules().unwrap();
        context.install(ask_module().unwrap()).unwrap();
        let runtime = Arc::new(context.runtime().unwrap());

        let script = r#"
            use crucible::ask::{AskBatch, AskQuestion, question, batch};

            pub fn main() {
                let q = question("Auth", "Which method?")
                    .choice("OAuth (Recommended)")
                    .choice("JWT")
                    .choice("Basic");

                let b = batch()
                    .question(q);

                b.question_count()
            }
        "#;

        let mut sources = Sources::new();
        sources
            .insert(Source::new("test", script).unwrap())
            .unwrap();

        let mut diagnostics = Diagnostics::new();
        let result = rune::prepare(&mut sources)
            .with_context(&context)
            .with_diagnostics(&mut diagnostics)
            .build();

        if !diagnostics.is_empty() {
            let mut writer = StandardStream::stderr(ColorChoice::Always);
            diagnostics.emit(&mut writer, &sources).unwrap();
        }

        let unit = result.expect("Should compile script with ask module");
        let unit = Arc::new(unit);

        let mut vm = Vm::new(runtime, unit);
        let output = vm.call(rune::Hash::type_hash(["main"]), ()).unwrap();
        let count: usize = rune::from_value(output).unwrap();

        assert_eq!(count, 1, "Should have 1 question in batch");
    }

    // =========================================================================
    // Tests for AgentAskContext JSON parsing
    // =========================================================================

    #[test]
    fn test_extract_json_from_code_block() {
        let content = r#"Here's my answer:
```json
{"answers": [{"selected": [0], "other": null}]}
```"#;
        let result = AgentAskContext::extract_json(content);
        assert!(result.is_ok());
        let json = result.unwrap();
        assert!(json.contains("answers"));
    }

    #[test]
    fn test_extract_json_from_plain_block() {
        let content = r#"Here's my answer:
```
{"answers": [{"selected": [1], "other": null}]}
```"#;
        let result = AgentAskContext::extract_json(content);
        assert!(result.is_ok());
    }

    #[test]
    fn test_extract_json_raw() {
        let content = r#"{"answers": [{"selected": [0], "other": null}]}"#;
        let result = AgentAskContext::extract_json(content);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), content);
    }

    #[test]
    fn test_extract_json_with_surrounding_text() {
        let content = r#"The answer is {"answers": [{"selected": [0], "other": null}]} and that's it."#;
        let result = AgentAskContext::extract_json(content);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_response_single_choice() {
        let batch = AskBatch::new()
            .question(AskQuestion::new("Q1", "First?").choice("A").choice("B"));

        let content = r#"{"answers": [{"selected": [0], "other": null}]}"#;
        let result = AgentAskContext::parse_response(content, batch);

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.answers.len(), 1);
        assert_eq!(response.answers[0].selected, vec![0]);
        assert!(response.answers[0].other.is_none());
    }

    #[test]
    fn test_parse_response_with_other() {
        let batch = AskBatch::new()
            .question(AskQuestion::new("Q1", "First?").choice("A").choice("B"));

        let content = r#"{"answers": [{"selected": [], "other": "custom answer"}]}"#;
        let result = AgentAskContext::parse_response(content, batch);

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.answers.len(), 1);
        assert!(response.answers[0].selected.is_empty());
        assert_eq!(response.answers[0].other, Some("custom answer".to_string()));
    }

    #[test]
    fn test_parse_response_multi_select() {
        let batch = AskBatch::new()
            .question(AskQuestion::new("Q1", "First?").choice("A").choice("B").choice("C"));

        let content = r#"{"answers": [{"selected": [0, 2], "other": null}]}"#;
        let result = AgentAskContext::parse_response(content, batch);

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.answers[0].selected, vec![0, 2]);
    }

    #[test]
    fn test_parse_response_multiple_questions() {
        let batch = AskBatch::new()
            .question(AskQuestion::new("Q1", "First?").choice("A").choice("B"))
            .question(AskQuestion::new("Q2", "Second?").choice("X").choice("Y"));

        let content = r#"{"answers": [
            {"selected": [0], "other": null},
            {"selected": [1], "other": null}
        ]}"#;
        let result = AgentAskContext::parse_response(content, batch);

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.answers.len(), 2);
        assert_eq!(response.answers[0].selected, vec![0]);
        assert_eq!(response.answers[1].selected, vec![1]);
    }

    #[test]
    fn test_format_batch_prompt() {
        let batch = AskBatch::new()
            .question(AskQuestion::new("Auth", "Method?").choice("OAuth").choice("JWT"));

        let prompt = AgentAskContext::format_batch_prompt(&batch);

        assert!(prompt.contains("Question 1: Method? (Auth)"));
        assert!(prompt.contains("0: OAuth"));
        assert!(prompt.contains("1: JWT"));
        assert!(prompt.contains("JSON format"));
    }
}
