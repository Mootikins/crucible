//! Comprehensive unit tests for the tool registry infrastructure
//!
//! This module tests the new tool registry implementation that was added in Phase 4.1,
//! including tool registration, discovery, dependency validation, and statistics.

use crucible_tools::registry::*;
use crucible_tools::types::{ToolCategory, ToolDependency};
use crucible_services::types::tool::*;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;

#[cfg(test)]
mod registry_basic_tests {
    use super::*;

    #[test]
    fn test_registry_creation() {
        let registry = ToolRegistry::new();

        assert_eq!(registry.tools.len(), 0);
        assert_eq!(registry.categories.len(), 0);
    }

    #[test]
    fn test_registry_default() {
        let registry = ToolRegistry::default();

        assert_eq!(registry.tools.len(), 0);
        assert_eq!(registry.categories.len(), 0);
    }

    #[test]
    fn test_single_tool_registration() {
        let mut registry = ToolRegistry::new();

        let tool = create_test_tool("test_tool", "Test Tool", "Testing");
        registry.register_tool(tool.clone());

        assert_eq!(registry.tools.len(), 1);
        assert!(registry.get_tool("test_tool").is_some());
        assert_eq!(registry.get_tool("test_tool").unwrap().name, "test_tool");
        assert_eq!(registry.get_tool("test_tool").unwrap().description, "Test Tool");
    }

    #[test]
    fn test_multiple_tool_registration() {
        let mut registry = ToolRegistry::new();

        let tools = vec![
            create_test_tool("tool_a", "Tool A", "System"),
            create_test_tool("tool_b", "Tool B", "Database"),
            create_test_tool("tool_c", "Tool C", "Network"),
        ];

        for tool in tools.iter() {
            registry.register_tool(tool.clone());
        }

        assert_eq!(registry.tools.len(), 3);
        assert_eq!(registry.categories.len(), 3);

        // Verify all tools are registered
        for tool in tools.iter() {
            assert!(registry.get_tool(&tool.name).is_some());
        }
    }

    #[test]
    fn test_tool_overwrite() {
        let mut registry = ToolRegistry::new();

        let tool_v1 = create_test_tool("versioned_tool", "Version 1", "Testing");
        registry.register_tool(tool_v1);

        let tool_v2 = ToolDefinition {
            name: "versioned_tool".to_string(),
            description: "Version 2 - Updated".to_string(),
            input_schema: json!({"type": "object"}),
            category: Some("Testing".to_string()),
            version: Some("2.0.0".to_string()),
            author: Some("Updated Author".to_string()),
            tags: vec!["updated".to_string()],
            enabled: true,
            parameters: vec![],
        };

        registry.register_tool(tool_v2);

        assert_eq!(registry.tools.len(), 1);
        let retrieved_tool = registry.get_tool("versioned_tool").unwrap();
        assert_eq!(retrieved_tool.description, "Version 2 - Updated");
        assert_eq!(retrieved_tool.version, Some("2.0.0".to_string()));
    }

    #[test]
    fn test_category_organization() {
        let mut registry = ToolRegistry::new();

        let system_tools = vec![
            create_test_tool("system_info", "System Info", "System"),
            create_test_tool("file_manager", "File Manager", "System"),
        ];

        let db_tools = vec![
            create_test_tool("db_query", "Database Query", "Database"),
            create_test_tool("db_migrate", "Database Migration", "Database"),
        ];

        for tool in system_tools.iter().chain(db_tools.iter()) {
            registry.register_tool(tool.clone());
        }

        assert_eq!(registry.categories.len(), 2);
        assert_eq!(registry.list_tools_by_category(&ToolCategory::System).len(), 2);
        assert_eq!(registry.list_tools_by_category(&ToolCategory::Database).len(), 2);
        assert_eq!(registry.list_tools_by_category(&ToolCategory::Network).len(), 0);
    }

