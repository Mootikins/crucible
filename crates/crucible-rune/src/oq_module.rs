//! Object Query module for Rune
//!
//! Provides universal querying for structured data (JSON, YAML, TOML, TOON).
//!
//! # Example
//!
//! ```rune
//! use oq::{query, parse, format};
//!
//! // Query any input (auto-detects format)
//! let recipes = oq::query(json_string, ".recipes")?;
//! let tags = oq::query(yaml_frontmatter, ".tags[]")?;
//!
//! // Parse string to value
//! let obj = oq::parse(string)?;              // Auto-detect
//! let obj = oq::parse_with_format(string, "yaml")?;
//!
//! // Format value to string
//! let toon = oq::format(value)?;             // Default: TOON
//! let json = oq::format_as(value, "json")?;
//! ```

use crate::mcp_types::{json_to_rune, rune_to_json};
use rune::runtime::VmResult;
use rune::{Any, ContextError, Module, Value};
use serde_json::Value as JsonValue;

/// Error type for oq operations (Rune-compatible)
#[derive(Debug, Clone, Any)]
#[rune(item = ::oq, name = OqError)]
pub struct RuneOqError {
    /// Error message
    #[rune(get)]
    pub message: String,
}

impl std::fmt::Display for RuneOqError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl RuneOqError {
    fn new(msg: impl Into<String>) -> Self {
        Self {
            message: msg.into(),
        }
    }
}

// =============================================================================
// Format Detection and Parsing
// =============================================================================

/// Supported input formats
#[derive(Debug, Clone, Copy, PartialEq)]
enum Format {
    Json,
    Yaml,
    Toml,
    Toon,
}

/// Detect format from content heuristics
fn detect_format(input: &str) -> Format {
    let trimmed = input.trim();

    if trimmed.is_empty() {
        return Format::Json;
    }

    let first_char = trimmed.chars().next().unwrap();

    match first_char {
        // JSON object
        '{' => Format::Json,
        // Could be JSON array or TOML section header
        '[' => {
            // TOML section header: [section] or [[section]] with no comma
            if let Some(line) = trimmed.lines().next() {
                let line = line.trim();
                if line.ends_with(']') && !line.contains(',') && !line.contains(':') {
                    return Format::Toml;
                }
            }
            // Otherwise JSON array
            Format::Json
        }
        // YAML frontmatter
        '-' if trimmed.starts_with("---") => Format::Yaml,
        _ => {
            // Check for JSON literals
            if trimmed == "true" || trimmed == "false" || trimmed == "null" {
                return Format::Json;
            }

            // Check if it looks like a number
            if trimmed.parse::<f64>().is_ok() {
                return Format::Json;
            }

            // Check for TOML patterns (key = value)
            if trimmed.lines().any(|line| {
                let line = line.trim();
                line.contains(" = ") || line.ends_with(']') && line.starts_with('[')
            }) {
                return Format::Toml;
            }

            // Check for YAML/TOON patterns (key: value)
            if trimmed.lines().any(|line| {
                let line = line.trim();
                line.contains(": ") || line.ends_with(':')
            }) {
                // Try to distinguish YAML from TOON
                // YAML typically has indentation-based structure
                // For now, default to YAML for key: value format
                return Format::Yaml;
            }

            // Default to trying JSON first
            if serde_json::from_str::<JsonValue>(trimmed).is_ok() {
                Format::Json
            } else {
                Format::Toon
            }
        }
    }
}

/// Parse string to JSON value based on format
fn parse_as_format(input: &str, format: Format) -> Result<JsonValue, String> {
    match format {
        Format::Json => serde_json::from_str(input).map_err(|e| format!("JSON parse error: {}", e)),
        Format::Yaml => {
            // Strip YAML frontmatter delimiters if present
            let content = if input.trim().starts_with("---") {
                input
                    .trim()
                    .strip_prefix("---")
                    .unwrap_or(input)
                    .trim()
                    .strip_suffix("---")
                    .unwrap_or(input)
                    .trim()
            } else {
                input
            };
            serde_yaml::from_str(content).map_err(|e| format!("YAML parse error: {}", e))
        }
        Format::Toml => toml::from_str::<toml::Value>(input)
            .map(toml_to_json)
            .map_err(|e| format!("TOML parse error: {}", e)),
        Format::Toon => {
            toon_format::decode_default(input).map_err(|e| format!("TOON parse error: {}", e))
        }
    }
}

