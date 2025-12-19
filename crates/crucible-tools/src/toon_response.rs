//! TOON-formatted tool responses
//!
//! Provides helpers for creating MCP tool responses in TOON format
//! instead of JSON, which is more token-efficient for LLM consumption.
//!
//! # Usage
//!
//! ```rust,ignore
//! use crucible_tools::toon_response::toon_success;
//!
//! // Instead of:
//! // Ok(CallToolResult::success(vec![rmcp::model::Content::json(json!({...}))?]))
//!
//! // Use:
//! Ok(toon_success(json!({
//!     "path": path,
//!     "content": content,
//!     "status": "success"
//! })))
//! ```

use rmcp::model::{CallToolResult, Content};

/// Create a successful tool response with TOON-formatted content
///
/// Converts the JSON value to TOON format for token efficiency.
/// Falls back to JSON if TOON encoding fails.
#[must_use]
pub fn toon_success(value: serde_json::Value) -> CallToolResult {
    let formatted = tq::format_tool_response(&value);
    CallToolResult::success(vec![Content::text(formatted)])
}

/// Create a successful tool response with smart TOON formatting
///
/// Extracts long content fields (like file content) into readable blocks.
#[must_use]
pub fn toon_success_smart(value: serde_json::Value) -> CallToolResult {
    let formatted = tq::format_tool_response_smart(&value);
    CallToolResult::success(vec![Content::text(formatted)])
}

/// Create a successful tool response with tool-type-aware formatting
///
/// Uses the tool name to determine optimal formatting strategy.
#[must_use]
pub fn toon_success_for_tool(tool_name: &str, value: serde_json::Value) -> CallToolResult {
    let tool_type = tq::ToolType::from_name(tool_name);
    let formatted = tq::format_tool_response_with(&value, tool_type);
    CallToolResult::success(vec![Content::text(formatted)])
}

/// Convert an existing `Content::json` response to TOON format
///
/// Useful for migrating existing tool implementations.
#[must_use]
pub fn content_to_toon(content: &Content) -> Content {
    if let Some(text_content) = content.as_text() {
        // Try to parse as JSON and convert to TOON
        let formatted = tq::format_content(&text_content.text);
        Content::text(formatted)
    } else {
        // Return as-is if not text content
        content.clone()
    }
}

/// Helper macro for creating TOON-formatted success responses
///
/// # Example
///
/// ```rust,ignore
/// use crucible_tools::toon_ok;
///
/// toon_ok!({
///     "path": path,
///     "status": "created"
/// })
/// ```
#[macro_export]
macro_rules! toon_ok {
    ($($json:tt)+) => {
        Ok($crate::toon_response::toon_success(serde_json::json!($($json)+)))
    };
}