    #[test]
    fn test_invalid_category_handling() {
        let mut registry = ToolRegistry::new();

        let tool_with_invalid_category = ToolDefinition {
            name: "invalid_category_tool".to_string(),
            description: "Tool with invalid category".to_string(),
            input_schema: json!({"type": "object"}),
            category: Some("InvalidCategory".to_string()), // This won't parse to ToolCategory
            version: None,
            author: None,
            tags: vec![],
            enabled: true,
            parameters: vec![],
        };

        registry.register_tool(tool_with_invalid_category);

        // Tool should be registered but not categorized
        assert_eq!(registry.tools.len(), 1);
        assert_eq!(registry.categories.len(), 0);
        assert!(registry.get_tool("invalid_category_tool").is_some());
    }

    #[test]
    fn test_list_tools() {
        let mut registry = ToolRegistry::new();

        let tools = vec![
            create_test_tool("alpha_tool", "Alpha", "System"),
            create_test_tool("beta_tool", "Beta", "Database"),
            create_test_tool("gamma_tool", "Gamma", "Network"),
        ];

        for tool in tools.iter() {
            registry.register_tool(tool.clone());
        }

        let all_tools = registry.list_tools();
        assert_eq!(all_tools.len(), 3);

        // Verify tool names (order may vary due to HashMap)
        let tool_names: Vec<&str> = all_tools.iter().map(|t| t.name.as_str()).collect();
        assert!(tool_names.contains(&"alpha_tool"));
        assert!(tool_names.contains(&"beta_tool"));
        assert!(tool_names.contains(&"gamma_tool"));
    }

    #[test]
    fn test_get_nonexistent_tool() {
        let registry = ToolRegistry::new();

        assert!(registry.get_tool("nonexistent_tool").is_none());
    }
}

#[cfg(test)]
mod dependency_tests {
    use super::*;

    #[test]
    fn test_no_dependencies() {
        let registry = ToolRegistry::new();

        let dependencies = registry.get_tool_dependencies("any_tool");
        assert!(dependencies.is_empty());
    }

    #[test]
    fn test_dependency_validation_no_deps() {
        let mut registry = ToolRegistry::new();

        let tool_no_deps = create_test_tool("no_deps_tool", "No Dependencies", "System");
        registry.register_tool(tool_no_deps);

        assert!(registry.validate_dependencies("no_deps_tool").is_ok());
    }

    #[test]
    fn test_dependency_validation_missing_required_dep() {
        let mut registry = ToolRegistry::new();

        let tool_with_deps = create_tool_with_dependencies(
            "dependent_tool",
            "Tool with Dependencies",
            "System",
            vec![
                ToolDependency {
                    name: "missing_dep".to_string(),
                    version: Some("1.0.0".to_string()),
                    optional: false,
                },
            ],
        );

        registry.register_tool(tool_with_deps);

        let result = registry.validate_dependencies("dependent_tool");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing required dependency: missing_dep"));
    }

    #[test]
    fn test_dependency_validation_missing_optional_dep() {
        let mut registry = ToolRegistry::new();

        let tool_with_optional_deps = create_tool_with_dependencies(
            "optional_dep_tool",
            "Tool with Optional Dependencies",
            "System",
            vec![
                ToolDependency {
                    name: "optional_dep".to_string(),
                    version: None,
                    optional: true,
                },
            ],
        );

        registry.register_tool(tool_with_optional_deps);

        // Should pass even though optional dependency is missing
        assert!(registry.validate_dependencies("optional_dep_tool").is_ok());
    }

    #[test]
    fn test_dependency_validation_satisfied_deps() {
        let mut registry = ToolRegistry::new();

        // Register dependency first
        let dependency_tool = create_test_tool("dependency_tool", "Dependency Tool", "System");
        registry.register_tool(dependency_tool);

        // Register tool that depends on it
        let dependent_tool = create_tool_with_dependencies(
            "dependent_tool",
            "Dependent Tool",
            "System",
            vec![
                ToolDependency {
                    name: "dependency_tool".to_string(),
                    version: None,
                    optional: false,
                },
            ],
        );

        registry.register_tool(dependent_tool);

        assert!(registry.validate_dependencies("dependent_tool").is_ok());
    }