/// Convert TOML value to JSON value
fn toml_to_json(toml: toml::Value) -> JsonValue {
    match toml {
        toml::Value::String(s) => JsonValue::String(s),
        toml::Value::Integer(i) => JsonValue::Number(i.into()),
        toml::Value::Float(f) => serde_json::Number::from_f64(f)
            .map(JsonValue::Number)
            .unwrap_or(JsonValue::Null),
        toml::Value::Boolean(b) => JsonValue::Bool(b),
        toml::Value::Datetime(dt) => JsonValue::String(dt.to_string()),
        toml::Value::Array(arr) => JsonValue::Array(arr.into_iter().map(toml_to_json).collect()),
        toml::Value::Table(table) => {
            let mut map = serde_json::Map::new();
            for (k, v) in table {
                map.insert(k, toml_to_json(v));
            }
            JsonValue::Object(map)
        }
    }
}

/// Convert JSON value to format string
fn format_as(value: &JsonValue, format: Format) -> Result<String, String> {
    match format {
        Format::Json => {
            serde_json::to_string_pretty(value).map_err(|e| format!("JSON format error: {}", e))
        }
        Format::Yaml => {
            serde_yaml::to_string(value).map_err(|e| format!("YAML format error: {}", e))
        }
        Format::Toml => {
            // TOML requires a table at the root
            if let JsonValue::Object(_map) = value {
                let toml_value = json_to_toml(value.clone());
                toml::to_string_pretty(&toml_value).map_err(|e| format!("TOML format error: {}", e))
            } else {
                Err("TOML requires an object at the root".to_string())
            }
        }
        Format::Toon => {
            toon_format::encode_default(value).map_err(|e| format!("TOON format error: {}", e))
        }
    }
}

/// Convert JSON value to TOML value
fn json_to_toml(json: JsonValue) -> toml::Value {
    match json {
        JsonValue::Null => toml::Value::String("null".to_string()),
        JsonValue::Bool(b) => toml::Value::Boolean(b),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                toml::Value::Integer(i)
            } else if let Some(f) = n.as_f64() {
                toml::Value::Float(f)
            } else {
                toml::Value::String(n.to_string())
            }
        }
        JsonValue::String(s) => toml::Value::String(s),
        JsonValue::Array(arr) => toml::Value::Array(arr.into_iter().map(json_to_toml).collect()),
        JsonValue::Object(map) => {
            let mut table = toml::map::Map::new();
            for (k, v) in map {
                table.insert(k, json_to_toml(v));
            }
            toml::Value::Table(table)
        }
    }
}

/// Parse format string to Format enum
fn parse_format_str(format: &str) -> Result<Format, String> {
    match format.to_lowercase().as_str() {
        "json" => Ok(Format::Json),
        "yaml" | "yml" => Ok(Format::Yaml),
        "toml" => Ok(Format::Toml),
        "toon" => Ok(Format::Toon),
        _ => Err(format!(
            "Unknown format '{}'. Supported: json, yaml, toml, toon",
            format
        )),
    }
}

// =============================================================================
// Rune Functions
// =============================================================================

/// Query structured data using jq-like expressions
///
/// Auto-detects format (JSON, YAML, TOML, TOON) and runs the query.
///
/// # Example
/// ```rune
/// let result = oq::query(json_string, ".name")?;
/// let items = oq::query(yaml_string, ".items[]")?;
/// ```
#[rune::function]
fn query(input: String, expr: String) -> Result<Value, RuneOqError> {
    // Detect and parse input format
    let format = detect_format(&input);
    let json = parse_as_format(&input, format).map_err(RuneOqError::new)?;

    // Compile and run jq filter
    let filter = tq::compile_filter(&expr).map_err(|e| RuneOqError::new(format!("{}", e)))?;

    let results = tq::run_filter(&filter, json).map_err(|e| RuneOqError::new(format!("{}", e)))?;

    // Return first result, or null if empty
    let result = if results.is_empty() {
        JsonValue::Null
    } else if results.len() == 1 {
        results.into_iter().next().unwrap()
    } else {
        // Multiple results become an array
        JsonValue::Array(results)
    };

    // Convert to Rune value
    match json_to_rune(&result) {
        VmResult::Ok(v) => Ok(v),
        VmResult::Err(e) => Err(RuneOqError::new(format!("Conversion error: {:?}", e))),
    }
}

