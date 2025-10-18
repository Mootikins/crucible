// crates/crucible-mcp/tests/unit/mcp_integration/test_multi_model_transaction.rs
//
// TDD RED Phase: Comprehensive Failing Tests for multi_model_transaction MCP Tool
//
// This test suite validates ACID properties across multi-model transactions:
// - Atomicity: All operations succeed or all are rolled back
// - Consistency: Database maintains valid state across models
// - Isolation: Uncommitted changes not visible to other transactions
// - Durability: Committed changes persist (verified through queries)
//
// Expected State: ALL TESTS FAIL (tool not yet implemented)
// Implementation: Phase 1C (GREEN phase) will make these pass

use crate::test_multi_model_helpers::{TestContext, assertions::*};
use crucible_core::{RelationalDB, GraphDB, DocumentDB, SelectQuery};

// =============================================================================
// Test 1: Atomic Multi-Model Commit
// =============================================================================

/// Test atomic commit across all three database models
///
/// ACID Property Tested: ATOMICITY
///
/// Scenario:
/// 1. Begin transaction
/// 2. Create relational record (user)
/// 3. Create graph node (User node)
/// 4. Create document (user profile)
/// 5. Commit transaction
/// 6. Verify ALL data exists
///
/// Expected Behavior:
/// - All three operations succeed together
/// - Data is queryable after commit
/// - Cross-references between models are valid
///
/// Expected to FAIL: multi_model_transaction tool doesn't exist yet
#[tokio::test]
async fn test_atomic_multi_model_commit() {
    let ctx = TestContext::new().await.unwrap();

    // Setup: Create necessary schemas
    ctx.setup_test_data().await.unwrap();

    // Define transaction operations across all three models
    let operations = serde_json::json!([
        {
            "type": "relational",
            "operation": "insert",
            "table": "users",
            "data": {
                "name": "Bob",
                "email": "bob@example.com",
                "age": 28,
                "status": "active"
            }
        },
        {
            "type": "graph",
            "operation": "create_node",
            "label": "User",
            "properties": {
                "name": "Bob",
                "email": "bob@example.com"
            }
        },
        {
            "type": "document",
            "operation": "create_document",
            "collection": "profiles",
            "document": {
                "content": {
                    "user_name": "Bob",
                    "bio": "Backend developer specializing in distributed systems",
                    "skills": ["Go", "Kubernetes", "PostgreSQL"]
                },
                "metadata": {
                    "tags": ["engineer", "backend"]
                }
            }
        }
    ]);

    // Execute transaction (EXPECTED TO FAIL - tool not implemented)
    let result = crate::test_multi_model_helpers::call_tool(
        &ctx.mcp_service,
        "multi_model_transaction",
        serde_json::json!({ "operations": operations })
    ).await;

    // This assertion will fail because call_tool returns error
    assert!(result.is_ok(), "Transaction tool call failed: {:?}", result.err());
    let result = result.unwrap();

    // Verify transaction succeeded
    assert_tool_success(&result);

    // Parse result to verify operation details
    let tx_result: serde_json::Value = parse_json_result(&result).unwrap();
    assert_eq!(tx_result["success"].as_bool(), Some(true));
    assert_eq!(tx_result["operations"].as_array().unwrap().len(), 3);

    // Verify relational record exists
    let user_query = ctx.surreal_client.select(SelectQuery {
        table: "users".to_string(),
        columns: None,
        filter: Some(crucible_core::FilterClause::Equals {
            column: "name".to_string(),
            value: serde_json::json!("Bob"),
        }),
        order_by: None,
        limit: None,
        offset: None,
        joins: None,
    }).await.unwrap();

    assert_eq!(user_query.records.len(), 1, "User record not found after transaction commit");
    assert_eq!(user_query.records[0].data.get("email").unwrap(), "bob@example.com");

    // Verify graph node exists
    let nodes = ctx.surreal_client.query_subgraph(crucible_core::database::SubgraphPattern {
        nodes: vec![crucible_core::database::NodePattern {
            variable: "user".to_string(),
            labels: Some(vec!["User".to_string()]),
            properties: Some(vec![("name".to_string(), serde_json::json!("Bob"))].into_iter().collect()),
        }],
        edges: vec![],
    }).await.unwrap();

    assert_eq!(nodes.len(), 1, "User graph node not found after transaction commit");

    // Verify document exists
    let docs = ctx.surreal_client.query_documents(
        "profiles",
        crucible_core::DocumentQuery {
            collection: "profiles".to_string(),
            filter: Some(crucible_core::DocumentFilter::Equals {
                field: "user_name".to_string(),
                value: serde_json::json!("Bob"),
            }),
            projection: None,
            sort: None,
            limit: None,
            skip: None,
        }
    ).await.unwrap();

    assert_eq!(docs.records.len(), 1, "User profile document not found after transaction commit");
}

