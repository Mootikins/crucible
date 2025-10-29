//! Tests for the simplified crucible-tools architecture
//!
//! This module contains tests for the Phase 1.1 simplified architecture
//! that focuses on simple async function composition instead of complex
//! enterprise patterns.

#[cfg(test)]
mod tests {
    use crate::types::{ToolDefinition, ToolExecutionContext, ToolResult};
    use serde_json::json;

    // Import Phase 4.1 API validation tests
    mod phase4_api_validation;

    #[test]
    fn test_tool_execution_context_creation() {
        let context = ToolExecutionContext::default();
        assert!(context.user_id.is_none());
        assert!(context.session_id.is_none());
    }

    #[test]
    fn test_tool_definition_creation() {
        let tool = ToolDefinition {
            name: "test_tool".to_string(),
            description: "Test tool for verification".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
            enabled: true,
        };

        assert_eq!(tool.name, "test_tool");
        assert_eq!(tool.description, "Test tool for verification");
        assert!(tool.enabled);
    }

    #[test]
    fn test_simple_tool_result() {
        let result = ToolResult {
            success: true,
            data: Some(serde_json::json!({"test": "data"})),
            error: None,
            duration_ms: 100,
            tool_name: "test_tool".to_string(),
        };

        assert!(result.success);
        assert!(result.error.is_none());
        assert!(result.data.is_some());
        assert_eq!(result.duration_ms, 100);
    }

    #[test]
    fn test_library_info() {
        let info = crate::library_info();
        assert_eq!(info.name, "crucible-tools");
        assert!(info.features.contains(&"simple_composition".to_string()));
        assert!(info
            .features
            .contains(&"direct_async_functions".to_string()));
        assert!(info.features.contains(&"unified_interface".to_string()));
        assert!(info.features.contains(&"25_tools_registered".to_string()));
    }

    #[test]
    fn test_version_constant() {
        let version = crate::VERSION;
        assert!(!version.is_empty());
    }

    #[test]
    fn test_phase21_context_simplification() {
        // Test the simplified context creation methods
        let context1 = ToolExecutionContext::with_user_session(
            Some("user123".to_string()),
            Some("session456".to_string()),
        );
        assert_eq!(context1.user_id, Some("user123".to_string()));
        assert_eq!(context1.session_id, Some("session456".to_string()));

        let context2 = ToolExecutionContext::with_working_dir("/tmp".to_string());
        assert_eq!(context2.working_directory, Some("/tmp".to_string()));

        let context3 = context2.with_env("TEST_VAR".to_string(), "test_value".to_string());
        assert!(context3.environment.contains_key("TEST_VAR"));
        assert_eq!(
            context3.environment.get("TEST_VAR"),
            Some(&"test_value".to_string())
        );
    }

    #[tokio::test]
    async fn test_tool_registry() {
        // Test that the simplified tool registry works
        crate::load_all_tools().await.unwrap();
        let tools = crate::list_registered_tools().await;
        assert!(!tools.is_empty()); // Should have 25 tools loaded

        // Test that we have the expected number of tools
        assert_eq!(tools.len(), 25); // 5 system + 8 kiln + 7 database + 5 search tools

        // Test library info includes current features
        let info = crate::library_info();
        assert!(info.features.contains(&"simple_composition".to_string()));
        assert!(info
            .features
            .contains(&"direct_tool_registration".to_string()));
        assert!(info.features.contains(&"25_tools_registered".to_string()));
    }

    #[tokio::test]
    async fn test_tool_loader() {
        // Test that the tool loader works
        let loader_info = crate::tool_loader_info();
        assert_eq!(loader_info.version, "3.2");
        assert_eq!(loader_info.total_tools, 25);

        // Load all tools
        crate::load_all_tools().await.unwrap();

        // Verify tools are registered
        let tools = crate::list_registered_tools().await;
        assert!(!tools.is_empty());
        assert_eq!(tools.len(), 25); // Should have all 25 tools

        // Test library info includes current features
        let info = crate::library_info();
        assert!(info.features.contains(&"simple_composition".to_string()));
        assert!(info
            .features
            .contains(&"direct_async_functions".to_string()));
        assert!(info.features.contains(&"database_tools".to_string()));
        assert!(info.features.contains(&"search_tools".to_string()));
        assert!(info.features.contains(&"kiln_tools".to_string()));
        assert!(info.features.contains(&"system_tools".to_string()));

        // Verify specific tools are registered
        assert!(tools.contains(&"system_info".to_string()));
        assert!(tools.contains(&"create_note".to_string()));
        assert!(tools.contains(&"semantic_search".to_string()));
        assert!(tools.contains(&"search_documents".to_string()));
    }

    #[tokio::test]
    async fn test_tool_execution() {
        // Load all tools first
        crate::load_all_tools().await.unwrap();

        // Test execution of a few representative tools from each category

        // Test system tool
        let result = crate::execute_tool("system_info".to_string(), json!({}), None, None)
            .await
            .unwrap();
        assert!(result.success);
        assert!(result.data.is_some());

        // Test get_environment tool (should work on any system)
        let result = crate::execute_tool("get_environment".to_string(), json!({}), None, None)
            .await
            .unwrap();
        assert!(result.success);
        assert!(result.data.is_some());

        // Verify tool count by category
        let tools = crate::list_registered_tools().await;

        let system_tools = vec![
            "system_info",
            "execute_command",
            "list_files",
            "read_file",
            "get_environment",
        ];
        let kiln_tools = vec![
            "search_by_properties",
            "search_by_tags",
            "search_by_folder",
            "create_note",
            "update_note",
            "delete_note",
            "get_kiln_stats",
            "list_tags",
        ];
        let database_tools = vec![
            "semantic_search",
            "search_by_content",
            "search_by_filename",
            "update_note_properties",
            "index_document",
            "get_document_stats",
            "sync_metadata",
        ];
        let search_tools = vec![
            "search_documents",
            "rebuild_index",
            "get_index_stats",
            "optimize_index",
            "advanced_search",
        ];

        for tool in &system_tools {
            assert!(
                tools.contains(&tool.to_string()),
                "Missing system tool: {}",
                tool
            );
        }
        for tool in &kiln_tools {
            assert!(
                tools.contains(&tool.to_string()),
                "Missing kiln tool: {}",
                tool
            );
        }
        for tool in &database_tools {
            assert!(
                tools.contains(&tool.to_string()),
                "Missing database tool: {}",
                tool
            );
        }
        for tool in &search_tools {
            assert!(
                tools.contains(&tool.to_string()),
                "Missing search tool: {}",
                tool
            );
        }

        assert_eq!(tools.len(), 25, "Should have exactly 25 tools total");
    }
}
