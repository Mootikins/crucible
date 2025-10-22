//! # Plugin Dependency Resolver Tests
//!
//! Comprehensive unit tests for the plugin dependency resolver component.
//! Tests cover dependency graph construction, validation, circular dependency
//! detection, startup ordering, and performance under various scenarios.

use super::*;
use crate::plugin_manager::dependency_resolver::*;
use crate::plugin_manager::types::*;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

#[cfg(test)]
mod dependency_resolver_tests {
    use super::*;

    // ============================================================================
    // BASIC DEPENDENCY RESOLVER FUNCTIONALITY TESTS
    // ============================================================================

    #[tokio::test]
    async fn test_dependency_resolver_creation() {
        let resolver = create_test_dependency_resolver();

        // Verify initial state
        let graph = resolver.get_dependency_graph().await;
        assert_eq!(graph.node_count(), 0);
        assert_eq!(graph.edge_count(), 0);

        let stats = resolver.get_graph_stats().await;
        assert_eq!(stats.total_nodes, 0);
        assert_eq!(stats.total_edges, 0);
        assert_eq!(stats.circular_dependencies, 0);
    }

    #[tokio::test]
    async fn test_dependency_resolver_initialization() {
        let resolver = create_test_dependency_resolver();

        // Initialize resolver
        let result = resolver.initialize().await;
        assert!(result.is_ok());

        // Verify initialization
        let stats = resolver.get_graph_stats().await;
        assert_eq!(stats.total_nodes, 0);
    }

    // ============================================================================
    // DEPENDENCY GRAPH CONSTRUCTION TESTS
    // ============================================================================

    #[tokio::test]
    async fn test_add_plugin_dependencies() {
        let resolver = create_test_dependency_resolver();
        resolver.initialize().await.unwrap();

        // Add plugin with dependencies
        let dependencies = vec![
            PluginDependency {
                plugin_id: "database".to_string(),
                version_requirement: "^1.0.0".to_string(),
                required: true,
                health_required: false,
                startup_order: DependencyStartupOrder::Before,
            },
            PluginDependency {
                plugin_id: "cache".to_string(),
                version_requirement: "^2.0.0".to_string(),
                required: false,
                health_required: false,
                startup_order: DependencyStartupOrder::After,
            },
        ];

        let result = resolver.add_plugin_dependencies("web-server", &dependencies).await;
        assert!(result.is_ok());

        // Verify graph structure
        let graph = resolver.get_dependency_graph().await;
        assert_eq!(graph.node_count(), 3); // web-server + database + cache
        assert_eq!(graph.edge_count(), 2);

        let stats = resolver.get_graph_stats().await;
        assert_eq!(stats.total_nodes, 3);
        assert_eq!(stats.total_edges, 2);
    }

    #[tokio::test]
    async fn test_add_instance_dependencies() {
        let resolver = create_test_dependency_resolver();
        resolver.initialize().await.unwrap();

        // Add instance dependencies
        let instance_deps = vec![
            InstanceDependency {
                instance_id: "db-instance-1".to_string(),
                required: true,
                health_required: true,
                startup_order: DependencyStartupOrder::Before,
            },
        ];

        let result = resolver.add_instance_dependencies("web-instance-1", &instance_deps).await;
        assert!(result.is_ok());

        // Verify graph structure
        let graph = resolver.get_dependency_graph().await;
        assert_eq!(graph.node_count(), 2);
        assert_eq!(graph.edge_count(), 1);
    }

    #[tokio::test]
    async fn test_remove_plugin_dependencies() {
        let resolver = create_test_dependency_resolver();
        resolver.initialize().await.unwrap();

        // Add dependencies first
        let dependencies = vec![
            PluginDependency {
                plugin_id: "database".to_string(),
                version_requirement: "^1.0.0".to_string(),
                required: true,
                health_required: false,
                startup_order: DependencyStartupOrder::Before,
            },
        ];

        resolver.add_plugin_dependencies("web-server", &dependencies).await.unwrap();

        // Verify initial state
        let stats = resolver.get_graph_stats().await;
        assert_eq!(stats.total_nodes, 2);

        // Remove dependencies
        let result = resolver.remove_plugin_dependencies("web-server").await;
        assert!(result.is_ok());

        // Verify removal
        let stats = resolver.get_graph_stats().await;
        assert_eq!(stats.total_nodes, 0);
    }

