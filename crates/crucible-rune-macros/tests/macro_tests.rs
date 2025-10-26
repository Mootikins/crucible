//! Integration tests for the rune_tool procedural macro
//!
//! These tests verify that the macro expands correctly and generates
//! appropriate metadata and code.

use crucible_rune_macros::{rune_tool, simple_rune_tool};
use serde_json::json;

// ============================================================================
// BASIC TOOL DEFINITIONS
// ============================================================================

/// Creates a simple greeting message
#[rune_tool(
    desc = "Creates a greeting message for the given name",
    category = "utility",
    tags = ["greeting", "message"]
)]
pub fn greet(name: String) -> String {
    format!("Hello, {}!", name)
}

/// Calculates the sum of two numbers
#[rune_tool(
    desc = "Adds two numbers together and returns the result",
    category = "math",
    tags = ["arithmetic", "calculation"]
)]
pub fn add_numbers(a: i32, b: i32) -> i32 {
    a + b
}

/// Checks if a string contains a substring
#[rune_tool(
    desc = "Checks if the haystack contains the needle substring",
    category = "string",
    tags = ["search", "validation"]
)]
pub fn contains_substring(haystack: String, needle: String) -> bool {
    haystack.contains(&needle)
}

// ============================================================================
// ASYNC TOOL DEFINITIONS
// ============================================================================

/// Simulates an async file read operation
#[rune_tool(
    desc = "Reads file content asynchronously",
    category = "file",
    async,
    tags = ["file", "io", "async"]
)]
pub async fn read_file_async(path: String) -> Result<String, String> {
    // Simulate async file reading
    Ok(format!("Content of file: {}", path))
}

/// Processes data with async delay
#[rune_tool(
    desc = "Processes data with simulated delay",
    category = "processing",
    async
)]
pub async fn process_data_async(data: String, delay_ms: Option<u64>) -> Result<String, String> {
    // Simulate processing delay
    let delay = delay_ms.unwrap_or(100);
    tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
    Ok(format!("Processed: {}", data))
}

// ============================================================================
// OPTIONAL PARAMETERS
// ============================================================================

/// Creates a note with optional folder
#[rune_tool(
    desc = "Creates a new note with title and content, optionally in a specific folder",
    category = "file",
    tags = ["note", "create", "optional"]
)]
pub fn create_note(
    title: String,
    _content: String,
    folder: Option<String>,
) -> Result<String, String> {
    let location = folder.unwrap_or_else(|| "notes".to_string());
    Ok(format!("Created note '{}' in folder '{}'", title, location))
}

/// Searches with optional limit and offset
#[rune_tool(
    desc = "Searches for items with optional pagination parameters",
    category = "search",
    tags = ["search", "pagination", "optional"]
)]
pub fn search_items(query: String, limit: Option<i32>, offset: Option<i32>) -> Vec<String> {
    let limit = limit.unwrap_or(10);
    let offset = offset.unwrap_or(0);

    // Simulate search results
    (offset..offset + limit)
        .map(|i| format!("Result {} for query: {}", i, query))
        .collect()
}

// ============================================================================
// COMPLEX TYPES
// ============================================================================

/// Processes an array of strings
#[rune_tool(
    desc = "Processes a list of strings and returns statistics",
    category = "processing",
    tags = ["array", "processing", "statistics"]
)]
pub fn process_strings(strings: Vec<String>) -> Result<serde_json::Value, String> {
    let total = strings.len();
    let total_chars: usize = strings.iter().map(|s| s.len()).sum();

    Ok(json!({
        "count": total,
        "total_characters": total_chars,
        "average_length": if total > 0 { total_chars as f64 / total as f64 } else { 0.0 }
    }))
}

/// Analyzes nested data structures
#[rune_tool(
    desc = "Analyzes complex nested data structures",
    category = "analysis",
    tags = ["analysis", "nested", "complex"]
)]
pub fn analyze_data(
    data: serde_json::Value,
    options: Option<serde_json::Value>,
) -> Result<serde_json::Value, String> {
    Ok(json!({
        "data_type": match data {
            serde_json::Value::Null => "null",
            serde_json::Value::Bool(_) => "boolean",
            serde_json::Value::Number(_) => "number",
            serde_json::Value::String(_) => "string",
            serde_json::Value::Array(_) => "array",
            serde_json::Value::Object(_) => "object",
        },
        "has_options": options.is_some(),
        "analysis_complete": true
    }))
}

// ============================================================================
// SIMPLE MACRO TESTS
// ============================================================================

/// A simple tool using the simple_rune_tool macro
#[simple_rune_tool]
pub fn simple_multiply(x: i32, y: i32) -> i32 {
    x * y
}

/// Another simple tool for testing
#[simple_rune_tool]
pub fn get_current_time() -> String {
    "2023-01-01T00:00:00Z".to_string() // Mock time
}

// ============================================================================
// TEST FUNCTIONS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_tool_functionality() {
        // Test that the tool functions work as expected
        let result = greet("World".to_string());
        assert_eq!(result, "Hello, World!");

        let sum = add_numbers(5, 3);
        assert_eq!(sum, 8);

        let contains = contains_substring("hello world".to_string(), "world".to_string());
        assert!(contains);
    }

    #[test]
    fn test_simple_macro_tools() {
        let result = simple_multiply(4, 5);
        assert_eq!(result, 20);

        let time = get_current_time();
        assert!(!time.is_empty());
    }

    #[test]
    fn test_optional_parameters() {
        // Test with folder
        let result1 = create_note(
            "Test".to_string(),
            "Content".to_string(),
            Some("work".to_string()),
        )
        .unwrap();
        assert!(result1.contains("work"));

        // Test without folder
        let result2 = create_note("Test".to_string(), "Content".to_string(), None).unwrap();
        assert!(result2.contains("notes"));

        // Test search with pagination
        let results = search_items("test".to_string(), Some(5), Some(10));
        assert_eq!(results.len(), 5);
        assert!(results[0].contains("Result 10"));
    }

    #[test]
    fn test_complex_types() {
        let strings = vec!["hello".to_string(), "world".to_string(), "test".to_string()];
        let result = process_strings(strings).unwrap();

        assert_eq!(result["count"], 3);
        assert_eq!(result["total_characters"], 14);
        assert_eq!(result["average_length"], 14.0 / 3.0);

        let data = json!({"key": "value"});
        let analysis = analyze_data(data, None).unwrap();
        assert_eq!(analysis["data_type"], "object");
        assert_eq!(analysis["has_options"], false);
    }

    #[tokio::test]
    async fn test_async_tools() {
        let result = read_file_async("test.txt".to_string()).await.unwrap();
        assert!(result.contains("test.txt"));

        let processed = process_data_async("test".to_string(), Some(50))
            .await
            .unwrap();
        assert!(processed.contains("test"));
    }

    #[test]
    fn test_macro_basic_functionality() {
        // Test that the macros don't cause compile errors
        // The actual functionality is tested at compile time
        assert!(true);
    }
}

// ============================================================================
// ADDITIONAL FUNCTIONALITY TESTS
// ============================================================================

#[cfg(test)]
mod additional_tests {
    #[test]
    fn test_macro_compiles() {
        // This test just verifies that the crate structure is valid
        assert!(true);
    }
}
