//! Scoring metrics for tool call accuracy
//!
//! Supports multiple tool call formats:
//! - **Structured**: `tool(param="value", ...)` - full param decomposition
//! - **Passthrough**: `tool(args="...")` - args passed through
//! - **Raw CLI**: `command args...` - directly executable

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Score for a single test result
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Score {
    /// Did the output parse successfully?
    pub parsed: bool,
    /// Was the correct tool selected?
    pub tool_correct: bool,
    /// Parameter accuracy (0.0 - 1.0)
    pub param_accuracy: f64,
    /// Task completion (for live mode)
    pub task_success: Option<bool>,
}

impl Score {
    /// Calculate overall score (0.0 - 1.0)
    pub fn overall(&self) -> f64 {
        let parse_score = if self.parsed { 1.0 } else { 0.0 };
        let tool_score = if self.tool_correct { 1.0 } else { 0.0 };
        let task_score = self.task_success.map(|s| if s { 1.0 } else { 0.0 });

        match task_score {
            Some(ts) => (parse_score + tool_score + self.param_accuracy + ts) / 4.0,
            None => (parse_score + tool_score + self.param_accuracy) / 3.0,
        }
    }
}

/// Expected tool call for comparison
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpectedToolCall {
    pub tool: String,
    pub params: HashMap<String, serde_json::Value>,
}

/// Parsed tool call - unified representation
#[derive(Debug, Clone, PartialEq)]
pub enum ParsedToolCall {
    /// Structured: tool(param="value", ...)
    Structured {
        tool: String,
        params: HashMap<String, String>,
    },
    /// Passthrough: tool(args="...")
    Passthrough { tool: String, args: String },
    /// Raw CLI: command args...
    RawCli { command: String, args: String },
}

impl ParsedToolCall {
    /// Get the tool/command name
    pub fn name(&self) -> &str {
        match self {
            Self::Structured { tool, .. } => tool,
            Self::Passthrough { tool, .. } => tool,
            Self::RawCli { command, .. } => command,
        }
    }

    /// Check if this matches an expected tool call
    pub fn matches(&self, expected: &ExpectedToolCall) -> bool {
        match self {
            Self::Structured { tool, params } => {
                if tool != &expected.tool {
                    return false;
                }
                // Check all expected params match
                expected.params.iter().all(|(key, val)| {
                    let expected_str = match val {
                        serde_json::Value::String(s) => s.clone(),
                        v => v.to_string().trim_matches('"').to_string(),
                    };
                    params.get(key).map(|v| v == &expected_str).unwrap_or(false)
                })
            }
            Self::Passthrough { tool, args } => {
                if tool != &expected.tool {
                    return false;
                }
                // For passthrough, check if expected args substring exists
                if let Some(expected_args) = expected.params.get("args") {
                    let expected_str = expected_args.as_str().unwrap_or("");
                    args.contains(expected_str) || expected_str.is_empty()
                } else {
                    true
                }
            }
            Self::RawCli { command, args } => {
                // Map CLI commands to tool names
                let tool_name = match command.as_str() {
                    "rg" => "rg",
                    "fd" => "fd",
                    "cat" => "read",
                    "ls" => "ls",
                    "grep" => "grep",
                    other => other,
                };
                if tool_name != expected.tool {
                    return false;
                }
                // For raw CLI, check key values appear in args
                expected.params.iter().all(|(key, val)| {
                    if key == "args" {
                        return true; // Skip generic args key
                    }
                    let val_str = match val {
                        serde_json::Value::String(s) => s.clone(),
                        v => v.to_string().trim_matches('"').to_string(),
                    };
                    args.contains(&val_str)
                })
            }
        }
    }
}

/// Scorer for comparing actual vs expected tool calls
pub struct Scorer;