    // ============================================================================
    // STARTUP ORDERING TESTS
    // ============================================================================

    #[tokio::test]
    async fn test_simple_startup_ordering() {
        let resolver = create_test_dependency_resolver();
        resolver.initialize().await.unwrap();

        // Create dependency chain: A -> B -> C
        resolver.add_plugin_dependencies("c", &[]).await.unwrap();
        resolver.add_plugin_dependencies("b", &[PluginDependency {
            plugin_id: "c".to_string(),
            version_requirement: "*".to_string(),
            required: true,
            health_required: false,
            startup_order: DependencyStartupOrder::Before,
        }]).await.unwrap();
        resolver.add_plugin_dependencies("a", &[PluginDependency {
            plugin_id: "b".to_string(),
            version_requirement: "*".to_string(),
            required: true,
            health_required: false,
            startup_order: DependencyStartupOrder::Before,
        }]).await.unwrap();

        // Get startup order
        let startup_order = resolver.get_startup_order(&["a", "b", "c"]).await.unwrap();

        // Verify order: C should start first, A should start last
        assert_eq!(startup_order[0], "c");
        assert_eq!(startup_order[1], "b");
        assert_eq!(startup_order[2], "a");
    }

    #[tokio::test]
    async fn test_complex_startup_ordering() {
        let resolver = create_test_dependency_resolver();
        resolver.initialize().await.unwrap();

        // Create complex dependency graph:
        // database -> (web-server, api-server)
        // cache -> web-server
        // auth -> api-server

        resolver.add_plugin_dependencies("database", &[]).await.unwrap();
        resolver.add_plugin_dependencies("cache", &[]).await.unwrap();
        resolver.add_plugin_dependencies("auth", &[]).await.unwrap();

        resolver.add_plugin_dependencies("web-server", &[
            PluginDependency {
                plugin_id: "database".to_string(),
                version_requirement: "*".to_string(),
                required: true,
                health_required: false,
                startup_order: DependencyStartupOrder::Before,
            },
            PluginDependency {
                plugin_id: "cache".to_string(),
                version_requirement: "*".to_string(),
                required: true,
                health_required: false,
                startup_order: DependencyStartupOrder::Before,
            },
        ]).await.unwrap();

        resolver.add_plugin_dependencies("api-server", &[
            PluginDependency {
                plugin_id: "database".to_string(),
                version_requirement: "*".to_string(),
                required: true,
                health_required: false,
                startup_order: DependencyStartupOrder::Before,
            },
            PluginDependency {
                plugin_id: "auth".to_string(),
                version_requirement: "*".to_string(),
                required: true,
                health_required: false,
                startup_order: DependencyStartupOrder::Before,
            },
        ]).await.unwrap();

        // Get startup order
        let startup_order = resolver.get_startup_order(&["web-server", "api-server", "database", "cache", "auth"]).await.unwrap();

        // Verify database comes before web-server and api-server
        let db_index = startup_order.iter().position(|p| p == "database").unwrap();
        let web_index = startup_order.iter().position(|p| p == "web-server").unwrap();
        let api_index = startup_order.iter().position(|p| p == "api-server").unwrap();

        assert!(db_index < web_index);
        assert!(db_index < api_index);

        // Verify cache comes before web-server
        let cache_index = startup_order.iter().position(|p| p == "cache").unwrap();
        assert!(cache_index < web_index);

        // Verify auth comes before api-server
        let auth_index = startup_order.iter().position(|p| p == "auth").unwrap();
        assert!(auth_index < api_index);
    }