/// Parse a string to a Rune value
///
/// Auto-detects format if not specified.
///
/// # Example
/// ```rune
/// let obj = oq::parse("{\"name\": \"Ada\"}")?;
/// ```
#[rune::function]
fn parse(input: String) -> Result<Value, RuneOqError> {
    let format = detect_format(&input);
    let json = parse_as_format(&input, format).map_err(RuneOqError::new)?;

    match json_to_rune(&json) {
        VmResult::Ok(v) => Ok(v),
        VmResult::Err(e) => Err(RuneOqError::new(format!("Conversion error: {:?}", e))),
    }
}

/// Parse a string with explicit format
#[rune::function]
fn parse_with_format(input: String, format: String) -> Result<Value, RuneOqError> {
    let fmt = parse_format_str(&format).map_err(RuneOqError::new)?;
    let json = parse_as_format(&input, fmt).map_err(RuneOqError::new)?;

    match json_to_rune(&json) {
        VmResult::Ok(v) => Ok(v),
        VmResult::Err(e) => Err(RuneOqError::new(format!("Conversion error: {:?}", e))),
    }
}

/// Format a value to a string
///
/// Defaults to TOON format.
///
/// # Example
/// ```rune
/// let toon = oq::format(value)?;
/// ```
#[rune::function]
fn format(value: Value) -> Result<String, RuneOqError> {
    let json = rune_to_json(&value).map_err(|e| RuneOqError::new(format!("{:?}", e)))?;
    format_as(&json, Format::Toon).map_err(RuneOqError::new)
}

/// Format a value with explicit format
#[rune::function]
fn format_to(value: Value, format: String) -> Result<String, RuneOqError> {
    let fmt = parse_format_str(&format).map_err(RuneOqError::new)?;
    let json = rune_to_json(&value).map_err(|e| RuneOqError::new(format!("{:?}", e)))?;
    format_as(&json, fmt).map_err(RuneOqError::new)
}

