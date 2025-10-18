// Example usage of test_multi_model_helpers
//
// This file demonstrates how the test helper infrastructure should be used
// in Phase 1B and 1C for testing multi-model MCP integration.

mod test_multi_model_helpers;
use test_multi_model_helpers::*;
use std::sync::Arc;

#[tokio::test]
async fn example_test_context_usage() {
    // Create test context with all dependencies
    let ctx = TestContext::new().await.unwrap();

    // Access components
    assert!(Arc::strong_count(&ctx.embedding_db) > 0);
    assert!(Arc::strong_count(&ctx.surreal_client) > 0);
    assert!(Arc::strong_count(&ctx.mock_provider) > 0);
}

#[tokio::test]
async fn example_test_data_setup() {
    // Create context with pre-populated test data
    let ctx = TestContext::with_test_data().await.unwrap();

    // Verify data was created across all models
    use crucible_core::{RelationalDB, SelectQuery};

    // Check relational data
    let users = ctx
        .surreal_client
        .select(SelectQuery {
            table: "users".to_string(),
            columns: None,
            filter: None,
            order_by: None,
            limit: None,
            offset: None,
            joins: None,
        })
        .await
        .unwrap();

    assert_eq!(users.records.len(), 1);
    assert_eq!(
        users.records[0].data.get("name").unwrap(),
        &serde_json::json!("Alice")
    );
}

#[tokio::test]
async fn example_tool_call_placeholder() {
    // NOTE: This demonstrates how call_tool will be used in Phase 1C
    // For Phase 1B (RED phase), this will fail as expected

    let ctx = TestContext::new().await.unwrap();

    let params = serde_json::json!({
        "query": {
            "relational": {
                "table": "users",
                "filter": "status = 'active'"
            }
        }
    });

    // This will fail in Phase 1B because tools aren't implemented yet
    let result = call_tool(&ctx.mcp_service, "cross_model_query", params).await;

    // In Phase 1B, we expect this to fail
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not yet implemented"));
}

#[tokio::test]
async fn example_assertion_helpers() {
    use assertions::*;

    // Example of how to assert tool success/failure
    let success_result =
        rmcp::model::CallToolResult::success(vec![rmcp::model::Content::text(
            "test".to_string(),
        )]);
    assert_tool_success(&success_result);

    let error_result = rmcp::model::CallToolResult::error(vec![rmcp::model::Content::text(
        "error".to_string(),
    )]);
    assert_tool_error(&error_result);

    // Example of extracting text content
    let text = extract_text_content(&success_result);
    assert!(text.is_some());

    // Example of parsing JSON results (would be used in Phase 1C)
    let json_result = rmcp::model::CallToolResult::success(vec![rmcp::model::Content::text(
        serde_json::json!({
            "records": [
                {"name": "Alice", "age": 30}
            ]
        })
        .to_string(),
    )]);

    #[derive(serde::Deserialize)]
    struct QueryResult {
        records: Vec<serde_json::Value>,
    }

    let parsed: QueryResult = parse_json_result(&json_result).unwrap();
    assert_eq!(parsed.records.len(), 1);
}

#[tokio::test]
async fn example_performance_timing() {
    let timer = PerformanceTimer::start("test operation");

    // Simulate some work
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    // Verify duration
    let elapsed = timer.elapsed_ms();
    assert!(elapsed >= 10);

    // This would be used in Phase 1D for performance baseline
    // timer.assert_duration_ms(200); // Assert operation took < 200ms
}
