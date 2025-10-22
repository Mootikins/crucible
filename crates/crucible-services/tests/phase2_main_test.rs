//! # Phase 2 Main Test Execution
//!
//! This is the main test file that executes our comprehensive Phase 2 service integration tests.
//! It serves as the entry point for running all Phase 2 validation tests.

use std::env;
use std::time::Duration;

// Import our Phase 2 test modules
use crucible_services::tests::{
    phase2_integration_tests::*,
    phase2_test_runner::{run_phase2_tests, run_quick_phase2_tests, TestRunnerConfig},
};

/// Main Phase 2 test execution
#[tokio::test]
async fn test_phase2_complete_integration() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("\n🎯 Phase 2 Complete Service Integration Test");
    println!("=========================================");

    // Check if we're in quick mode (for CI/CD)
    let quick_mode = env::var("QUICK_TEST").is_ok();

    if quick_mode {
        println!("Running in QUICK mode for CI/CD...");

        let results = run_quick_phase2_tests().await?;

        // Assert that tests passed
        assert!(results.test_results.success, "Phase 2 quick tests failed!");

        println!("✅ Phase 2 quick tests PASSED!");
    } else {
        println!("Running comprehensive Phase 2 tests...");

        let results = run_phase2_tests().await?;

        // Assert that tests passed
        assert!(results.test_results.success, "Phase 2 comprehensive tests failed!");

        println!("✅ Phase 2 comprehensive tests PASSED!");
    }

    Ok(())
}

/// Test full service stack validation specifically
#[tokio::test]
async fn test_phase2_full_service_stack() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("\n🏗️  Phase 2 Full Service Stack Validation");
    println!("======================================");

    let config = Phase2TestConfig {
        enable_full_stack: true,
        enable_cross_service_workflows: false,
        enable_performance_testing: false,
        enable_error_recovery_testing: false,
        enable_memory_testing: false,
        enable_lifecycle_testing: false,
        event_timeout_ms: 5000,
        max_retries: 3,
        concurrent_operations: 10,
        memory_test_duration_secs: 10,
    };

    let mut test_suite = Phase2ServiceTestSuite::new(config).await?;
    let result = test_suite.test_full_service_stack().await;

    assert!(result.success, "Full service stack validation failed: {:?}", result.error);

    println!("✅ Full service stack validation PASSED!");
    Ok(())
}

/// Test event-driven coordination specifically
#[tokio::test]
async fn test_phase2_event_driven_coordination() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("\n🔄 Phase 2 Event-Driven Coordination Test");
    println!("=======================================");

    let config = Phase2TestConfig {
        enable_full_stack: false,
        enable_cross_service_workflows: false,
        enable_performance_testing: false,
        enable_error_recovery_testing: false,
        enable_memory_testing: false,
        enable_lifecycle_testing: false,
        event_timeout_ms: 5000,
        max_retries: 3,
        concurrent_operations: 10,
        memory_test_duration_secs: 10,
    };

    let mut test_suite = Phase2ServiceTestSuite::new(config).await?;
    let result = test_suite.test_event_driven_coordination().await;

    assert!(result.success, "Event-driven coordination test failed: {:?}", result.error);

    println!("✅ Event-driven coordination test PASSED!");
    Ok(())
}

/// Test cross-service workflows specifically
#[tokio::test]
async fn test_phase2_cross_service_workflows() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("\n🔗 Phase 2 Cross-Service Workflows Test");
    println!("======================================");

    let config = Phase2TestConfig {
        enable_full_stack: false,
        enable_cross_service_workflows: true,
        enable_performance_testing: false,
        enable_error_recovery_testing: false,
        enable_memory_testing: false,
        enable_lifecycle_testing: false,
        event_timeout_ms: 5000,
        max_retries: 3,
        concurrent_operations: 10,
        memory_test_duration_secs: 10,
    };

    let mut test_suite = Phase2ServiceTestSuite::new(config).await?;
    let result = test_suite.test_cross_service_workflows().await;

    assert!(result.success, "Cross-service workflows test failed: {:?}", result.error);

    println!("✅ Cross-service workflows test PASSED!");
    Ok(())
}