/// Create the oq module for Rune
pub fn oq_module() -> Result<Module, ContextError> {
    let mut module = Module::with_crate("oq")?;

    // Register the error type
    module.ty::<RuneOqError>()?;

    // Register functions
    module.function_meta(query)?;
    module.function_meta(parse)?;
    module.function_meta(parse_with_format)?;
    module.function_meta(format)?;
    module.function_meta(format_to)?;

    Ok(module)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_oq_module_creation() {
        let module = oq_module();
        assert!(module.is_ok(), "Should create oq module");
    }

    #[test]
    fn test_detect_format_json() {
        assert_eq!(detect_format(r#"{"name": "Ada"}"#), Format::Json);
        assert_eq!(detect_format(r#"[1, 2, 3]"#), Format::Json);
        assert_eq!(detect_format("true"), Format::Json);
        assert_eq!(detect_format("42"), Format::Json);
    }

    #[test]
    fn test_detect_format_yaml() {
        assert_eq!(detect_format("---\ntitle: Test\n---"), Format::Yaml);
        assert_eq!(detect_format("name: Ada\nage: 30"), Format::Yaml);
    }

    #[test]
    fn test_detect_format_toml() {
        assert_eq!(detect_format("[package]\nname = \"test\""), Format::Toml);
    }

    #[test]
    fn test_parse_json() {
        let result = parse_as_format(r#"{"name": "Ada"}"#, Format::Json);
        assert!(result.is_ok());
        let json = result.unwrap();
        assert_eq!(json["name"], "Ada");
    }

    #[test]
    fn test_parse_yaml() {
        let result = parse_as_format("name: Ada\nage: 30", Format::Yaml);
        assert!(result.is_ok());
        let json = result.unwrap();
        assert_eq!(json["name"], "Ada");
        assert_eq!(json["age"], 30);
    }

    #[test]
    fn test_parse_yaml_frontmatter() {
        let result = parse_as_format("---\ntitle: My Note\ntags:\n  - rust\n---", Format::Yaml);
        assert!(result.is_ok());
        let json = result.unwrap();
        assert_eq!(json["title"], "My Note");
    }

    /// Test that query can be called from Rune script
    #[test]
    fn test_query_json_from_rune() {
        use rune::termcolor::{ColorChoice, StandardStream};
        use rune::{Context, Diagnostics, Source, Sources, Vm};
        use std::sync::Arc;

        // Create context with oq module
        let mut context = Context::with_default_modules().unwrap();
        context.install(oq_module().unwrap()).unwrap();
        let runtime = Arc::new(context.runtime().unwrap());

        // Rune script that calls oq::query on JSON
        let script = r#"
            use oq::query;

            pub fn main() {
                let json = "{\"name\": \"Ada\", \"age\": 30}";
                let result = query(json, ".name")?;
                result
            }
        "#;

        // Compile
        let mut sources = Sources::new();
        sources
            .insert(Source::new("test", script).unwrap())
            .unwrap();

        let mut diagnostics = Diagnostics::new();
        let result = rune::prepare(&mut sources)
            .with_context(&context)
            .with_diagnostics(&mut diagnostics)
            .build();

        if !diagnostics.is_empty() {
            let mut writer = StandardStream::stderr(ColorChoice::Always);
            diagnostics.emit(&mut writer, &sources).unwrap();
        }

        let unit = result.expect("Should compile script with oq::query");
        let unit = Arc::new(unit);

        // Execute
        let mut vm = Vm::new(runtime, unit);
        let output = vm.call(rune::Hash::type_hash(["main"]), ()).unwrap();
        let output: String = rune::from_value(output).unwrap();

        assert_eq!(output, "Ada", "Should extract name from JSON");
    }

    /// Test query on YAML-like input
    #[test]
    fn test_query_yaml_from_rune() {
        use rune::termcolor::{ColorChoice, StandardStream};
        use rune::{Context, Diagnostics, Source, Sources, Vm};
        use std::sync::Arc;

        let mut context = Context::with_default_modules().unwrap();
        context.install(oq_module().unwrap()).unwrap();
        let runtime = Arc::new(context.runtime().unwrap());

        // YAML frontmatter style
        let script = r#"
            use oq::query;

            pub fn main() {
                let yaml = "---
title: My Note
tags:
  - rust
  - programming
---";
                let result = query(yaml, ".title")?;
                result
            }
        "#;

        let mut sources = Sources::new();
        sources
            .insert(Source::new("test", script).unwrap())
            .unwrap();

        let mut diagnostics = Diagnostics::new();
        let result = rune::prepare(&mut sources)
            .with_context(&context)
            .with_diagnostics(&mut diagnostics)
            .build();

        if !diagnostics.is_empty() {
            let mut writer = StandardStream::stderr(ColorChoice::Always);
            diagnostics.emit(&mut writer, &sources).unwrap();
        }

        let unit = result.expect("Should compile");
        let unit = Arc::new(unit);

        let mut vm = Vm::new(runtime, unit);
        let output = vm.call(rune::Hash::type_hash(["main"]), ()).unwrap();
        let output: String = rune::from_value(output).unwrap();

        assert_eq!(output, "My Note", "Should extract title from YAML");
    }

    /// Test parse with auto-detection
    #[test]
    fn test_parse_json_from_rune() {
        use rune::termcolor::{ColorChoice, StandardStream};
        use rune::{Context, Diagnostics, Source, Sources, Vm};
        use std::sync::Arc;

        let mut context = Context::with_default_modules().unwrap();
        context.install(oq_module().unwrap()).unwrap();
        let runtime = Arc::new(context.runtime().unwrap());

        let script = r#"
            use oq::parse;

            pub fn main() {
                let json = "{\"name\": \"Ada\"}";
                let obj = parse(json)?;
                obj["name"]
            }
        "#;

        let mut sources = Sources::new();
        sources
            .insert(Source::new("test", script).unwrap())
            .unwrap();

        let mut diagnostics = Diagnostics::new();
        let result = rune::prepare(&mut sources)
            .with_context(&context)
            .with_diagnostics(&mut diagnostics)
            .build();

        if !diagnostics.is_empty() {
            let mut writer = StandardStream::stderr(ColorChoice::Always);
            diagnostics.emit(&mut writer, &sources).unwrap();
        }

        let unit = result.expect("Should compile");
        let unit = Arc::new(unit);

        let mut vm = Vm::new(runtime, unit);
        let output = vm.call(rune::Hash::type_hash(["main"]), ()).unwrap();
        let output: String = rune::from_value(output).unwrap();

        assert_eq!(output, "Ada", "Should parse JSON and access field");
    }

    /// Test format to TOON (default)
    #[test]
    fn test_format_to_toon_from_rune() {
        use rune::termcolor::{ColorChoice, StandardStream};
        use rune::{Context, Diagnostics, Source, Sources, Vm};
        use std::sync::Arc;

        let mut context = Context::with_default_modules().unwrap();
        context.install(oq_module().unwrap()).unwrap();
        let runtime = Arc::new(context.runtime().unwrap());

        let script = r#"
            use oq::format;

            pub fn main() {
                let obj = #{ name: "Ada", age: 30 };
                let toon = format(obj)?;
                toon
            }
        "#;

        let mut sources = Sources::new();
        sources
            .insert(Source::new("test", script).unwrap())
            .unwrap();

        let mut diagnostics = Diagnostics::new();
        let result = rune::prepare(&mut sources)
            .with_context(&context)
            .with_diagnostics(&mut diagnostics)
            .build();

        if !diagnostics.is_empty() {
            let mut writer = StandardStream::stderr(ColorChoice::Always);
            diagnostics.emit(&mut writer, &sources).unwrap();
        }

        let unit = result.expect("Should compile");
        let unit = Arc::new(unit);

        let mut vm = Vm::new(runtime, unit);
        let output = vm.call(rune::Hash::type_hash(["main"]), ()).unwrap();
        let output: String = rune::from_value(output).unwrap();

        // TOON format should contain key-value pairs
        assert!(output.contains("name:"), "Should contain name field");
        assert!(output.contains("Ada"), "Should contain Ada value");
    }

    /// Test format to JSON
    #[test]
    fn test_format_to_json_from_rune() {
        use rune::termcolor::{ColorChoice, StandardStream};
        use rune::{Context, Diagnostics, Source, Sources, Vm};
        use std::sync::Arc;

        let mut context = Context::with_default_modules().unwrap();
        context.install(oq_module().unwrap()).unwrap();
        let runtime = Arc::new(context.runtime().unwrap());

        let script = r#"
            use oq::format_to;

            pub fn main() {
                let obj = #{ name: "Ada" };
                let json = format_to(obj, "json")?;
                json
            }
        "#;

        let mut sources = Sources::new();
        sources
            .insert(Source::new("test", script).unwrap())
            .unwrap();

        let mut diagnostics = Diagnostics::new();
        let result = rune::prepare(&mut sources)
            .with_context(&context)
            .with_diagnostics(&mut diagnostics)
            .build();

        if !diagnostics.is_empty() {
            let mut writer = StandardStream::stderr(ColorChoice::Always);
            diagnostics.emit(&mut writer, &sources).unwrap();
        }

        let unit = result.expect("Should compile");
        let unit = Arc::new(unit);

        let mut vm = Vm::new(runtime, unit);
        let output = vm.call(rune::Hash::type_hash(["main"]), ()).unwrap();
        let output: String = rune::from_value(output).unwrap();

        // Should be valid JSON
        assert!(output.contains("\"name\""), "Should be JSON format");
        assert!(output.contains("\"Ada\""), "Should contain value");
    }
}
