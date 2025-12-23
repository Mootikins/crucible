//! Report generation for TOON LLM evaluation
//!
//! Generates markdown reports with summary tables and detailed results.

use super::prompts::{ConversionDirection, PromptConfig};
use super::validation::{ToonError, ValidationResult};
use chrono::Utc;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// A single test result
#[derive(Debug, Clone)]
pub struct TestResult {
    /// Fixture ID
    pub fixture_id: String,
    /// Conversion direction
    pub direction: ConversionDirection,
    /// Prompt configuration used
    pub config: PromptConfig,
    /// Validation result
    pub validation: ValidationResult,
}

/// Aggregated results for a configuration
#[derive(Debug, Default)]
pub struct ConfigResults {
    pub passed: usize,
    pub failed: usize,
    pub errors_by_type: HashMap<String, usize>,
}

impl ConfigResults {
    pub fn pass_rate(&self) -> f64 {
        let total = self.passed + self.failed;
        if total == 0 {
            0.0
        } else {
            (self.passed as f64 / total as f64) * 100.0
        }
    }
}

/// Complete evaluation report
#[derive(Debug)]
pub struct EvalReport {
    /// Model name
    pub model: String,
    /// Endpoint URL (sanitized)
    pub endpoint: String,
    /// All test results
    pub results: Vec<TestResult>,
}

impl EvalReport {
    /// Create a new report
    pub fn new(model: String, endpoint: String) -> Self {
        Self {
            model,
            endpoint,
            results: Vec::new(),
        }
    }

    /// Add a test result
    pub fn add_result(&mut self, result: TestResult) {
        self.results.push(result);
    }

    /// Aggregate results by direction and config
    pub fn aggregate(&self) -> HashMap<(ConversionDirection, String), ConfigResults> {
        let mut aggregated: HashMap<(ConversionDirection, String), ConfigResults> = HashMap::new();

        for result in &self.results {
            let key = (result.direction, result.config.to_string());
            let entry = aggregated.entry(key).or_default();

            if result.validation.success {
                entry.passed += 1;
            } else {
                entry.failed += 1;
                for error in &result.validation.errors {
                    let error_type = error_type_name(error);
                    *entry
                        .errors_by_type
                        .entry(error_type.to_string())
                        .or_default() += 1;
                }
            }
        }

        aggregated
    }

    /// Generate markdown report
    pub fn to_markdown(&self) -> String {
        let mut md = String::new();

        // Header
        md.push_str("# TOON LLM Evaluation Report\n\n");
        md.push_str(&format!(
            "**Generated:** {}\n\n",
            Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
        ));
        md.push_str(&format!("**Model:** {}\n\n", self.model));
        md.push_str(&format!(
            "**Endpoint:** {}\n\n",
            sanitize_endpoint(&self.endpoint)
        ));

        // Summary table
        md.push_str("## Summary\n\n");
        md.push_str("| Direction | Config | Pass | Fail | Rate |\n");
        md.push_str("|-----------|--------|------|------|------|\n");

        let aggregated = self.aggregate();
        let mut keys: Vec<_> = aggregated.keys().collect();
        keys.sort_by(|a, b| a.0.to_string().cmp(&b.0.to_string()).then(a.1.cmp(&b.1)));

        for key in keys {
            let stats = &aggregated[key];
            md.push_str(&format!(
                "| {} | {} | {} | {} | {:.1}% |\n",
                key.0,
                key.1,
                stats.passed,
                stats.failed,
                stats.pass_rate()
            ));
        }
        md.push('\n');

        // Error analysis
        md.push_str("## Error Analysis\n\n");

        let mut all_errors: HashMap<String, usize> = HashMap::new();
        for result in &self.results {
            for error in &result.validation.errors {
                let error_type = error_type_name(error);
                *all_errors.entry(error_type.to_string()).or_default() += 1;
            }
        }

        if all_errors.is_empty() {
            md.push_str("No errors recorded.\n\n");
        } else {
            md.push_str("| Error Type | Count |\n");
            md.push_str("|------------|-------|\n");

            let mut error_types: Vec<_> = all_errors.into_iter().collect();
            error_types.sort_by(|a, b| b.1.cmp(&a.1));

            for (error_type, count) in error_types {
                md.push_str(&format!("| {} | {} |\n", error_type, count));
            }
            md.push('\n');
        }

        // Detailed results by direction
        for direction in [
            ConversionDirection::JsonToToon,
            ConversionDirection::ToonToJson,
        ] {
            let dir_results: Vec<_> = self
                .results
                .iter()
                .filter(|r| r.direction == direction)
                .collect();

            if dir_results.is_empty() {
                continue;
            }

            md.push_str(&format!("## {} Results\n\n", direction));

            // Group by config
            let mut by_config: HashMap<String, Vec<&TestResult>> = HashMap::new();
            for result in dir_results {
                by_config
                    .entry(result.config.to_string())
                    .or_default()
                    .push(result);
            }

            let mut configs: Vec<_> = by_config.keys().cloned().collect();
            configs.sort();

            for config in configs {
                let results = &by_config[&config];
                md.push_str(&format!("### Config: {}\n\n", config));

                for result in results.iter() {
                    let status = if result.validation.success {
                        "✓"
                    } else {
                        "✗"
                    };
                    md.push_str(&format!("- {} **{}**", status, result.fixture_id));

                    if !result.validation.errors.is_empty() {
                        let error_summary: Vec<_> = result
                            .validation
                            .errors
                            .iter()
                            .map(|e| error_type_name(e))
                            .collect();
                        md.push_str(&format!(" - {}", error_summary.join(", ")));
                    }
                    md.push('\n');
                }
                md.push('\n');
            }
        }

        // Failure details
        let failures: Vec<_> = self
            .results
            .iter()
            .filter(|r| !r.validation.success)
            .collect();

        if !failures.is_empty() {
            md.push_str("## Failure Details\n\n");

            for (i, result) in failures.iter().enumerate().take(10) {
                md.push_str(&format!(
                    "### {} ({} / {})\n\n",
                    result.fixture_id, result.direction, result.config
                ));

                md.push_str("**Errors:**\n");
                for error in &result.validation.errors {
                    md.push_str(&format!("- {}\n", error));
                }
                md.push('\n');

                // Truncated raw response
                let raw = &result.validation.raw_response;
                let preview: String = raw.chars().take(500).collect();
                md.push_str("**Response preview:**\n```\n");
                md.push_str(&preview);
                if raw.len() > 500 {
                    md.push_str("\n...(truncated)");
                }
                md.push_str("\n```\n\n");

                if i >= 9 {
                    md.push_str(&format!(
                        "*...and {} more failures not shown*\n\n",
                        failures.len() - 10
                    ));
                    break;
                }
            }
        }

        md
    }

