// crates/crucible-mcp/tests/test_cross_model_query_red_phase.rs
//
// TDD RED PHASE: Failing Tests for cross_model_query MCP Tool
//
// This file contains comprehensive failing tests for the cross_model_query tool
// following TDD RED-GREEN-REFACTOR methodology. These tests define the expected
// behavior of the tool before implementation.
//
// STATUS: Phase 1B (RED) - All tests expected to FAIL
// NEXT: Phase 1C (GREEN) - Implement tool to make tests pass

// Import test infrastructure from test_multi_model_helpers
mod test_multi_model_helpers;

use test_multi_model_helpers::{assertions::*, PerformanceTimer, TestContext};

// =============================================================================
// Test 1: Simple Relational to Graph Query
// =============================================================================

/// Test simple cross-model query combining relational table with graph traversal
///
/// SCENARIO: Query users table and traverse User->Post graph relationships
///
/// EXPECTED BEHAVIOR:
/// - Tool accepts query parameters with both relational and graph components
/// - Returns combined result set with data from both models
/// - Result includes user data (relational) and related posts (graph)
/// - Data is properly joined on user_id
///
/// EXPECTED TO FAIL: Tool doesn't exist yet (RED phase)
#[tokio::test]
async fn test_simple_relational_to_graph_query() {
    // Setup: Create test context with pre-populated data
    let ctx = TestContext::with_test_data().await.unwrap();

    // Define cross-model query parameters
    // This query starts with relational users table and traverses graph edges
    let params = serde_json::json!({
        "query": {
            "relational": {
                "table": "users",
                "filter": "status = 'active'"
            },
            "graph": {
                "start_from": "users.id",
                "traversal": "User-(AUTHORED)->Post"
            },
            "projection": ["users.name", "users.email", "posts.title"]
        }
    });

    // Call the cross_model_query tool
    // NOTE: This will fail because the tool doesn't exist yet
    let result = test_multi_model_helpers::call_tool(
        &ctx.mcp_service,
        "cross_model_query",
        params,
    )
    .await;

    // ASSERTIONS: Define what we WANT to happen when tool is implemented

    // 1. Tool call should succeed
    assert!(
        result.is_ok(),
        "cross_model_query tool should exist and be callable"
    );
    let result = result.unwrap();
    assert_tool_success(&result);

    // 2. Parse JSON result
    #[derive(serde::Deserialize, Debug)]
    struct QueryResult {
        records: Vec<serde_json::Value>,
        total_count: Option<u64>,
        has_more: bool,
    }

    let data: QueryResult = parse_json_result(&result).expect("Result should be valid JSON");

    // 3. Verify we got records back
    assert!(
        !data.records.is_empty(),
        "Should return at least one record combining user and post data"
    );

    // 4. Verify record structure contains data from both models
    let first_record = &data.records[0];
    assert_document_has_field(first_record, "users.name");
    assert_document_has_field(first_record, "users.email");
    assert_document_has_field(first_record, "posts.title");

    // 5. Verify data integrity
    assert_record_contains(first_record, "users.name", &serde_json::json!("Alice"));
    assert_record_contains(first_record, "posts.title", &serde_json::json!("First Post"));
}

// =============================================================================
// Test 2: Graph to Document Enrichment
// =============================================================================

/// Test cross-model query starting with graph traversal and enriching with documents
///
/// SCENARIO: Traverse graph relationships and enrich nodes with document data
///
/// EXPECTED BEHAVIOR:
/// - Start with graph traversal User->Post
/// - Enrich with document data from profiles collection
/// - Join documents with graph nodes based on user_id
/// - Return unified result with graph structure + document content
///
/// EXPECTED TO FAIL: Tool doesn't exist yet (RED phase)
#[tokio::test]
async fn test_graph_to_document_enrichment() {
    let ctx = TestContext::with_test_data().await.unwrap();

    let params = serde_json::json!({
        "query": {
            "graph": {
                "traversal": "User-(AUTHORED)->Post",
                "edge_filter": {
                    "labels": ["AUTHORED"]
                }
            },
            "document": {
                "collection": "profiles",
                "match": "profiles.user_id = user_node.id"
            },
            "projection": [
                "user_node.name",
                "post_node.title",
                "profiles.bio",
                "profiles.skills"
            ]
        }
    });

    let result = test_multi_model_helpers::call_tool(
        &ctx.mcp_service,
        "cross_model_query",
        params,
    )
    .await;

    // Tool should exist
    assert!(result.is_ok(), "cross_model_query tool should be callable");
    let result = result.unwrap();
    assert_tool_success(&result);

    // Parse result
    #[derive(serde::Deserialize)]
    struct QueryResult {
        records: Vec<serde_json::Value>,
    }

    let data: QueryResult = parse_json_result(&result).unwrap();

    // Should have records with graph + document data
    assert!(!data.records.is_empty(), "Should return enriched records");

    let record = &data.records[0];

    // Verify graph data present
    assert_document_has_field(record, "user_node.name");
    assert_document_has_field(record, "post_node.title");

    // Verify document data present (enrichment)
    assert_document_has_field(record, "profiles.bio");
    assert_document_has_field(record, "profiles.skills");

    // Verify data is properly joined
    let bio = record.get("profiles.bio").and_then(|v| v.as_str()).unwrap();
    assert!(
        bio.contains("engineer"),
        "Profile bio should contain expected content"
    );

    let skills = record.get("profiles.skills").and_then(|v| v.as_array()).unwrap();
    assert!(
        skills.len() >= 2,
        "Profile should have multiple skills from document"
    );
}