    #[tokio::test]
    async fn test_parallel_startup_ordering() {
        let resolver = create_test_dependency_resolver();
        resolver.initialize().await.unwrap();

        // Create dependencies that allow parallel startup:
        // database -> (web-server, api-server)
        // web-server and api-server can start in parallel

        resolver.add_plugin_dependencies("database", &[]).await.unwrap();
        resolver.add_plugin_dependencies("web-server", &[PluginDependency {
            plugin_id: "database".to_string(),
            version_requirement: "*".to_string(),
            required: true,
            health_required: false,
            startup_order: DependencyStartupOrder::Before,
        }]).await.unwrap();
        resolver.add_plugin_dependencies("api-server", &[PluginDependency {
            plugin_id: "database".to_string(),
            version_requirement: "*".to_string(),
            required: true,
            health_required: false,
            startup_order: DependencyStartupOrder::Before,
        }]).await.unwrap();

        // Get parallel startup levels
        let startup_levels = resolver.get_parallel_startup_levels(&["web-server", "api-server", "database"]).await.unwrap();

        // Verify structure
        assert_eq!(startup_levels.len(), 2); // Two levels

        // First level should contain database
        assert_eq!(startup_levels[0].len(), 1);
        assert!(startup_levels[0].contains(&"database".to_string()));

        // Second level should contain web-server and api-server
        assert_eq!(startup_levels[1].len(), 2);
        assert!(startup_levels[1].contains(&"web-server".to_string()));
        assert!(startup_levels[1].contains(&"api-server".to_string()));
    }

    // ============================================================================
    // CIRCULAR DEPENDENCY DETECTION TESTS
    // ============================================================================