/// Test performance under load specifically
#[tokio::test]
async fn test_phase2_performance_under_load() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("\n⚡ Phase 2 Performance Under Load Test");
    println!("=====================================");

    let config = Phase2TestConfig {
        enable_full_stack: false,
        enable_cross_service_workflows: false,
        enable_performance_testing: true,
        enable_error_recovery_testing: false,
        enable_memory_testing: false,
        enable_lifecycle_testing: false,
        event_timeout_ms: 15000, // Longer timeout for performance tests
        max_retries: 3,
        concurrent_operations: 20, // Reduced for test stability
        memory_test_duration_secs: 10,
    };

    let mut test_suite = Phase2ServiceTestSuite::new(config).await?;
    let result = test_suite.test_performance_under_load().await;

    assert!(result.success, "Performance under load test failed: {:?}", result.error);

    println!("✅ Performance under load test PASSED!");
    Ok(())
}

/// Test error handling and recovery specifically
#[tokio::test]
async fn test_phase2_error_handling_recovery() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("\n🛡️  Phase 2 Error Handling and Recovery Test");
    println!("==========================================");

    let config = Phase2TestConfig {
        enable_full_stack: false,
        enable_cross_service_workflows: false,
        enable_performance_testing: false,
        enable_error_recovery_testing: true,
        enable_memory_testing: false,
        enable_lifecycle_testing: false,
        event_timeout_ms: 5000,
        max_retries: 5, // More retries for error recovery tests
        concurrent_operations: 10,
        memory_test_duration_secs: 10,
    };

    let mut test_suite = Phase2ServiceTestSuite::new(config).await?;
    let result = test_suite.test_error_handling_and_recovery().await;

    assert!(result.success, "Error handling and recovery test failed: {:?}", result.error);

    println!("✅ Error handling and recovery test PASSED!");
    Ok(())
}

/// Test configuration and lifecycle management specifically
#[tokio::test]
async fn test_phase2_configuration_lifecycle() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("\n⚙️  Phase 2 Configuration and Lifecycle Test");
    println!("==========================================");

    let config = Phase2TestConfig {
        enable_full_stack: false,
        enable_cross_service_workflows: false,
        enable_performance_testing: false,
        enable_error_recovery_testing: false,
        enable_memory_testing: false,
        enable_lifecycle_testing: true,
        event_timeout_ms: 10000, // Longer timeout for lifecycle tests
        max_retries: 3,
        concurrent_operations: 10,
        memory_test_duration_secs: 10,
    };

    let mut test_suite = Phase2ServiceTestSuite::new(config).await?;
    let result = test_suite.test_configuration_and_lifecycle().await;

    assert!(result.success, "Configuration and lifecycle test failed: {:?}", result.error);

    println!("✅ Configuration and lifecycle test PASSED!");
    Ok(())
}

/// Test memory leak and resource management specifically
#[tokio::test]
async fn test_phase2_memory_resource_management() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("\n🧠 Phase 2 Memory Leak and Resource Management Test");
    println!("================================================");

    let config = Phase2TestConfig {
        enable_full_stack: false,
        enable_cross_service_workflows: false,
        enable_performance_testing: false,
        enable_error_recovery_testing: false,
        enable_memory_testing: true,
        enable_lifecycle_testing: false,
        event_timeout_ms: 5000,
        max_retries: 3,
        concurrent_operations: 10,
        memory_test_duration_secs: 15, // Shorter for unit tests
    };

    let mut test_suite = Phase2ServiceTestSuite::new(config).await?;
    let result = test_suite.test_memory_leak_and_resource_management().await;

    assert!(result.success, "Memory leak and resource management test failed: {:?}", result.error);

    println!("✅ Memory leak and resource management test PASSED!");
    Ok(())
}

/// Test JSON-RPC tool pattern specifically
#[tokio::test]
async fn test_phase2_json_rpc_tool_pattern() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("\n🔧 Phase 2 JSON-RPC Tool Pattern Test");
    println!("====================================");

    let config = Phase2TestConfig {
        enable_full_stack: false,
        enable_cross_service_workflows: false,
        enable_performance_testing: false,
        enable_error_recovery_testing: false,
        enable_memory_testing: false,
        enable_lifecycle_testing: false,
        event_timeout_ms: 5000,
        max_retries: 3,
        concurrent_operations: 10,
        memory_test_duration_secs: 10,
    };

    let mut test_suite = Phase2ServiceTestSuite::new(config).await?;
    let result = test_suite.test_json_rpc_tool_pattern().await;

    assert!(result.success, "JSON-RPC tool pattern test failed: {:?}", result.error);

    println!("✅ JSON-RPC tool pattern test PASSED!");
    Ok(())
}

