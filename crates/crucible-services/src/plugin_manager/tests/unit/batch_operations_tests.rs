//! # Plugin Batch Operations Tests
//!
//! Comprehensive unit tests for the plugin batch operations coordinator.
//! Tests cover batch creation, execution strategies, dependency handling,
//! progress tracking, and performance under various scenarios.

use super::*;
use crate::plugin_manager::batch_operations::*;
use crate::plugin_manager::lifecycle_manager::*;
use crate::plugin_manager::state_machine::*;
use crate::plugin_manager::dependency_resolver::*;
use crate::plugin_manager::lifecycle_policy::*;
use crate::plugin_manager::automation_engine::*;
use crate::plugin_manager::types::*;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

#[cfg(test)]
mod batch_operations_tests {
    use super::*;

    // ============================================================================
    // BASIC BATCH OPERATIONS FUNCTIONALITY TESTS
    // ============================================================================

    #[tokio::test]
    async fn test_batch_operations_coordinator_creation() {
        let coordinator = create_test_batch_coordinator();

        // Verify initial state
        let batches = coordinator.list_batches(None).await.unwrap();
        assert_eq!(batches.len(), 1); // Default rolling restart template

        let metrics = coordinator.get_metrics().await.unwrap();
        assert_eq!(metrics.total_batches_created, 1);
        assert_eq!(metrics.total_executions_completed, 0);
    }

    #[tokio::test]
    async fn test_batch_operations_coordinator_initialization() {
        let coordinator = create_test_batch_coordinator();

        // Initialize coordinator
        let result = coordinator.initialize().await;
        assert!(result.is_ok());

        // Verify initialization (should load default templates)
        let templates = coordinator.list_templates().await.unwrap();
        assert!(templates.len() >= 1);

        let metrics = coordinator.get_metrics().await.unwrap();
        assert!(metrics.total_batches_created >= 1);
    }

    // ============================================================================
    // BATCH CREATION TESTS
    // ============================================================================

    #[tokio::test]
    async fn test_create_batch_operation() {
        let coordinator = create_test_batch_coordinator();
        coordinator.initialize().await.unwrap();

        let batch = create_test_batch_operation("test-batch", "Test Batch");
        let batch_with_items = BatchOperation {
            operations: vec![
                BatchOperationItem {
                    item_id: "item-1".to_string(),
                    operation: LifecycleOperation::Start { instance_id: "instance-1".to_string() },
                    target: "instance-1".to_string(),
                    priority: BatchItemPriority::Normal,
                    dependencies: vec![],
                    timeout: Some(Duration::from_secs(30)),
                    retry_config: None,
                    rollback_config: None,
                    metadata: HashMap::new(),
                },
                BatchOperationItem {
                    item_id: "item-2".to_string(),
                    operation: LifecycleOperation::Start { instance_id: "instance-2".to_string() },
                    target: "instance-2".to_string(),
                    priority: BatchItemPriority::Normal,
                    dependencies: vec![],
                    timeout: Some(Duration::from_secs(30)),
                    retry_config: None,
                    rollback_config: None,
                    metadata: HashMap::new(),
                },
            ],
            ..batch
        };

        // Create batch
        let result = coordinator.create_batch(batch_with_items.clone()).await;
        assert!(result.is_ok());

        let batch_id = result.unwrap();
        assert_eq!(batch_id, "test-batch");

        // Verify batch was created
        let retrieved_batch = coordinator.get_batch("test-batch").await.unwrap();
        assert!(retrieved_batch.is_some());
        assert_eq!(retrieved_batch.unwrap().name, "Test Batch");

        let metrics = coordinator.get_metrics().await.unwrap();
        assert_eq!(metrics.total_batches_created, 2); // Default template + new batch
    }

