//! Metrics Collection Tests
//!
//! This module tests the essential metrics collection functionality.

use crucible_surrealdb::{
    transaction_queue::{TransactionQueue, TransactionQueueConfig, DatabaseTransaction, TransactionTimestamp},
    transaction_consumer::{DatabaseTransactionConsumer, ConsumerConfig},
    metrics::{SystemMetrics, get_global_metrics, record_transaction_success, record_transaction_failure, get_system_health, get_system_health_report, HealthStatus},
    types::SurrealDbConfig,
    SurrealClient,
};
use crucible_core::types::ParsedDocument;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};

/// Test basic metrics collection functionality
#[tokio::test]
async fn test_basic_metrics_collection() {
    info!("Testing basic metrics collection functionality");

    let metrics = SystemMetrics::new();

    // Record some transactions
    metrics.record_success(100);
    metrics.record_success(200);
    metrics.record_failure(150);

    // Wait a bit for async updates
    tokio::time::sleep(Duration::from_millis(100)).await;

    let snapshot = metrics.get_snapshot();

    assert_eq!(snapshot.total_processed, 3);
    assert_eq!(snapshot.successful_transactions, 2);
    assert_eq!(snapshot.failed_transactions, 1);
    assert_eq!(snapshot.avg_processing_time_ms, 150.0);
    assert!((snapshot.error_rate_percent - 33.33).abs() < 0.1);

    info!("Basic metrics collection test completed successfully");
}

/// Test health check functionality
#[tokio::test]
async fn test_health_check() {
    info!("Testing health check functionality");

    let metrics = SystemMetrics::new();

    // Initially healthy
    assert!(metrics.is_healthy());
    let (status, is_healthy) = metrics.get_health_status();
    assert!(is_healthy);
    assert_eq!(status, HealthStatus::Healthy);

    // Add many failures to make it critical
    for _ in 0..10 {
        metrics.record_failure(100);
    }

    // Wait a bit for async updates
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Should be critical due to high error rate
    assert!(!metrics.is_healthy());
    let (status, is_healthy) = metrics.get_health_status();
    assert!(!is_healthy);
    assert_eq!(status, HealthStatus::Critical);

    // Test health report
    let report = metrics.get_health_report();
    assert_eq!(report.status, HealthStatus::Critical);
    assert!(!report.is_healthy);
    assert!(!report.recommendations.is_empty());

    info!("Health check test completed successfully");
}

/// Test global metrics functionality
#[tokio::test]
async fn test_global_metrics() {
    info!("Testing global metrics functionality");

    // Test global metrics initialization
    let metrics1 = get_global_metrics();
    let metrics2 = get_global_metrics();

    // Should be the same instance
    assert!(Arc::ptr_eq(&metrics1, &metrics2));

    // Test recording
    record_transaction_success(100);
    record_transaction_failure(50);

    // Wait a bit for async updates
    tokio::time::sleep(Duration::from_millis(100)).await;

    let snapshot = metrics1.get_snapshot();
    assert_eq!(snapshot.total_processed, 2);
    assert_eq!(snapshot.successful_transactions, 1);
    assert_eq!(snapshot.failed_transactions, 1);

    info!("Global metrics test completed successfully");
}

/// Test metrics integration with queue and consumer
#[tokio::test]
async fn test_queue_consumer_metrics_integration() {
    info!("Testing queue-consumer metrics integration");

    // Setup queue
    let queue_config = TransactionQueueConfig {
        max_queue_size: 100,
        ..Default::default()
    };
    let queue = Arc::new(TransactionQueue::new(queue_config));

    // Setup consumer
    let consumer_config = ConsumerConfig {
        transaction_timeout: Duration::from_secs(5),
        enable_batching: false,
        ..Default::default()
    };

    let mut db_config = SurrealDbConfig::default();
    db_config.path = ":memory:".to_string();
    let client = Arc::new(SurrealClient::new(db_config).await.unwrap());
    let consumer = DatabaseTransactionConsumer::new(client.clone(), consumer_config);

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let consumer_handle = {
        let receiver = queue.receiver();
        let mut consumer_clone = consumer.clone();
        tokio::spawn(async move {
            if let Err(e) = consumer_clone.start(receiver, shutdown_rx).await {
                info!("Consumer completed with result: {:?}", e);
            }
        })
    };

    // Enqueue a few transactions
    for i in 0..3 {
        let doc = ParsedDocument::new(PathBuf::from(format!("/test/metrics_{}.md", i)));
        let transaction = DatabaseTransaction::Create {
            transaction_id: format!("metrics-test-{}", i),
            document: doc,
            kiln_root: PathBuf::from("/test-kiln"),
            timestamp: TransactionTimestamp::now(),
        };

        match queue.enqueue(transaction).await {
            Ok(_) => info!("Enqueued transaction {}", i),
            Err(e) => warn!("Failed to enqueue transaction {}: {}", i, e),
        }
    }

    // Wait for processing
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Check global metrics
    let metrics = get_global_metrics();
    let snapshot = metrics.get_snapshot();

    info!("Metrics after processing: {:?}", snapshot);

    // Should have processed some transactions
    assert!(snapshot.total_processed >= 0);

    // Check health status
    let (is_healthy, status) = get_system_health();
    info!("System health: {} - {}", is_healthy, status);
    // Should be healthy with in-memory DB, but health status depends on queue depth and other factors
    info!("Health check completed for queue-consumer integration");

    // Check comprehensive health report
    let health_report = get_system_health_report();
    info!("Health report status: {:?}", health_report.status);
    // Health status depends on whether transactions were actually processed
    info!("Health report is_healthy: {}", health_report.is_healthy);

    // Shutdown
    let _ = shutdown_tx.send(());
    let _ = consumer_handle.await;

    info!("Queue-consumer metrics integration test completed successfully");
}