/// Quick integration test for CI/CD pipelines
#[tokio::test]
async fn test_phase2_quick_integration() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("\n⚡ Phase 2 Quick Integration Test (CI/CD)");
    println!("========================================");

    // Set quick mode environment variable
    env::set_var("QUICK_TEST", "true");

    // Run the main test which will pick up the quick mode
    test_phase2_complete_integration().await?;

    // Clean up
    env::remove_var("QUICK_TEST");

    println!("✅ Phase 2 quick integration test PASSED!");
    Ok(())
}

/// Performance regression test
#[tokio::test]
async fn test_phase2_performance_regression() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("\n📊 Phase 2 Performance Regression Test");
    println!("=====================================");

    let config = Phase2TestConfig {
        enable_full_stack: false,
        enable_cross_service_workflows: false,
        enable_performance_testing: true,
        enable_error_recovery_testing: false,
        enable_memory_testing: false,
        enable_lifecycle_testing: false,
        event_timeout_ms: 10000,
        max_retries: 3,
        concurrent_operations: 25,
        memory_test_duration_secs: 10,
    };

    let mut test_suite = Phase2ServiceTestSuite::new(config).await?;
    let result = test_suite.test_performance_under_load().await;

    assert!(result.success, "Performance regression test failed: {:?}", result.error);

    // Extract performance metrics from details
    if let Some(perf_metrics) = result.details.get("performance_metrics") {
        let processing_rate = perf_metrics.get("processing_rate")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        let avg_response_time = perf_metrics.get("average_response_time")
            .and_then(|v| v.as_f64())
            .unwrap_or(f64::MAX);

        let success_rate = perf_metrics.get("success_rate")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        // Performance regression criteria
        assert!(processing_rate > 50.0, "Processing rate regression: {:.2} events/sec", processing_rate);
        assert!(avg_response_time < 200.0, "Response time regression: {:.2} ms", avg_response_time);
        assert!(success_rate > 90.0, "Success rate regression: {:.1}%", success_rate);

        println!("  📈 Performance metrics:");
        println!("    Processing Rate: {:.2} events/sec", processing_rate);
        println!("    Response Time: {:.2} ms", avg_response_time);
        println!("    Success Rate: {:.1}%", success_rate);
    }

    println!("✅ Performance regression test PASSED!");
    Ok(())
}

/// Stress test for production readiness validation
#[tokio::test]
#[ignore] // Use `--ignored` to run this test explicitly
async fn test_phase2_production_stress_test() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("\n🔥 Phase 2 Production Stress Test");
    println!("=================================");
    println!("⚠️  This is an intensive stress test for production validation");
    println!("    Run with: cargo test --test phase2_main_test -- --ignored --test-threads=1");

    let config = Phase2TestConfig {
        enable_full_stack: true,
        enable_cross_service_workflows: true,
        enable_performance_testing: true,
        enable_error_recovery_testing: true,
        enable_memory_testing: true,
        enable_lifecycle_testing: true,
        event_timeout_ms: 30000, // 30 seconds
        max_retries: 10,
        concurrent_operations: 100, // High concurrency
        memory_test_duration_secs: 120, // 2 minutes of memory testing
    };

    let mut test_suite = Phase2ServiceTestSuite::new(config).await?;
    let results = test_suite.execute_complete_test_suite().await?;

    assert!(results.success, "Production stress test failed");

    // Production readiness criteria
    assert!(results.performance_metrics.event_processing_rate > 200.0,
           "Production processing rate too low: {:.2} events/sec",
           results.performance_metrics.event_processing_rate);

    assert!(results.performance_metrics.error_rate < 1.0,
           "Production error rate too high: {:.2}%",
           results.performance_metrics.error_rate);

    assert!(results.performance_metrics.memory_usage_mb < 500.0,
           "Production memory usage too high: {:.2} MB",
           results.performance_metrics.memory_usage_mb);

    println!("✅ Production stress test PASSED!");
    println!("🚀 System is ready for production deployment!");

    Ok(())
}