//! Object Query (oq) module for Steel scripts
//!
//! Provides multi-format parsing (JSON, YAML, TOML, TOON) and jq-style querying.
//!
//! ## Steel Usage
//!
//! ```scheme
//! ;; Parse any format (auto-detected)
//! (oq-parse "{\"name\": \"Alice\"}")        ; JSON
//! (oq-parse "name: Alice\nage: 30")         ; YAML or TOON
//! (oq-parse "name = \"Alice\"")             ; TOML
//!
//! ;; Explicit format parsing
//! (oq-parse-as "name: Alice" "yaml")
//! (oq-parse-as "name = \"Alice\"" "toml")
//!
//! ;; Encode to different formats
//! (oq-json obj)           ; {"name":"Alice","age":30}
//! (oq-yaml obj)           ; name: Alice\nage: 30
//! (oq-toml obj)           ; name = "Alice"
//! (oq-toon obj)           ; name: Alice
//! (oq-json-pretty obj)    ; Pretty-printed JSON
//!
//! ;; Format conversion
//! (oq-convert obj "yaml")
//! (oq-convert obj "toml")
//!
//! ;; jq-style queries
//! (oq-query users ".[] | .name")
//! (oq-query users ".[0]")
//!
//! ;; Detect format
//! (oq-detect str)  ; => "json", "yaml", "toml", or "toon"
//! ```

use crate::error::SteelError;
use oq::{compile_filter, run_filter};
use serde_json::Value as JsonValue;

/// Supported data formats
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Json,
    Yaml,
    Toml,
    Toon,
}

impl Format {
    /// Parse format from string name
    pub fn from_name(name: &str) -> Option<Self> {
        match name.to_lowercase().as_str() {
            "json" => Some(Format::Json),
            "yaml" | "yml" => Some(Format::Yaml),
            "toml" => Some(Format::Toml),
            "toon" => Some(Format::Toon),
            _ => None,
        }
    }

    /// Get format name as string
    pub fn name(&self) -> &'static str {
        match self {
            Format::Json => "json",
            Format::Yaml => "yaml",
            Format::Toml => "toml",
            Format::Toon => "toon",
        }
    }
}

/// Detect the format of a string based on content heuristics
pub fn detect_format(s: &str) -> Format {
    let trimmed = s.trim();
    let lines: Vec<&str> = trimmed.lines().collect();

    // Check for TOML section headers [section] or [[table]] first
    let first_line = lines.first().map(|l| l.trim()).unwrap_or("");
    let is_toml_section = first_line.starts_with('[')
        && first_line.ends_with(']')
        && !first_line.contains(',')
        && !first_line.contains(':');

    if is_toml_section {
        return Format::Toml;
    }

    // JSON: starts with { or [
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return Format::Json;
    }

    // Check for TOML section headers [section] anywhere in file
    let has_toml_sections = lines.iter().any(|line| {
        let l = line.trim();
        l.starts_with('[')
            && l.ends_with(']')
            && !l.starts_with("[[")
            && !l.contains(',')
            && !l.contains(':')
    });

    // Check for TOML array of tables [[section]]
    let has_toml_array_tables = lines
        .iter()
        .any(|line| line.trim().starts_with("[[") && line.trim().ends_with("]]"));

    // Check for TOML-style assignments: key = "value" or key = 123
    let has_toml_assignments = lines.iter().any(|line| {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') || l.starts_with('[') {
            return false;
        }
        // Must have = with spaces around it (TOML style)
        if let Some(eq_pos) = l.find(" = ") {
            let key = &l[..eq_pos];
            !key.is_empty()
                && key
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '"')
        } else {
            false
        }
    });

    if has_toml_sections || has_toml_array_tables || has_toml_assignments {
        return Format::Toml;
    }

    // YAML detection
    if trimmed.starts_with("---") {
        return Format::Yaml;
    }

    let has_yaml_list = lines.iter().any(|line| {
        let l = line.trim();
        l.starts_with("- ") || l == "-"
    });

    let has_yaml_multiline = lines.iter().any(|line| {
        let l = line.trim();
        l.ends_with(": |") || l.ends_with(": >") || l.ends_with(": |+") || l.ends_with(": |-")
    });

    let has_yaml_nesting = lines
        .iter()
        .any(|line| line.starts_with("  ") && line.trim().contains(": "));

    if has_yaml_list || has_yaml_multiline || has_yaml_nesting {
        return Format::Yaml;
    }

    // TOON: simple key: value without YAML complexity
    let has_colon_space = lines.iter().any(|line| {
        let l = line.trim();
        !l.is_empty() && !l.starts_with('#') && l.contains(": ")
    });

    if has_colon_space {
        return Format::Toon;
    }

    // Default to JSON
    Format::Json
}

