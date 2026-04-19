//! Health module integration tests.

use crucible_lua::LuaExecutor;
use serde_json::json;

// ============================================================================
// HEALTH MODULE INTEGRATION TESTS
// ============================================================================

#[tokio::test]
async fn test_health_collect_ok_and_warn() {
    let executor = LuaExecutor::new().unwrap();

    let source = r#"
function handler(args)
    cru.health.start("test-plugin")
    cru.health.ok("Database connected")
    cru.health.ok("Cache initialized")
    cru.health.warn("Memory usage at 75%", {"Consider increasing heap size"})
    cru.health.info("Last sync: 5 minutes ago")
    
    local results = cru.health.get_results()
    return {
        name = results.name,
        healthy = results.healthy,
        check_count = #results.checks,
        first_check_level = results.checks[1].level,
        first_check_msg = results.checks[1].msg,
        warn_check_level = results.checks[3].level,
        warn_has_advice = results.checks[3].advice ~= nil,
    }
end
"#;

    let result = executor
        .execute_source(source, false, json!({}))
        .await
        .unwrap();

    assert!(result.success);
    let content = result.content.unwrap();
    assert_eq!(content["name"], "test-plugin");
    assert_eq!(content["healthy"], true);
    assert_eq!(content["check_count"], 4);
    assert_eq!(content["first_check_level"], "ok");
    assert_eq!(content["first_check_msg"], "Database connected");
    assert_eq!(content["warn_check_level"], "warn");
    assert_eq!(content["warn_has_advice"], true);
}

#[tokio::test]
async fn test_health_error_makes_unhealthy() {
    let executor = LuaExecutor::new().unwrap();

    let source = r#"
function handler(args)
    cru.health.start("failing-plugin")
    cru.health.ok("Config loaded")
    cru.health.error("API key missing", {"Set CRUCIBLE_API_KEY env var", "Or use config file"})
    cru.health.warn("Fallback mode active")
    
    local results = cru.health.get_results()
    return {
        name = results.name,
        healthy = results.healthy,
        check_count = #results.checks,
        error_check_level = results.checks[2].level,
        error_msg = results.checks[2].msg,
        error_advice_count = results.checks[2].advice and #results.checks[2].advice or 0,
    }
end
"#;

    let result = executor
        .execute_source(source, false, json!({}))
        .await
        .unwrap();

    assert!(result.success);
    let content = result.content.unwrap();
    assert_eq!(content["name"], "failing-plugin");
    assert_eq!(content["healthy"], false);
    assert_eq!(content["check_count"], 3);
    assert_eq!(content["error_check_level"], "error");
    assert_eq!(content["error_msg"], "API key missing");
    assert_eq!(content["error_advice_count"], 2);
}
