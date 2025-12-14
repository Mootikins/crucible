//! Test harness for grammar-constrained generation experiments

use crate::api::{ChatMessage, CompletionRequest, LlamaClient, TextCompletionRequest};
use crate::grammar::Grammar;
use crate::scoring::{AggregateScore, ExpectedToolCall, Score, Scorer};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

/// Chat template format for text completions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChatTemplate {
    /// Qwen3/ChatML format: <|im_start|>role\ncontent<|im_end|>
    #[default]
    Qwen3,
    /// Llama 3 format: <|start_header_id|>role<|end_header_id|>\ncontent<|eot_id|>
    Llama3,
    /// GPT-OSS format: <|start|>role<|message|>content<|end|>
    /// Uses <|channel|>analysis for thinking, <|channel|>final for output
    GptOss,
    /// DeepSeek R1 format (similar to Qwen but uses <think> tags in content)
    DeepSeekR1,
}

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
    /// If true, use text completions with thinking-aware grammar
    /// This allows models to think freely before constrained tool output
    pub allow_thinking: bool,
    /// Chat template format (used for text completions with thinking)
    pub chat_template: ChatTemplate,
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
            chat_template: ChatTemplate::default(),
        }
    }
}

impl ChatTemplate {
    /// Build a text completion prompt from chat messages
    /// Returns the prompt string with thinking mode started
    pub fn build_prompt_with_thinking(&self, messages: &[ChatMessage]) -> String {
        match self {
            ChatTemplate::Qwen3 | ChatTemplate::DeepSeekR1 => {
                let mut prompt = String::new();
                for msg in messages {
                    prompt.push_str(&format!(
                        "<|im_start|>{}\n{}<|im_end|>\n",
                        msg.role, msg.content
                    ));
                }
                // Start assistant response with thinking
                prompt.push_str("<|im_start|>assistant\n<think>");
                prompt
            }
            ChatTemplate::Llama3 => {
                let mut prompt = String::new();
                for msg in messages {
                    prompt.push_str(&format!(
                        "<|start_header_id|>{}<|end_header_id|>\n\n{}<|eot_id|>",
                        msg.role, msg.content
                    ));
                }
                // Start assistant response with thinking
                prompt.push_str("<|start_header_id|>assistant<|end_header_id|>\n\n<think>");
                prompt
            }
            ChatTemplate::GptOss => {
                let mut prompt = String::new();
                for msg in messages {
                    prompt.push_str(&format!(
                        "<|start|>{}<|message|>{}<|end|>\n",
                        msg.role, msg.content
                    ));
                }
                // Start assistant response with analysis channel (thinking)
                prompt.push_str("<|start|>assistant<|channel|>analysis<|message|>");
                prompt
            }
        }
    }

    /// Build a text completion prompt without thinking mode
    pub fn build_prompt(&self, messages: &[ChatMessage]) -> String {
        match self {
            ChatTemplate::Qwen3 | ChatTemplate::DeepSeekR1 => {
                let mut prompt = String::new();
                for msg in messages {
                    prompt.push_str(&format!(
                        "<|im_start|>{}\n{}<|im_end|>\n",
                        msg.role, msg.content
                    ));
                }
                prompt.push_str("<|im_start|>assistant\n");
                prompt
            }
            ChatTemplate::Llama3 => {
                let mut prompt = String::new();
                for msg in messages {
                    prompt.push_str(&format!(
                        "<|start_header_id|>{}<|end_header_id|>\n\n{}<|eot_id|>",
                        msg.role, msg.content
                    ));
                }
                prompt.push_str("<|start_header_id|>assistant<|end_header_id|>\n\n");
                prompt
            }
            ChatTemplate::GptOss => {
                let mut prompt = String::new();
                for msg in messages {
                    prompt.push_str(&format!(
                        "<|start|>{}<|message|>{}<|end|>\n",
                        msg.role, msg.content
                    ));
                }
                // Start with final channel for direct output
                prompt.push_str("<|start|>assistant<|channel|>final<|message|>");
                prompt
            }
        }
    }