/// Helper macro for smart TOON-formatted success responses
#[macro_export]
macro_rules! toon_ok_smart {
    ($($json:tt)+) => {
        Ok($crate::toon_response::toon_success_smart(serde_json::json!($($json)+)))
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ==================== Basic Formatting Tests ====================

    #[test]
    fn test_toon_success_simple() {
        let result = toon_success(json!({"name": "Ada", "age": 30}));
        assert!(!result.content.is_empty());
        let text = result.content[0].as_text().unwrap();
        assert!(text.text.contains("name: Ada"));
        assert!(text.text.contains("age: 30"));
    }

    #[test]
    fn test_toon_success_with_nested_object() {
        let result = toon_success(json!({
            "user": {
                "name": "Ada",
                "email": "ada@example.com"
            },
            "status": "active"
        }));
        assert!(!result.content.is_empty());
        let text = result.content[0].as_text().unwrap();
        assert!(text.text.contains("status: active"));
        assert!(text.text.contains("name: Ada"));
    }

    #[test]
    fn test_toon_success_with_array() {
        let result = toon_success(json!({
            "tags": ["rust", "mcp", "toon"],
            "count": 3
        }));
        assert!(!result.content.is_empty());
        let text = result.content[0].as_text().unwrap();
        assert!(text.text.contains("count: 3"));
        // Array should be represented
        assert!(text.text.contains("rust"));
    }

    #[test]
    fn test_toon_success_with_special_chars() {
        let result = toon_success(json!({
            "message": "Hello, world!",
            "path": "/home/user/test.md"
        }));
        assert!(!result.content.is_empty());
        let text = result.content[0].as_text().unwrap();
        // TOON quotes strings with special characters
        assert!(text.text.contains("Hello"));
        assert!(text.text.contains("path:"));
    }

    // ==================== Smart Formatting Tests ====================

    #[test]
    fn test_toon_success_smart_with_content() {
        let result = toon_success_smart(json!({
            "path": "test.rs",
            "content": "fn main() {\n    println!(\"Hello\");\n}",
            "lines": 3
        }));
        assert!(!result.content.is_empty());
        let text = result.content[0].as_text().unwrap();
        assert!(text.text.contains("path: test.rs"));
    }

    #[test]
    fn test_toon_success_smart_with_long_content() {
        // Content longer than threshold should be extracted
        let long_content = "x".repeat(300);
        let result = toon_success_smart(json!({
            "path": "big.txt",
            "content": long_content,
            "size": 300
        }));
        assert!(!result.content.is_empty());
        let text = result.content[0].as_text().unwrap();
        assert!(text.text.contains("path: big.txt"));
        assert!(text.text.contains("size: 300"));
    }

    #[test]
    fn test_toon_success_smart_preserves_short_content() {
        let result = toon_success_smart(json!({
            "message": "ok",
            "status": "success"
        }));
        assert!(!result.content.is_empty());
        let text = result.content[0].as_text().unwrap();
        assert!(text.text.contains("status: success"));
    }

    // ==================== Tool-Type-Aware Formatting Tests ====================

    #[test]
    fn test_toon_success_for_read_tool() {
        let result = toon_success_for_tool(
            "read_note",
            json!({
                "path": "notes/test.md",
                "content": "# Test\nSome content here",
                "modified": "2024-01-01"
            }),
        );
        assert!(!result.content.is_empty());
        let text = result.content[0].as_text().unwrap();
        assert!(text.text.contains("path:"));
    }

    #[test]
    fn test_toon_success_for_search_tool() {
        let result = toon_success_for_tool(
            "semantic_search",
            json!({
                "query": "rust programming",
                "results": [
                    {"path": "a.md", "score": 0.95},
                    {"path": "b.md", "score": 0.87}
                ],
                "total": 2
            }),
        );
        assert!(!result.content.is_empty());
        let text = result.content[0].as_text().unwrap();
        assert!(text.text.contains("total: 2"));
    }

    #[test]
    fn test_toon_success_for_command_tool() {
        let result = toon_success_for_tool(
            "run_command",
            json!({
                "command": "ls -la",
                "stdout": "file1.txt\nfile2.txt",
                "exit_code": 0
            }),
        );
        assert!(!result.content.is_empty());
        let text = result.content[0].as_text().unwrap();
        assert!(text.text.contains("exit_code: 0"));
    }

    // ==================== Tool Type Detection Tests ====================

    #[test]
    fn test_tool_type_detection_read_variants() {
        use tq::ToolType;

        assert_eq!(ToolType::from_name("read_note"), ToolType::ReadFile);
        assert_eq!(ToolType::from_name("read_file"), ToolType::ReadFile);
        assert_eq!(ToolType::from_name("get_note_content"), ToolType::ReadFile);
        assert_eq!(ToolType::from_name("file_read"), ToolType::ReadFile);
    }

    #[test]
    fn test_tool_type_detection_search_variants() {
        use tq::ToolType;

        // search_notes should be Search, NOT ReadFile (even though it contains "note")
        assert_eq!(ToolType::from_name("search_notes"), ToolType::Search);
        assert_eq!(ToolType::from_name("semantic_search"), ToolType::Search);
        assert_eq!(ToolType::from_name("find_files"), ToolType::Search);
        assert_eq!(ToolType::from_name("grep_content"), ToolType::Search);
        assert_eq!(ToolType::from_name("text_search"), ToolType::Search);
    }

    #[test]
    fn test_tool_type_detection_command_variants() {
        use tq::ToolType;

        assert_eq!(ToolType::from_name("exec_command"), ToolType::Command);
        assert_eq!(ToolType::from_name("run_script"), ToolType::Command);
        assert_eq!(ToolType::from_name("shell_exec"), ToolType::Command);
        assert_eq!(ToolType::from_name("command_runner"), ToolType::Command);
    }

    #[test]
    fn test_tool_type_detection_generic_fallback() {
        use tq::ToolType;

        assert_eq!(ToolType::from_name("get_info"), ToolType::Generic);
        assert_eq!(ToolType::from_name("list_tags"), ToolType::Generic);
        assert_eq!(ToolType::from_name("create_note"), ToolType::Generic);
        assert_eq!(ToolType::from_name("delete_item"), ToolType::Generic);
    }

    #[test]
    fn test_tool_type_case_insensitive() {
        use tq::ToolType;

        assert_eq!(ToolType::from_name("READ_FILE"), ToolType::ReadFile);
        assert_eq!(ToolType::from_name("SEARCH_NOTES"), ToolType::Search);
        assert_eq!(ToolType::from_name("Exec_Command"), ToolType::Command);
    }

    // ==================== Content Conversion Tests ====================

    #[test]
    fn test_content_to_toon_from_json_string() {
        let json_str = r#"{"name": "test", "value": 42}"#;
        let content = Content::text(json_str);
        let converted = content_to_toon(&content);

        let text = converted.as_text().unwrap();
        assert!(text.text.contains("name: test"));
        assert!(text.text.contains("value: 42"));
    }

    #[test]
    fn test_content_to_toon_from_plain_text() {
        let plain = "This is not JSON";
        let content = Content::text(plain);
        let converted = content_to_toon(&content);

        let text = converted.as_text().unwrap();
        // Non-JSON should be preserved as a string value
        assert!(text.text.contains("This is not JSON"));
    }

    // ==================== Edge Cases ====================

    #[test]
    fn test_toon_success_empty_object() {
        let result = toon_success(json!({}));
        assert!(!result.content.is_empty());
    }

    #[test]
    fn test_toon_success_null_value() {
        let result = toon_success(json!(null));
        assert!(!result.content.is_empty());
    }

    #[test]
    fn test_toon_success_array_root() {
        let result = toon_success(json!([1, 2, 3]));
        assert!(!result.content.is_empty());
        let text = result.content[0].as_text().unwrap();
        assert!(text.text.contains('1'));
    }

    #[test]
    fn test_toon_success_unicode_content() {
        let result = toon_success(json!({
            "greeting": "こんにちは",
            "emoji": "🦀"
        }));
        assert!(!result.content.is_empty());
        let text = result.content[0].as_text().unwrap();
        assert!(text.text.contains("こんにちは") || text.text.contains("greeting"));
    }

    #[test]
    fn test_toon_success_deeply_nested() {
        let result = toon_success(json!({
            "level1": {
                "level2": {
                    "level3": {
                        "value": "deep"
                    }
                }
            }
        }));
        assert!(!result.content.is_empty());
        let text = result.content[0].as_text().unwrap();
        assert!(text.text.contains("deep"));
    }
}