// =============================================================================
// Test 3: Complex Multi-Model Join
// =============================================================================

/// Test complex cross-model query joining all three database models
///
/// SCENARIO: Execute a query that spans relational, graph, and document models
///
/// EXPECTED BEHAVIOR:
/// - Query relational users table
/// - Traverse graph relationships (AUTHORED, TAGGED_WITH)
/// - Fetch document profiles
/// - Return fully joined result set
/// - Apply filters at each level
///
/// EXPECTED TO FAIL: Tool doesn't exist yet (RED phase)
#[tokio::test]
async fn test_complex_multi_model_join() {
    let ctx = TestContext::with_test_data().await.unwrap();

    let params = serde_json::json!({
        "query": {
            "relational": {
                "table": "users",
                "filter": "status = 'active' AND age >= 18"
            },
            "graph": {
                "start_from": "users.id",
                "traversal": "User-(AUTHORED)->Post-(TAGGED_WITH)->Tag"
            },
            "document": {
                "collection": "profiles",
                "match": "profiles.user_id = users.id"
            },
            "projection": [
                "users.name",
                "users.email",
                "users.age",
                "posts.title",
                "posts.content",
                "tags.name",
                "profiles.bio",
                "profiles.experience_years"
            ],
            "limit": 10
        }
    });

    let result = test_multi_model_helpers::call_tool(
        &ctx.mcp_service,
        "cross_model_query",
        params,
    )
    .await;

    assert!(result.is_ok(), "Complex cross-model query should be supported");
    let result = result.unwrap();
    assert_tool_success(&result);

    #[derive(serde::Deserialize)]
    struct QueryResult {
        records: Vec<serde_json::Value>,
        has_more: bool,
    }

    let data: QueryResult = parse_json_result(&result).unwrap();

    // Verify records returned
    assert!(
        !data.records.is_empty(),
        "Should return records from complex join"
    );

    // Verify limit was applied
    assert!(
        data.records.len() <= 10,
        "Should respect limit parameter"
    );

    let record = &data.records[0];

    // Verify all three models are represented in results
    // Relational data
    assert_document_has_field(record, "users.name");
    assert_document_has_field(record, "users.email");
    assert_document_has_field(record, "users.age");

    // Graph data
    assert_document_has_field(record, "posts.title");
    assert_document_has_field(record, "tags.name");

    // Document data
    assert_document_has_field(record, "profiles.bio");
    assert_document_has_field(record, "profiles.experience_years");

    // Verify filter was applied (age >= 18)
    let age = record.get("users.age").and_then(|v| v.as_i64()).unwrap();
    assert!(age >= 18, "Filter should be applied: age >= 18");

    // Verify data integrity across models
    assert_record_contains(record, "users.name", &serde_json::json!("Alice"));
    assert_record_contains(record, "posts.title", &serde_json::json!("First Post"));
    assert_record_contains(record, "tags.name", &serde_json::json!("rust"));
}

// =============================================================================
// Test 4: Cross-Model Query with Filters
// =============================================================================