/// Parse string with explicit format
pub fn parse_with_format(s: &str, format: Format) -> Result<JsonValue, SteelError> {
    match format {
        Format::Json => serde_json::from_str(s)
            .map_err(|e| SteelError::Conversion(format!("JSON parse error: {}", e))),
        Format::Yaml => serde_yaml::from_str(s)
            .map_err(|e| SteelError::Conversion(format!("YAML parse error: {}", e))),
        Format::Toml => {
            let toml_value: toml::Value = toml::from_str(s)
                .map_err(|e| SteelError::Conversion(format!("TOML parse error: {}", e)))?;
            toml_to_json(toml_value)
        }
        Format::Toon => oq::parse_auto(s)
            .map_err(|e| SteelError::Conversion(format!("TOON parse error: {}", e))),
    }
}

/// Parse string with auto-detection
pub fn parse_auto(s: &str) -> Result<JsonValue, SteelError> {
    let format = detect_format(s);
    parse_with_format(s, format)
}

/// Convert TOML value to JSON value
fn toml_to_json(value: toml::Value) -> Result<JsonValue, SteelError> {
    match value {
        toml::Value::String(s) => Ok(JsonValue::String(s)),
        toml::Value::Integer(i) => Ok(JsonValue::Number(i.into())),
        toml::Value::Float(f) => serde_json::Number::from_f64(f)
            .map(JsonValue::Number)
            .ok_or_else(|| SteelError::Conversion("Invalid float value".to_string())),
        toml::Value::Boolean(b) => Ok(JsonValue::Bool(b)),
        toml::Value::Datetime(dt) => Ok(JsonValue::String(dt.to_string())),
        toml::Value::Array(arr) => {
            let json_arr: Result<Vec<_>, _> = arr.into_iter().map(toml_to_json).collect();
            Ok(JsonValue::Array(json_arr?))
        }
        toml::Value::Table(table) => {
            let mut map = serde_json::Map::new();
            for (k, v) in table {
                map.insert(k, toml_to_json(v)?);
            }
            Ok(JsonValue::Object(map))
        }
    }
}

/// Convert JSON value to TOML value
fn json_to_toml(value: &JsonValue) -> Result<toml::Value, SteelError> {
    match value {
        JsonValue::Null => Ok(toml::Value::String("null".to_string())),
        JsonValue::Bool(b) => Ok(toml::Value::Boolean(*b)),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(toml::Value::Integer(i))
            } else if let Some(f) = n.as_f64() {
                Ok(toml::Value::Float(f))
            } else {
                Err(SteelError::Conversion("Invalid number".to_string()))
            }
        }
        JsonValue::String(s) => Ok(toml::Value::String(s.clone())),
        JsonValue::Array(arr) => {
            let toml_arr: Result<Vec<_>, _> = arr.iter().map(json_to_toml).collect();
            Ok(toml::Value::Array(toml_arr?))
        }
        JsonValue::Object(obj) => {
            let mut table = toml::map::Map::new();
            for (k, v) in obj {
                table.insert(k.clone(), json_to_toml(v)?);
            }
            Ok(toml::Value::Table(table))
        }
    }
}

/// Encode JSON value to specified format
pub fn encode_to_format(value: &JsonValue, format: Format) -> Result<String, SteelError> {
    match format {
        Format::Json => serde_json::to_string(value)
            .map_err(|e| SteelError::Conversion(format!("JSON encode error: {}", e))),
        Format::Yaml => serde_yaml::to_string(value)
            .map_err(|e| SteelError::Conversion(format!("YAML encode error: {}", e))),
        Format::Toml => {
            let toml_value = json_to_toml(value)?;
            toml::to_string(&toml_value)
                .map_err(|e| SteelError::Conversion(format!("TOML encode error: {}", e)))
        }
        Format::Toon => oq::json_to_toon(value.clone())
            .map_err(|e| SteelError::Conversion(format!("TOON encode error: {}", e))),
    }
}

/// Encode JSON value to pretty-printed JSON
pub fn encode_json_pretty(value: &JsonValue) -> Result<String, SteelError> {
    serde_json::to_string_pretty(value)
        .map_err(|e| SteelError::Conversion(format!("JSON encode error: {}", e)))
}

