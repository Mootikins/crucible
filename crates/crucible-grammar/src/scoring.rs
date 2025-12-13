//! Scoring metrics for tool call accuracy

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

/// Scorer for comparing actual vs expected tool calls
pub struct Scorer;

impl Scorer {
    /// Parse a tool call from output string
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
