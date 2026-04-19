//! Lua tool discovery, execution, and schema generation tests.

use crucible_lua::{register_oq_module, LuaExecutor, LuaToolRegistry};
use serde_json::json;
use tokio::fs;

use super::shared::setup_tool_dir;

// ============================================================================
// LUA TOOL DISCOVERY AND EXECUTION
// ============================================================================

#[tokio::test]
async fn test_lua_tool_discovery_and_execution() {
    let dir = setup_tool_dir().await;

    // Create a Lua tool with annotations
    let tool_source = r#"
--- Add two numbers
-- @tool handler
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
-- @tool handler
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

// ============================================================================
// FULL PIPELINE: DISCOVERY -> REGISTRY -> EXECUTION
// ============================================================================

#[tokio::test]
async fn test_full_pipeline_lua() {
    let dir = setup_tool_dir().await;

    // Create a tool that uses both JSON and math
    let tool_source = r#"
--- Process data with transformations
-- @tool handler
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
-- @tool handler
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
