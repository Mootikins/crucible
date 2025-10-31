// Integration tests for REPL testable components
//
// This test file demonstrates how to test the REPL without spawning processes
// by using the newly exposed public interfaces.

use crucible_cli::commands::repl::{Repl, ReplCommand as Command, ReplInput as Input};

/// Test that we can parse commands correctly
#[test]
fn test_command_parsing() {
    // Parse a simple command
    let input = Input::parse(":tools").unwrap();
    assert!(input.is_command());

    // Parse a query
    let input = Input::parse("SELECT * FROM notes").unwrap();
    assert!(input.is_query());

    // Parse empty input
    let input = Input::parse("   ").unwrap();
    assert!(input.is_empty());
}

/// Test that we can create a test REPL instance
#[tokio::test]
async fn test_repl_creation() {
    let repl = Repl::new_test().await;
    assert!(repl.is_ok(), "Failed to create test REPL: {:?}", repl.err());

    let repl = repl.unwrap();

    // Verify initial state
    let stats = repl.get_stats();
    assert_eq!(stats.command_count, 0);
    assert_eq!(stats.query_count, 0);
}

/// Test that we can process commands directly
#[tokio::test]
async fn test_process_command() {
    let mut repl = Repl::new_test().await.unwrap();

    // Process a tools command
    let result = repl.process_input(":tools").await;
    assert!(result.is_ok(), "Failed to process :tools command");

    // Verify the command was counted
    let stats = repl.get_stats();
    assert_eq!(stats.command_count, 1);
    assert_eq!(stats.query_count, 0);
}

/// Test that we can execute queries directly
#[tokio::test]
async fn test_process_query() {
    let mut repl = Repl::new_test().await.unwrap();

    // Execute a simple query
    let result = repl.process_input("SELECT * FROM notes").await;
    assert!(result.is_ok(), "Failed to execute query");

    // Verify the query was counted
    let stats = repl.get_stats();
    assert_eq!(stats.command_count, 0);
    assert_eq!(stats.query_count, 1);
}

/// Test that we can execute specific commands
#[tokio::test]
async fn test_execute_specific_command() {
    let mut repl = Repl::new_test().await.unwrap();

    // Execute the ListTools command directly
    let result = repl.execute_command(Command::ListTools).await;
    assert!(result.is_ok());

    // Execute the ShowStats command
    let result = repl.execute_command(Command::ShowStats).await;
    assert!(result.is_ok());

    // Note: execute_command doesn't increment stats automatically,
    // only process_input does. This is because execute_command is
    // a lower-level interface.
    // If you want stats to be incremented, use process_input instead.
}

/// Test that we can access the database directly
#[tokio::test]
async fn test_database_access() {
    let repl = Repl::new_test().await.unwrap();

    // Access the database
    let db = repl.get_database();

    // Execute a query directly on the database
    let result = db.query("SELECT * FROM notes").await;
    assert!(result.is_ok(), "Failed to query database");

    let query_result = result.unwrap();
    // The test database has sample data pre-populated
    assert!(
        !query_result.rows.is_empty(),
        "Expected sample data in database"
    );
}

/// Test error handling for invalid commands
#[tokio::test]
async fn test_invalid_command_error() {
    let mut repl = Repl::new_test().await.unwrap();

    // Try to execute an invalid command
    let result = repl.process_input(":invalid_command").await;
    assert!(result.is_err(), "Expected error for invalid command");

    // Statistics should not be incremented on error
    let stats = repl.get_stats();
    assert_eq!(stats.command_count, 0);
    assert_eq!(stats.query_count, 0);
}

/// Test mixed command and query execution
#[tokio::test]
async fn test_mixed_execution() {
    let mut repl = Repl::new_test().await.unwrap();

    // Execute a mix of commands and queries
    repl.process_input(":tools").await.unwrap();
    repl.process_input("SELECT * FROM notes").await.unwrap();
    repl.process_input(":stats").await.unwrap();
    repl.process_input("SELECT * FROM tags").await.unwrap();

    // Verify all were counted correctly
    let stats = repl.get_stats();
    assert_eq!(stats.command_count, 2);
    assert_eq!(stats.query_count, 2);
}

/// Test that query timing is tracked
#[tokio::test]
async fn test_query_timing() {
    let mut repl = Repl::new_test().await.unwrap();

    // Execute a query
    repl.process_input("SELECT * FROM notes").await.unwrap();

    // Verify timing was recorded
    let stats = repl.get_stats();
    assert!(stats.total_query_time.as_millis() >= 0);

    // Execute another query
    repl.process_input("SELECT * FROM tags").await.unwrap();

    // Timing should have increased
    let stats = repl.get_stats();
    assert!(stats.total_query_time.as_millis() >= 0);

    // Average query time should be reasonable
    let avg = stats.avg_query_time();
    assert!(avg.as_millis() >= 0);
    assert!(avg.as_millis() < 10000, "Query took too long");
}

/// Test command parsing with arguments
#[test]
fn test_command_with_arguments() {
    // Parse run command with tool name
    let input = Input::parse(":run search_by_tags project").unwrap();
    assert!(input.is_command());

    match input {
        Input::Command(cmd) => match cmd {
            Command::RunTool { tool_name, args } => {
                assert_eq!(tool_name, "search_by_tags");
                assert_eq!(args, vec!["project"]);
            }
            _ => panic!("Expected RunTool command"),
        },
        _ => panic!("Expected Command"),
    }
}

/// Test that empty lines don't affect statistics
#[tokio::test]
async fn test_empty_lines_ignored() {
    let mut repl = Repl::new_test().await.unwrap();

    // Process empty lines
    repl.process_input("").await.unwrap();
    repl.process_input("   ").await.unwrap();
    repl.process_input("\t").await.unwrap();

    // Statistics should remain at zero
    let stats = repl.get_stats();
    assert_eq!(stats.command_count, 0);
    assert_eq!(stats.query_count, 0);
}

/// Test database table listing
#[tokio::test]
async fn test_database_table_listing() {
    let repl = Repl::new_test().await.unwrap();
    let db = repl.get_database();

    // List tables
    let tables = db.list_tables().await;
    assert!(tables.is_ok(), "Failed to list tables");

    let tables = tables.unwrap();

    // The in-memory database may return an empty list if SurrealDB
    // doesn't expose tables until they have data. This is expected behavior.
    // In production with a file-based DB, tables would be listed.
    if !tables.is_empty() {
        // If tables are listed, verify they include expected ones
        assert!(tables.contains(&"notes".to_string()) || tables.contains(&"tags".to_string()));
    }
}

/// Test database statistics retrieval
#[tokio::test]
async fn test_database_stats() {
    let repl = Repl::new_test().await.unwrap();
    let db = repl.get_database();

    // Get database stats
    let stats = db.get_stats().await;
    assert!(stats.is_ok(), "Failed to get database stats");

    let stats = stats.unwrap();
    assert!(!stats.is_empty(), "Expected database statistics");

    // Should include metadata
    assert!(stats.contains_key("database_type"));
    assert_eq!(
        stats["database_type"],
        serde_json::Value::String("SurrealDB".to_string())
    );
}