    #[tokio::test]
    async fn test_simple_circular_dependency_detection() {
        let resolver = create_test_dependency_resolver();
        resolver.initialize().await.unwrap();

        // Add A -> B
        resolver.add_plugin_dependencies("b", &[]).await.unwrap();
        resolver.add_plugin_dependencies("a", &[PluginDependency {
            plugin_id: "b".to_string(),
            version_requirement: "*".to_string(),
            required: true,
            health_required: false,
            startup_order: DependencyStartupOrder::Before,
        }]).await.unwrap();

        // Try to add B -> A (creates circular dependency)
        let result = resolver.add_plugin_dependencies("b", &[PluginDependency {
            plugin_id: "a".to_string(),
            version_requirement: "*".to_string(),
            required: true,
            health_required: false,
            startup_order: DependencyStartupOrder::Before,
        }]).await;

        // Should fail
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PluginError::CircularDependency(_)));
    }

    #[tokio::test]
    async fn test_complex_circular_dependency_detection() {
        let resolver = create_test_dependency_resolver();
        resolver.initialize().await.unwrap();

        // Create chain: A -> B -> C -> D
        resolver.add_plugin_dependencies("d", &[]).await.unwrap();
        resolver.add_plugin_dependencies("c", &[PluginDependency {
            plugin_id: "d".to_string(),
            version_requirement: "*".to_string(),
            required: true,
            health_required: false,
            startup_order: DependencyStartupOrder::Before,
        }]).await.unwrap();
        resolver.add_plugin_dependencies("b", &[PluginDependency {
            plugin_id: "c".to_string(),
            version_requirement: "*".to_string(),
            required: true,
            health_required: false,
            startup_order: DependencyStartupOrder::Before,
        }]).await.unwrap();
        resolver.add_plugin_dependencies("a", &[PluginDependency {
            plugin_id: "b".to_string(),
            version_requirement: "*".to_string(),
            required: true,
            health_required: false,
            startup_order: DependencyStartupOrder::Before,
        }]).await.unwrap();

        // Try to add D -> A (creates circular dependency A -> B -> C -> D -> A)
        let result = resolver.add_plugin_dependencies("d", &[PluginDependency {
            plugin_id: "a".to_string(),
            version_requirement: "*".to_string(),
            required: true,
            health_required: false,
            startup_order: DependencyStartupOrder::Before,
        }]).await;

        // Should fail
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PluginError::CircularDependency(_)));
    }

    #[tokio::test]
    async fn test_self_dependency_detection() {
        let resolver = create_test_dependency_resolver();
        resolver.initialize().await.unwrap();

        // Try to add self dependency
        let result = resolver.add_plugin_dependencies("plugin", &[PluginDependency {
            plugin_id: "plugin".to_string(),
            version_requirement: "*".to_string(),
            required: true,
            health_required: false,
            startup_order: DependencyStartupOrder::Before,
        }]).await;

        // Should fail
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PluginError::CircularDependency(_)));
    }

    // ============================================================================
    // HEALTH DEPENDENCY TESTS
    // ============================================================================

    #[tokio::test]
    async fn test_health_dependency_requirements() {
        let resolver = create_test_dependency_resolver();
        resolver.initialize().await.unwrap();

        // Add dependencies with health requirements
        resolver.add_plugin_dependencies("database", &[]).await.unwrap();
        resolver.add_plugin_dependencies("web-server", &[PluginDependency {
            plugin_id: "database".to_string(),
            version_requirement: "*".to_string(),
            required: true,
            health_required: true, // Require healthy state
            startup_order: DependencyStartupOrder::Before,
        }]).await.unwrap();

        // Check health requirements
        let health_deps = resolver.get_health_dependencies("web-server").await.unwrap();
        assert_eq!(health_deps.len(), 1);
        assert!(health_deps.contains(&"database".to_string()));

        // Check if dependencies are satisfied (simulate unhealthy database)
        let health_status = HashMap::from([
            ("database".to_string(), PluginHealthStatus::Unhealthy),
        ]);

        let satisfied = resolver.check_health_dependencies("web-server", &health_status).await.unwrap();
        assert!(!satisfied);
    }

    #[tokio::test]
    async fn test_optional_dependencies() {
        let resolver = create_test_dependency_resolver();
        resolver.initialize().await.unwrap();

        // Add optional dependency
        resolver.add_plugin_dependencies("web-server", &[PluginDependency {
            plugin_id: "cache".to_string(),
            version_requirement: "*".to_string(),
            required: false, // Optional
            health_required: false,
            startup_order: DependencyStartupOrder::After,
        }]).await.unwrap();

        // Check if optional dependency is available
        let available = resolver.is_dependency_available("web-server", "cache").await.unwrap();
        assert!(!available); // cache not registered

        // Register cache
        resolver.add_plugin_dependencies("cache", &[]).await.unwrap();

        // Check again
        let available = resolver.is_dependency_available("web-server", "cache").await.unwrap();
        assert!(available);
    }

    // ============================================================================
    // VERSION COMPATIBILITY TESTS
    // ============================================================================

    #[tokio::test]
    async fn test_version_requirement_matching() {
        let resolver = create_test_dependency_resolver();
        resolver.initialize().await.unwrap();

        // Register dependency with version requirement
        resolver.add_plugin_dependencies("web-server", &[PluginDependency {
            plugin_id: "database".to_string(),
            version_requirement: "^1.2.0".to_string(), // Compatible with 1.2.0 <= version < 2.0.0
            required: true,
            health_required: false,
            startup_order: DependencyStartupOrder::Before,
        }]).await.unwrap();

        // Test compatible versions
        let compatible_versions = vec!["1.2.0", "1.2.5", "1.9.9"];
        for version in compatible_versions {
            let result = resolver.check_version_compatibility("web-server", "database", version).await.unwrap();
            assert!(result, "Version {} should be compatible", version);
        }

        // Test incompatible versions
        let incompatible_versions = vec!["1.1.9", "2.0.0", "2.1.0"];
        for version in incompatible_versions {
            let result = resolver.check_version_compatibility("web-server", "database", version).await.unwrap();
            assert!(!result, "Version {} should be incompatible", version);
        }
    }

    // ============================================================================
    // DYNAMIC DEPENDENCY UPDATES TESTS
    // ============================================================================

    #[tokio::test]
    async fn test_dynamic_dependency_addition() {
        let resolver = create_test_dependency_resolver();
        resolver.initialize().await.unwrap();

        // Initial setup
        resolver.add_plugin_dependencies("database", &[]).await.unwrap();
        resolver.add_plugin_dependencies("web-server", &[]).await.unwrap();

        // Verify no dependencies initially
        let deps = resolver.get_plugin_dependencies("web-server").await.unwrap();
        assert_eq!(deps.len(), 0);

        // Add dependency dynamically
        let result = resolver.add_plugin_dependencies("web-server", &[PluginDependency {
            plugin_id: "database".to_string(),
            version_requirement: "*".to_string(),
            required: true,
            health_required: false,
            startup_order: DependencyStartupOrder::Before,
        }]).await;

        assert!(result.is_ok());

        // Verify dependency was added
        let deps = resolver.get_plugin_dependencies("web-server").await.unwrap();
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].plugin_id, "database");
    }

    #[tokio::test]
    async fn test_dependency_rollback() {
        let resolver = create_test_dependency_resolver();
        resolver.initialize().await.unwrap();

        // Create backup snapshot
        let snapshot = resolver.create_graph_snapshot().await.unwrap();

        // Add dependencies
        resolver.add_plugin_dependencies("database", &[]).await.unwrap();
        resolver.add_plugin_dependencies("web-server", &[PluginDependency {
            plugin_id: "database".to_string(),
            version_requirement: "*".to_string(),
            required: true,
            health_required: false,
            startup_order: DependencyStartupOrder::Before,
        }]).await.unwrap();

        // Verify dependencies exist
        let stats = resolver.get_graph_stats().await;
        assert_eq!(stats.total_nodes, 2);

        // Rollback to snapshot
        let result = resolver.restore_graph_snapshot(&snapshot).await;
        assert!(result.is_ok());

        // Verify rollback
        let stats = resolver.get_graph_stats().await;
        assert_eq!(stats.total_nodes, 0);
    }

    // ============================================================================
    // DEPENDENCY VALIDATION TESTS
    // ============================================================================

    #[tokio::test]
    async fn test_dependency_graph_validation() {
        let resolver = create_test_dependency_resolver();
        resolver.initialize().await.unwrap();

        // Create valid dependency graph
        resolver.add_plugin_dependencies("database", &[]).await.unwrap();
        resolver.add_plugin_dependencies("cache", &[]).await.unwrap();
        resolver.add_plugin_dependencies("web-server", &[
            PluginDependency {
                plugin_id: "database".to_string(),
                version_requirement: "*".to_string(),
                required: true,
                health_required: false,
                startup_order: DependencyStartupOrder::Before,
            },
            PluginDependency {
                plugin_id: "cache".to_string(),
                version_requirement: "*".to_string(),
                required: false,
                health_required: false,
                startup_order: DependencyStartupOrder::After,
            },
        ]).await.unwrap();

        // Validate graph
        let validation_result = resolver.validate_dependency_graph().await.unwrap();

        assert!(validation_result.is_valid);
        assert!(validation_result.errors.is_empty());
        assert!(validation_result.warnings.is_empty());
    }

    #[tokio::test]
    async fn test_dependency_graph_validation_with_errors() {
        let resolver = create_test_dependency_resolver();
        resolver.initialize().await.unwrap();

        // Add dependency on non-existent plugin
        resolver.add_plugin_dependencies("web-server", &[PluginDependency {
            plugin_id: "database".to_string(),
            version_requirement: "*".to_string(),
            required: true,
            health_required: false,
            startup_order: DependencyStartupOrder::Before,
        }]).await.unwrap();

        // Validate graph
        let validation_result = resolver.validate_dependency_graph().await.unwrap();

        assert!(!validation_result.is_valid);
        assert!(!validation_result.errors.is_empty());
        assert!(validation_result.errors.iter().any(|e| e.contains("Missing dependency")));
    }

    // ============================================================================
    // PERFORMANCE TESTS
    // ============================================================================

    #[tokio::test]
    async fn test_large_dependency_graph_performance() {
        let resolver = create_test_dependency_resolver();
        resolver.initialize().await.unwrap();

        // Create large dependency graph (100 plugins)
        let start_time = SystemTime::now();

        for i in 0..100 {
            let plugin_id = format!("plugin-{}", i);

            if i == 0 {
                // First plugin has no dependencies
                resolver.add_plugin_dependencies(&plugin_id, &[]).await.unwrap();
            } else {
                // Each plugin depends on the previous one (chain)
                let dep_id = format!("plugin-{}", i - 1);
                resolver.add_plugin_dependencies(&plugin_id, &[PluginDependency {
                    plugin_id: dep_id,
                    version_requirement: "*".to_string(),
                    required: true,
                    health_required: false,
                    startup_order: DependencyStartupOrder::Before,
                }]).await.unwrap();
            }
        }

        let construction_time = SystemTime::now().duration_since(start_time).unwrap();

        // Test startup ordering performance
        let start_time = SystemTime::now();
        let plugin_ids: Vec<String> = (0..100).map(|i| format!("plugin-{}", i)).collect();
        let _startup_order = resolver.get_startup_order(&plugin_ids).await.unwrap();
        let ordering_time = SystemTime::now().duration_since(start_time).unwrap();

        // Performance assertions
        assert!(construction_time < Duration::from_millis(100), "Graph construction too slow: {:?}", construction_time);
        assert!(ordering_time < Duration::from_millis(50), "Startup ordering too slow: {:?}", ordering_time);

        // Verify graph stats
        let stats = resolver.get_graph_stats().await;
        assert_eq!(stats.total_nodes, 100);
        assert_eq!(stats.total_edges, 99);
        assert_eq!(stats.circular_dependencies, 0);
    }

    #[tokio::test]
    async fn test_concurrent_dependency_operations() {
        let resolver = Arc::new(create_test_dependency_resolver());
        resolver.initialize().await.unwrap();

        let mut handles = Vec::new();

        // Add plugins concurrently
        for i in 0..50 {
            let resolver_clone = resolver.clone();
            let handle = tokio::spawn(async move {
                let plugin_id = format!("plugin-{}", i);
                resolver_clone.add_plugin_dependencies(&plugin_id, &[]).await
            });
            handles.push(handle);
        }

        // Wait for all operations to complete
        let mut successes = 0;
        for handle in handles {
            if handle.await.unwrap().is_ok() {
                successes += 1;
            }
        }

        assert_eq!(successes, 50);

        // Verify all plugins were added
        let stats = resolver.get_graph_stats().await;
        assert_eq!(stats.total_nodes, 50);
    }

    // ============================================================================
    // ERROR HANDLING AND RECOVERY TESTS
    // ============================================================================

    #[tokio::test]
    async fn test_dependency_resolver_error_handling() {
        let resolver = create_test_dependency_resolver();
        resolver.initialize().await.unwrap();

        // Test operations on non-existent plugin
        let result = resolver.get_plugin_dependencies("non-existent").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PluginError::NotFound(_)));

        let result = resolver.get_startup_order(&["non-existent"]).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PluginError::NotFound(_)));

        // Test invalid version requirements
        resolver.add_plugin_dependencies("plugin-a", &[]).await.unwrap();
        let result = resolver.add_plugin_dependencies("plugin-b", &[PluginDependency {
            plugin_id: "plugin-a".to_string(),
            version_requirement: "invalid.version".to_string(),
            required: true,
            health_required: false,
            startup_order: DependencyStartupOrder::Before,
        }]).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_partial_dependency_resolution() {
        let resolver = create_test_dependency_resolver();
        resolver.initialize().await.unwrap();

        // Create partial dependency graph (some dependencies missing)
        resolver.add_plugin_dependencies("database", &[]).await.unwrap();
        // cache plugin not registered
        resolver.add_plugin_dependencies("web-server", &[
            PluginDependency {
                plugin_id: "database".to_string(),
                version_requirement: "*".to_string(),
                required: true,
                health_required: false,
                startup_order: DependencyStartupOrder::Before,
            },
            PluginDependency {
                plugin_id: "cache".to_string(),
                version_requirement: "*".to_string(),
                required: false, // Optional
                health_required: false,
                startup_order: DependencyStartupOrder::After,
            },
        ]).await.unwrap();

        // Should still get startup order for available dependencies
        let startup_order = resolver.get_startup_order(&["web-server", "database"]).await.unwrap();
        assert_eq!(startup_order.len(), 2);
        assert_eq!(startup_order[0], "database");
        assert_eq!(startup_order[1], "web-server");

        // Check dependency status
        let status = resolver.get_dependency_status("web-server").await.unwrap();
        assert!(status.missing_dependencies.contains(&"cache".to_string()));
        assert!(status.satisfied_dependencies.contains(&"database".to_string()));
    }

    // ============================================================================
    // STRESS TESTS
    // ============================================================================

    #[tokio::test]
    async fn test_dependency_resolution_under_stress() {
        let resolver = create_test_dependency_resolver();
        resolver.initialize().await.unwrap();

        // Create complex dependency graph with many nodes and edges
        let node_count = 200;

        for i in 0..node_count {
            let plugin_id = format!("plugin-{}", i);

            // Create random dependencies
            let mut dependencies = Vec::new();
            if i > 0 {
                // Depend on previous plugins
                for j in 0..(i % 5).min(3) {
                    let dep_id = format!("plugin-{}", i - 1 - j);
                    dependencies.push(PluginDependency {
                        plugin_id: dep_id,
                        version_requirement: "*".to_string(),
                        required: j == 0, // Only first dependency is required
                        health_required: false,
                        startup_order: DependencyStartupOrder::Before,
                    });
                }
            }

            resolver.add_plugin_dependencies(&plugin_id, &dependencies).await.unwrap();
        }

        // Perform many startup order calculations
        let plugin_ids: Vec<String> = (0..node_count).map(|i| format!("plugin-{}", i)).collect();

        for _ in 0..100 {
            let _startup_order = resolver.get_startup_order(&plugin_ids).await.unwrap();
        }

        // Verify graph integrity
        let stats = resolver.get_graph_stats().await;
        assert_eq!(stats.total_nodes, node_count);
        assert_eq!(stats.circular_dependencies, 0);
    }

    #[tokio::test]
    async fn test_memory_usage_with_large_graph() {
        let resolver = create_test_dependency_resolver();
        resolver.initialize().await.unwrap();

        // Create very large dependency graph
        let node_count = 1000;

        for i in 0..node_count {
            let plugin_id = format!("plugin-{}", i);

            if i == 0 {
                resolver.add_plugin_dependencies(&plugin_id, &[]).await.unwrap();
            } else {
                // Create chain dependency
                let dep_id = format!("plugin-{}", i - 1);
                resolver.add_plugin_dependencies(&plugin_id, &[PluginDependency {
                    plugin_id: dep_id,
                    version_requirement: "*".to_string(),
                    required: true,
                    health_required: false,
                    startup_order: DependencyStartupOrder::Before,
                }]).await.unwrap();
            }
        }

        // Verify memory usage is reasonable by checking graph stats
        let stats = resolver.get_graph_stats().await;
        assert_eq!(stats.total_nodes, node_count);
        assert_eq!(stats.total_edges, node_count - 1);

        // Perform operations to ensure no memory leaks
        let plugin_ids: Vec<String> = (0..node_count).map(|i| format!("plugin-{}", i)).collect();
        let _startup_order = resolver.get_startup_order(&plugin_ids).await.unwrap();

        let validation_result = resolver.validate_dependency_graph().await.unwrap();
        assert!(validation_result.is_valid);
    }
}