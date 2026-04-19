//! cru.tbl_deep_extend / cru.tbl_get tests.

use crucible_lua::LuaExecutor;
use serde_json::json;

#[tokio::test]
async fn test_cru_tbl_deep_extend_force() {
    let executor = LuaExecutor::new().unwrap();

    let source = r#"
function handler(args)
    local t1 = {a = 1, b = {x = 10}}
    local t2 = {b = {x = 20, y = 30}, c = 3}
    
    local result = cru.tbl_deep_extend("force", t1, t2)
    
    return {
        a = result.a,
        b_x = result.b.x,
        b_y = result.b.y,
        c = result.c,
    }
end
"#;

    let result = executor
        .execute_source(source, false, json!({}))
        .await
        .unwrap();

    assert!(result.success);
    let content = result.content.unwrap();
    assert_eq!(content["a"], 1);
    assert_eq!(content["b_x"], 20);
    assert_eq!(content["b_y"], 30);
    assert_eq!(content["c"], 3);
}

#[tokio::test]
async fn test_cru_tbl_deep_extend_keep() {
    let executor = LuaExecutor::new().unwrap();

    let source = r#"
function handler(args)
    local t1 = {a = 1, b = {x = 10}}
    local t2 = {b = {x = 20, y = 30}, c = 3}
    
    local result = cru.tbl_deep_extend("keep", t1, t2)
    
    return {
        a = result.a,
        b_x = result.b.x,
        b_y = result.b.y,
        c = result.c,
    }
end
"#;

    let result = executor
        .execute_source(source, false, json!({}))
        .await
        .unwrap();

    assert!(result.success);
    let content = result.content.unwrap();
    assert_eq!(content["a"], 1);
    assert_eq!(content["b_x"], 10);
    assert_eq!(content["b_y"], 30);
    assert_eq!(content["c"], 3);
}

#[tokio::test]
async fn test_cru_tbl_deep_extend_multiple_tables() {
    let executor = LuaExecutor::new().unwrap();

    let source = r#"
function handler(args)
    local t1 = {a = 1}
    local t2 = {b = 2}
    local t3 = {c = 3}
    
    local result = cru.tbl_deep_extend("force", t1, t2, t3)
    
    return {
        a = result.a,
        b = result.b,
        c = result.c,
    }
end
"#;

    let result = executor
        .execute_source(source, false, json!({}))
        .await
        .unwrap();

    assert!(result.success);
    let content = result.content.unwrap();
    assert_eq!(content["a"], 1);
    assert_eq!(content["b"], 2);
    assert_eq!(content["c"], 3);
}

#[tokio::test]
async fn test_cru_tbl_get_simple() {
    let executor = LuaExecutor::new().unwrap();

    let source = r#"
function handler(args)
    local t = {a = 1, b = 2}
    
    return {
        a = cru.tbl_get(t, "a"),
        b = cru.tbl_get(t, "b"),
        missing = cru.tbl_get(t, "c"),
    }
end
"#;

    let result = executor
        .execute_source(source, false, json!({}))
        .await
        .unwrap();

    assert!(result.success);
    let content = result.content.unwrap();
    assert_eq!(content["a"], 1);
    assert_eq!(content["b"], 2);
    assert!(content["missing"].is_null());
}

#[tokio::test]
async fn test_cru_tbl_get_nested() {
    let executor = LuaExecutor::new().unwrap();

    let source = r#"
function handler(args)
    local t = {a = {b = {c = 42}}}
    
    return {
        deep = cru.tbl_get(t, "a", "b", "c"),
        partial = cru.tbl_get(t, "a", "b"),
        missing = cru.tbl_get(t, "a", "x", "y"),
    }
end
"#;

    let result = executor
        .execute_source(source, false, json!({}))
        .await
        .unwrap();

    assert!(result.success);
    let content = result.content.unwrap();
    assert_eq!(content["deep"], 42);
    assert!(content["partial"].is_object());
    assert!(content["missing"].is_null());
}

#[tokio::test]
async fn test_cru_tbl_get_non_table_intermediate() {
    let executor = LuaExecutor::new().unwrap();

    let source = r#"
function handler(args)
    local t = {a = 42}
    
    local result = cru.tbl_get(t, "a", "b", "c")
    
    return {
        is_nil = result == nil,
    }
end
"#;

    let result = executor
        .execute_source(source, false, json!({}))
        .await
        .unwrap();

    assert!(result.success);
    let content = result.content.unwrap();
    assert_eq!(content["is_nil"], true);
}
