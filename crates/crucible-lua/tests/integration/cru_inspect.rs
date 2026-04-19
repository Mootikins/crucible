//! cru.inspect tests.

use crucible_lua::LuaExecutor;
use serde_json::json;

#[tokio::test]
async fn test_cru_inspect_simple_values() {
    let executor = LuaExecutor::new().unwrap();

    let source = r#"
function handler(args)
    return {
        nil_str = cru.inspect(nil),
        bool_true = cru.inspect(true),
        bool_false = cru.inspect(false),
        number = cru.inspect(42),
        string = cru.inspect("hello"),
        func = cru.inspect(function() end),
    }
end
"#;

    let result = executor
        .execute_source(source, false, json!({}))
        .await
        .unwrap();

    assert!(result.success);
    let content = result.content.unwrap();
    assert_eq!(content["nil_str"], "nil");
    assert_eq!(content["bool_true"], "true");
    assert_eq!(content["bool_false"], "false");
    assert_eq!(content["number"], "42");
    assert_eq!(content["string"], "\"hello\"");
    assert_eq!(content["func"], "<function>");
}

#[tokio::test]
async fn test_cru_inspect_tables() {
    let executor = LuaExecutor::new().unwrap();

    let source = r#"
function handler(args)
    local empty = {}
    local simple = {a = 1, b = "test"}
    local nested = {x = {y = {z = 42}}}
    
    return {
        empty = cru.inspect(empty),
        simple = cru.inspect(simple),
        nested = cru.inspect(nested),
    }
end
"#;

    let result = executor
        .execute_source(source, false, json!({}))
        .await
        .unwrap();

    assert!(result.success);
    let content = result.content.unwrap();
    assert_eq!(content["empty"], "{}");
    assert!(content["simple"].as_str().unwrap().contains("a = 1"));
    assert!(content["simple"].as_str().unwrap().contains("b = \"test\""));
    assert!(content["nested"].as_str().unwrap().contains("z = 42"));
}

#[tokio::test]
async fn test_cru_inspect_cycle_detection() {
    let executor = LuaExecutor::new().unwrap();

    let source = r#"
function handler(args)
    local t = {a = 1}
    t.self = t
    
    local result = cru.inspect(t)
    return {
        has_cycle = string.find(result, "cycle") ~= nil,
        result = result,
    }
end
"#;

    let result = executor
        .execute_source(source, false, json!({}))
        .await
        .unwrap();

    assert!(result.success);
    let content = result.content.unwrap();
    assert_eq!(content["has_cycle"], true);
    assert!(content["result"].as_str().unwrap().contains("cycle"));
}

#[tokio::test]
async fn test_cru_inspect_depth_limit() {
    let executor = LuaExecutor::new().unwrap();

    let source = r#"
function handler(args)
    local deep = {a = {b = {c = {d = 42}}}}
    
    local unlimited = cru.inspect(deep)
    local limited = cru.inspect(deep, {max_depth = 2})
    
    return {
        unlimited_has_42 = string.find(unlimited, "42") ~= nil,
        limited_has_42 = string.find(limited, "42") ~= nil,
        limited_has_dots = string.find(limited, "%.%.%.") ~= nil,
    }
end
"#;

    let result = executor
        .execute_source(source, false, json!({}))
        .await
        .unwrap();

    assert!(result.success);
    let content = result.content.unwrap();
    assert_eq!(content["unlimited_has_42"], true);
    assert_eq!(content["limited_has_42"], false);
    assert_eq!(content["limited_has_dots"], true);
}

#[tokio::test]
async fn test_cru_inspect_global_alias() {
    let executor = LuaExecutor::new().unwrap();

    let source = r#"
function handler(args)
    local result = inspect({x = 1})
    return {
        has_x = string.find(result, "x") ~= nil,
        is_string = type(result) == "string",
    }
end
"#;

    let result = executor
        .execute_source(source, false, json!({}))
        .await
        .unwrap();

    assert!(result.success);
    let content = result.content.unwrap();
    assert_eq!(content["has_x"], true);
    assert_eq!(content["is_string"], true);
}