    /// Save report to file
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let content = self.to_markdown();
        fs::write(path, content)
    }
}

/// Get error type name for categorization
fn error_type_name(error: &ToonError) -> &'static str {
    match error {
        ToonError::InvalidSyntax(_) => "InvalidSyntax",
        ToonError::MissingField(_) => "MissingField",
        ToonError::ExtraField(_) => "ExtraField",
        ToonError::WrongType { .. } => "WrongType",
        ToonError::WrongArrayLength { .. } => "WrongArrayLength",
        ToonError::ValueMismatch { .. } => "ValueMismatch",
        ToonError::EmptyResponse => "EmptyResponse",
        ToonError::ContainsNonToon(_) => "ContainsNonToon",
    }
}

/// Sanitize endpoint URL for display (hide credentials, internal hostnames)
fn sanitize_endpoint(endpoint: &str) -> String {
    // Replace any credentials
    let sanitized = endpoint.replace('@', "[at]").to_string();

    // Just show the host pattern
    if let Some(host_start) = sanitized.find("://") {
        let after_scheme = &sanitized[host_start + 3..];
        if let Some(path_start) = after_scheme.find('/') {
            let host = &after_scheme[..path_start];
            return format!("{}://{}/*", &sanitized[..host_start], host);
        }
    }

    sanitized
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::toon_eval::validation::ValidationResult;
    use serde_json::json;

    #[test]
    fn test_config_results_pass_rate() {
        let results = ConfigResults {
            passed: 7,
            failed: 3,
            ..Default::default()
        };
        assert!((results.pass_rate() - 70.0).abs() < 0.01);
    }

    #[test]
    fn test_report_generation() {
        let mut report = EvalReport::new(
            "test-model".to_string(),
            "http://localhost:11434".to_string(),
        );

        report.add_result(TestResult {
            fixture_id: "test1".to_string(),
            direction: ConversionDirection::JsonToToon,
            config: PromptConfig::ZeroShot,
            validation: ValidationResult::success(json!({}), "test".to_string()),
        });

        report.add_result(TestResult {
            fixture_id: "test2".to_string(),
            direction: ConversionDirection::JsonToToon,
            config: PromptConfig::ZeroShot,
            validation: ValidationResult::failure(
                vec![ToonError::InvalidSyntax("test error".to_string())],
                "bad output".to_string(),
            ),
        });

        let md = report.to_markdown();
        assert!(md.contains("# TOON LLM Evaluation Report"));
        assert!(md.contains("test-model"));
        assert!(md.contains("| json_to_toon | zero_shot |"));
        assert!(md.contains("InvalidSyntax"));
    }

    #[test]
    fn test_sanitize_endpoint() {
        assert_eq!(
            sanitize_endpoint("https://llama.example.com/api"),
            "https://llama.example.com/*"
        );
        assert_eq!(
            sanitize_endpoint("http://localhost:11434"),
            "http://localhost:11434"
        );
    }
}
