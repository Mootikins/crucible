//! Model discovery for external agents
//!
//! This module provides functionality to discover available models from external
//! agents like OpenCode.

use anyhow::Result;
use tokio::process::Command;

/// Parse the output of `opencode models list`
///
/// Expects one model ID per line in the format `provider/model-name`.
/// Empty lines are filtered out.
pub fn parse_opencode_models(output: &str) -> Vec<String> {
    output
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .map(|line| line.to_string())
        .collect()
}

/// Fetch available models from OpenCode
///
/// Runs `opencode models list` and parses the output.
/// Returns an empty vector if OpenCode is not available or the command fails.
pub async fn fetch_opencode_models() -> Result<Vec<String>> {
    fetch_opencode_models_from_path("opencode").await
}

/// Fetch available models from a specific OpenCode binary path
///
/// Returns an empty vector if the binary doesn't exist or the command fails.
/// This function is primarily for testing.
async fn fetch_opencode_models_from_path(opencode_path: &str) -> Result<Vec<String>> {
    let output = match Command::new(opencode_path)
        .arg("models")
        .arg("list")
        .output()
        .await
    {
        Ok(output) => output,
        Err(_) => {
            // Binary not found or execution failed
            return Ok(Vec::new());
        }
    };

    if !output.status.success() {
        // Command failed
        return Ok(Vec::new());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(parse_opencode_models(&stdout))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_opencode_models_output() {
        let output = "anthropic/claude-sonnet-4\nopenai/gpt-4o\nollama/llama3.2\n";
        let models = parse_opencode_models(output);
        assert_eq!(
            models,
            vec![
                "anthropic/claude-sonnet-4",
                "openai/gpt-4o",
                "ollama/llama3.2"
            ]
        );
    }

    #[test]
    fn test_parse_opencode_models_empty() {
        let output = "";
        let models = parse_opencode_models(output);
        assert_eq!(models, Vec::<String>::new());
    }

    #[test]
    fn test_parse_opencode_models_with_empty_lines() {
        let output = "model-a\n\nmodel-b\n\n";
        let models = parse_opencode_models(output);
        assert_eq!(models, vec!["model-a", "model-b"]);
    }

    #[tokio::test]
    async fn test_fetch_models_opencode_not_found() {
        // Should return empty vec when binary doesn't exist
        let result = fetch_opencode_models_from_path("/nonexistent/opencode").await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_fetch_models_command_fails() {
        // Should handle command execution failures gracefully
        let result = fetch_opencode_models_from_path("/bin/false").await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }
}
