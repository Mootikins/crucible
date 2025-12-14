//! Tool call format conversion tests
//!
//! Tests for parsing and converting tool calls between different LLM provider formats.
//! Covers both Ollama and OpenAI format handling, including edge cases like missing IDs,
//! argument format variations, and delta accumulation for streaming.

use crucible_core::traits::llm::{FunctionCall, FunctionCallDelta, ToolCall, ToolCallDelta};
use serde_json::json;

// ============================================================================
// Basic Structure Tests
// ============================================================================

#[test]
fn test_tool_call_basic_structure() {
    let tool_call = ToolCall {
        id: "call_123".to_string(),
        r#type: "function".to_string(),
        function: FunctionCall {
            name: "search_notes".to_string(),
            arguments: r#"{"query": "rust"}"#.to_string(),
        },
    };

    assert_eq!(tool_call.id, "call_123");
    assert_eq!(tool_call.r#type, "function");
    assert_eq!(tool_call.function.name, "search_notes");

    // Arguments should be parseable JSON
    let args: serde_json::Value = serde_json::from_str(&tool_call.function.arguments).unwrap();
    assert_eq!(args["query"], "rust");
}

#[test]
fn test_tool_call_constructor() {
    let tool_call = ToolCall::new("call_456", "get_time", "{}".to_string());

    assert_eq!(tool_call.id, "call_456");
    assert_eq!(tool_call.r#type, "function");
    assert_eq!(tool_call.function.name, "get_time");
    assert_eq!(tool_call.function.arguments, "{}");
}

#[test]
fn test_tool_call_empty_arguments() {
    let tool_call = ToolCall {
        id: "call_empty".to_string(),
        r#type: "function".to_string(),
        function: FunctionCall {
            name: "get_time".to_string(),
            arguments: "{}".to_string(),
        },
    };

    let args: serde_json::Value = serde_json::from_str(&tool_call.function.arguments).unwrap();
    assert!(args.as_object().unwrap().is_empty());
}

#[test]
fn test_tool_call_complex_arguments() {
    let args = json!({
        "query": "rust programming",
        "limit": 10,
        "filters": {
            "tags": ["rust", "programming"],
            "date_after": "2024-01-01"
        }
    });

    let tool_call = ToolCall {
        id: "call_complex".to_string(),
        r#type: "function".to_string(),
        function: FunctionCall {
            name: "advanced_search".to_string(),
            arguments: args.to_string(),
        },
    };

    let parsed: serde_json::Value = serde_json::from_str(&tool_call.function.arguments).unwrap();
    assert_eq!(parsed["limit"], 10);
    assert_eq!(parsed["filters"]["tags"][0], "rust");
    assert_eq!(parsed["query"], "rust programming");
}

#[test]
fn test_tool_call_invalid_json_arguments() {
    let tool_call = ToolCall {
        id: "call_bad".to_string(),
        r#type: "function".to_string(),
        function: FunctionCall {
            name: "test".to_string(),
            arguments: "not valid json".to_string(),
        },
    };

    // Should fail to parse
    let result: Result<serde_json::Value, _> = serde_json::from_str(&tool_call.function.arguments);
    assert!(result.is_err());
}

#[test]
fn test_multiple_tool_calls() {
    let tool_calls = vec![
        ToolCall {
            id: "call_1".to_string(),
            r#type: "function".to_string(),
            function: FunctionCall {
                name: "tool_a".to_string(),
                arguments: r#"{"param": "a"}"#.to_string(),
            },
        },
        ToolCall {
            id: "call_2".to_string(),
            r#type: "function".to_string(),
            function: FunctionCall {
                name: "tool_b".to_string(),
                arguments: r#"{"param": "b"}"#.to_string(),
            },
        },
    ];

    assert_eq!(tool_calls.len(), 2);
    assert_eq!(tool_calls[0].function.name, "tool_a");
    assert_eq!(tool_calls[1].function.name, "tool_b");

    // Verify IDs are unique
    assert_ne!(tool_calls[0].id, tool_calls[1].id);
}

// ============================================================================
// Ollama Format Tests
// ============================================================================

#[test]
fn test_ollama_tool_call_parsing() {
    // Simulate parsing Ollama format where arguments can be an object
    let ollama_json = json!({
        "id": "call_ollama_1",
        "function": {
            "name": "create_note",
            "arguments": {
                "title": "Test Note",
                "content": "Sample content"
            }
        }
    });

    // In real code, this would be deserialized and converted
    let arguments = ollama_json["function"]["arguments"].to_string();
    let tool_call = ToolCall::new(
        ollama_json["id"].as_str().unwrap(),
        ollama_json["function"]["name"].as_str().unwrap(),
        arguments,
    );

    assert_eq!(tool_call.id, "call_ollama_1");
    assert_eq!(tool_call.function.name, "create_note");

    let parsed: serde_json::Value = serde_json::from_str(&tool_call.function.arguments).unwrap();
    assert_eq!(parsed["title"], "Test Note");
}

#[test]
fn test_ollama_missing_id_generates_uuid() {
    // Ollama format might not include an ID
    let ollama_json = json!({
        "function": {
            "name": "search",
            "arguments": {
                "query": "test"
            }
        }
    });

    // Simulate generating UUID when ID is missing (like in ollama.rs line 156)
    let id = ollama_json["id"]
        .as_str()
        .map(String::from)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let tool_call = ToolCall::new(
        id.clone(),
        ollama_json["function"]["name"].as_str().unwrap(),
        ollama_json["function"]["arguments"].to_string(),
    );

    // ID should be a valid UUID
    assert!(uuid::Uuid::parse_str(&tool_call.id).is_ok());
    assert_eq!(tool_call.function.name, "search");
}

#[test]
fn test_ollama_object_args() {
    // Ollama returns arguments as an object, not a string
    let args_object = json!({
        "query": "rust",
        "limit": 5
    });

    // Convert to string for storage
    let args_string = args_object.to_string();
    let tool_call = ToolCall::new("call_obj", "search", args_string);

    // Should be parseable back to object
    let parsed: serde_json::Value = serde_json::from_str(&tool_call.function.arguments).unwrap();
    assert_eq!(parsed["query"], "rust");
    assert_eq!(parsed["limit"], 5);
}

#[test]
fn test_ollama_string_args() {
    // Some Ollama responses might have arguments as a string already
    let args_string = r#"{"query": "test"}"#.to_string();
    let tool_call = ToolCall::new("call_str", "search", args_string);

    let parsed: serde_json::Value = serde_json::from_str(&tool_call.function.arguments).unwrap();
    assert_eq!(parsed["query"], "test");
}

// ============================================================================
// OpenAI Format Tests
// ============================================================================

#[test]
fn test_openai_tool_call_format() {
    // OpenAI format has arguments as a JSON string
    let openai_json = json!({
        "id": "call_openai_1",
        "type": "function",
        "function": {
            "name": "create_note",
            "arguments": r#"{"title":"Test","content":"Content"}"#
        }
    });

    let tool_call = ToolCall {
        id: openai_json["id"].as_str().unwrap().to_string(),
        r#type: openai_json["type"].as_str().unwrap().to_string(),
        function: FunctionCall {
            name: openai_json["function"]["name"].as_str().unwrap().to_string(),
            arguments: openai_json["function"]["arguments"]
                .as_str()
                .unwrap()
                .to_string(),
        },
    };

    assert_eq!(tool_call.id, "call_openai_1");
    assert_eq!(tool_call.r#type, "function");

    let parsed: serde_json::Value = serde_json::from_str(&tool_call.function.arguments).unwrap();
    assert_eq!(parsed["title"], "Test");
}

// ============================================================================
// Delta Accumulation Tests (for streaming)
// ============================================================================

#[test]
fn test_tool_call_delta_accumulation() {
    // Simulate OpenAI streaming deltas
    let deltas = vec![
        ToolCallDelta {
            index: 0,
            id: Some("call_".to_string()),
            function: Some(FunctionCallDelta {
                name: Some("search".to_string()),
                arguments: Some(r#"{"qu"#.to_string()),
            }),
        },
        ToolCallDelta {
            index: 0,
            id: Some("abc".to_string()),
            function: Some(FunctionCallDelta {
                name: None,
                arguments: Some(r#"ery": "test"}"#.to_string()),
            }),
        },
    ];

    // Accumulate
    let mut id = String::new();
    let mut name = String::new();
    let mut arguments = String::new();

    for delta in deltas {
        if let Some(d_id) = delta.id {
            id.push_str(&d_id);
        }
        if let Some(func) = delta.function {
            if let Some(n) = func.name {
                name.push_str(&n);
            }
            if let Some(a) = func.arguments {
                arguments.push_str(&a);
            }
        }
    }

    assert_eq!(id, "call_abc");
    assert_eq!(name, "search");
    assert_eq!(arguments, r#"{"query": "test"}"#);

    // Verify the accumulated arguments are valid JSON
    let parsed: serde_json::Value = serde_json::from_str(&arguments).unwrap();
    assert_eq!(parsed["query"], "test");
}

#[test]
fn test_openai_delta_accumulation() {
    // More realistic OpenAI streaming example
    let deltas = vec![
        ToolCallDelta {
            index: 0,
            id: Some("call_123".to_string()),
            function: Some(FunctionCallDelta {
                name: Some("create_note".to_string()),
                arguments: None,
            }),
        },
        ToolCallDelta {
            index: 0,
            id: None,
            function: Some(FunctionCallDelta {
                name: None,
                arguments: Some(r#"{"title":""#.to_string()),
            }),
        },
        ToolCallDelta {
            index: 0,
            id: None,
            function: Some(FunctionCallDelta {
                name: None,
                arguments: Some(r#"My Note","co"#.to_string()),
            }),
        },
        ToolCallDelta {
            index: 0,
            id: None,
            function: Some(FunctionCallDelta {
                name: None,
                arguments: Some(r#"ntent":"Test"}"#.to_string()),
            }),
        },
    ];

    let mut accumulated_id = String::new();
    let mut accumulated_name = String::new();
    let mut accumulated_args = String::new();

    for delta in deltas {
        if let Some(id) = delta.id {
            accumulated_id.push_str(&id);
        }
        if let Some(func) = delta.function {
            if let Some(name) = func.name {
                accumulated_name.push_str(&name);
            }
            if let Some(args) = func.arguments {
                accumulated_args.push_str(&args);
            }
        }
    }

    assert_eq!(accumulated_id, "call_123");
    assert_eq!(accumulated_name, "create_note");
    assert_eq!(accumulated_args, r#"{"title":"My Note","content":"Test"}"#);

    // Verify valid JSON
    let parsed: serde_json::Value = serde_json::from_str(&accumulated_args).unwrap();
    assert_eq!(parsed["title"], "My Note");
    assert_eq!(parsed["content"], "Test");
}

#[test]
fn test_openai_out_of_order_index() {
    // Test handling multiple parallel tool calls with different indices
    let deltas = vec![
        ToolCallDelta {
            index: 0,
            id: Some("call_1".to_string()),
            function: Some(FunctionCallDelta {
                name: Some("tool_a".to_string()),
                arguments: Some(r#"{"a":"#.to_string()),
            }),
        },
        ToolCallDelta {
            index: 1,
            id: Some("call_2".to_string()),
            function: Some(FunctionCallDelta {
                name: Some("tool_b".to_string()),
                arguments: Some(r#"{"b":"#.to_string()),
            }),
        },
        ToolCallDelta {
            index: 0,
            id: None,
            function: Some(FunctionCallDelta {
                name: None,
                arguments: Some(r#"1}"#.to_string()),
            }),
        },
        ToolCallDelta {
            index: 1,
            id: None,
            function: Some(FunctionCallDelta {
                name: None,
                arguments: Some(r#"2}"#.to_string()),
            }),
        },
    ];

    // Group by index
    use std::collections::HashMap;
    let mut calls: HashMap<u32, (String, String, String)> = HashMap::new();

    for delta in deltas {
        let entry = calls.entry(delta.index).or_insert_with(|| {
            (String::new(), String::new(), String::new())
        });

        if let Some(id) = delta.id {
            entry.0.push_str(&id);
        }
        if let Some(func) = delta.function {
            if let Some(name) = func.name {
                entry.1.push_str(&name);
            }
            if let Some(args) = func.arguments {
                entry.2.push_str(&args);
            }
        }
    }

    assert_eq!(calls.len(), 2);
    assert_eq!(calls[&0].0, "call_1");
    assert_eq!(calls[&0].1, "tool_a");
    assert_eq!(calls[&0].2, r#"{"a":1}"#);
    assert_eq!(calls[&1].0, "call_2");
    assert_eq!(calls[&1].1, "tool_b");
    assert_eq!(calls[&1].2, r#"{"b":2}"#);
}

#[test]
fn test_openai_empty_function() {
    // Test delta with no function data (should be ignored)
    let delta = ToolCallDelta {
        index: 0,
        id: Some("call_empty".to_string()),
        function: None,
    };

    assert_eq!(delta.index, 0);
    assert_eq!(delta.id, Some("call_empty".to_string()));
    assert!(delta.function.is_none());
}

// ============================================================================
// Tool Call ID Preservation Tests
// ============================================================================

#[test]
fn test_tool_call_id_preserved() {
    let original_id = "call_test_12345";

    // Create tool call
    let tool_call = ToolCall::new(original_id, "test_function", r#"{"arg":"value"}"#.to_string());

    // Verify ID is preserved exactly
    assert_eq!(tool_call.id, original_id);

    // Simulate conversion (like in handle.rs)
    let chat_tool_call = crucible_core::traits::chat::ChatToolCall {
        name: tool_call.function.name.clone(),
        arguments: serde_json::from_str(&tool_call.function.arguments).ok(),
        id: Some(tool_call.id.clone()),
    };

    assert_eq!(chat_tool_call.id.unwrap(), original_id);
}

#[test]
fn test_tool_call_id_types() {
    // Test various ID formats that might be used by different providers
    let test_ids = vec![
        "call_123",
        "call_abc_def_123",
        "call-with-dashes",
        "CallWithCaps",
        "123-numeric-start",
        "call_very_long_identifier_with_many_parts_12345678",
    ];

    for id in test_ids {
        let tool_call = ToolCall::new(id, "test", "{}".to_string());
        assert_eq!(tool_call.id, id);
    }
}

// ============================================================================
// Argument Format Variation Tests
// ============================================================================

#[test]
fn test_arguments_whitespace_handling() {
    // Test that whitespace in JSON arguments is preserved
    let args_compact = r#"{"key":"value"}"#;
    let args_pretty = r#"{
  "key": "value"
}"#;

    let tool1 = ToolCall::new("call_1", "test", args_compact.to_string());
    let tool2 = ToolCall::new("call_2", "test", args_pretty.to_string());

    // Both should parse to the same value
    let parsed1: serde_json::Value = serde_json::from_str(&tool1.function.arguments).unwrap();
    let parsed2: serde_json::Value = serde_json::from_str(&tool2.function.arguments).unwrap();
    assert_eq!(parsed1, parsed2);
}

#[test]
fn test_arguments_unicode_handling() {
    let args = json!({
        "text": "Hello 世界 🌍",
        "emoji": "✨🚀💫"
    });

    let tool_call = ToolCall::new("call_unicode", "test", args.to_string());

    let parsed: serde_json::Value = serde_json::from_str(&tool_call.function.arguments).unwrap();
    assert_eq!(parsed["text"], "Hello 世界 🌍");
    assert_eq!(parsed["emoji"], "✨🚀💫");
}

#[test]
fn test_arguments_nested_objects() {
    let args = json!({
        "level1": {
            "level2": {
                "level3": {
                    "value": 42
                }
            }
        }
    });

    let tool_call = ToolCall::new("call_nested", "test", args.to_string());

    let parsed: serde_json::Value = serde_json::from_str(&tool_call.function.arguments).unwrap();
    assert_eq!(parsed["level1"]["level2"]["level3"]["value"], 42);
}

#[test]
fn test_arguments_arrays() {
    let args = json!({
        "tags": ["rust", "testing", "tools"],
        "numbers": [1, 2, 3, 4, 5],
        "mixed": ["text", 42, true, null]
    });

    let tool_call = ToolCall::new("call_arrays", "test", args.to_string());

    let parsed: serde_json::Value = serde_json::from_str(&tool_call.function.arguments).unwrap();
    assert_eq!(parsed["tags"][0], "rust");
    assert_eq!(parsed["numbers"][2], 3);
    assert_eq!(parsed["mixed"][1], 42);
}

// ============================================================================
// Edge Cases
// ============================================================================

#[test]
fn test_empty_tool_call_list() {
    let tool_calls: Vec<ToolCall> = vec![];
    assert_eq!(tool_calls.len(), 0);
}

#[test]
fn test_tool_call_clone() {
    let original = ToolCall::new("call_clone", "test", r#"{"x":1}"#.to_string());
    let cloned = original.clone();

    assert_eq!(original.id, cloned.id);
    assert_eq!(original.function.name, cloned.function.name);
    assert_eq!(original.function.arguments, cloned.function.arguments);
}

#[test]
fn test_function_call_delta_optional_fields() {
    // All fields can be None
    let delta = FunctionCallDelta {
        name: None,
        arguments: None,
    };

    assert!(delta.name.is_none());
    assert!(delta.arguments.is_none());

    // Only name
    let delta_name = FunctionCallDelta {
        name: Some("test".to_string()),
        arguments: None,
    };
    assert_eq!(delta_name.name, Some("test".to_string()));

    // Only arguments
    let delta_args = FunctionCallDelta {
        name: None,
        arguments: Some("{}".to_string()),
    };
    assert_eq!(delta_args.arguments, Some("{}".to_string()));
}

#[test]
fn test_tool_call_serialization() {
    let tool_call = ToolCall::new("call_ser", "serialize_test", r#"{"test":true}"#.to_string());

    // Should be serializable
    let json = serde_json::to_string(&tool_call).unwrap();
    assert!(json.contains("call_ser"));
    assert!(json.contains("serialize_test"));

    // Should be deserializable
    let deserialized: ToolCall = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.id, tool_call.id);
    assert_eq!(deserialized.function.name, tool_call.function.name);
}