impl Scorer {
    /// Parse output into a unified ParsedToolCall
    ///
    /// Auto-detects format:
    /// - `tool(param="val")` → Structured
    /// - `tool(args="...")` → Passthrough
    /// - `command args` → RawCli
    pub fn parse(output: &str) -> Option<ParsedToolCall> {
        let output = output.trim();

        // Try structured/passthrough first (has parentheses)
        if let Some(paren_start) = output.find('(') {
            if let Some(paren_end) = output.rfind(')') {
                let tool_name = output[..paren_start].trim().to_string();
                let params_str = &output[paren_start + 1..paren_end];

                // Check if it's passthrough format: args="..."
                if params_str.trim().starts_with("args=") {
                    let args = params_str
                        .trim()
                        .strip_prefix("args=")
                        .unwrap_or("")
                        .trim_matches('"')
                        .to_string();
                    return Some(ParsedToolCall::Passthrough { tool: tool_name, args });
                }

                // Otherwise it's structured
                let mut params = HashMap::new();
                for part in Self::split_params(params_str) {
                    let part = part.trim();
                    if part.is_empty() {
                        continue;
                    }
                    if let Some(eq_pos) = part.find('=') {
                        let key = part[..eq_pos].trim().to_string();
                        let value = part[eq_pos + 1..].trim().trim_matches('"').to_string();
                        params.insert(key, value);
                    }
                }
                return Some(ParsedToolCall::Structured { tool: tool_name, params });
            }
        }

        // Try raw CLI format: command args
        let mut parts = output.splitn(2, ' ');
        if let Some(command) = parts.next() {
            let command = command.trim().to_string();
            // Only accept known CLI commands
            if matches!(command.as_str(), "rg" | "fd" | "cat" | "ls" | "grep" | "find") {
                let args = parts.next().unwrap_or("").trim().to_string();
                return Some(ParsedToolCall::RawCli { command, args });
            }
        }

        None
    }

    /// Parse a tool call from output string (legacy)
    ///
    /// Expects format: `tool_name(param="value", ...)`
    pub fn parse_tool_call(output: &str) -> Option<(String, HashMap<String, String>)> {
        let output = output.trim();

        // Find tool name and params
        let paren_start = output.find('(')?;
        let paren_end = output.rfind(')')?;

        if paren_start >= paren_end {
            return None;
        }

        let tool_name = output[..paren_start].trim().to_string();
        let params_str = &output[paren_start + 1..paren_end];

        let mut params = HashMap::new();

        // Parse params: key="value", key="value"
        for part in Self::split_params(params_str) {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }

            if let Some(eq_pos) = part.find('=') {
                let key = part[..eq_pos].trim().to_string();
                let value = part[eq_pos + 1..].trim();
                // Remove quotes if present
                let value = value.trim_matches('"').to_string();
                params.insert(key, value);
            }
        }

        Some((tool_name, params))
    }

    /// Split params respecting quoted strings
    fn split_params(s: &str) -> Vec<&str> {
        let mut parts = Vec::new();
        let mut start = 0;
        let mut in_quotes = false;

        for (i, c) in s.char_indices() {
            match c {
                '"' => in_quotes = !in_quotes,
                ',' if !in_quotes => {
                    parts.push(&s[start..i]);
                    start = i + 1;
                }
                _ => {}
            }
        }

        if start < s.len() {
            parts.push(&s[start..]);
        }

        parts
    }

    /// Score a tool call against expected
    pub fn score(output: &str, expected: &ExpectedToolCall) -> Score {
        let parsed = Self::parse_tool_call(output);

        match parsed {
            None => Score {
                parsed: false,
                tool_correct: false,
                param_accuracy: 0.0,
                task_success: None,
            },
            Some((tool_name, actual_params)) => {
                let tool_correct = tool_name == expected.tool;
                let param_accuracy = Self::param_accuracy(&actual_params, &expected.params);

                Score {
                    parsed: true,
                    tool_correct,
                    param_accuracy,
                    task_success: None,
                }
            }
        }
    }

    /// Score using unified parser (handles all formats)
    pub fn score_unified(output: &str, expected: &ExpectedToolCall) -> Score {
        match Self::parse(output) {
            None => Score {
                parsed: false,
                tool_correct: false,
                param_accuracy: 0.0,
                task_success: None,
            },
            Some(parsed) => {
                let tool_correct = parsed.name() == expected.tool
                    || (parsed.name() == "cat" && expected.tool == "read"); // Alias

                // For raw CLI, check if key values appear in args
                let param_accuracy = match &parsed {
                    ParsedToolCall::RawCli { args, .. } => {
                        if expected.params.is_empty() {
                            1.0
                        } else {
                            let matches = expected.params.iter().filter(|(key, val)| {
                                if *key == "args" {
                                    return true;
                                }
                                let val_str = match val {
                                    serde_json::Value::String(s) => s.clone(),
                                    v => v.to_string().trim_matches('"').to_string(),
                                };
                                args.contains(&val_str)
                            }).count();
                            matches as f64 / expected.params.len() as f64
                        }
                    }
                    ParsedToolCall::Passthrough { args, .. } => {
                        // Check if expected args/values appear
                        if expected.params.is_empty() {
                            1.0
                        } else {
                            let matches = expected.params.iter().filter(|(_, val)| {
                                let val_str = match val {
                                    serde_json::Value::String(s) => s.clone(),
                                    v => v.to_string().trim_matches('"').to_string(),
                                };
                                args.contains(&val_str)
                            }).count();
                            matches as f64 / expected.params.len() as f64
                        }
                    }
                    ParsedToolCall::Structured { params: actual, .. } => {
                        Self::param_accuracy(actual, &expected.params)
                    }
                };

                Score {
                    parsed: true,
                    tool_correct,
                    param_accuracy,
                    task_success: None,
                }
            }
        }
    }

    /// Calculate parameter accuracy (Jaccard-like)
    fn param_accuracy(
        actual: &HashMap<String, String>,
        expected: &HashMap<String, serde_json::Value>,
    ) -> f64 {
        if expected.is_empty() && actual.is_empty() {
            return 1.0;
        }

        if expected.is_empty() || actual.is_empty() {
            return 0.0;
        }

        let mut matches = 0;
        let total = expected.len();

        for (key, expected_val) in expected {
            if let Some(actual_val) = actual.get(key) {
                // Compare values (convert expected to string for comparison)
                let expected_str = match expected_val {
                    serde_json::Value::String(s) => s.clone(),
                    v => v.to_string().trim_matches('"').to_string(),
                };

                if actual_val == &expected_str {
                    matches += 1;
                }
            }
        }

        matches as f64 / total as f64
    }
}