/// Test cross-model query with filters applied at each model level
///
/// SCENARIO: Apply filters to relational, graph, and document queries independently
///
/// EXPECTED BEHAVIOR:
/// - Relational filter: SQL-like WHERE clause
/// - Graph filter: Filter on edge labels and properties
/// - Document filter: Search expression on document content
/// - All filters combine correctly (AND logic)
/// - Results include only records matching all filters
///
/// EXPECTED TO FAIL: Tool doesn't exist yet (RED phase)
#[tokio::test]
async fn test_cross_model_query_with_filters() {
    let ctx = TestContext::with_test_data().await.unwrap();

    let params = serde_json::json!({
        "query": {
            "relational": {
                "table": "users",
                "filter": "status = 'active' AND age > 25"
            },
            "graph": {
                "start_from": "users.id",
                "traversal": "User-(AUTHORED)->Post",
                "edge_filter": {
                    "labels": ["AUTHORED"]
                }
            },
            "document": {
                "collection": "profiles",
                "search": "bio contains 'Rust'",
                "match": "profiles.user_id = users.id"
            },
            "projection": ["users.name", "users.age", "posts.title", "profiles.bio"]
        }
    });

    let result = test_multi_model_helpers::call_tool(
        &ctx.mcp_service,
        "cross_model_query",
        params,
    )
    .await;

    assert!(result.is_ok(), "Filtered cross-model query should work");
    let result = result.unwrap();
    assert_tool_success(&result);

    #[derive(serde::Deserialize)]
    struct QueryResult {
        records: Vec<serde_json::Value>,
    }

    let data: QueryResult = parse_json_result(&result).unwrap();

    // All returned records should match all filters
    for record in &data.records {
        // Relational filter: age > 25
        let age = record.get("users.age").and_then(|v| v.as_i64()).unwrap();
        assert!(age > 25, "Relational filter should be applied: age > 25");

        // Graph filter: only AUTHORED edges (implicit - verify post exists)
        assert_document_has_field(record, "posts.title");

        // Document filter: bio contains 'Rust'
        let bio = record.get("profiles.bio").and_then(|v| v.as_str()).unwrap();
        assert!(
            bio.contains("Rust"),
            "Document filter should be applied: bio contains 'Rust'"
        );
    }

    // Should return Alice (age 30, Rust in bio)
    assert!(!data.records.is_empty(), "Should match at least one record");
    let alice = &data.records[0];
    assert_record_contains(alice, "users.name", &serde_json::json!("Alice"));
}

// =============================================================================
// Test 5: Cross-Model Query Error Handling
// =============================================================================

/// Test error handling for cross-model queries with invalid parameters
///
/// SCENARIO: Test various error conditions and verify helpful error messages
///
/// EXPECTED BEHAVIOR:
/// - Invalid table names return clear error
/// - Invalid graph patterns return descriptive error
/// - Missing collections return appropriate error
/// - Malformed queries return validation errors
/// - All errors include context about what went wrong
///
/// EXPECTED TO FAIL: Tool doesn't exist yet (RED phase)
#[tokio::test]
async fn test_cross_model_query_error_handling() {
    let ctx = TestContext::with_test_data().await.unwrap();

    // Test 1: Invalid table name
    let invalid_table_params = serde_json::json!({
        "query": {
            "relational": {
                "table": "nonexistent_table",
                "filter": "id > 0"
            }
        }
    });

    let result = test_multi_model_helpers::call_tool(
        &ctx.mcp_service,
        "cross_model_query",
        invalid_table_params,
    )
    .await;

    assert!(result.is_ok(), "Tool should return error result, not panic");
    let result = result.unwrap();
    assert_tool_error(&result);

    let error_msg = extract_text_content(&result).unwrap();
    assert!(
        error_msg.contains("table") || error_msg.contains("not found"),
        "Error message should mention missing table: {}",
        error_msg
    );

    // Test 2: Invalid graph pattern
    let invalid_graph_params = serde_json::json!({
        "query": {
            "graph": {
                "traversal": "InvalidPattern"
            }
        }
    });

    let result = test_multi_model_helpers::call_tool(
        &ctx.mcp_service,
        "cross_model_query",
        invalid_graph_params,
    )
    .await;

    assert!(result.is_ok(), "Tool should handle invalid graph patterns");
    let result = result.unwrap();
    assert_tool_error(&result);

    let error_msg = extract_text_content(&result).unwrap();
    assert!(
        error_msg.contains("pattern") || error_msg.contains("traversal"),
        "Error message should mention invalid pattern: {}",
        error_msg
    );

    // Test 3: Missing collection
    let missing_collection_params = serde_json::json!({
        "query": {
            "document": {
                "collection": "nonexistent_collection",
                "search": "test"
            }
        }
    });

    let result = test_multi_model_helpers::call_tool(
        &ctx.mcp_service,
        "cross_model_query",
        missing_collection_params,
    )
    .await;

    assert!(result.is_ok(), "Tool should handle missing collections");
    let result = result.unwrap();
    assert_tool_error(&result);

    let error_msg = extract_text_content(&result).unwrap();
    assert!(
        error_msg.contains("collection") || error_msg.contains("not found"),
        "Error message should mention missing collection: {}",
        error_msg
    );

    // Test 4: Malformed query (missing required fields)
    let malformed_params = serde_json::json!({
        "query": {}
    });

    let result = test_multi_model_helpers::call_tool(
        &ctx.mcp_service,
        "cross_model_query",
        malformed_params,
    )
    .await;

    assert!(result.is_ok(), "Tool should validate query structure");
    let result = result.unwrap();
    assert_tool_error(&result);

    let error_msg = extract_text_content(&result).unwrap();
    assert!(
        error_msg.contains("query") || error_msg.contains("required") || error_msg.contains("empty"),
        "Error message should mention query validation: {}",
        error_msg
    );
}
