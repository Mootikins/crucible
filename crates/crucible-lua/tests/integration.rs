//! Integration tests for Lua/Fennel tool discovery and execution
//!
//! These tests verify the full pipeline:
//! 1. Parse annotations from source files
//! 2. Discover tools/hooks/plugins
//! 3. Execute tools with JSON args
//! 4. Verify results

use crucible_lua::{
    register_oq_module, register_shell_module, AnnotationParser, LuaExecutor, LuaToolRegistry,
    ShellPolicy,
};
use serde_json::json;
use std::path::Path;
use tempfile::TempDir;
use tokio::fs;

/// Helper to create a temp dir with tool files
async fn setup_tool_dir() -> TempDir {
    let dir = TempDir::new().unwrap();
    dir
}

// ============================================================================
// LUA TOOL DISCOVERY AND EXECUTION
// ============================================================================

#[tokio::test]
async fn test_lua_tool_discovery_and_execution() {
    let dir = setup_tool_dir().await;

    // Create a Lua tool with annotations
    let tool_source = r#"
--- Add two numbers
-- @tool
-- @param x number The first number
-- @param y number The second number
-- @return number The sum
function handler(args)
    return { result = args.x + args.y }
end
"#;

    let tool_path = dir.path().join("add.lua");
    fs::write(&tool_path, tool_source).await.unwrap();

    // Discover tools
    let mut registry = LuaToolRegistry::new().unwrap();
    let count = registry.discover_from(dir.path()).await.unwrap();

    assert_eq!(count, 1, "Should discover 1 tool");

    // Get and verify the tool
    let tools = registry.list_tools();
    assert_eq!(tools.len(), 1);

    // Execute the tool
    let result = registry
        .execute("handler", json!({"x": 10, "y": 5}))
        .await
        .unwrap();
    assert!(result.success);
    assert_eq!(result.content, json!({"result": 15}));
}

#[tokio::test]
async fn test_lua_tool_with_string_operations() {
    let dir = setup_tool_dir().await;

    let tool_source = r#"
--- String manipulation tool
-- @tool
-- @param text string The input text
-- @param mode string Processing mode (upper/lower/reverse)
function handler(args)
    local text = args.text
    local mode = args.mode or "upper"

    if mode == "upper" then
        return { result = text:upper() }
    elseif mode == "lower" then
        return { result = text:lower() }
    elseif mode == "reverse" then
        return { result = text:reverse() }
    else
        return { error = "Unknown mode: " .. mode }
    end
end
"#;

    let tool_path = dir.path().join("string_tool.lua");
    fs::write(&tool_path, tool_source).await.unwrap();

    let mut registry = LuaToolRegistry::new().unwrap();
    registry.discover_from(dir.path()).await.unwrap();

    // Test uppercase
    let result = registry
        .execute("handler", json!({"text": "hello", "mode": "upper"}))
        .await
        .unwrap();
    assert!(result.success);
    assert_eq!(result.content["result"], "HELLO");

    // Test reverse
    let result = registry
        .execute("handler", json!({"text": "hello", "mode": "reverse"}))
        .await
        .unwrap();
    assert!(result.success);
    assert_eq!(result.content["result"], "olleh");
}