    #[test]
    fn test_dependency_validation_multiple_deps() {
        let mut registry = ToolRegistry::new();

        // Register some dependencies
        let dep1 = create_test_tool("dep1", "Dependency 1", "System");
        let dep2 = create_test_tool("dep2", "Dependency 2", "Database");
        registry.register_tool(dep1);
        registry.register_tool(dep2);

        // Register tool with mixed dependencies
        let tool_with_mixed_deps = create_tool_with_dependencies(
            "mixed_deps_tool",
            "Tool with Mixed Dependencies",
            "System",
            vec![
                ToolDependency {
                    name: "dep1".to_string(),
                    version: None,
                    optional: false, // Required and present
                },
                ToolDependency {
                    name: "dep2".to_string(),
                    version: None,
                    optional: false, // Required and present
                },
                ToolDependency {
                    name: "missing_optional".to_string(),
                    version: None,
                    optional: true, // Optional and missing
                },
                ToolDependency {
                    name: "missing_required".to_string(),
                    version: None,
                    optional: false, // Required and missing
                },
            ],
        );

        registry.register_tool(tool_with_mixed_deps);

        let result = registry.validate_dependencies("mixed_deps_tool");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing required dependency: missing_required"));
        assert!(!result.unwrap_err().contains("missing_optional"));
    }

    #[test]
    fn test_dependency_validation_nonexistent_tool() {
        let registry = ToolRegistry::new();

        // Validating dependencies for a nonexistent tool should not panic
        let result = registry.validate_dependencies("nonexistent_tool");
        // Should succeed since there are no dependencies to validate
        assert!(result.is_ok());
    }
}

#[cfg(test)]
mod statistics_tests {
    use super::*;

    #[test]
    fn test_empty_registry_stats() {
        let registry = ToolRegistry::new();
        let stats = registry.get_stats();

        assert_eq!(stats.total_tools, 0);
        assert_eq!(stats.categories, 0);
        assert_eq!(stats.tools_with_dependencies, 0);
    }

    #[test]
    fn test_registry_stats_no_dependencies() {
        let mut registry = ToolRegistry::new();

        let tools = vec![
            create_test_tool("tool1", "Tool 1", "System"),
            create_test_tool("tool2", "Tool 2", "Database"),
            create_test_tool("tool3", "Tool 3", "Network"),
        ];

        for tool in tools.iter() {
            registry.register_tool(tool.clone());
        }

        let stats = registry.get_stats();
        assert_eq!(stats.total_tools, 3);
        assert_eq!(stats.categories, 3);
        assert_eq!(stats.tools_with_dependencies, 0);
    }

    #[test]
    fn test_registry_stats_with_dependencies() {
        let mut registry = ToolRegistry::new();

        // Register tools with dependencies
        let tool_with_deps1 = create_tool_with_dependencies(
            "dep_tool1",
            "Tool with Dependencies 1",
            "System",
            vec![ToolDependency {
                name: "some_dep".to_string(),
                version: None,
                optional: true,
            }],
        );

        let tool_with_deps2 = create_tool_with_dependencies(
            "dep_tool2",
            "Tool with Dependencies 2",
            "Database",
            vec![
                ToolDependency {
                    name: "dep1".to_string(),
                    version: None,
                    optional: false,
                },
                ToolDependency {
                    name: "dep2".to_string(),
                    version: None,
                    optional: true,
                },
            ],
        );

        let tool_no_deps = create_test_tool("no_deps", "No Dependencies", "Network");

        registry.register_tool(tool_with_deps1);
        registry.register_tool(tool_with_deps2);
        registry.register_tool(tool_no_deps);

        let stats = registry.get_stats();
        assert_eq!(stats.total_tools, 3);
        assert_eq!(stats.categories, 3);
        assert_eq!(stats.tools_with_dependencies, 2);
    }

    #[test]
    fn test_registry_stats_same_category() {
        let mut registry = ToolRegistry::new();

        let system_tools = vec![
            create_test_tool("sys_tool1", "System Tool 1", "System"),
            create_test_tool("sys_tool2", "System Tool 2", "System"),
            create_test_tool("sys_tool3", "System Tool 3", "System"),
        ];

        for tool in system_tools.iter() {
            registry.register_tool(tool.clone());
        }

        let stats = registry.get_stats();
        assert_eq!(stats.total_tools, 3);
        assert_eq!(stats.categories, 1); // All in the same category
        assert_eq!(stats.tools_with_dependencies, 0);
    }
}