/// Aggregate scores across multiple tests
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AggregateScore {
    pub total_tests: usize,
    pub parse_rate: f64,
    pub tool_accuracy: f64,
    pub param_accuracy: f64,
    pub task_success_rate: Option<f64>,
    pub overall: f64,
}

impl AggregateScore {
    pub fn from_scores(scores: &[Score]) -> Self {
        if scores.is_empty() {
            return Self::default();
        }

        let n = scores.len() as f64;
        let parsed_count = scores.iter().filter(|s| s.parsed).count() as f64;
        let tool_correct_count = scores.iter().filter(|s| s.tool_correct).count() as f64;
        let param_sum: f64 = scores.iter().map(|s| s.param_accuracy).sum();

        let task_scores: Vec<f64> = scores
            .iter()
            .filter_map(|s| s.task_success.map(|b| if b { 1.0 } else { 0.0 }))
            .collect();

        let task_success_rate = if task_scores.is_empty() {
            None
        } else {
            Some(task_scores.iter().sum::<f64>() / task_scores.len() as f64)
        };

        let overall = scores.iter().map(|s| s.overall()).sum::<f64>() / n;

        Self {
            total_tests: scores.len(),
            parse_rate: parsed_count / n,
            tool_accuracy: tool_correct_count / n,
            param_accuracy: param_sum / n,
            task_success_rate,
            overall,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===== New unified parser tests =====

    #[test]
    fn test_parse_structured() {
        let parsed = Scorer::parse(r#"rg(pattern="TODO", path="src")"#).unwrap();
        match parsed {
            ParsedToolCall::Structured { tool, params } => {
                assert_eq!(tool, "rg");
                assert_eq!(params.get("pattern"), Some(&"TODO".to_string()));
                assert_eq!(params.get("path"), Some(&"src".to_string()));
            }
            _ => panic!("Expected Structured, got {:?}", parsed),
        }
    }

    #[test]
    fn test_parse_passthrough() {
        let parsed = Scorer::parse(r#"rg(args="-n TODO src/")"#).unwrap();
        match parsed {
            ParsedToolCall::Passthrough { tool, args } => {
                assert_eq!(tool, "rg");
                assert_eq!(args, "-n TODO src/");
            }
            _ => panic!("Expected Passthrough, got {:?}", parsed),
        }
    }

    #[test]
    fn test_parse_raw_cli() {
        let parsed = Scorer::parse("rg -n TODO src/").unwrap();
        match parsed {
            ParsedToolCall::RawCli { command, args } => {
                assert_eq!(command, "rg");
                assert_eq!(args, "-n TODO src/");
            }
            _ => panic!("Expected RawCli, got {:?}", parsed),
        }
    }

    #[test]
    fn test_parse_raw_cli_fd() {
        let parsed = Scorer::parse("fd -e rs .").unwrap();
        match parsed {
            ParsedToolCall::RawCli { command, args } => {
                assert_eq!(command, "fd");
                assert_eq!(args, "-e rs .");
            }
            _ => panic!("Expected RawCli, got {:?}", parsed),
        }
    }

    #[test]
    fn test_score_unified_raw_cli() {
        let expected = ExpectedToolCall {
            tool: "rg".to_string(),
            params: [("pattern".to_string(), serde_json::json!("TODO"))]
                .into_iter()
                .collect(),
        };

        let score = Scorer::score_unified("rg -n TODO src/", &expected);
        assert!(score.parsed);
        assert!(score.tool_correct);
        assert_eq!(score.param_accuracy, 1.0); // "TODO" appears in args
    }

    #[test]
    fn test_score_unified_passthrough() {
        let expected = ExpectedToolCall {
            tool: "rg".to_string(),
            params: [("pattern".to_string(), serde_json::json!("TODO"))]
                .into_iter()
                .collect(),
        };

        let score = Scorer::score_unified(r#"rg(args="-n TODO src/")"#, &expected);
        assert!(score.parsed);
        assert!(score.tool_correct);
        assert_eq!(score.param_accuracy, 1.0);
    }

    #[test]
    fn test_score_unified_cat_read_alias() {
        let expected = ExpectedToolCall {
            tool: "read".to_string(),
            params: [("path".to_string(), serde_json::json!("README.md"))]
                .into_iter()
                .collect(),
        };

        // Raw CLI "cat" should match expected "read"
        let score = Scorer::score_unified("cat README.md", &expected);
        assert!(score.parsed);
        assert!(score.tool_correct);
        assert_eq!(score.param_accuracy, 1.0);
    }

    // ===== Legacy parser tests =====

    #[test]
    fn test_parse_tool_call_simple() {
        let (tool, params) = Scorer::parse_tool_call(r#"read(path="README.md")"#).unwrap();
        assert_eq!(tool, "read");
        assert_eq!(params.get("path"), Some(&"README.md".to_string()));
    }

    #[test]
    fn test_parse_tool_call_multiple_params() {
        let (tool, params) =
            Scorer::parse_tool_call(r#"rg(pattern="TODO", path="src")"#).unwrap();
        assert_eq!(tool, "rg");
        assert_eq!(params.get("pattern"), Some(&"TODO".to_string()));
        assert_eq!(params.get("path"), Some(&"src".to_string()));
    }

    #[test]
    fn test_parse_tool_call_with_spaces() {
        let (tool, params) =
            Scorer::parse_tool_call(r#"edit(path="main.rs", search="foo", replace="bar")"#)
                .unwrap();
        assert_eq!(tool, "edit");
        assert_eq!(params.len(), 3);
    }

    #[test]
    fn test_score_correct() {
        let expected = ExpectedToolCall {
            tool: "read".to_string(),
            params: [("path".to_string(), serde_json::json!("README.md"))]
                .into_iter()
                .collect(),
        };

        let score = Scorer::score(r#"read(path="README.md")"#, &expected);
        assert!(score.parsed);
        assert!(score.tool_correct);
        assert_eq!(score.param_accuracy, 1.0);
    }

    #[test]
    fn test_score_wrong_tool() {
        let expected = ExpectedToolCall {
            tool: "rg".to_string(),
            params: [("pattern".to_string(), serde_json::json!("TODO"))]
                .into_iter()
                .collect(),
        };

        let score = Scorer::score(r#"read(path="src")"#, &expected);
        assert!(score.parsed);
        assert!(!score.tool_correct);
    }

    #[test]
    fn test_aggregate_scores() {
        let scores = vec![
            Score {
                parsed: true,
                tool_correct: true,
                param_accuracy: 1.0,
                task_success: None,
            },
            Score {
                parsed: true,
                tool_correct: false,
                param_accuracy: 0.5,
                task_success: None,
            },
        ];

        let agg = AggregateScore::from_scores(&scores);
        assert_eq!(agg.total_tests, 2);
        assert_eq!(agg.parse_rate, 1.0);
        assert_eq!(agg.tool_accuracy, 0.5);
        assert_eq!(agg.param_accuracy, 0.75);
    }
}