    #[tokio::test]
    async fn test_batch_validation() {
        let coordinator = create_test_batch_coordinator();
        coordinator.initialize().await.unwrap();

        // Test invalid batch (empty ID)
        let invalid_batch = BatchOperation {
            batch_id: "".to_string(), // Invalid
            name: "Invalid Batch".to_string(),
            description: "Batch with empty ID".to_string(),
            operations: vec![],
            strategy: BatchExecutionStrategy::Sequential {
                stop_on_failure: true,
                failure_handling: FailureHandling::Stop,
            },
            config: BatchConfig::default(),
            scope: BatchScope::default(),
            metadata: BatchMetadata::default(),
        };

        let result = coordinator.create_batch(invalid_batch);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PluginError::batch(_)));

        // Test invalid batch (no operations)
        let no_ops_batch = BatchOperation {
            batch_id: "no-ops".to_string(),
            name: "No Operations Batch".to_string(),
            description: "Batch with no operations".to_string(),
            operations: vec![], // Invalid - no operations
            strategy: BatchExecutionStrategy::Sequential {
                stop_on_failure: true,
                failure_handling: FailureHandling::Stop,
            },
            config: BatchConfig::default(),
            scope: BatchScope::default(),
            metadata: BatchMetadata::default(),
        };

        let result = coordinator.create_batch(no_ops_batch);
        assert!(result.is_err());

        // Test batch with invalid dependencies
        let invalid_deps_batch = BatchOperation {
            batch_id: "invalid-deps".to_string(),
            name: "Invalid Dependencies Batch".to_string(),
            description: "Batch with invalid dependencies".to_string(),
            operations: vec![
                BatchOperationItem {
                    item_id: "item-1".to_string(),
                    operation: LifecycleOperation::Start { instance_id: "instance-1".to_string() },
                    target: "instance-1".to_string(),
                    priority: BatchItemPriority::Normal,
                    dependencies: vec!["non-existent-item".to_string()], // Invalid dependency
                    timeout: None,
                    retry_config: None,
                    rollback_config: None,
                    metadata: HashMap::new(),
                },
            ],
            strategy: BatchExecutionStrategy::Sequential {
                stop_on_failure: true,
                failure_handling: FailureHandling::Stop,
            },
            config: BatchConfig::default(),
            scope: BatchScope::default(),
            metadata: BatchMetadata::default(),
        };

        let result = coordinator.create_batch(invalid_deps_batch);
        assert!(result.is_err());
    }

    // ============================================================================
    // SEQUENTIAL EXECUTION TESTS
    // ============================================================================

    #[tokio::test]
    async fn test_sequential_batch_execution() {
        let coordinator = create_test_batch_coordinator();
        coordinator.initialize().await.unwrap();

        let batch = BatchOperation {
            batch_id: "sequential-test".to_string(),
            name: "Sequential Test".to_string(),
            description: "Test sequential execution".to_string(),
            operations: vec![
                BatchOperationItem {
                    item_id: "item-1".to_string(),
                    operation: LifecycleOperation::Start { instance_id: "instance-1".to_string() },
                    target: "instance-1".to_string(),
                    priority: BatchItemPriority::Normal,
                    dependencies: vec![],
                    timeout: Some(Duration::from_millis(100)),
                    retry_config: None,
                    rollback_config: None,
                    metadata: HashMap::new(),
                },
                BatchOperationItem {
                    item_id: "item-2".to_string(),
                    operation: LifecycleOperation::Start { instance_id: "instance-2".to_string() },
                    target: "instance-2".to_string(),
                    priority: BatchItemPriority::Normal,
                    dependencies: vec![],
                    timeout: Some(Duration::from_millis(100)),
                    retry_config: None,
                    rollback_config: None,
                    metadata: HashMap::new(),
                },
                BatchOperationItem {
                    item_id: "item-3".to_string(),
                    operation: LifecycleOperation::Start { instance_id: "instance-3".to_string() },
                    target: "instance-3".to_string(),
                    priority: BatchItemPriority::Normal,
                    dependencies: vec![],
                    timeout: Some(Duration::from_millis(100)),
                    retry_config: None,
                    rollback_config: None,
                    metadata: HashMap::new(),
                },
            ],
            strategy: BatchExecutionStrategy::Sequential {
                stop_on_failure: false,
                failure_handling: FailureHandling::Continue,
            },
            config: BatchConfig::default(),
            scope: BatchScope::default(),
            metadata: BatchMetadata::default(),
        };

        coordinator.create_batch(batch).await.unwrap();

        // Execute batch
        let context = BatchExecutionContext {
            batch_id: "sequential-test".to_string(),
            execution_id: uuid::Uuid::new_v4().to_string(),
            timestamp: SystemTime::now(),
            mode: ExecutionMode::Normal,
            dry_run: false,
            additional_context: HashMap::new(),
        };

        let execution_id = coordinator.execute_batch("sequential-test", context).await.unwrap();

        // Wait for execution to complete
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Check execution result
        let result = coordinator.get_execution_result(&execution_id).await.unwrap();
        assert!(result.is_some());

        let execution_result = result.unwrap();
        assert!(execution_result.success);
        assert_eq!(execution_result.item_results.len(), 3);
    }

    // ============================================================================
    // PARALLEL EXECUTION TESTS
    // ============================================================================

    #[tokio::test]
    async fn test_parallel_batch_execution() {
        let coordinator = create_test_batch_coordinator();
        coordinator.initialize().await.unwrap();

        let batch = BatchOperation {
            batch_id: "parallel-test".to_string(),
            name: "Parallel Test".to_string(),
            description: "Test parallel execution".to_string(),
            operations: vec![
                BatchOperationItem {
                    item_id: "item-1".to_string(),
                    operation: LifecycleOperation::Start { instance_id: "instance-1".to_string() },
                    target: "instance-1".to_string(),
                    priority: BatchItemPriority::Normal,
                    dependencies: vec![],
                    timeout: Some(Duration::from_millis(100)),
                    retry_config: None,
                    rollback_config: None,
                    metadata: HashMap::new(),
                },
                BatchOperationItem {
                    item_id: "item-2".to_string(),
                    operation: LifecycleOperation::Start { instance_id: "instance-2".to_string() },
                    target: "instance-2".to_string(),
                    priority: BatchItemPriority::Normal,
                    dependencies: vec![],
                    timeout: Some(Duration::from_millis(100)),
                    retry_config: None,
                    rollback_config: None,
                    metadata: HashMap::new(),
                },
                BatchOperationItem {
                    item_id: "item-3".to_string(),
                    operation: LifecycleOperation::Start { instance_id: "instance-3".to_string() },
                    target: "instance-3".to_string(),
                    priority: BatchItemPriority::Normal,
                    dependencies: vec![],
                    timeout: Some(Duration::from_millis(100)),
                    retry_config: None,
                    rollback_config: None,
                    metadata: HashMap::new(),
                },
                BatchOperationItem {
                    item_id: "item-4".to_string(),
                    operation: LifecycleOperation::Start { instance_id: "instance-4".to_string() },
                    target: "instance-4".to_string(),
                    priority: BatchItemPriority::Normal,
                    dependencies: vec![],
                    timeout: Some(Duration::from_millis(100)),
                    retry_config: None,
                    rollback_config: None,
                    metadata: HashMap::new(),
                },
            ],
            strategy: BatchExecutionStrategy::Parallel {
                max_concurrent: 2,
                stop_on_failure: false,
                failure_handling: FailureHandling::Continue,
            },
            config: BatchConfig::default(),
            scope: BatchScope::default(),
            metadata: BatchMetadata::default(),
        };

        coordinator.create_batch(batch).await.unwrap();

        // Execute batch
        let context = BatchExecutionContext {
            batch_id: "parallel-test".to_string(),
            execution_id: uuid::Uuid::new_v4().to_string(),
            timestamp: SystemTime::now(),
            mode: ExecutionMode::Normal,
            dry_run: false,
            additional_context: HashMap::new(),
        };

        let execution_id = coordinator.execute_batch("parallel-test", context).await.unwrap();

        // Wait for execution to complete
        tokio::time::sleep(Duration::from_millis(300)).await;

        // Check execution result
        let result = coordinator.get_execution_result(&execution_id).await.unwrap();
        assert!(result.is_some());

        let execution_result = result.unwrap();
        assert!(execution_result.success);
        assert_eq!(execution_result.item_results.len(), 4);
    }

    // ============================================================================
    // DEPENDENCY-ORDERED EXECUTION TESTS
    // ============================================================================

    #[tokio::test]
    async fn test_dependency_ordered_execution() {
        let coordinator = create_test_batch_coordinator();
        coordinator.initialize().await.unwrap();

        let batch = BatchOperation {
            batch_id: "dependency-test".to_string(),
            name: "Dependency Test".to_string(),
            description: "Test dependency-ordered execution".to_string(),
            operations: vec![
                BatchOperationItem {
                    item_id: "database".to_string(),
                    operation: LifecycleOperation::Start { instance_id: "db-instance".to_string() },
                    target: "db-instance".to_string(),
                    priority: BatchItemPriority::High,
                    dependencies: vec![], // No dependencies - starts first
                    timeout: Some(Duration::from_millis(100)),
                    retry_config: None,
                    rollback_config: None,
                    metadata: HashMap::new(),
                },
                BatchOperationItem {
                    item_id: "cache".to_string(),
                    operation: LifecycleOperation::Start { instance_id: "cache-instance".to_string() },
                    target: "cache-instance".to_string(),
                    priority: BatchItemPriority::Normal,
                    dependencies: vec![], // No dependencies - can start in parallel with database
                    timeout: Some(Duration::from_millis(100)),
                    retry_config: None,
                    rollback_config: None,
                    metadata: HashMap::new(),
                },
                BatchOperationItem {
                    item_id: "web-server".to_string(),
                    operation: LifecycleOperation::Start { instance_id: "web-instance".to_string() },
                    target: "web-instance".to_string(),
                    priority: BatchItemPriority::Normal,
                    dependencies: vec!["database".to_string(), "cache".to_string()], // Depends on both
                    timeout: Some(Duration::from_millis(100)),
                    retry_config: None,
                    rollback_config: None,
                    metadata: HashMap::new(),
                },
                BatchOperationItem {
                    item_id: "api-server".to_string(),
                    operation: LifecycleOperation::Start { instance_id: "api-instance".to_string() },
                    target: "api-instance".to_string(),
                    priority: BatchItemPriority::Normal,
                    dependencies: vec!["database".to_string()], // Depends only on database
                    timeout: Some(Duration::from_millis(100)),
                    retry_config: None,
                    rollback_config: None,
                    metadata: HashMap::new(),
                },
            ],
            strategy: BatchExecutionStrategy::DependencyOrdered {
                max_concurrent_per_level: 2,
                stop_on_failure: false,
                failure_handling: FailureHandling::Continue,
            },
            config: BatchConfig::default(),
            scope: BatchScope::default(),
            metadata: BatchMetadata::default(),
        };

        coordinator.create_batch(batch).await.unwrap();

        // Execute batch
        let context = BatchExecutionContext {
            batch_id: "dependency-test".to_string(),
            execution_id: uuid::Uuid::new_v4().to_string(),
            timestamp: SystemTime::now(),
            mode: ExecutionMode::Normal,
            dry_run: false,
            additional_context: HashMap::new(),
        };

        let execution_id = coordinator.execute_batch("dependency-test", context).await.unwrap();

        // Wait for execution to complete
        tokio::time::sleep(Duration::from_millis(400)).await;

        // Check execution result
        let result = coordinator.get_execution_result(&execution_id).await.unwrap();
        assert!(result.is_some());

        let execution_result = result.unwrap();
        assert!(execution_result.success);
        assert_eq!(execution_result.item_results.len(), 4);

        // Verify execution order (database and cache should complete before web-server)
        let mut completion_times = HashMap::new();
        for item_result in &execution_result.item_results {
            if let Some(completed_at) = item_result.completed_at {
                completion_times.insert(&item_result.item_id, completed_at);
            }
        }

        // Database and cache should complete before web-server
        if let (Some(db_time), Some(cache_time), Some(web_time)) = (
            completion_times.get(&"database".to_string()),
            completion_times.get(&"cache".to_string()),
            completion_times.get(&"web-server".to_string()),
        ) {
            assert!(db_time <= web_time);
            assert!(cache_time <= web_time);
        }
    }

    // ============================================================================
    // ROLLING EXECUTION TESTS
    // ============================================================================

    #[tokio::test]
    async fn test_rolling_batch_execution() {
        let coordinator = create_test_batch_coordinator();
        coordinator.initialize().await.unwrap();

        let batch = BatchOperation {
            batch_id: "rolling-test".to_string(),
            name: "Rolling Test".to_string(),
            description: "Test rolling execution".to_string(),
            operations: vec![
                BatchOperationItem {
                    item_id: "instance-1".to_string(),
                    operation: LifecycleOperation::Restart { instance_id: "instance-1".to_string() },
                    target: "instance-1".to_string(),
                    priority: BatchItemPriority::Normal,
                    dependencies: vec![],
                    timeout: Some(Duration::from_millis(50)),
                    retry_config: None,
                    rollback_config: None,
                    metadata: HashMap::new(),
                },
                BatchOperationItem {
                    item_id: "instance-2".to_string(),
                    operation: LifecycleOperation::Restart { instance_id: "instance-2".to_string() },
                    target: "instance-2".to_string(),
                    priority: BatchItemPriority::Normal,
                    dependencies: vec![],
                    timeout: Some(Duration::from_millis(50)),
                    retry_config: None,
                    rollback_config: None,
                    metadata: HashMap::new(),
                },
                BatchOperationItem {
                    item_id: "instance-3".to_string(),
                    operation: LifecycleOperation::Restart { instance_id: "instance-3".to_string() },
                    target: "instance-3".to_string(),
                    priority: BatchItemPriority::Normal,
                    dependencies: vec![],
                    timeout: Some(Duration::from_millis(50)),
                    retry_config: None,
                    rollback_config: None,
                    metadata: HashMap::new(),
                },
                BatchOperationItem {
                    item_id: "instance-4".to_string(),
                    operation: LifecycleOperation::Restart { instance_id: "instance-4".to_string() },
                    target: "instance-4".to_string(),
                    priority: BatchItemPriority::Normal,
                    dependencies: vec![],
                    timeout: Some(Duration::from_millis(50)),
                    retry_config: None,
                    rollback_config: None,
                    metadata: HashMap::new(),
                },
            ],
            strategy: BatchExecutionStrategy::Rolling {
                batch_size: 2,
                pause_duration: Duration::from_millis(100),
                health_check_between_batches: true,
                rollback_on_batch_failure: false,
            },
            config: BatchConfig::default(),
            scope: BatchScope::default(),
            metadata: BatchMetadata::default(),
        };

        coordinator.create_batch(batch).await.unwrap();

        // Execute batch
        let context = BatchExecutionContext {
            batch_id: "rolling-test".to_string(),
            execution_id: uuid::Uuid::new_v4().to_string(),
            timestamp: SystemTime::now(),
            mode: ExecutionMode::Normal,
            dry_run: false,
            additional_context: HashMap::new(),
        };

        let execution_id = coordinator.execute_batch("rolling-test", context).await.unwrap();

        // Wait for execution to complete (should take longer due to rolling strategy)
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Check execution result
        let result = coordinator.get_execution_result(&execution_id).await.unwrap();
        assert!(result.is_some());

        let execution_result = result.unwrap();
        assert!(execution_result.success);
        assert_eq!(execution_result.item_results.len(), 4);
    }

    // ============================================================================
    // CANARY EXECUTION TESTS
    // ============================================================================

    #[tokio::test]
    async fn test_canary_batch_execution() {
        let coordinator = create_test_batch_coordinator();
        coordinator.initialize().await.unwrap();

        let batch = BatchOperation {
            batch_id: "canary-test".to_string(),
            name: "Canary Test".to_string(),
            description: "Test canary execution".to_string(),
            operations: vec![
                BatchOperationItem {
                    item_id: "instance-1".to_string(),
                    operation: LifecycleOperation::UpdateConfig {
                        instance_id: "instance-1".to_string(),
                        config: HashMap::from([("version".to_string(), serde_json::Value::String("2.0.0".to_string()))]),
                    },
                    target: "instance-1".to_string(),
                    priority: BatchItemPriority::Normal,
                    dependencies: vec![],
                    timeout: Some(Duration::from_millis(50)),
                    retry_config: None,
                    rollback_config: None,
                    metadata: HashMap::new(),
                },
                BatchOperationItem {
                    item_id: "instance-2".to_string(),
                    operation: LifecycleOperation::UpdateConfig {
                        instance_id: "instance-2".to_string(),
                        config: HashMap::from([("version".to_string(), serde_json::Value::String("2.0.0".to_string()))]),
                    },
                    target: "instance-2".to_string(),
                    priority: BatchItemPriority::Normal,
                    dependencies: vec![],
                    timeout: Some(Duration::from_millis(50)),
                    retry_config: None,
                    rollback_config: None,
                    metadata: HashMap::new(),
                },
                BatchOperationItem {
                    item_id: "instance-3".to_string(),
                    operation: LifecycleOperation::UpdateConfig {
                        instance_id: "instance-3".to_string(),
                        config: HashMap::from([("version".to_string(), serde_json::Value::String("2.0.0".to_string()))]),
                    },
                    target: "instance-3".to_string(),
                    priority: BatchItemPriority::Normal,
                    dependencies: vec![],
                    timeout: Some(Duration::from_millis(50)),
                    retry_config: None,
                    rollback_config: None,
                    metadata: HashMap::new(),
                },
                BatchOperationItem {
                    item_id: "instance-4".to_string(),
                    operation: LifecycleOperation::UpdateConfig {
                        instance_id: "instance-4".to_string(),
                        config: HashMap::from([("version".to_string(), serde_json::Value::String("2.0.0".to_string()))]),
                    },
                    target: "instance-4".to_string(),
                    priority: BatchItemPriority::Normal,
                    dependencies: vec![],
                    timeout: Some(Duration::from_millis(50)),
                    retry_config: None,
                    rollback_config: None,
                    metadata: HashMap::new(),
                },
            ],
            strategy: BatchExecutionStrategy::Canary {
                canary_size: CanarySize::Percentage(25), // 25% = 1 instance for canary
                pause_duration: Duration::from_millis(100),
                success_criteria: CanarySuccessCriteria {
                    success_rate_threshold: 100.0,
                    health_criteria: vec![],
                    performance_criteria: vec![],
                    evaluation_window: Duration::from_millis(50),
                },
                auto_promote: true,
            },
            config: BatchConfig::default(),
            scope: BatchScope::default(),
            metadata: BatchMetadata::default(),
        };

        coordinator.create_batch(batch).await.unwrap();

        // Execute batch
        let context = BatchExecutionContext {
            batch_id: "canary-test".to_string(),
            execution_id: uuid::Uuid::new_v4().to_string(),
            timestamp: SystemTime::now(),
            mode: ExecutionMode::Normal,
            dry_run: false,
            additional_context: HashMap::new(),
        };

        let execution_id = coordinator.execute_batch("canary-test", context).await.unwrap();

        // Wait for execution to complete
        tokio::time::sleep(Duration::from_millis(400)).await;

        // Check execution result
        let result = coordinator.get_execution_result(&execution_id).await.unwrap();
        assert!(result.is_some());

        let execution_result = result.unwrap();
        // Canary should succeed (all items succeed in this mock scenario)
        assert!(execution_result.success);
        assert_eq!(execution_result.item_results.len(), 4);
    }

    // ============================================================================
    // PROGRESS TRACKING TESTS
    // ============================================================================

    #[tokio::test]
    async fn test_batch_progress_tracking() {
        let coordinator = create_test_batch_coordinator();
        coordinator.initialize().await.unwrap();

        let batch = BatchOperation {
            batch_id: "progress-test".to_string(),
            name: "Progress Test".to_string(),
            description: "Test progress tracking".to_string(),
            operations: vec![
                BatchOperationItem {
                    item_id: "item-1".to_string(),
                    operation: LifecycleOperation::Start { instance_id: "instance-1".to_string() },
                    target: "instance-1".to_string(),
                    priority: BatchItemPriority::Normal,
                    dependencies: vec![],
                    timeout: Some(Duration::from_millis(100)),
                    retry_config: None,
                    rollback_config: None,
                    metadata: HashMap::new(),
                },
                BatchOperationItem {
                    item_id: "item-2".to_string(),
                    operation: LifecycleOperation::Start { instance_id: "instance-2".to_string() },
                    target: "instance-2".to_string(),
                    priority: BatchItemPriority::Normal,
                    dependencies: vec![],
                    timeout: Some(Duration::from_millis(100)),
                    retry_config: None,
                    rollback_config: None,
                    metadata: HashMap::new(),
                },
                BatchOperationItem {
                    item_id: "item-3".to_string(),
                    operation: LifecycleOperation::Start { instance_id: "instance-3".to_string() },
                    target: "instance-3".to_string(),
                    priority: BatchItemPriority::Normal,
                    dependencies: vec![],
                    timeout: Some(Duration::from_millis(100)),
                    retry_config: None,
                    rollback_config: None,
                    metadata: HashMap::new(),
                },
            ],
            strategy: BatchExecutionStrategy::Sequential {
                stop_on_failure: false,
                failure_handling: FailureHandling::Continue,
            },
            config: BatchConfig {
                enable_progress_tracking: true,
                progress_report_interval: Duration::from_millis(50),
                ..BatchConfig::default()
            },
            scope: BatchScope::default(),
            metadata: BatchMetadata::default(),
        };

        coordinator.create_batch(batch).await.unwrap();

        // Execute batch
        let context = BatchExecutionContext {
            batch_id: "progress-test".to_string(),
            execution_id: uuid::Uuid::new_v4().to_string(),
            timestamp: SystemTime::now(),
            mode: ExecutionMode::Normal,
            dry_run: false,
            additional_context: HashMap::new(),
        };

        let execution_id = coordinator.execute_batch("progress-test", context).await.unwrap();

        // Monitor progress
        let mut progress_updates = Vec::new();
        let start_time = SystemTime::now();

        while SystemTime::now().duration_since(start_time).unwrap() < Duration::from_millis(500) {
            if let Some(progress) = coordinator.get_execution_progress(&execution_id).await.unwrap() {
                progress_updates.push(progress.clone());

                if progress.progress_percentage >= 100.0 {
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }

        // Verify progress tracking
        assert!(!progress_updates.is_empty());

        // Progress should increase over time
        for i in 1..progress_updates.len() {
            assert!(progress_updates[i].progress_percentage >= progress_updates[i-1].progress_percentage);
        }

        // Final progress should be 100%
        if let Some(final_progress) = progress_updates.last() {
            assert_eq!(final_progress.progress_percentage, 100.0);
            assert_eq!(final_progress.items_completed, final_progress.total_items);
        }
    }

    // ============================================================================
    // BATCH TEMPLATES TESTS
    // ============================================================================

    #[tokio::test]
    async fn test_batch_templates() {
        let coordinator = create_test_batch_coordinator();
        coordinator.initialize().await.unwrap();

        // Create custom template
        let template = BatchTemplate {
            template_id: "custom-restart".to_string(),
            name: "Custom Restart Template".to_string(),
            description: "Template for restarting instances".to_string(),
            operations: vec![
                TemplateOperation {
                    operation_template: "Restart".to_string(),
                    target_template: "{{instance_id}}".to_string(),
                    parameters: HashMap::from([
                        ("timeout".to_string(), serde_json::Value::String("60".to_string())),
                    ]),
                },
            ],
            parameters: vec![
                TemplateParameter {
                    name: "instances".to_string(),
                    parameter_type: ParameterType::Array,
                    description: "List of instance IDs to restart".to_string(),
                    required: true,
                    default_value: None,
                    validation_rules: vec![],
                },
                TemplateParameter {
                    name: "timeout".to_string(),
                    parameter_type: ParameterType::Number,
                    description: "Operation timeout in seconds".to_string(),
                    required: false,
                    default_value: Some(serde_json::Value::Number(60.into())),
                    validation_rules: vec![],
                },
            ],
            metadata: TemplateMetadata {
                created_at: SystemTime::now(),
                created_by: "test".to_string(),
                updated_at: SystemTime::now(),
                updated_by: "test".to_string(),
                tags: vec!["restart".to_string(), "template".to_string()],
                usage_count: 0,
            },
        };

        // Create template
        let result = coordinator.create_template(template.clone()).await;
        assert!(result.is_ok());

        let template_id = result.unwrap();
        assert_eq!(template_id, "custom-restart");

        // Verify template was created
        let retrieved_template = coordinator.get_template("custom-restart").await.unwrap();
        assert!(retrieved_template.is_some());
        assert_eq!(retrieved_template.unwrap().name, "Custom Restart Template");

        // List templates
        let templates = coordinator.list_templates().await.unwrap();
        assert!(templates.len() >= 2); // Default + custom template

        // Execute batch from template
        let parameters = HashMap::from([
            ("instances".to_string(), serde_json::Value::Array(vec![
                serde_json::Value::String("instance-1".to_string()),
                serde_json::Value::String("instance-2".to_string()),
            ])),
        ]);

        let context = BatchExecutionContext {
            batch_id: "template-execution".to_string(),
            execution_id: uuid::Uuid::new_v4().to_string(),
            timestamp: SystemTime::now(),
            mode: ExecutionMode::Normal,
            dry_run: true, // Use dry run for template test
            additional_context: HashMap::new(),
        };

        let execution_id = coordinator.execute_from_template("custom-restart", parameters, context).await.unwrap();
        assert!(!execution_id.is_empty());
    }

    // ============================================================================
    // ERROR HANDLING AND ROLLBACK TESTS
    // ============================================================================

    #[tokio::test]
    async fn test_batch_execution_failure_handling() {
        let coordinator = create_test_batch_coordinator();
        coordinator.initialize().await.unwrap();

        let batch = BatchOperation {
            batch_id: "failure-test".to_string(),
            name: "Failure Test".to_string(),
            description: "Test failure handling".to_string(),
            operations: vec![
                BatchOperationItem {
                    item_id: "item-1".to_string(),
                    operation: LifecycleOperation::Start { instance_id: "instance-1".to_string() },
                    target: "instance-1".to_string(),
                    priority: BatchItemPriority::Normal,
                    dependencies: vec![],
                    timeout: Some(Duration::from_millis(50)),
                    retry_config: None,
                    rollback_config: None,
                    metadata: HashMap::new(),
                },
                BatchOperationItem {
                    item_id: "item-2".to_string(),
                    operation: LifecycleOperation::Start { instance_id: "non-existent-instance".to_string() }, // This should fail
                    target: "non-existent-instance".to_string(),
                    priority: BatchItemPriority::Normal,
                    dependencies: vec![],
                    timeout: Some(Duration::from_millis(50)),
                    retry_config: None,
                    rollback_config: None,
                    metadata: HashMap::new(),
                },
                BatchOperationItem {
                    item_id: "item-3".to_string(),
                    operation: LifecycleOperation::Start { instance_id: "instance-3".to_string() },
                    target: "instance-3".to_string(),
                    priority: BatchItemPriority::Normal,
                    dependencies: vec![],
                    timeout: Some(Duration::from_millis(50)),
                    retry_config: None,
                    rollback_config: None,
                    metadata: HashMap::new(),
                },
            ],
            strategy: BatchExecutionStrategy::Sequential {
                stop_on_failure: false, // Continue on failure
                failure_handling: FailureHandling::Continue,
            },
            config: BatchConfig::default(),
            scope: BatchScope::default(),
            metadata: BatchMetadata::default(),
        };

        coordinator.create_batch(batch).await.unwrap();

        // Execute batch
        let context = BatchExecutionContext {
            batch_id: "failure-test".to_string(),
            execution_id: uuid::Uuid::new_v4().to_string(),
            timestamp: SystemTime::now(),
            mode: ExecutionMode::Normal,
            dry_run: false,
            additional_context: HashMap::new(),
        };

        let execution_id = coordinator.execute_batch("failure-test", context).await.unwrap();

        // Wait for execution to complete
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Check execution result
        let result = coordinator.get_execution_result(&execution_id).await.unwrap();
        assert!(result.is_some());

        let execution_result = result.unwrap();

        // Execution should complete (some items may fail)
        assert_eq!(execution_result.item_results.len(), 3);

        // Check that we have both successes and failures
        let success_count = execution_result.item_results.iter().filter(|r| r.success).count();
        let failure_count = execution_result.item_results.iter().filter(|r| !r.success).count();

        assert!(success_count > 0);
        assert!(failure_count > 0);
    }

    // ============================================================================
    // PERFORMANCE TESTS
    // ============================================================================

    #[tokio::test]
    async fn test_large_batch_execution_performance() {
        let coordinator = create_test_batch_coordinator();
        coordinator.initialize().await.unwrap();

        // Create large batch
        let mut operations = Vec::new();
        for i in 0..100 {
            operations.push(BatchOperationItem {
                item_id: format!("item-{}", i),
                operation: LifecycleOperation::Start { instance_id: format!("instance-{}", i) },
                target: format!("instance-{}", i),
                priority: BatchItemPriority::Normal,
                dependencies: vec![],
                timeout: Some(Duration::from_millis(10)),
                retry_config: None,
                rollback_config: None,
                metadata: HashMap::new(),
            });
        }

        let batch = BatchOperation {
            batch_id: "large-batch".to_string(),
            name: "Large Batch".to_string(),
            description: "Test large batch execution".to_string(),
            operations,
            strategy: BatchExecutionStrategy::Parallel {
                max_concurrent: 10,
                stop_on_failure: false,
                failure_handling: FailureHandling::Continue,
            },
            config: BatchConfig::default(),
            scope: BatchScope::default(),
            metadata: BatchMetadata::default(),
        };

        coordinator.create_batch(batch).await.unwrap();

        // Benchmark batch execution
        let start_time = SystemTime::now();

        let context = BatchExecutionContext {
            batch_id: "large-batch".to_string(),
            execution_id: uuid::Uuid::new_v4().to_string(),
            timestamp: SystemTime::now(),
            mode: ExecutionMode::Normal,
            dry_run: false,
            additional_context: HashMap::new(),
        };

        let execution_id = coordinator.execute_batch("large-batch", context).await.unwrap();

        // Wait for completion
        tokio::time::sleep(Duration::from_millis(1000)).await;

        let execution_time = SystemTime::now().duration_since(start_time).unwrap();

        // Check result
        let result = coordinator.get_execution_result(&execution_id).await.unwrap();
        assert!(result.is_some());

        let execution_result = result.unwrap();
        assert_eq!(execution_result.item_results.len(), 100);

        // Performance assertion - should handle 100 items efficiently
        assert!(execution_time < Duration::from_secs(2), "Large batch execution too slow: {:?}", execution_time);

        // Check metrics
        let metrics = coordinator.get_metrics().await.unwrap();
        assert_eq!(metrics.total_items_processed, 100);
    }

    // ============================================================================
    // MOCK HELPERS
    // ============================================================================

    fn create_test_batch_coordinator() -> BatchOperationsCoordinator {
        let lifecycle_manager = Arc::new(create_mock_lifecycle_manager());
        let policy_engine = Arc::new(create_test_lifecycle_policy());
        let dependency_resolver = Arc::new(create_test_dependency_resolver());
        let state_machine = Arc::new(create_test_state_machine());
        let automation_engine = Arc::new(create_test_automation_engine());

        BatchOperationsCoordinator::new(
            lifecycle_manager,
            policy_engine,
            dependency_resolver,
            state_machine,
            automation_engine,
        )
    }

    fn create_test_automation_engine() -> AutomationEngine {
        let lifecycle_manager = Arc::new(create_mock_lifecycle_manager());
        let policy_engine = Arc::new(create_test_lifecycle_policy());
        let dependency_resolver = Arc::new(create_test_dependency_resolver());
        let state_machine = Arc::new(create_test_state_machine());

        AutomationEngine::new(
            lifecycle_manager,
            policy_engine,
            dependency_resolver,
            state_machine,
        )
    }

    fn create_mock_lifecycle_manager() -> MockLifecycleManager {
        MockLifecycleManager::new()
    }

    // Mock lifecycle manager for testing
    struct MockLifecycleManager {
        // Mock implementation would go here
    }

    impl MockLifecycleManager {
        fn new() -> Self {
            Self {}
        }
    }

    #[async_trait]
    impl LifecycleManagerService for MockLifecycleManager {
        async fn queue_operation(&self, _request: LifecycleOperationRequest) -> PluginResult<String> {
            Ok(uuid::Uuid::new_v4().to_string())
        }

        async fn get_operation_status(&self, _operation_id: &str) -> PluginResult<Option<LifecycleOperationResult>> {
            Ok(None)
        }

        async fn cancel_operation(&self, _operation_id: &str) -> PluginResult<bool> {
            Ok(false)
        }

        async fn execute_batch(&self, _request: BatchOperationRequest) -> PluginResult<String> {
            Ok(uuid::Uuid::new_v4().to_string())
        }

        async fn get_batch_status(&self, _batch_id: &str) -> PluginResult<Option<BatchOperationResult>> {
            Ok(None)
        }

        async fn cancel_batch(&self, _batch_id: &str) -> PluginResult<bool> {
            Ok(false)
        }

        async fn start_instance_with_dependencies(&self, _instance_id: &str) -> PluginResult<()> {
            Ok(())
        }

        async fn stop_instance_gracefully(&self, _instance_id: &str, _drain_period: Option<Duration>) -> PluginResult<()> {
            Ok(())
        }

        async fn restart_instance_zero_downtime(&self, _instance_id: &str) -> PluginResult<()> {
            Ok(())
        }

        async fn scale_plugin(&self, _plugin_id: &str, _target_instances: u32) -> PluginResult<Vec<String>> {
            Ok(vec![])
        }

        async fn rolling_update(&self, _plugin_id: &str, _target_version: String, _strategy: RollingUpdateStrategy) -> PluginResult<()> {
            Ok(())
        }

        async fn subscribe_events(&self) -> tokio::sync::mpsc::UnboundedReceiver<LifecycleEvent> {
            let (_tx, rx) = tokio::sync::mpsc::unbounded_channel();
            rx
        }

        async fn get_metrics(&self) -> PluginResult<LifecycleManagerMetrics> {
            Ok(LifecycleManagerMetrics::default())
        }
    }
}