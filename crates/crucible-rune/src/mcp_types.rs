//! MCP types for Rune integration
//!
//! Provides typed wrappers for MCP tool results and value conversion utilities.

use crucible_core::traits::{ContentBlock, ToolCallResult};
use rune::alloc::fmt::TryWrite;
use rune::runtime::{Formatter, ToValue, VmError, VmResult};
use rune::vm_try;
use rune::{Any, ContextError, Module, Value};
use serde_json::Value as JsonValue;

// =============================================================================
// McpResult - Wrapper for MCP tool call results
// =============================================================================

/// Result wrapper for MCP tool calls in Rune
///
/// Provides convenience methods for extracting content in various formats.
///
/// # Example (Rune)
/// ```rune
/// let result = cru::mcp::github::search_repositories("rust", 1, 30).await?;
///
/// // Get as text
/// let text = result.text()?;
///
/// // Parse as JSON
/// let data = result.json()?;
///
/// // Check for errors
/// if result.is_error() {
///     println("Tool failed!");
/// }
/// ```
#[derive(Debug, Clone, Any)]
#[rune(item = ::cru::mcp)]
pub struct McpResult {
    content: Vec<ContentBlock>,
    is_error: bool,
}

impl McpResult {
    /// Create from MCP ToolCallResult
    pub fn from_tool_result(result: ToolCallResult) -> Self {
        Self {
            content: result.content,
            is_error: result.is_error,
        }
    }

    /// Get first text content as string (internal impl)
    fn text_impl(&self) -> Option<String> {
        self.content.iter().find_map(|c| match c {
            ContentBlock::Text { text } => Some(text.clone()),
            _ => None,
        })
    }

    /// Get first text content as string
    #[rune::function(instance)]
    pub fn text(&self) -> Option<String> {
        self.text_impl()
    }

    /// Try to parse first text content as JSON, return as Rune Value
    #[rune::function(instance)]
    pub fn json(&self) -> VmResult<Value> {
        let text = match self.text_impl() {
            Some(t) => t,
            None => return VmResult::err(VmError::panic("No text content available")),
        };

        let parsed: JsonValue = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(e) => return VmResult::err(VmError::panic(format!("JSON parse error: {}", e))),
        };

        json_to_rune(&parsed)
    }

    /// Check if the tool returned an error
    #[rune::function(instance)]
    pub fn is_error(&self) -> bool {
        self.is_error
    }

    /// Get all text content blocks joined with newlines
    #[rune::function(instance)]
    pub fn all_text(&self) -> String {
        self.content
            .iter()
            .filter_map(|c| match c {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Get the number of content blocks
    #[rune::function(instance)]
    pub fn content_count(&self) -> usize {
        self.content.len()
    }

    /// Debug display
    #[rune::function(instance, protocol = DISPLAY_FMT)]
    fn string_display(&self, f: &mut Formatter) -> VmResult<()> {
        vm_try!(write!(
            f,
            "McpResult {{ is_error: {}, content_blocks: {} }}",
            self.is_error,
            self.content.len()
        ));
        VmResult::Ok(())
    }
}

impl From<ToolCallResult> for McpResult {
    fn from(result: ToolCallResult) -> Self {
        Self::from_tool_result(result)
    }
}

// =============================================================================
// Value Conversion: JSON <-> Rune
// =============================================================================

/// Convert a serde_json Value to a Rune Value
///
/// Handles: null, bool, numbers, strings, arrays, objects
pub fn json_to_rune(json: &JsonValue) -> VmResult<Value> {
    match json {
        JsonValue::Null => VmResult::Ok(Value::empty()),
        JsonValue::Bool(b) => match b.to_value() {
            Ok(v) => VmResult::Ok(v),
            Err(e) => VmResult::err(VmError::panic(format!("Bool conversion error: {}", e))),
        },
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                match i.to_value() {
                    Ok(v) => VmResult::Ok(v),
                    Err(e) => {
                        VmResult::err(VmError::panic(format!("Integer conversion error: {}", e)))
                    }
                }
            } else if let Some(f) = n.as_f64() {
                match f.to_value() {
                    Ok(v) => VmResult::Ok(v),
                    Err(e) => {
                        VmResult::err(VmError::panic(format!("Float conversion error: {}", e)))
                    }
                }
            } else {
                VmResult::err(VmError::panic(format!("Unsupported number: {}", n)))
            }
        }
        JsonValue::String(s) => match s.clone().to_value() {
            Ok(v) => VmResult::Ok(v),
            Err(e) => VmResult::err(VmError::panic(format!("String conversion error: {}", e))),
        },
        JsonValue::Array(arr) => {
            let mut values: Vec<Value> = Vec::new();
            for item in arr {
                let value = vm_try!(json_to_rune(item));
                values.push(value);
            }
            match values.to_value() {
                Ok(v) => VmResult::Ok(v),
                Err(e) => VmResult::err(VmError::panic(format!("Vec conversion error: {}", e))),
            }
        }
        JsonValue::Object(obj) => {
            let mut map: std::collections::HashMap<String, Value> =
                std::collections::HashMap::new();
            for (key, value) in obj {
                let rune_value = vm_try!(json_to_rune(value));
                map.insert(key.clone(), rune_value);
            }
            match map.to_value() {
                Ok(v) => VmResult::Ok(v),
                Err(e) => VmResult::err(VmError::panic(format!("Object conversion error: {}", e))),
            }
        }
    }
}