/// Execute a jq-style query on JSON data
pub fn query(value: &JsonValue, filter_str: &str) -> Result<JsonValue, SteelError> {
    let filter = compile_filter(filter_str)
        .map_err(|e| SteelError::Execution(format!("Query compile error: {}", e)))?;

    let results = run_filter(&filter, value.clone())
        .map_err(|e| SteelError::Execution(format!("Query execution error: {}", e)))?;

    // Return single value or array of results
    if results.len() == 1 {
        Ok(results.into_iter().next().unwrap())
    } else {
        Ok(JsonValue::Array(results))
    }
}

/// OQ module providing data format functions for Steel
///
/// This module doesn't directly integrate with Steel's engine
/// (since Steel is !Send/!Sync). Instead, it provides functions
/// that can be called via the executor's call_function pattern.
pub struct OqModule;

impl OqModule {
    /// Generate Steel stubs for oq-* functions
    pub fn steel_stubs() -> &'static str {
        r#"
;; Object Query (oq) functions (stubs - replaced by Rust)
;; Provides multi-format parsing, encoding, and jq-style queries.

(define (oq-parse s)
  (error "oq-parse not available: no oq module registered"))

(define (oq-parse-as s format)
  (error "oq-parse-as not available: no oq module registered"))

(define (oq-json obj)
  (error "oq-json not available: no oq module registered"))

(define (oq-json-pretty obj)
  (error "oq-json-pretty not available: no oq module registered"))

(define (oq-yaml obj)
  (error "oq-yaml not available: no oq module registered"))

(define (oq-toml obj)
  (error "oq-toml not available: no oq module registered"))

(define (oq-toon obj)
  (error "oq-toon not available: no oq module registered"))

(define (oq-convert obj format)
  (error "oq-convert not available: no oq module registered"))

(define (oq-detect s)
  (error "oq-detect not available: no oq module registered"))

(define (oq-query obj filter)
  (error "oq-query not available: no oq module registered"))
"#
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // Format detection tests
    // =========================================================================