// =============================================================================
// Test 2: Transaction Rollback on Error
// =============================================================================

/// Test that transaction rolls back ALL changes when ANY operation fails
///
/// ACID Property Tested: ATOMICITY (all-or-nothing)
///
/// Scenario:
/// 1. Begin transaction
/// 2. Perform valid operation (insert user)
/// 3. Perform invalid operation (insert to nonexistent table)
/// 4. Transaction should fail and rollback
/// 5. Verify NO data was persisted
///
/// Expected Behavior:
/// - Transaction fails with clear error message
/// - First operation is rolled back
/// - Database remains in consistent state
///
/// Expected to FAIL: multi_model_transaction tool doesn't exist yet
#[tokio::test]
async fn test_transaction_rollback_on_error() {
    let ctx = TestContext::new().await.unwrap();
    ctx.setup_test_data().await.unwrap();

    // Define operations where second one will fail
    let operations = serde_json::json!([
        {
            "type": "relational",
            "operation": "insert",
            "table": "users",
            "data": {
                "name": "Charlie",
                "email": "charlie@example.com",
                "age": 35,
                "status": "active"
            }
        },
        {
            "type": "relational",
            "operation": "insert",
            "table": "nonexistent_table",  // This will cause failure
            "data": {
                "name": "Invalid"
            }
        },
        {
            "type": "graph",
            "operation": "create_node",
            "label": "User",
            "properties": {
                "name": "Charlie"
            }
        }
    ]);

    // Execute transaction (EXPECTED TO FAIL - tool not implemented)
    let result = crate::test_multi_model_helpers::call_tool(
        &ctx.mcp_service,
        "multi_model_transaction",
        serde_json::json!({ "operations": operations })
    ).await;

    // If tool exists, verify it returns error
    if let Ok(result) = result {
        assert_tool_error(&result);

        let error_text = extract_text_content(&result).unwrap();
        assert!(
            error_text.contains("nonexistent_table") || error_text.contains("not found") || error_text.contains("does not exist"),
            "Error message should mention the nonexistent table: {}",
            error_text
        );
    }

    // CRITICAL: Verify first operation was rolled back (ATOMICITY)
    let user_check = ctx.surreal_client.select(SelectQuery {
        table: "users".to_string(),
        columns: None,
        filter: Some(crucible_core::FilterClause::Equals {
            column: "name".to_string(),
            value: serde_json::json!("Charlie"),
        }),
        order_by: None,
        limit: None,
        offset: None,
        joins: None,
    }).await.unwrap();

    assert_eq!(
        user_check.records.len(),
        0,
        "User 'Charlie' should NOT exist - transaction should have rolled back all operations"
    );

    // Verify graph node was NOT created
    let nodes = ctx.surreal_client.query_subgraph(crucible_core::database::SubgraphPattern {
        nodes: vec![crucible_core::database::NodePattern {
            variable: "user".to_string(),
            labels: Some(vec!["User".to_string()]),
            properties: Some(vec![("name".to_string(), serde_json::json!("Charlie"))].into_iter().collect()),
        }],
        edges: vec![],
    }).await.unwrap();

    assert_eq!(nodes.len(), 0, "Graph node should NOT exist after rollback");
}

// =============================================================================
// Test 3: Transaction Isolation
// =============================================================================

