use crucible_core::interaction::{AskBatch, AskBatchResponse, QuestionAnswer};
use crucible_core::traits::completion_backend::{BackendCompletionRequest, CompletionBackend};
use crucible_core::traits::context_ops::ContextMessage;
use std::sync::Arc;

use super::context::LuaAskError;
use super::types::{LuaAskBatch, LuaAskBatchResponse};

/// Context for asking questions to an LLM agent instead of a user.
///
/// This enables script-to-agent communication where the "user" answering
/// questions is another LLM. Mirrors the Lua AgentAskContext pattern.
///
/// # Example
///
/// ```rust,ignore
/// use crucible_lua::ask::{LuaAgentAskContext, register_ask_module_with_agent};
/// use std::sync::Arc;
///
/// let backend: Arc<dyn CompletionBackend> = /* create backend */;
/// let context = LuaAgentAskContext::new(backend);
///
/// let lua = mlua::Lua::new();
/// register_ask_module_with_agent(&lua, context)?;
///
/// // Lua scripts can now use ask.agent(batch)
/// ```
#[derive(Clone)]
pub struct LuaAgentAskContext {
    backend: Arc<dyn CompletionBackend>,
}

impl LuaAgentAskContext {
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
    pub fn ask_agent(&self, batch: LuaAskBatch) -> Result<LuaAskBatchResponse, LuaAskError> {
        // Use tokio runtime to run async code
        let rt = tokio::runtime::Handle::try_current()
            .map_err(|_| LuaAskError::new("No tokio runtime available".to_string()))?;

        let backend = self.backend.clone();
        let core_batch = batch.inner.clone();

        rt.block_on(async move { Self::ask_agent_async(&backend, core_batch).await })
            .map(|r| LuaAskBatchResponse { inner: r })
            .map_err(LuaAskError::new)
    }

    /// Async implementation of ask_agent
    async fn ask_agent_async(
        backend: &Arc<dyn CompletionBackend>,
        batch: AskBatch,
    ) -> Result<AskBatchResponse, String> {
        let prompt = Self::format_batch_prompt(&batch);
        let system_prompt = Self::system_prompt();

        let request =
            BackendCompletionRequest::new(system_prompt, vec![ContextMessage::user(prompt)]);

        let response = backend
            .complete(request)
            .await
            .map_err(|e| format!("LLM completion failed: {}", e))?;

        Self::parse_response(&response.content, batch)
    }

    /// Format the batch as a structured prompt for the LLM.
    pub fn format_batch_prompt(batch: &AskBatch) -> String {
        let mut prompt = String::from(
            "Please answer the following questions by selecting from the provided choices.\n\n",
        );
        prompt.push_str("For each question, respond with ONLY the choice number (0-indexed) or \"other: <your text>\" if none fit.\n\n");

        for (i, q) in batch.questions.iter().enumerate() {
            prompt.push_str(&format!(
                "Question {}: {} ({})\n",
                i + 1,
                q.question,
                q.header
            ));
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
        prompt
            .push_str("    {\"selected\": [], \"other\": \"custom answer\"}  // if using other\n");
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
             If no choice fits, use the 'other' field with your custom answer.",
        )
    }

    /// Parse the LLM response into AskBatchResponse.
    pub fn parse_response(content: &str, batch: AskBatch) -> Result<AskBatchResponse, String> {
        // Try to extract JSON from the response (may be wrapped in markdown code blocks)
        let json_str = Self::extract_json(content)?;

        // Parse the JSON
        let parsed: serde_json::Value =
            serde_json::from_str(&json_str).map_err(|e| format!("Invalid JSON response: {}", e))?;

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

            let other: Option<String> = answer.get("other").and_then(|o| {
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
    pub fn extract_json(content: &str) -> Result<String, String> {
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
