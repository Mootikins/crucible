//! TOON-formatted tool responses
//!
//! Provides helpers for creating MCP tool responses in TOON format
//! instead of JSON, which is more token-efficient for LLM consumption.

use rmcp::model::{CallToolResult, ContentBlock};

/// Create a successful tool response with smart TOON formatting
///
/// Extracts long content fields (like file content) into readable blocks.
#[must_use]
pub fn toon_success_smart(value: &serde_json::Value) -> CallToolResult {
    let formatted = oq::format_tool_response_smart(value);
    CallToolResult::success(vec![ContentBlock::text(formatted)])
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_toon_success_smart_with_content() {
        let result = toon_success_smart(&json!({
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
        let result = toon_success_smart(&json!({
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
        let result = toon_success_smart(&json!({
            "message": "ok",
            "status": "success"
        }));
        assert!(!result.content.is_empty());
        let text = result.content[0].as_text().unwrap();
        assert!(text.text.contains("status: success"));
    }
}