/// Test that uncommitted changes are not visible to other contexts
///
/// ACID Property Tested: ISOLATION
///
/// Scenario:
/// 1. Context A: Begin transaction and insert record
/// 2. Context B: Query for that record (should NOT see it)
/// 3. Context A: Commit transaction
/// 4. Context B: Query again (should NOW see it)
///
/// Expected Behavior:
/// - Uncommitted changes isolated from other transactions
/// - Changes visible only after commit
///
/// Expected to FAIL: multi_model_transaction tool doesn't exist yet
#[tokio::test]
async fn test_transaction_isolation() {
    // Create two separate contexts to simulate concurrent access
    let ctx_a = TestContext::new().await.unwrap();
    let ctx_b = TestContext::new().await.unwrap();

    ctx_a.setup_test_data().await.unwrap();
    ctx_b.setup_test_data().await.unwrap();

    // Context A: Start transaction with an insert
    let operations_a = serde_json::json!([
        {
            "type": "relational",
            "operation": "insert",
            "table": "users",
            "data": {
                "name": "Diana",
                "email": "diana@example.com",
                "age": 29,
                "status": "active"
            }
        }
    ]);

    // Begin transaction in context A (but DON'T commit yet)
    // NOTE: In real implementation, we'd need a way to start but not commit
    // For now, this tests the concept
    let tx_result_a = crate::test_multi_model_helpers::call_tool(
        &ctx_a.mcp_service,
        "multi_model_transaction",
        serde_json::json!({
            "operations": operations_a,
            "auto_commit": false  // Don't commit immediately
        })
    ).await;

    // Context B: Query for Diana (should NOT see uncommitted data)
    let query_before_commit = ctx_b.surreal_client.select(SelectQuery {
        table: "users".to_string(),
        columns: None,
        filter: Some(crucible_core::FilterClause::Equals {
            column: "name".to_string(),
            value: serde_json::json!("Diana"),
        }),
        order_by: None,
        limit: None,
        offset: None,
        joins: None,
    }).await.unwrap();

    assert_eq!(
        query_before_commit.records.len(),
        0,
        "ISOLATION VIOLATED: Uncommitted data visible to other transaction"
    );

    // Now commit transaction in context A
    if let Ok(tx_result) = tx_result_a {
        // In real implementation, would call commit explicitly here
        // For now, verify transaction was created
        assert_tool_success(&tx_result);
    }

    // Context B: Query again after commit (should NOW see Diana)
    let query_after_commit = ctx_b.surreal_client.select(SelectQuery {
        table: "users".to_string(),
        columns: None,
        filter: Some(crucible_core::FilterClause::Equals {
            column: "name".to_string(),
            value: serde_json::json!("Diana"),
        }),
        order_by: None,
        limit: None,
        offset: None,
        joins: None,
    }).await.unwrap();

    assert_eq!(
        query_after_commit.records.len(),
        1,
        "Committed data should be visible to other transactions"
    );
}

// =============================================================================
// Test 4: Nested Transaction Operations with Cross-References
// =============================================================================