    /// Get the thinking end marker for this template
    pub fn thinking_end_marker(&self) -> &'static str {
        match self {
            ChatTemplate::Qwen3 | ChatTemplate::DeepSeekR1 | ChatTemplate::Llama3 => "</think>",
            ChatTemplate::GptOss => "<|end|>",
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

        // Use text completions for constrained mode with thinking
        // This bypasses llama-server's thinking extraction which breaks grammar
        if mode == Mode::Constrained && self.config.allow_thinking {
            return self.run_test_text_completion(case, &messages).await;
        }

        // Build chat completion request
        let mut request = CompletionRequest::new(&self.config.model, messages)
            .with_max_tokens(self.config.max_tokens)
            .with_temperature(0.0);

        // Add grammar if constrained mode
        if mode == Mode::Constrained {
            if let Some(grammar) = &self.config.grammar {
                request = request.with_grammar(grammar.as_str());
                // Disable thinking for non-thinking grammar
                request = request.without_thinking();
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

    /// Run test using text completions (for thinking mode with grammar)
    async fn run_test_text_completion(
        &self,
        case: &TestCase,
        messages: &[ChatMessage],
    ) -> Result<TestResult, Box<dyn std::error::Error>> {
        // Build prompt with thinking tag included
        let prompt = self.config.chat_template.build_prompt_with_thinking(messages);

        // Build thinking-aware grammar
        // The grammar expects: thinking_content </think> ws tool
        let grammar = self.build_thinking_grammar();

        let request = TextCompletionRequest::new(&self.config.model, prompt)
            .with_max_tokens(self.config.max_tokens)
            .with_temperature(0.0)
            .with_grammar(&grammar);

        // Execute
        let (response, duration) = self.client.complete_text(request).await?;

        let text = LlamaClient::extract_text(&response).unwrap_or("").to_string();

        // Parse thinking and output from response
        // Format is: thinking_content</think>\n\ntool_call
        let (thinking, output) = self.parse_thinking_response(&text);

        let score = Scorer::score(&output, &case.expected);

        Ok(TestResult {
            case: case.name.clone(),
            mode: Mode::Constrained,
            model: self.config.model.clone(),
            output,
            thinking,
            score,
            latency_ms: duration.as_millis() as u64,
            tokens: response.usage.completion_tokens,
        })
    }

    /// Build grammar for text completions with thinking
    ///
    /// Template-aware grammar generation:
    /// - Qwen/DeepSeek: thinking_content </think> ws tool
    /// - GPT-OSS: thinking_content <|end|>\n<|start|>assistant<|channel|>final<|message|> tool
    fn build_thinking_grammar(&self) -> String {
        let tool_rules = self.extract_tool_rules();

        match self.config.chat_template {
            ChatTemplate::GptOss => {
                // GPT-OSS: analysis ends with <|end|>, then final channel starts
                format!(
                    r#"root ::= think-content "<|end|>\n<|start|>assistant<|channel|>final<|message|>" tool
think-content ::= think-char*
think-char ::= [^<] | "<" [^|] | "<|" [^e] | "<|e" [^n] | "<|en" [^d] | "<|end" [^|] | "<|end|" [^>]
{}"#,
                    tool_rules
                )
            }
            _ => {
                // Qwen3/DeepSeek/Llama: uses </think> marker
                format!(
                    r#"root ::= think-content "</think>" ws tool
think-content ::= think-char*
think-char ::= [^<] | "<" [^/] | "</" [^t] | "</t" [^h] | "</th" [^i] | "</thi" [^n] | "</thin" [^k] | "</think" [^>]
ws ::= [ \t\n]*
{}"#,
                    tool_rules
                )
            }
        }
    }

    /// Extract tool definition rules from the configured grammar
    fn extract_tool_rules(&self) -> String {
        let grammar = self.config.grammar.as_ref()
            .map(|g| g.as_str())
            .unwrap_or("");

        // Find where tool definitions start (after any thinking-related rules)
        // Look for "tool ::=" which is the main tool definition
        if let Some(idx) = grammar.find("tool ::=") {
            // Return everything from "tool ::=" onwards, but skip any thinking-related lines
            let tool_section = &grammar[idx..];

            // Filter out thinking-related rules if present
            tool_section
                .lines()
                .filter(|line| {
                    let trimmed = line.trim();
                    !trimmed.starts_with("thinking")
                        && !trimmed.starts_with("think-")
                        && !trimmed.starts_with("# Optional thinking")
                        && !trimmed.starts_with("root ::=")
                })
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            // Fallback: simple tool grammar
            r#"tool ::= name "(" params? ")"
name ::= [a-z_]+
params ::= param ("," ws param)*
param ::= [a-z_]+ "=\"" [^"]* "\""
ws ::= [ \t]*"#.to_string()
        }
    }

    /// Parse thinking content and tool output from text completion response
    fn parse_thinking_response(&self, text: &str) -> (Option<String>, String) {
        let marker = self.config.chat_template.thinking_end_marker();

        // For GPT-OSS, the full transition is longer
        let (split_marker, tool_prefix) = match self.config.chat_template {
            ChatTemplate::GptOss => {
                // Look for the full channel transition
                ("<|end|>", "<|start|>assistant<|channel|>final<|message|>")
            }
            _ => (marker, ""),
        };

        if let Some(idx) = text.find(split_marker) {
            let thinking = text[..idx].trim().to_string();
            let mut output = text[idx + split_marker.len()..].trim().to_string();

            // For GPT-OSS, strip the channel prefix if present
            if !tool_prefix.is_empty() {
                if let Some(stripped) = output.strip_prefix(tool_prefix) {
                    output = stripped.trim().to_string();
                }
            }

            (
                if thinking.is_empty() { None } else { Some(thinking) },
                output,
            )
        } else {
            // No marker found - treat whole thing as output
            (None, text.to_string())
        }
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
