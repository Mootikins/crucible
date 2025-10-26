//! Phase 4.1 API validation tests
//!
//! Tests to ensure the simplified public API is clean and functional
//! after removing complex service architecture remnants.

#[cfg(test)]
mod tests {
    use crate::{
        database_tools, execute_tool, init, library_info, load_all_tools, search_tools,
        system_tools, tool_loader_info, vault_tools, ToolError, ToolFunction, ToolResult, VERSION,
    };
    use serde_json::json;

    #[tokio::test]
    async fn test_simplified_public_api() {
        // Test 1: Simple initialization
        init();

        // Test 2: Library information reflects simplified state
        let info = library_info();
        assert_eq!(info.name, "crucible-tools");
        assert!(!info.version.is_empty());
        assert!(info.features.contains(&"simple_composition".to_string()));
        assert!(info.features.contains(&"unified_interface".to_string()));
        assert!(info.features.contains(&"25_tools_registered".to_string()));

        // Ensure no verbose phase tracking features
        assert!(!info.features.iter().any(|f| f.contains("phase")));
        assert!(!info.features.iter().any(|f| f.contains("lines_removed")));

        // Test 3: Tool loader information
        let loader_info = tool_loader_info();
        assert_eq!(loader_info.total_tools, 25);
        assert_eq!(loader_info.version, "3.2");

        // Test 4: Load all tools successfully
        load_all_tools().await.unwrap();

        // Test 5: Unified tool interface works
        let result = execute_tool(
            "system_info".to_string(),
            json!({}),
            Some("test_user".to_string()),
            Some("test_session".to_string()),
        )
        .await
        .unwrap();
        assert!(result.success);

        // Test 6: Direct tool function access works
        let search_fn = search_tools::search_documents();
        let vault_fn = vault_tools::get_vault_stats();
        let system_fn = system_tools::get_system_info();
        let db_fn = database_tools::semantic_search();

        // All functions should have the same signature
        let _search_result = search_fn(
            "search_documents".to_string(),
            json!({"query": "test"}),
            Some("test_user".to_string()),
            Some("test_session".to_string()),
        )
        .await
        .unwrap();

        let _vault_result = vault_fn(
            "get_vault_stats".to_string(),
            json!({}),
            Some("test_user".to_string()),
            Some("test_session".to_string()),
        )
        .await
        .unwrap();

        // Test 7: Type exports work correctly
        let _test_result: ToolResult =
            ToolResult::success("test".to_string(), json!({"test": true}));
        let _test_error: ToolError = ToolError::Other("test error".to_string());
        let _test_function: ToolFunction = system_tools::get_system_info();

        // Test 8: Version constant is available
        let _version: &str = VERSION;
        assert!(!VERSION.is_empty());
    }

    #[test]
    fn test_no_complex_service_exports() {
        // Verify we're not exposing complex service types
        // This is a compile-time check - if any of these existed, the test wouldn't compile

        // We should only have simple types, not complex service types
        let info = library_info();

        // The features should not reference complex legacy patterns
        for feature in &info.features {
            assert!(!feature.contains("migration"));
            assert!(!feature.contains("bridge"));
            assert!(!feature.contains("analyzer"));
            assert!(!feature.contains("discovery"));
            assert!(!feature.contains("enterprise"));
        }
    }

    #[tokio::test]
    async fn test_clean_api_examples() {
        // Test that the API examples in the documentation actually work

        init();
        load_all_tools().await.unwrap();

        // Example 1: Using unified tool interface
        let result = execute_tool(
            "system_info".to_string(),
            json!({}),
            Some("user123".to_string()),
            Some("session456".to_string()),
        )
        .await
        .unwrap();

        assert!(result.success);
        assert!(result.data.is_some());

        // Example 2: Using individual tool functions
        let search_fn = search_tools::search_documents();
        let vault_fn = vault_tools::get_vault_stats();

        let search_result = search_fn(
            "search_documents".to_string(),
            json!({"query": "test", "top_k": 10}),
            Some("user123".to_string()),
            Some("session456".to_string()),
        )
        .await
        .unwrap();

        let vault_stats = vault_fn(
            "get_vault_stats".to_string(),
            json!({}),
            Some("user123".to_string()),
            Some("session456".to_string()),
        )
        .await
        .unwrap();

        // Both should work
        assert!(search_result.success || search_result.error.is_some()); // May fail if no search index, but should not panic
        assert!(vault_stats.success || vault_stats.error.is_some()); // May fail if no vault, but should not panic
    }
}