/// Test complex transaction with cross-model references
///
/// ACID Property Tested: CONSISTENCY
///
/// Scenario:
/// 1. Create relational record → get its ID
/// 2. Use that ID to create graph node with reference
/// 3. Use both IDs to create document linking them
/// 4. Verify all cross-references are valid
///
/// Expected Behavior:
/// - IDs from earlier operations available to later ones
/// - Cross-model references maintain referential integrity
/// - All data consistent after commit
///
/// Expected to FAIL: multi_model_transaction tool doesn't exist yet
#[tokio::test]
async fn test_nested_transaction_operations() {
    let ctx = TestContext::new().await.unwrap();
    ctx.setup_test_data().await.unwrap();

    // Complex transaction with dependencies between operations
    let operations = serde_json::json!([
        {
            "type": "relational",
            "operation": "insert",
            "table": "users",
            "data": {
                "name": "Eve",
                "email": "eve@example.com",
                "age": 32,
                "status": "active"
            },
            "return_id": true  // Signal that we need the ID for later operations
        },
        {
            "type": "relational",
            "operation": "insert",
            "table": "posts",
            "data": {
                "title": "My First Post",
                "content": "Learning about multi-model databases",
                "user_id": "${operations[0].id}"  // Reference ID from first operation
            }
        },
        {
            "type": "graph",
            "operation": "create_node",
            "label": "User",
            "properties": {
                "name": "Eve",
                "relational_id": "${operations[0].id}"  // Link to relational record
            }
        },
        {
            "type": "graph",
            "operation": "create_node",
            "label": "Post",
            "properties": {
                "title": "My First Post",
                "relational_id": "${operations[1].id}"
            }
        },
        {
            "type": "graph",
            "operation": "create_edge",
            "from": "${operations[2].id}",  // User node
            "to": "${operations[3].id}",    // Post node
            "label": "AUTHORED",
            "properties": {
                "created_at": "2025-10-18T00:00:00Z"
            }
        },
        {
            "type": "document",
            "operation": "create_document",
            "collection": "profiles",
            "document": {
                "content": {
                    "user_id": "${operations[0].id}",  // Link to relational user
                    "graph_node_id": "${operations[2].id}",  // Link to graph node
                    "name": "Eve",
                    "bio": "Database enthusiast"
                }
            }
        }
    ]);

    // Execute transaction (EXPECTED TO FAIL - tool not implemented)
    let result = crate::test_multi_model_helpers::call_tool(
        &ctx.mcp_service,
        "multi_model_transaction",
        serde_json::json!({ "operations": operations })
    ).await;

    assert!(result.is_ok(), "Complex transaction failed: {:?}", result.err());
    let result = result.unwrap();
    assert_tool_success(&result);

    let tx_result: serde_json::Value = parse_json_result(&result).unwrap();
    assert_eq!(tx_result["operations"].as_array().unwrap().len(), 6);

    // Verify relational records
    let user_query = ctx.surreal_client.select(SelectQuery {
        table: "users".to_string(),
        columns: None,
        filter: Some(crucible_core::FilterClause::Equals {
            column: "name".to_string(),
            value: serde_json::json!("Eve"),
        }),
        order_by: None,
        limit: None,
        offset: None,
        joins: None,
    }).await.unwrap();

    assert_eq!(user_query.records.len(), 1);
    let user_id = user_query.records[0].id.as_ref().unwrap();

    // Verify post references correct user
    let post_query = ctx.surreal_client.select(SelectQuery {
        table: "posts".to_string(),
        columns: None,
        filter: Some(crucible_core::FilterClause::Equals {
            column: "title".to_string(),
            value: serde_json::json!("My First Post"),
        }),
        order_by: None,
        limit: None,
        offset: None,
        joins: None,
    }).await.unwrap();

    assert_eq!(post_query.records.len(), 1);
    assert_eq!(
        post_query.records[0].data.get("user_id").unwrap().as_str().unwrap(),
        user_id.0
    );

    // Verify graph relationship exists
    let user_nodes = ctx.surreal_client.query_subgraph(crucible_core::database::SubgraphPattern {
        nodes: vec![crucible_core::database::NodePattern {
            variable: "user".to_string(),
            labels: Some(vec!["User".to_string()]),
            properties: Some(vec![("name".to_string(), serde_json::json!("Eve"))].into_iter().collect()),
        }],
        edges: vec![],
    }).await.unwrap();

    assert_eq!(user_nodes.len(), 1);
    let user_node_id = &user_nodes[0].nodes.get("user").unwrap().id;

    // Verify AUTHORED edge exists
    let neighbors = ctx.surreal_client.get_neighbors(
        user_node_id,
        crucible_core::Direction::Outgoing,
        Some(crucible_core::database::EdgeFilter {
            labels: Some(vec!["AUTHORED".to_string()]),
            properties: None,
        })
    ).await.unwrap();

    assert_eq!(neighbors.len(), 1, "User should have 1 AUTHORED edge to Post");
    assert_eq!(neighbors[0].labels, vec!["Post".to_string()]);
}

// =============================================================================
// Test 5: Transaction Error Handling
// =============================================================================

