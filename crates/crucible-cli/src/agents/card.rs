use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Backend configuration for an agent
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum BackendConfig {
    Ollama {
        endpoint: String,
        model: String,
    },
    OpenAI {
        endpoint: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        api_key: Option<String>,
        model: String,
    },
    Anthropic {
        api_key: String,
        model: String,
    },
    #[allow(dead_code)]
    A2A {
        agent_id: String,
    },
}

/// Agent card - unified representation of an agent
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentCard {
    pub name: String,
    pub capabilities: Vec<String>,
    pub tags: Vec<String>,
    pub backend: BackendConfig,
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(default = "default_owner")]
    pub owner: String,
    #[serde(default = "default_shareable")]
    pub shareable: bool,
    #[serde(skip)]
    pub system_prompt: String,
}

fn default_owner() -> String {
    "local".to_string()
}

fn default_shareable() -> bool {
    true
}

impl AgentCard {
    /// Parse an agent card from a markdown file with YAML frontmatter
    pub fn from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read agent file: {}", path.display()))?;
        Self::from_str(&content)
    }

    /// Parse an agent card from a string with YAML frontmatter
    pub fn from_str(content: &str) -> Result<Self> {
        // Manually extract YAML frontmatter
        let yaml_section = content
            .strip_prefix("---\n")
            .or_else(|| content.strip_prefix("---\r\n"))
            .ok_or_else(|| anyhow::anyhow!("No frontmatter delimiter found"))?;

        let end_marker_idx = yaml_section.find("\n---\n")
            .or_else(|| yaml_section.find("\r\n---\r\n"))
            .ok_or_else(|| anyhow::anyhow!("No closing frontmatter delimiter found"))?;

        let yaml_str = &yaml_section[..end_marker_idx];

        // Get content after the closing delimiter, properly handling both \n and \r\n
        let after_marker = &yaml_section[end_marker_idx..];
        let system_prompt = after_marker
            .strip_prefix("\n---\n")
            .or_else(|| after_marker.strip_prefix("\r\n---\r\n"))
            .unwrap_or(after_marker)
            .trim()
            .to_string();

        // Parse the YAML frontmatter
        let mut card: AgentCard = serde_yaml::from_str(yaml_str)
            .context("Failed to parse agent card frontmatter")?;

        // Set the system prompt from the content after frontmatter
        card.system_prompt = system_prompt;

        Ok(card)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;

    #[test]
    fn test_parse_agent_card_with_minimal_frontmatter() {
        let content = "---
name: test-agent
capabilities:
  - rust
  - api-design
tags:
  - '#development/backend'
backend:
  type: ollama
  endpoint: http://localhost:11434
  model: qwen2.5-coder:7b
---

You are a test agent.";

        let card = AgentCard::from_str(content).unwrap();

        assert_eq!(card.name, "test-agent");
        assert_eq!(card.capabilities, vec!["rust", "api-design"]);
        assert_eq!(card.tags, vec!["#development/backend"]);
        assert_eq!(card.system_prompt, "You are a test agent.");
        assert_eq!(card.owner, "local"); // default
        assert!(card.shareable); // default

        match card.backend {
            BackendConfig::Ollama { endpoint, model } => {
                assert_eq!(endpoint, "http://localhost:11434");
                assert_eq!(model, "qwen2.5-coder:7b");
            }
            _ => panic!("Expected Ollama backend"),
        }
    }

    #[test]
    fn test_parse_agent_card_with_all_fields() {
        let content = "---
name: full-agent
capabilities:
  - rust
  - python
  - testing
tags:
  - '#development'
  - '#testing'
backend:
  type: ollama
  endpoint: https://llama.terminal.krohnos.io
  model: deepseek-coder:6.7b
temperature: 0.2
max_tokens: 4096
owner: test-user
shareable: false
---

You are a comprehensive test agent with all fields specified.";

        let card = AgentCard::from_str(content).unwrap();

        assert_eq!(card.name, "full-agent");
        assert_eq!(card.temperature, Some(0.2));
        assert_eq!(card.max_tokens, Some(4096));
        assert_eq!(card.owner, "test-user");
        assert!(!card.shareable);
        assert_eq!(card.system_prompt, "You are a comprehensive test agent with all fields specified.");
    }

    #[test]
    fn test_parse_agent_card_from_file() {
        let temp_dir = TempDir::new().unwrap();
        let agent_file = temp_dir.path().join("backend-dev.md");

        let content = "---
name: backend-dev
capabilities:
  - rust
  - api-design
  - database
tags:
  - '#development/backend/rust'
backend:
  type: ollama
  endpoint: http://localhost:11434
  model: qwen2.5-coder:7b
temperature: 0.2
---

You are an expert backend developer specializing in Rust.";

        fs::write(&agent_file, content).unwrap();

        let card = AgentCard::from_file(&agent_file).unwrap();

        assert_eq!(card.name, "backend-dev");
        assert_eq!(card.capabilities, vec!["rust", "api-design", "database"]);
        assert_eq!(card.system_prompt, "You are an expert backend developer specializing in Rust.");
    }

    #[test]
    fn test_parse_agent_card_with_openai_backend() {
        let content = "---
name: openai-agent
capabilities:
  - general
tags:
  - '#general'
backend:
  type: openai
  endpoint: https://api.openai.com/v1
  api_key: sk-test123
  model: gpt-4
---

You are an OpenAI-powered agent.";

        let card = AgentCard::from_str(content).unwrap();

        match card.backend {
            BackendConfig::OpenAI { endpoint, api_key, model } => {
                assert_eq!(endpoint, "https://api.openai.com/v1");
                assert_eq!(api_key, Some("sk-test123".to_string()));
                assert_eq!(model, "gpt-4");
            }
            _ => panic!("Expected OpenAI backend"),
        }
    }

    #[test]
    fn test_parse_agent_card_multiline_prompt() {
        let content = "---
name: multiline-agent
capabilities:
  - test
tags:
  - '#test'
backend:
  type: ollama
  endpoint: http://localhost:11434
  model: test-model
---

You are a test agent.

This is a multiline system prompt.
It should preserve all lines and formatting.";

        let card = AgentCard::from_str(content).unwrap();

        let expected_prompt = "You are a test agent.\n\nThis is a multiline system prompt.\nIt should preserve all lines and formatting.";
        assert_eq!(card.system_prompt, expected_prompt);
    }

    #[test]
    fn test_parse_agent_card_fails_on_invalid_yaml() {
        let content = "---
name: bad-agent
this is not valid YAML{{{
---

Prompt.";

        let result = AgentCard::from_str(content);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_agent_card_fails_on_missing_required_fields() {
        let content = "---
name: incomplete-agent
# Missing capabilities, tags, backend
---

Prompt.";

        let result = AgentCard::from_str(content);
        assert!(result.is_err());
    }
}