    #[test]
    fn test_detect_json() {
        assert_eq!(detect_format(r#"{"name": "Alice"}"#), Format::Json);
        assert_eq!(detect_format(r#"[1, 2, 3]"#), Format::Json);
    }

    #[test]
    fn test_detect_yaml() {
        assert_eq!(detect_format("---\nname: Alice"), Format::Yaml);
        assert_eq!(detect_format("- item1\n- item2"), Format::Yaml);
        assert_eq!(detect_format("user:\n  name: Alice"), Format::Yaml);
    }

    #[test]
    fn test_detect_toml() {
        assert_eq!(detect_format("[package]\nname = \"test\""), Format::Toml);
        assert_eq!(detect_format("name = \"Alice\"\nage = 30"), Format::Toml);
        assert_eq!(detect_format("name = \"Alice\""), Format::Toml);
    }

    #[test]
    fn test_detect_toon() {
        assert_eq!(detect_format("name: Alice\nage: 30"), Format::Toon);
    }

    // =========================================================================
    // Parsing tests
    // =========================================================================

    #[test]
    fn test_parse_json() {
        let result = parse_with_format(r#"{"name": "Alice", "age": 30}"#, Format::Json).unwrap();
        assert_eq!(result["name"], "Alice");
        assert_eq!(result["age"], 30);
    }

    #[test]
    fn test_parse_json_array() {
        let result = parse_with_format("[1, 2, 3]", Format::Json).unwrap();
        assert_eq!(result[0], 1);
        assert_eq!(result[1], 2);
        assert_eq!(result[2], 3);
    }

    #[test]
    fn test_parse_yaml() {
        let result = parse_with_format("name: Alice\nage: 30", Format::Yaml).unwrap();
        assert_eq!(result["name"], "Alice");
        assert_eq!(result["age"], 30);
    }

    #[test]
    fn test_parse_yaml_nested() {
        let result = parse_with_format(
            "user:\n  name: Alice\n  profile:\n    score: 100",
            Format::Yaml,
        )
        .unwrap();
        assert_eq!(result["user"]["name"], "Alice");
        assert_eq!(result["user"]["profile"]["score"], 100);
    }

    #[test]
    fn test_parse_toml() {
        let result = parse_with_format("name = \"Alice\"\nage = 30", Format::Toml).unwrap();
        assert_eq!(result["name"], "Alice");
        assert_eq!(result["age"], 30);
    }

    #[test]
    fn test_parse_toml_section() {
        let result = parse_with_format(
            "[package]\nname = \"my-app\"\nversion = \"1.0.0\"",
            Format::Toml,
        )
        .unwrap();
        assert_eq!(result["package"]["name"], "my-app");
        assert_eq!(result["package"]["version"], "1.0.0");
    }

    #[test]
    fn test_parse_auto_json() {
        let result = parse_auto(r#"{"name": "Alice"}"#).unwrap();
        assert_eq!(result["name"], "Alice");
    }

    #[test]
    fn test_parse_auto_yaml() {
        let result = parse_auto("- item1\n- item2").unwrap();
        assert_eq!(result[0], "item1");
        assert_eq!(result[1], "item2");
    }

    // =========================================================================
    // Encoding tests
    // =========================================================================

    #[test]
    fn test_encode_json() {
        let obj = serde_json::json!({"name": "Alice", "age": 30});
        let result = encode_to_format(&obj, Format::Json).unwrap();
        assert!(result.contains("\"name\""));
        assert!(result.contains("Alice"));
    }

    #[test]
    fn test_encode_json_pretty() {
        let obj = serde_json::json!({"name": "Alice"});
        let result = encode_json_pretty(&obj).unwrap();
        assert!(result.contains('\n'));
        assert!(result.contains("  ")); // indentation
    }

    #[test]
    fn test_encode_yaml() {
        let obj = serde_json::json!({"name": "Bob", "age": 25});
        let result = encode_to_format(&obj, Format::Yaml).unwrap();
        assert!(result.contains("name"));
        assert!(result.contains("Bob"));
    }

    #[test]
    fn test_encode_toml() {
        let obj = serde_json::json!({"name": "Bob", "age": 25});
        let result = encode_to_format(&obj, Format::Toml).unwrap();
        assert!(result.contains("name = "));
        assert!(result.contains("\"Bob\""));
    }

    // =========================================================================
    // Query tests
    // =========================================================================

    #[test]
    fn test_query_field() {
        let obj = serde_json::json!({"name": "Alice", "age": 30});
        let result = query(&obj, ".name").unwrap();
        assert_eq!(result, "Alice");
    }

    #[test]
    fn test_query_nested() {
        let obj = serde_json::json!({"user": {"profile": {"score": 100}}});
        let result = query(&obj, ".user.profile.score").unwrap();
        assert_eq!(result, 100);
    }

    #[test]
    fn test_query_array_index() {
        let arr = serde_json::json!([10, 20, 30]);
        let result = query(&arr, ".[1]").unwrap();
        assert_eq!(result, 20);
    }

    #[test]
    fn test_query_iterate() {
        let users = serde_json::json!([
            {"name": "Alice", "age": 30},
            {"name": "Bob", "age": 25}
        ]);
        let result = query(&users, ".[] | .name").unwrap();
        assert_eq!(result, serde_json::json!(["Alice", "Bob"]));
    }

    // =========================================================================
    // Roundtrip tests
    // =========================================================================

    #[test]
    fn test_roundtrip_yaml() {
        let original = serde_json::json!({"name": "Test", "values": ["a", "b", "c"]});
        let yaml_str = encode_to_format(&original, Format::Yaml).unwrap();
        let parsed = parse_with_format(&yaml_str, Format::Yaml).unwrap();
        assert_eq!(parsed["name"], "Test");
        assert_eq!(parsed["values"][0], "a");
    }

    #[test]
    fn test_roundtrip_toml() {
        let original = serde_json::json!({"name": "Test", "version": "1.0.0"});
        let toml_str = encode_to_format(&original, Format::Toml).unwrap();
        let parsed = parse_with_format(&toml_str, Format::Toml).unwrap();
        assert_eq!(parsed["name"], "Test");
        assert_eq!(parsed["version"], "1.0.0");
    }

    // =========================================================================
    // Steel stubs test
    // =========================================================================

    #[test]
    fn test_steel_stubs_exist() {
        let stubs = OqModule::steel_stubs();
        assert!(stubs.contains("oq-parse"));
        assert!(stubs.contains("oq-json"));
        assert!(stubs.contains("oq-yaml"));
        assert!(stubs.contains("oq-toml"));
        assert!(stubs.contains("oq-query"));
    }
}