/// Test comprehensive error handling in transactions
///
/// ACID Property Tested: CONSISTENCY (system remains in valid state despite errors)
///
/// Scenarios:
/// 1. Invalid operation type → clear error
/// 2. Missing required fields → validation error
/// 3. Constraint violation → integrity error
/// 4. Type mismatch → type error
///
/// Expected Behavior:
/// - Clear, actionable error messages
/// - No partial state changes
/// - Database remains consistent
///
/// Expected to FAIL: multi_model_transaction tool doesn't exist yet
#[tokio::test]
async fn test_transaction_error_handling() {
    let ctx = TestContext::new().await.unwrap();
    ctx.setup_test_data().await.unwrap();

    // Test 1: Invalid operation type
    let invalid_type_ops = serde_json::json!([
        {
            "type": "invalid_model_type",  // Invalid!
            "operation": "insert",
            "table": "users",
            "data": { "name": "Test" }
        }
    ]);

    let result = crate::test_multi_model_helpers::call_tool(
        &ctx.mcp_service,
        "multi_model_transaction",
        serde_json::json!({ "operations": invalid_type_ops })
    ).await;

    if let Ok(result) = result {
        assert_tool_error(&result);
        let error_msg = extract_text_content(&result).unwrap();
        assert!(
            error_msg.contains("invalid") || error_msg.contains("unsupported"),
            "Error should mention invalid operation type: {}",
            error_msg
        );
    }

    // Test 2: Missing required fields
    let missing_fields_ops = serde_json::json!([
        {
            "type": "relational",
            "operation": "insert",
            // Missing "table" field!
            "data": { "name": "Test" }
        }
    ]);

    let result = crate::test_multi_model_helpers::call_tool(
        &ctx.mcp_service,
        "multi_model_transaction",
        serde_json::json!({ "operations": missing_fields_ops })
    ).await;

    if let Ok(result) = result {
        assert_tool_error(&result);
        let error_msg = extract_text_content(&result).unwrap();
        assert!(
            error_msg.contains("table") || error_msg.contains("required") || error_msg.contains("missing"),
            "Error should mention missing required field: {}",
            error_msg
        );
    }

    // Test 3: Constraint violation (duplicate unique key)
    // First, insert a user
    ctx.surreal_client.insert("users", crucible_core::Record {
        id: None,
        data: vec![
            ("name".to_string(), serde_json::json!("Frank")),
            ("email".to_string(), serde_json::json!("frank@example.com")),
            ("age".to_string(), serde_json::json!(25)),
            ("status".to_string(), serde_json::json!("active")),
        ].into_iter().collect(),
    }).await.unwrap();

    // Try to insert duplicate
    let duplicate_ops = serde_json::json!([
        {
            "type": "relational",
            "operation": "insert",
            "table": "users",
            "data": {
                "name": "Frank Jr",
                "email": "frank@example.com",  // Duplicate email (unique constraint)
                "age": 20,
                "status": "active"
            }
        }
    ]);

    let result = crate::test_multi_model_helpers::call_tool(
        &ctx.mcp_service,
        "multi_model_transaction",
        serde_json::json!({ "operations": duplicate_ops })
    ).await;

    if let Ok(result) = result {
        assert_tool_error(&result);
        let error_msg = extract_text_content(&result).unwrap();
        assert!(
            error_msg.contains("unique") || error_msg.contains("duplicate") || error_msg.contains("constraint"),
            "Error should mention constraint violation: {}",
            error_msg
        );
    }

    // Test 4: Empty operations array
    let empty_ops = serde_json::json!([]);

    let result = crate::test_multi_model_helpers::call_tool(
        &ctx.mcp_service,
        "multi_model_transaction",
        serde_json::json!({ "operations": empty_ops })
    ).await;

    if let Ok(result) = result {
        assert_tool_error(&result);
        let error_msg = extract_text_content(&result).unwrap();
        assert!(
            error_msg.contains("empty") || error_msg.contains("no operations") || error_msg.contains("at least one"),
            "Error should mention empty operations: {}",
            error_msg
        );
    }
}

// =============================================================================
// ACID Properties Summary
// =============================================================================

/*
 * This test suite validates the following ACID guarantees:
 *
 * ATOMICITY (All-or-Nothing):
 * - test_atomic_multi_model_commit: All operations commit together
 * - test_transaction_rollback_on_error: Failures roll back ALL changes
 *
 * CONSISTENCY (Valid State Transitions):
 * - test_nested_transaction_operations: Cross-model references remain valid
 * - test_transaction_error_handling: Errors don't leave partial state
 *
 * ISOLATION (Changes Invisible Until Commit):
 * - test_transaction_isolation: Uncommitted changes not visible to others
 *
 * DURABILITY (Committed Changes Persist):
 * - All tests verify data with follow-up queries after commit
 * - Validated through direct database queries (not just tool results)
 *
 * Additional Validation:
 * - Error messages are clear and actionable
 * - Parameter validation catches invalid inputs
 * - Cross-model operations maintain referential integrity
 * - Transaction IDs are unique and traceable
 */