/// Convert a Rune Value to serde_json Value
///
/// Handles: unit, bool, integers, floats, strings, vecs, objects
pub fn rune_to_json(value: &Value) -> Result<JsonValue, VmError> {
    // Use type info to determine conversion
    let type_info = value.type_info();
    let type_name = format!("{}", type_info);

    if type_name.contains("Unit") || type_name.contains("()") || type_name == "unit" {
        return Ok(JsonValue::Null);
    }

    // Try common conversions using rune::from_value
    if type_name.contains("bool") {
        let b: bool = rune::from_value(value.clone())
            .map_err(|e| VmError::panic(format!("Bool conversion error: {}", e)))?;
        return Ok(JsonValue::Bool(b));
    }

    if type_name.contains("i64") {
        let i: i64 = rune::from_value(value.clone())
            .map_err(|e| VmError::panic(format!("Integer conversion error: {}", e)))?;
        return Ok(JsonValue::Number(i.into()));
    }

    if type_name.contains("f64") {
        let f: f64 = rune::from_value(value.clone())
            .map_err(|e| VmError::panic(format!("Float conversion error: {}", e)))?;
        return serde_json::Number::from_f64(f)
            .map(JsonValue::Number)
            .ok_or_else(|| VmError::panic("Invalid float value"));
    }

    if type_name.contains("String") {
        let s: String = rune::from_value(value.clone())
            .map_err(|e| VmError::panic(format!("String conversion error: {}", e)))?;
        return Ok(JsonValue::String(s));
    }

    if type_name.contains("Vec") {
        let vec: Vec<Value> = rune::from_value(value.clone())
            .map_err(|e| VmError::panic(format!("Vec conversion error: {}", e)))?;
        let mut arr = Vec::new();
        for item in vec {
            arr.push(rune_to_json(&item)?);
        }
        return Ok(JsonValue::Array(arr));
    }

    if type_name.contains("Object") || type_name.contains("HashMap") {
        let map: std::collections::HashMap<String, Value> = rune::from_value(value.clone())
            .map_err(|e| VmError::panic(format!("Object conversion error: {}", e)))?;
        let mut obj = serde_json::Map::new();
        for (key, val) in map {
            obj.insert(key, rune_to_json(&val)?);
        }
        return Ok(JsonValue::Object(obj));
    }

    Err(VmError::panic(format!(
        "Cannot convert Rune type '{}' to JSON",
        type_name
    )))
}

/// Build a JSON object from parameter names and Rune values
///
/// Used by generated MCP tool functions to convert positional args to JSON.
pub fn build_args_json(param_names: &[String], values: Vec<Value>) -> Result<JsonValue, VmError> {
    let mut obj = serde_json::Map::new();
    for (name, value) in param_names.iter().zip(values.iter()) {
        obj.insert(name.clone(), rune_to_json(value)?);
    }
    Ok(JsonValue::Object(obj))
}

// =============================================================================
// Rune Module Registration
// =============================================================================

/// Create the cru::mcp base module with McpResult type
pub fn mcp_types_module() -> Result<Module, ContextError> {
    let mut module = Module::with_crate_item("cru", ["mcp"])?;

    // Register McpResult type
    module.ty::<McpResult>()?;
    module.function_meta(McpResult::text)?;
    module.function_meta(McpResult::json)?;
    module.function_meta(McpResult::is_error)?;
    module.function_meta(McpResult::all_text)?;
    module.function_meta(McpResult::content_count)?;
    module.function_meta(McpResult::string_display)?;

    Ok(module)
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_result_from_tool_result() {
        let tool_result = ToolCallResult {
            content: vec![ContentBlock::Text {
                text: "hello world".to_string(),
            }],
            is_error: false,
        };

        let mcp_result = McpResult::from(tool_result);
        assert!(!mcp_result.is_error);
        assert_eq!(mcp_result.text_impl(), Some("hello world".to_string()));
        assert_eq!(mcp_result.content.len(), 1);
    }

    #[test]
    fn test_mcp_result_error() {
        let tool_result = ToolCallResult {
            content: vec![ContentBlock::Text {
                text: "error message".to_string(),
            }],
            is_error: true,
        };

        let mcp_result = McpResult::from(tool_result);
        assert!(mcp_result.is_error);
    }

    #[test]
    fn test_mcp_result_all_text() {
        let tool_result = ToolCallResult {
            content: vec![
                ContentBlock::Text {
                    text: "line 1".to_string(),
                },
                ContentBlock::Text {
                    text: "line 2".to_string(),
                },
            ],
            is_error: false,
        };

        let mcp_result = McpResult::from(tool_result);
        assert_eq!(
            mcp_result
                .content
                .iter()
                .filter_map(|c| match c {
                    ContentBlock::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n"),
            "line 1\nline 2"
        );
    }

    #[test]
    fn test_json_to_rune_primitives() {
        // These tests need a Rune VM context to fully validate
        // For now, just test that they don't panic

        let _ = json_to_rune(&JsonValue::Null);
        let _ = json_to_rune(&JsonValue::Bool(true));
        let _ = json_to_rune(&JsonValue::Number(42.into()));
        let _ = json_to_rune(&JsonValue::String("test".to_string()));
    }

    #[test]
    fn test_build_args_json() {
        // This would need Rune values to test properly
        let params = vec!["query".to_string(), "page".to_string()];
        let values: Vec<Value> = vec![]; // Empty for now

        // Just verify it compiles and handles empty case
        let result = build_args_json(&params, values);
        assert!(result.is_ok());
    }

    #[test]
    fn test_mcp_types_module_creation() {
        let module = mcp_types_module();
        assert!(module.is_ok(), "Should create mcp types module");
    }
}
