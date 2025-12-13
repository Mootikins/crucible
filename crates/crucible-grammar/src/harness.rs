//! Test harness for grammar-constrained generation experiments

use crate::api::{ChatMessage, CompletionRequest, LlamaClient};
use crate::grammar::Grammar;
use crate::scoring::{AggregateScore, ExpectedToolCall, Score, Scorer};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

/// Test execution mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Grammar-constrained generation
    Constrained,
    /// Unconstrained baseline
    Unconstrained,
}

/// A single test case
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCase {
    /// Test case name
    pub name: String,
    /// User prompt
    pub prompt: String,
    /// System prompt (optional)
    pub system: Option<String>,
    /// Expected tool call
    pub expected: ExpectedToolCall,
}

/// Result of running a single test
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    /// Test case name
    pub case: String,
    /// Execution mode
    pub mode: Mode,
    /// Model used
    pub model: String,
    /// Raw model output
    pub output: String,
    /// Thinking/reasoning content (if present)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,
    /// Scoring results
    pub score: Score,
    /// Latency in milliseconds
    pub latency_ms: u64,
    /// Token usage
    pub tokens: u32,
}

/// Collection of test cases
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestSuite {
    /// Suite name
    pub name: String,
    /// Test cases
    pub cases: Vec<TestCase>,
}

impl TestSuite {
    /// Load a test suite from TOML file
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        let suite: TestSuite = toml::from_str(&content)?;
        Ok(suite)
    }
}

/// Harness configuration
#[derive(Debug, Clone)]
pub struct HarnessConfig {
    pub endpoint: String,
    pub model: String,
    pub grammar: Option<Grammar>,
    pub system_prompt: Option<String>,
    pub max_tokens: u32,
    /// If true, allow thinking mode (don't add without_thinking() to requests)
    pub allow_thinking: bool,
}

impl Default for HarnessConfig {
    fn default() -> Self {
        Self {
            endpoint: "https://llama.krohnos.io".to_string(),
            model: "qwen3-14b-ud-q8_k_xl".to_string(),
            grammar: None,
            system_prompt: Some(
                "You are a tool-calling assistant. Available tools: read(path), write(path, content), edit(path, search, replace), ls(path), git(args), rg(pattern, path). Output ONLY the tool call, nothing else.".to_string()
            ),
            max_tokens: 128,
            allow_thinking: false,
        }
    }
}

/// Test harness for running experiments
pub struct TestHarness {
    client: LlamaClient,
    config: HarnessConfig,
}

impl TestHarness {
    pub fn new(config: HarnessConfig) -> Self {
        let client = LlamaClient::new(&config.endpoint).with_timeout(Duration::from_secs(120));
        Self { client, config }
    }

    /// Run a single test case
    pub async fn run_test(&self, case: &TestCase, mode: Mode) -> Result<TestResult, Box<dyn std::error::Error>> {
        let mut messages = Vec::new();

        // Add system prompt
        if let Some(sys) = case.system.as_ref().or(self.config.system_prompt.as_ref()) {
            messages.push(ChatMessage::system(sys));
        }

        // Add user prompt
        messages.push(ChatMessage::user(&case.prompt));

        // Build request
        let mut request = CompletionRequest::new(&self.config.model, messages)
            .with_max_tokens(self.config.max_tokens)
            .with_temperature(0.0);

        // Add grammar if constrained mode
        if mode == Mode::Constrained {
            if let Some(grammar) = &self.config.grammar {
                request = request.with_grammar(grammar.as_str());
                // Disable thinking unless the grammar supports it
                if !self.config.allow_thinking {
                    request = request.without_thinking();
                }
            }
        }

        // Execute
        let (response, duration) = self.client.complete(request).await?;

        let output = LlamaClient::extract_content(&response)
            .unwrap_or("")
            .to_string();

        // Extract thinking content if present
        let thinking = response
            .choices
            .first()
            .and_then(|c| c.message.reasoning_content.clone());

        let score = Scorer::score(&output, &case.expected);

        Ok(TestResult {
            case: case.name.clone(),
            mode,
            model: self.config.model.clone(),
            output,
            thinking,
            score,
            latency_ms: duration.as_millis() as u64,
            tokens: response.usage.completion_tokens,
        })
    }

    /// Run all test cases in both modes
    pub async fn run_suite(&self, suite: &TestSuite) -> Result<Vec<TestResult>, Box<dyn std::error::Error>> {
        let mut results = Vec::new();

        for case in &suite.cases {
            // Run constrained
            if self.config.grammar.is_some() {
                match self.run_test(case, Mode::Constrained).await {
                    Ok(result) => results.push(result),
                    Err(e) => eprintln!("Error running {} (constrained): {}", case.name, e),
                }
            }

            // Run unconstrained
            match self.run_test(case, Mode::Unconstrained).await {
                Ok(result) => results.push(result),
                Err(e) => eprintln!("Error running {} (unconstrained): {}", case.name, e),
            }
        }

        Ok(results)
    }

    /// Generate summary statistics
    pub fn summarize(results: &[TestResult]) -> HashMap<Mode, AggregateScore> {
        let mut by_mode: HashMap<Mode, Vec<Score>> = HashMap::new();

        for result in results {
            by_mode
                .entry(result.mode)
                .or_default()
                .push(result.score.clone());
        }

        by_mode
            .into_iter()
            .map(|(mode, scores)| (mode, AggregateScore::from_scores(&scores)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mode_serialization() {
        assert_eq!(
            serde_json::to_string(&Mode::Constrained).unwrap(),
            "\"constrained\""
        );
    }

    #[test]
    fn test_harness_config_default() {
        let config = HarnessConfig::default();
        assert!(config.endpoint.contains("llama.krohnos.io"));
    }
}