#[cfg(test)]
mod registry_initialization_tests {
    use super::*;

    #[test]
    fn test_initialize_registry() {
        let registry = initialize_registry();

        // Should have built-in tools
        assert!(registry.tools.len() > 0);
        assert!(registry.categories.len() > 0);

        // Check for expected system tools
        assert!(registry.get_tool("system_info").is_some());
        assert!(registry.get_tool("file_list").is_some());

        // Check for expected vault tools
        assert!(registry.get_tool("vault_search").is_some());

        // Check for expected database tools
        assert!(registry.get_tool("database_query").is_some());

        // Check for expected search tools
        assert!(registry.get_tool("semantic_search").is_some());
    }

    #[test]
    fn test_system_tools_registration() {
        let mut registry = ToolRegistry::new();
        register_system_tools(&mut registry);

        assert!(registry.get_tool("system_info").is_some());
        assert!(registry.get_tool("file_list").is_some());

        let system_info = registry.get_tool("system_info").unwrap();
        assert_eq!(system_info.name, "system_info");
        assert_eq!(system_info.category, Some("System".to_string()));
        assert_eq!(system_info.version, Some("1.0.0".to_string()));
        assert!(system_info.tags.contains(&"system".to_string()));
        assert!(system_info.tags.contains(&"info".to_string()));

        let file_list = registry.get_tool("file_list").unwrap();
        assert_eq!(file_list.name, "file_list");
        assert_eq!(file_list.category, Some("System".to_string()));
        assert!(file_list.input_schema["properties"]["path"]["type"] == "string");
        assert!(file_list.input_schema["required"].as_array().unwrap().contains(&"path"));
    }

    #[test]
    fn test_vault_tools_registration() {
        let mut registry = ToolRegistry::new();
        register_vault_tools(&mut registry);

        assert!(registry.get_tool("vault_search").is_some());

        let vault_search = registry.get_tool("vault_search").unwrap();
        assert_eq!(vault_search.name, "vault_search");
        assert_eq!(vault_search.category, Some("Vault".to_string()));
        assert_eq!(vault_search.input_schema["required"].as_array().unwrap().contains(&"query"), true);
        assert!(vault_search.tags.contains(&"vault".to_string()));
        assert!(vault_search.tags.contains(&"search".to_string()));
    }

    #[test]
    fn test_database_tools_registration() {
        let mut registry = ToolRegistry::new();
        register_database_tools(&mut registry);

        assert!(registry.get_tool("database_query").is_some());

        let db_query = registry.get_tool("database_query").unwrap();
        assert_eq!(db_query.name, "database_query");
        assert_eq!(db_query.category, Some("Database".to_string()));
        assert_eq!(db_query.input_schema["required"].as_array().unwrap().contains(&"query"), true);
        assert!(db_query.tags.contains(&"database".to_string()));
        assert!(db_query.tags.contains(&"query".to_string()));
    }

    #[test]
    fn test_search_tools_registration() {
        let mut registry = ToolRegistry::new();
        register_search_tools(&mut registry);

        assert!(registry.get_tool("semantic_search").is_some());

        let semantic_search = registry.get_tool("semantic_search").unwrap();
        assert_eq!(semantic_search.name, "semantic_search");
        assert_eq!(semantic_search.category, Some("Search".to_string()));
        assert_eq!(semantic_search.input_schema["properties"]["limit"]["default"], 10);
        assert!(semantic_search.tags.contains(&"search".to_string()));
        assert!(semantic_search.tags.contains(&"semantic".to_string()));
    }