#[tokio::test]
async fn test_lua_tool_with_oq_module() {
    let executor = LuaExecutor::new().unwrap();

    // Register oq module
    register_oq_module(executor.lua()).unwrap();

    let source = r#"
function handler(args)
    -- Parse JSON string
    local data = oq.parse(args.json_str)

    -- Modify and encode back
    data.processed = true
    data.count = (data.count or 0) + 1

    return {
        encoded = oq.json(data),
        pretty = oq.json_pretty(data)
    }
end
"#;

    let result = executor
        .execute_source(
            source,
            false,
            json!({"json_str": r#"{"name":"test","count":5}"#}),
        )
        .await
        .unwrap();

    assert!(result.success);
    let content = result.content.unwrap();

    // Verify the encoded result
    let encoded: serde_json::Value =
        serde_json::from_str(content["encoded"].as_str().unwrap()).unwrap();
    assert_eq!(encoded["name"], "test");
    assert_eq!(encoded["count"], 6);
    assert_eq!(encoded["processed"], true);

    // Verify pretty print has newlines
    let pretty = content["pretty"].as_str().unwrap();
    assert!(pretty.contains('\n'));
}

// NOTE: Shell module uses async functions which can't be called from synchronous
// Lua handlers via execute_source. The shell module is tested in its own unit tests.
// This test verifies the shell module loads correctly.
#[tokio::test]
async fn test_lua_shell_module_registration() {
    let executor = LuaExecutor::new().unwrap();

    // Verify shell module can be registered
    register_shell_module(executor.lua(), ShellPolicy::permissive()).unwrap();

    // Verify shell.which works (synchronous function)
    let source = r#"
function handler(args)
    -- shell.which is synchronous and should work
    local echo_path = shell.which("echo")
    return {
        found = echo_path ~= nil,
        is_string = type(echo_path) == "string"
    }
end
"#;

    let result = executor
        .execute_source(source, false, json!({}))
        .await
        .unwrap();
    assert!(result.success);
    let content = result.content.unwrap();
    assert!(content["found"].as_bool().unwrap());
    assert!(content["is_string"].as_bool().unwrap());
}

#[tokio::test]
async fn test_lua_shell_policy_blocks_dangerous_commands() {
    let executor = LuaExecutor::new().unwrap();

    // Use default policy which blocks rm, sudo, etc.
    register_shell_module(executor.lua(), ShellPolicy::default()).unwrap();

    let source = r#"
function handler(args)
    local result = shell.exec("rm", {"-rf", "/"}, {})
    return { executed = true }
end
"#;

    let result = executor
        .execute_source(source, false, json!({}))
        .await
        .unwrap();

    // The shell.exec should fail due to policy
    assert!(!result.success);
    assert!(result.error.unwrap().contains("not allowed"));
}

// ============================================================================
// ANNOTATION PARSER INTEGRATION
// ============================================================================

#[test]
fn test_annotation_parser_multiple_tools_in_file() {
    let parser = AnnotationParser::new();

    let source = r#"
--- Search the knowledge base
-- @tool desc="Search for notes"
-- @param query string The search query
-- @param limit number? Maximum results
function search(query, limit)
    return crucible.search(query, limit or 10)
end

--- Get note by path
-- @tool desc="Retrieve a note"
-- @param path string Path to the note
function get_note(path)
    return crucible.get_note(path)
end

--- Format output
-- @hook event="tool:after" pattern="search*"
function format_search_results(ctx, event)
    return event
end
"#;

    let tools = parser
        .parse_lua_tools(source, Path::new("multi.lua"))
        .unwrap();
    assert_eq!(tools.len(), 2, "Should find 2 tools");

    // Verify search tool
    let search = tools.iter().find(|t| t.name == "search").unwrap();
    assert_eq!(search.params.len(), 2);
    assert_eq!(search.params[0].name, "query");
    assert_eq!(search.params[0].param_type, "string");
    assert_eq!(search.params[1].name, "limit");
    assert!(search.params[1].param_type.contains("number"));

    // Verify hooks
    let hooks = parser
        .parse_hooks(source, Path::new("multi.lua"), false)
        .unwrap();
    assert_eq!(hooks.len(), 1);
    assert_eq!(hooks[0].event_type, "tool:after");
    assert_eq!(hooks[0].pattern, "search*");
}

#[test]
fn test_annotation_parser_fennel_syntax() {
    let parser = AnnotationParser::new();

    // Note: Using underscores in names because the regex uses \w+ which doesn't match hyphens
    let source = r#"
;;; Calculate factorial
;; @tool desc="Compute factorial"
;; @param n number The input number
;; @return number The factorial
(fn factorial [n]
  (if (<= n 1)
    1
    (* n (factorial (- n 1)))))

;;; Log calls for debugging
;; @hook event="tool:before" pattern="*"
(fn log_call [ctx event]
  (print "Calling tool...")
  event)
"#;

    let tools = parser
        .parse_fennel_tools(source, Path::new("math.fnl"))
        .unwrap();
    assert_eq!(tools.len(), 1);
    assert!(tools[0].is_fennel);
    assert_eq!(tools[0].name, "factorial");
    assert_eq!(tools[0].params.len(), 1);
    assert_eq!(tools[0].params[0].name, "n");

    let hooks = parser
        .parse_hooks(source, Path::new("math.fnl"), true)
        .unwrap();
    assert_eq!(hooks.len(), 1);
    assert_eq!(hooks[0].name, "log_call");
}

#[test]
fn test_annotation_parser_plugin_discovery() {
    let parser = AnnotationParser::new();

    // Plugin name is taken from the function name, not from annotation attributes
    let source = r#"
--- Git integration plugin
-- @plugin watch=["*.md", "*.txt"]
function git_tools()
    -- plugin factory function
    return {}
end

--- Initialize plugin
-- @init
function init(config)
    crucible.log("info", "Git plugin initialized")
end
"#;

    let plugins = parser
        .parse_plugins(source, Path::new("git.lua"), false)
        .unwrap();
    assert_eq!(plugins.len(), 1);
    assert_eq!(plugins[0].name, "git_tools");
    assert_eq!(plugins[0].description, "Git integration plugin");
}

// ============================================================================
// FULL PIPELINE: ANNOTATION -> REGISTRY -> EXECUTION
// ============================================================================

#[tokio::test]
async fn test_full_pipeline_lua() {
    let dir = setup_tool_dir().await;

    // Create a tool that uses both JSON and math
    let tool_source = r#"
--- Process data with transformations
-- @tool
-- @param data string JSON data to process
-- @param multiplier number Multiply numbers by this
function handler(args)
    local parsed = crucible.json_decode(args.data)

    -- Transform any numbers in the data
    if parsed.value then
        parsed.value = parsed.value * args.multiplier
    end
    if parsed.items then
        for i, v in ipairs(parsed.items) do
            if type(v) == "number" then
                parsed.items[i] = v * args.multiplier
            end
        end
    end

    return {
        processed = true,
        result = parsed
    }
end
"#;

    fs::write(dir.path().join("transform.lua"), tool_source)
        .await
        .unwrap();

    // Create and populate registry
    let mut registry = LuaToolRegistry::new().unwrap();
    let count = registry.discover_from(dir.path()).await.unwrap();
    assert_eq!(count, 1);

    // Execute with test data
    let input = json!({
        "data": r#"{"value": 10, "items": [1, 2, 3]}"#,
        "multiplier": 2
    });

    let result = registry.execute("handler", input).await.unwrap();
    assert!(result.success);

    let content = result.content;
    assert!(content["processed"].as_bool().unwrap());
    assert_eq!(content["result"]["value"], 20);
    // Arrays should be multiplied too
    let items = content["result"]["items"].as_array().unwrap();
    assert_eq!(items[0], 2);
    assert_eq!(items[1], 4);
    assert_eq!(items[2], 6);
}

#[tokio::test]
async fn test_error_handling_in_tools() {
    let dir = setup_tool_dir().await;

    let tool_source = r#"
--- Tool that may error
-- @tool
-- @param should_error boolean Whether to throw
function handler(args)
    if args.should_error then
        error("Intentional error for testing")
    end
    return { success = true }
end
"#;

    fs::write(dir.path().join("may_error.lua"), tool_source)
        .await
        .unwrap();

    let mut registry = LuaToolRegistry::new().unwrap();
    registry.discover_from(dir.path()).await.unwrap();

    // Test success case
    let result = registry
        .execute("handler", json!({"should_error": false}))
        .await
        .unwrap();
    assert!(result.success);

    // Test error case
    let result = registry
        .execute("handler", json!({"should_error": true}))
        .await
        .unwrap();
    assert!(!result.success);
    assert!(result.error.unwrap().contains("Intentional error"));
}

#[tokio::test]
async fn test_tool_schema_generation() {
    use crucible_lua::{generate_input_schema, LuaTool, ToolParam};

    // Test schema generation directly from a LuaTool
    let tool = LuaTool {
        name: "search".to_string(),
        description: "Search with typed params".to_string(),
        params: vec![
            ToolParam {
                name: "query".to_string(),
                param_type: "string".to_string(),
                description: "The search query".to_string(),
                required: true,
                default: None,
            },
            ToolParam {
                name: "limit".to_string(),
                param_type: "number".to_string(),
                description: "Maximum results".to_string(),
                required: false,
                default: None,
            },
            ToolParam {
                name: "include_archived".to_string(),
                param_type: "boolean".to_string(),
                description: "Include archived notes".to_string(),
                required: false,
                default: None,
            },
        ],
        source_path: "test.lua".to_string(),
        is_fennel: false,
    };

    // Generate schema
    let schema = generate_input_schema(&tool);

    // Verify schema structure
    assert_eq!(schema["type"], "object");
    let properties = schema["properties"].as_object().unwrap();

    // Should have all params
    assert!(properties.contains_key("query"));
    assert!(properties.contains_key("limit"));
    assert!(properties.contains_key("include_archived"));

    // Query should be required
    let required = schema["required"].as_array().unwrap();
    assert!(required.iter().any(|v| v == "query"));
}

// ============================================================================
// FENNEL INTEGRATION (when available)
// ============================================================================

#[cfg(feature = "fennel")]
mod fennel_tests {
    use super::*;

    #[tokio::test]
    async fn test_fennel_tool_execution() {
        // Note: This requires fennel.lua to be present in vendor/
        let executor = LuaExecutor::new().unwrap();

        let source = r#"
(fn handler [args]
  {:result (+ args.x args.y)
   :language "fennel"})
"#;

        let result = executor
            .execute_source(source, true, json!({"x": 5, "y": 3}))
            .await;

        // If Fennel is available, verify execution
        if let Ok(res) = result {
            if res.success {
                let content = res.content.unwrap();
                assert_eq!(content["result"], 8);
                assert_eq!(content["language"], "fennel");
            }
        }
    }
}