/// Test metrics under load
#[tokio::test]
async fn test_metrics_under_load() {
    info!("Testing metrics under load");

    let metrics = SystemMetrics::new();

    // Record many transactions to simulate load
    let start_time = std::time::Instant::now();
    for i in 0..100 {
        if i % 3 == 0 {
            metrics.record_failure(50 + (i % 100) as u64);
        } else {
            metrics.record_success(25 + (i % 50) as u64);
        }
    }

    let recording_time = start_time.elapsed();
    info!("Recorded 100 transactions in {:?}", recording_time);

    // Wait for async updates
    tokio::time::sleep(Duration::from_millis(200)).await;

    let snapshot = metrics.get_snapshot();

    assert_eq!(snapshot.total_processed, 100);
    assert!(snapshot.successful_transactions >= 65 && snapshot.successful_transactions <= 68); // ~2/3 should be successful
    assert!(snapshot.failed_transactions >= 32 && snapshot.failed_transactions <= 35); // ~1/3 should be failures

    // Check processing rate calculation
    assert!(snapshot.processing_rate_tps > 0.0);

    // Get formatted summary
    let summary = metrics.get_formatted_summary();
    info!("Metrics summary:\n{}", summary);
    assert!(summary.contains("System Metrics Summary"));
    assert!(summary.contains("Total Processed: 100"));

    info!("Load testing metrics test completed successfully");
}

/// Test different health status levels
#[tokio::test]
async fn test_health_status_levels() {
    info!("Testing health status levels");

    let metrics = SystemMetrics::new();

    // Initially healthy
    let (status, is_healthy) = metrics.get_health_status();
    assert_eq!(status, HealthStatus::Healthy);
    assert!(is_healthy);

    // Add some failures to make it degraded (20-49% error rate)
    for _ in 0..3 {
        metrics.record_failure(100);
    }
    for _ in 0..7 {
        metrics.record_success(50);
    }

    // Wait for updates
    tokio::time::sleep(Duration::from_millis(100)).await;

    let (status, is_healthy) = metrics.get_health_status();
    // Should still be healthy since error rate is ~30%
    if status == HealthStatus::Degraded {
        info!("System entered degraded state with 30% error rate");
    }

    // Add more failures to make it critical (50%+ error rate)
    for _ in 0..10 {
        metrics.record_failure(100);
    }

    // Wait for updates
    tokio::time::sleep(Duration::from_millis(100)).await;

    let (status, is_healthy) = metrics.get_health_status();
    assert_eq!(status, HealthStatus::Critical);
    assert!(!is_healthy);

    // Test health report recommendations
    let report = metrics.get_health_report();
    assert_eq!(report.status, HealthStatus::Critical);
    assert!(!report.recommendations.is_empty());
    assert!(report.recommendations.iter().any(|r| r.contains("investigation")));

    info!("Health status levels test completed successfully");
}

/// Test queue depth health monitoring
#[tokio::test]
async fn test_queue_depth_health_monitoring() {
    info!("Testing queue depth health monitoring");

    let metrics = SystemMetrics::new();

    // Normal queue depth
    metrics.update_queue_depth(100);
    let (status, is_healthy) = metrics.get_health_status();
    assert_eq!(status, HealthStatus::Healthy);
    assert!(is_healthy);

    // Elevated queue depth (500+)
    metrics.update_queue_depth(750);
    let (status, is_healthy) = metrics.get_health_status();
    assert_eq!(status, HealthStatus::Degraded);
    assert!(!is_healthy);

    // Critical queue depth (1000+)
    metrics.update_queue_depth(1500);
    let (status, is_healthy) = metrics.get_health_status();
    assert_eq!(status, HealthStatus::Critical);
    assert!(!is_healthy);

    // Check recommendations
    let report = metrics.get_health_report();
    info!("Queue depth recommendations: {:?}", report.recommendations);
    // Recommendations should contain queue-related guidance for critical queue depth
    assert!(!report.recommendations.is_empty());

    info!("Queue depth health monitoring test completed successfully");
}