    #[test]
    fn test_all_categories_initialized() {
        let registry = initialize_registry();

        // Check that all expected categories are present
        let expected_categories = vec![
            ToolCategory::System,
            ToolCategory::Vault,
            ToolCategory::Database,
            ToolCategory::Search,
        ];

        for category in expected_categories {
            assert!(
                !registry.list_tools_by_category(&category).is_empty(),
                "Category {:?} should have tools",
                category
            );
        }
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    fn test_registry_lifecycle() {
        let mut registry = ToolRegistry::new();

        // Initial state
        assert_eq!(registry.tools.len(), 0);
        assert_eq!(registry.categories.len(), 0);

        // Add tools
        let tool1 = create_test_tool("lifecycle_tool1", "Lifecycle Tool 1", "System");
        let tool2 = create_test_tool("lifecycle_tool2", "Lifecycle Tool 2", "Database");

        registry.register_tool(tool1);
        assert_eq!(registry.tools.len(), 1);
        assert_eq!(registry.categories.len(), 1);

        registry.register_tool(tool2);
        assert_eq!(registry.tools.len(), 2);
        assert_eq!(registry.categories.len(), 2);

        // Verify tools are accessible
        assert!(registry.get_tool("lifecycle_tool1").is_some());
        assert!(registry.get_tool("lifecycle_tool2").is_some());

        // Check statistics
        let stats = registry.get_stats();
        assert_eq!(stats.total_tools, 2);
        assert_eq!(stats.categories, 2);
        assert_eq!(stats.tools_with_dependencies, 0);

        // List by category
        let system_tools = registry.list_tools_by_category(&ToolCategory::System);
        let db_tools = registry.list_tools_by_category(&ToolCategory::Database);
        assert_eq!(system_tools.len(), 1);
        assert_eq!(db_tools.len(), 1);
        assert_eq!(system_tools[0].name, "lifecycle_tool1");
        assert_eq!(db_tools[0].name, "lifecycle_tool2");
    }

    #[test]
    fn test_complex_dependency_graph() {
        let mut registry = ToolRegistry::new();

        // Create a dependency graph:
        // base_tool <- level1_tool <- level2_tool <- top_tool
        // standalone_tool

        let base_tool = create_test_tool("base_tool", "Base Tool", "System");
        let level1_tool = create_tool_with_dependencies(
            "level1_tool",
            "Level 1 Tool",
            "System",
            vec![ToolDependency {
                name: "base_tool".to_string(),
                version: None,
                optional: false,
            }],
        );

        let level2_tool = create_tool_with_dependencies(
            "level2_tool",
            "Level 2 Tool",
            "System",
            vec![ToolDependency {
                name: "level1_tool".to_string(),
                version: None,
                optional: false,
            }],
        );

        let top_tool = create_tool_with_dependencies(
            "top_tool",
            "Top Tool",
            "System",
            vec![
                ToolDependency {
                    name: "level2_tool".to_string(),
                    version: None,
                    optional: false,
                },
                ToolDependency {
                    name: "standalone_tool".to_string(),
                    version: None,
                    optional: true,
                },
            ],
        );

        let standalone_tool = create_test_tool("standalone_tool", "Standalone Tool", "Database");

        // Register in dependency order
        registry.register_tool(base_tool);
        registry.register_tool(standalone_tool);
        registry.register_tool(level1_tool);
        registry.register_tool(level2_tool);
        registry.register_tool(top_tool);

        // Validate all dependencies
        assert!(registry.validate_dependencies("base_tool").is_ok());
        assert!(registry.validate_dependencies("standalone_tool").is_ok());
        assert!(registry.validate_dependencies("level1_tool").is_ok());
        assert!(registry.validate_dependencies("level2_tool").is_ok());
        assert!(registry.validate_dependencies("top_tool").is_ok());

        // Check statistics
        let stats = registry.get_stats();
        assert_eq!(stats.total_tools, 5);
        assert_eq!(stats.categories, 2); // System and Database
        assert_eq!(stats.tools_with_dependencies, 3); // level1, level2, and top tools
    }

    #[test]
    fn test_registry_with_enabled_disabled_tools() {
        let mut registry = ToolRegistry::new();

        let enabled_tool = ToolDefinition {
            name: "enabled_tool".to_string(),
            description: "Enabled Tool".to_string(),
            input_schema: json!({"type": "object"}),
            category: Some("System".to_string()),
            version: None,
            author: None,
            tags: vec![],
            enabled: true,
            parameters: vec![],
        };

        let disabled_tool = ToolDefinition {
            name: "disabled_tool".to_string(),
            description: "Disabled Tool".to_string(),
            input_schema: json!({"type": "object"}),
            category: Some("System".to_string()),
            version: None,
            author: None,
            tags: vec![],
            enabled: false,
            parameters: vec![],
        };

        registry.register_tool(enabled_tool);
        registry.register_tool(disabled_tool);

        assert_eq!(registry.tools.len(), 2);
        assert_eq!(registry.list_tools().len(), 2);

        let enabled_retrieved = registry.get_tool("enabled_tool").unwrap();
        let disabled_retrieved = registry.get_tool("disabled_tool").unwrap();

        assert!(enabled_retrieved.enabled);
        assert!(!disabled_retrieved.enabled);
    }

    #[test]
    fn test_registry_arc_sharing() {
        let registry = Arc::new(initialize_registry());

        // Clone the Arc and use from multiple references
        let registry_clone1 = Arc::clone(&registry);
        let registry_clone2 = Arc::clone(&registry);

        // All references should see the same data
        assert_eq!(registry.tools.len(), registry_clone1.tools.len());
        assert_eq!(registry.tools.len(), registry_clone2.tools.len());

        // All should have the same built-in tools
        assert!(registry.get_tool("system_info").is_some());
        assert!(registry_clone1.get_tool("system_info").is_some());
        assert!(registry_clone2.get_tool("system_info").is_some());

        // Should be able to list tools from all references
        assert!(!registry.list_tools().is_empty());
        assert!(!registry_clone1.list_tools().is_empty());
        assert!(!registry_clone2.list_tools().is_empty());
    }

    #[test]
    fn test_registry_performance_with_many_tools() {
        let mut registry = ToolRegistry::new();
        let num_tools = 1000;

        let start = std::time::Instant::now();

        // Register many tools
        for i in 0..num_tools {
            let tool = create_test_tool(
                &format!("perf_tool_{}", i),
                &format!("Performance Tool {}", i),
                match i % 4 {
                    0 => "System",
                    1 => "Database",
                    2 => "Network",
                    _ => "Vault",
                },
            );
            registry.register_tool(tool);
        }

        let registration_time = start.elapsed();

        // Test lookup performance
        let lookup_start = std::time::Instant::now();
        for i in 0..num_tools {
            assert!(registry.get_tool(&format!("perf_tool_{}", i)).is_some());
        }
        let lookup_time = lookup_start.elapsed();

        // Test listing performance
        let list_start = std::time::Instant::now();
        let all_tools = registry.list_tools();
        let list_time = list_start.elapsed();

        // Verify results
        assert_eq!(registry.tools.len(), num_tools);
        assert_eq!(all_tools.len(), num_tools);
        assert_eq!(registry.categories.len(), 4);

        // Performance should be reasonable
        assert!(registration_time.as_millis() < 1000, "Registration too slow: {:?}", registration_time);
        assert!(lookup_time.as_millis() < 100, "Lookup too slow: {:?}", lookup_time);
        assert!(list_time.as_millis() < 50, "Listing too slow: {:?}", list_time);

        println!("Registry performance with {} tools:", num_tools);
        println!("  Registration: {:?}", registration_time);
        println!("  Lookup ({} ops): {:?}", num_tools, lookup_time);
        println!("  Listing: {:?}", list_time);
    }
}

// Helper functions for test data creation
fn create_test_tool(name: &str, description: &str, category: &str) -> ToolDefinition {
    ToolDefinition {
        name: name.to_string(),
        description: description.to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "input": {"type": "string"}
            },
            "required": ["input"]
        }),
        category: Some(category.to_string()),
        version: Some("1.0.0".to_string()),
        author: Some("Test Suite".to_string()),
        tags: vec!["test".to_string(), category.to_lowercase()],
        enabled: true,
        parameters: vec![],
    }
}

fn create_tool_with_dependencies(
    name: &str,
    description: &str,
    category: &str,
    dependencies: Vec<ToolDependency>,
) -> ToolDefinition {
    ToolDefinition {
        name: name.to_string(),
        description: description.to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "input": {"type": "string"}
            },
            "required": ["input"]
        }),
        category: Some(category.to_string()),
        version: Some("1.0.0".to_string()),
        author: Some("Test Suite".to_string()),
        tags: vec!["test".to_string(), category.to_lowercase(), "with-deps".to_string()],
        enabled: true,
        parameters: vec![],
    }
}