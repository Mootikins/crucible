//! Format conversion between JSON and TOON

use crate::TqError;
use serde_json::Value;

/// Supported input formats
#[derive(Debug, Clone, Copy, Default, clap::ValueEnum)]
pub enum InputFormat {
    /// Auto-detect based on content
    #[default]
    Auto,
    /// JSON format
    Json,
    /// TOON format
    Toon,
}

/// Internal format representation
#[derive(Debug, Clone, Copy)]
pub enum Format {
    Json,
    Toon,
}

/// Output format options
#[derive(Debug, Clone, Copy)]
pub enum OutputFormat {
    Json,
    Toon,
}

impl InputFormat {
    /// Detect the actual format from input content
    pub fn detect(&self, input: &str) -> Format {
        match self {
            InputFormat::Json => Format::Json,
            InputFormat::Toon => Format::Toon,
            InputFormat::Auto => detect_format(input),
        }
    }
}

/// Auto-detect format based on content heuristics
fn detect_format(input: &str) -> Format {
    let trimmed = input.trim();

    // Empty input defaults to JSON
    if trimmed.is_empty() {
        return Format::Json;
    }

    // JSON starts with { or [ or is a literal (true, false, null, number, string)
    let first_char = trimmed.chars().next().unwrap();

    match first_char {
        '{' | '[' => Format::Json,
        '"' => {
            // Could be JSON string or TOON - check if it's valid JSON
            if serde_json::from_str::<Value>(trimmed).is_ok() {
                Format::Json
            } else {
                Format::Toon
            }
        }
        _ => {
            // Check for JSON literals
            if trimmed == "true" || trimmed == "false" || trimmed == "null" {
                return Format::Json;
            }

            // Check if it looks like a number
            if trimmed.parse::<f64>().is_ok() {
                return Format::Json;
            }

            // Check for TOON-like patterns: "key: value" or "key:" on a line
            if trimmed.lines().any(|line| {
                let line = line.trim();
                line.contains(": ") || line.ends_with(':')
            }) {
                return Format::Toon;
            }

            // Default to trying JSON first
            if serde_json::from_str::<Value>(trimmed).is_ok() {
                Format::Json
            } else {
                Format::Toon
            }
        }
    }
}

/// Parse input in the detected format
pub fn parse_input(input: &str, format: Format) -> Result<Value, TqError> {
    match format {
        Format::Json => serde_json::from_str(input).map_err(TqError::JsonParse),
        Format::Toon => {
            toon_format::decode_default(input).map_err(|e| TqError::ToonParse(e.to_string()))
        }
    }
}

/// Convert a JSON value to TOON string
#[allow(dead_code)]
pub fn to_toon(value: &Value) -> Result<String, TqError> {
    toon_format::encode_default(value).map_err(|e| TqError::ToonParse(e.to_string()))
}

/// Convert a JSON value to JSON string
#[allow(dead_code)]
pub fn to_json(value: &Value, pretty: bool) -> Result<String, TqError> {
    if pretty {
        serde_json::to_string_pretty(value).map_err(TqError::JsonParse)
    } else {
        serde_json::to_string(value).map_err(TqError::JsonParse)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_json_object() {
        assert!(matches!(detect_format(r#"{"name": "Ada"}"#), Format::Json));
    }

    #[test]
    fn test_detect_json_array() {
        assert!(matches!(detect_format(r#"[1, 2, 3]"#), Format::Json));
    }

    #[test]
    fn test_detect_toon_object() {
        assert!(matches!(detect_format("name: Ada\nage: 30"), Format::Toon));
    }

    #[test]
    fn test_detect_json_literals() {
        assert!(matches!(detect_format("true"), Format::Json));
        assert!(matches!(detect_format("false"), Format::Json));
        assert!(matches!(detect_format("null"), Format::Json));
        assert!(matches!(detect_format("42"), Format::Json));
        assert!(matches!(detect_format("3.14"), Format::Json));
    }

    #[test]
    fn test_parse_json() {
        let input = r#"{"name": "Ada", "age": 30}"#;
        let result = parse_input(input, Format::Json).unwrap();
        assert_eq!(result["name"], "Ada");
        assert_eq!(result["age"], 30);
    }

    #[test]
    fn test_parse_toon() {
        let input = "name: Ada\nage: 30";
        let result = parse_input(input, Format::Toon).unwrap();
        assert_eq!(result["name"], "Ada");
        assert_eq!(result["age"], 30);
    }

    #[test]
    fn test_roundtrip_json_toon() {
        let json = serde_json::json!({"name": "Ada", "active": true});
        let toon = to_toon(&json).unwrap();
        let back = parse_input(&toon, Format::Toon).unwrap();
        assert_eq!(json, back);
    }
}
