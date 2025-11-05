//! Comprehensive concurrent CLI operations tests
//!
//! This test suite validates race conditions, database access safety,
//! and performance under concurrent load for the Crucible CLI system.

mod common;

use anyhow::{Context, Result};
use common::{create_basic_kiln, kiln_path_str, run_cli_command as run_cli_support};
use crucible_config::Config;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tempfile::TempDir;
use tokio::sync::{Barrier, RwLock};
use tokio::task::JoinSet;
use uuid::Uuid;

/// Configuration for concurrent operations testing
#[derive(Debug, Clone)]
struct ConcurrentTestConfig {
    /// Number of concurrent operations
    concurrent_count: usize,
    /// Maximum wait time for operations
    timeout_duration: Duration,
    /// Whether to enable detailed timing collection
    collect_timing: bool,
}

impl Default for ConcurrentTestConfig {
    fn default() -> Self {
        Self {
            concurrent_count: 10,
            timeout_duration: Duration::from_secs(30),
            collect_timing: true,
        }
    }
}

/// Results from concurrent operations testing
#[derive(Debug)]
struct ConcurrentTestResults {
    /// Total operations attempted
    total_operations: usize,
    /// Successful operations
    successful_operations: usize,
    /// Failed operations
    failed_operations: usize,
    /// Operation timings (if collected)
    operation_timings: Vec<Duration>,
    /// Average operation duration
    average_duration: Option<Duration>,
    /// Maximum operation duration
    max_duration: Option<Duration>,
    /// Minimum operation duration
    min_duration: Option<Duration>,
}

impl ConcurrentTestResults {
    fn new(collect_timing: bool) -> Self {
        Self {
            total_operations: 0,
            successful_operations: 0,
            failed_operations: 0,
            operation_timings: if collect_timing { Vec::new() } else { Vec::new() },
            average_duration: None,
            max_duration: None,
            min_duration: None,
        }
    }

    fn record_operation(&mut self, success: bool, duration: Option<Duration>) {
        self.total_operations += 1;
        if success {
            self.successful_operations += 1;
        } else {
            self.failed_operations += 1;
        }

        if let Some(dur) = duration {
            self.operation_timings.push(dur);

            self.max_duration = match self.max_duration {
                Some(max) => Some(max.max(dur)),
                None => Some(dur),
            };

            self.min_duration = match self.min_duration {
                Some(min) => Some(min.min(dur)),
                None => Some(dur),
            };
        }

        if !self.operation_timings.is_empty() {
            let total: Duration = self.operation_timings.iter().sum();
            self.average_duration = Some(total / self.operation_timings.len() as u32);
        }
    }

    fn success_rate(&self) -> f64 {
        if self.total_operations == 0 {
            0.0
        } else {
            self.successful_operations as f64 / self.total_operations as f64
        }
    }
}

/// Test fixture for concurrent operations
struct ConcurrentTestFixture {
    /// Temporary directory for test kiln
    temp_dir: TempDir,
    /// Configuration for CLI operations
    config: Config,
    /// Shared results for concurrent operations
    results: Arc<Mutex<ConcurrentTestResults>>,
    /// Cache of the kiln path string
    kiln_path: String,
}

impl ConcurrentTestFixture {
    fn new() -> Result<Self> {
        let temp_dir = create_basic_kiln()?;
        let kiln_path = kiln_path_str(temp_dir.path());
        let config = crucible_config::TestConfig::with_kiln_path(&kiln_path);

        Ok(Self {
            temp_dir,
            config,
            results: Arc::new(Mutex::new(ConcurrentTestResults::new(true))),
            kiln_path,
        })
    }

    fn path(&self) -> &str {
        &self.kiln_path
    }
}

/// Helper to execute CLI command with timing
async fn execute_with_timing(
    args: &[&str],
    config: &Config,
    results: &Arc<Mutex<ConcurrentTestResults>>,
) -> Result<()> {
    let start = Instant::now();

    let result = run_cli_support(args, config).await;
    let duration = start.elapsed();

    let success = result.is_ok();
    let mut results_guard = results.lock().unwrap();
    results_guard.record_operation(success, Some(duration));

    // If operation failed, include error context
    if let Err(e) = result {
        return Err(e.context(format!("CLI operation failed with args: {:?}", args)));
    }

    Ok(())
}

#[tokio::test]
async fn test_concurrent_database_search_operations() -> Result<()> {
    let fixture = ConcurrentTestFixture::new()?;
    let config = fixture.config;
    let results = Arc::new(Mutex::new(ConcurrentTestResults::new(true)));
    let barrier = Arc::new(Barrier::new(20)); // 20 concurrent searches

    let concurrent_config = ConcurrentTestConfig {
        concurrent_count: 20,
        timeout_duration: Duration::from_secs(30),
        collect_timing: true,
    };

    tracing::info!(
        concurrent_count = concurrent_config.concurrent_count,
        "Starting concurrent database search operations test"
    );

    let mut join_set = JoinSet::new();

    for i in 0..concurrent_config.concurrent_count {
        let config = config.clone();
        let results = Arc::clone(&results);
        let barrier = Arc::clone(&barrier);

        join_set.spawn(async move {
            // Wait for all tasks to be ready before starting
            barrier.wait().await;

            // Unique identifier for this operation instance
            let operation_id = Uuid::new_v4().to_string();

            let start_time = Instant::now();

            // Vary search terms to test different query patterns
            let search_terms = ["getting", "started", "test", "content", "basic"];
            let term_idx = i % search_terms.len();

            let result = execute_with_timing(
                &["search", search_terms[term_idx]],
                &config,
                &results,
            ).await;

            let duration = start_time.elapsed();

            // Record results
            let mut results_guard = results.lock().unwrap();
            let success = result.is_ok();
            results_guard.record_operation(success, Some(duration));

            if let Err(e) = result {
                tracing::error!(
                    operation_id = %operation_id,
                    operation_index = i,
                    duration_ms = duration.as_millis(),
                    error = %e,
                    "Concurrent operation failed"
                );
                Err(e)
            } else {
                tracing::debug!(
                    operation_id = %operation_id,
                    operation_index = i,
                    duration_ms = duration.as_millis(),
                    "Concurrent operation succeeded"
                );
                Ok(())
            }
        });
    }

    // Wait for all operations to complete with timeout
    let test_start = Instant::now();
    while !join_set.is_empty() {
        if test_start.elapsed() > concurrent_config.timeout_duration {
            // Abort remaining tasks
            join_set.abort_all();
            anyhow::bail!("Concurrent operations timed out after {:?}", concurrent_config.timeout_duration);
        }

        if let Some(join_result) = join_set.join_next().await {
            match join_result {
                Ok(task_result) => {
                    task_result.with_context(|| "Concurrent task panicked")?;
                }
                Err(join_error) => {
                    tracing::warn!(error = %join_error, "Task join failed (likely cancelled)");
                }
            }
        } else {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    // Analyze results
    let final_results = results.lock().unwrap();
    let success_rate = final_results.success_rate();

    tracing::info!(
        total_operations = final_results.total_operations,
        successful_operations = final_results.successful_operations,
        failed_operations = final_results.failed_operations,
        success_rate = format!("{:.2}%", success_rate * 100.0),
        average_duration_ms = final_results.average_duration.map(|d| d.as_millis()),
        max_duration_ms = final_results.max_duration.map(|d| d.as_millis()),
        min_duration_ms = final_results.min_duration.map(|d| d.as_millis()),
        "Concurrent search operations completed"
    );

    // Assertions for test validation
    assert_eq!(final_results.total_operations, concurrent_config.concurrent_count);
    assert!(success_rate >= 0.8, "Success rate should be at least 80%, got {:.2}%", success_rate * 100.0);

    if let Some(max_duration) = final_results.max_duration {
        assert!(max_duration < Duration::from_secs(10),
                "Maximum operation duration should be under 10 seconds, got {:?}", max_duration);
    }

    Ok(())
}

#[tokio::test]
async fn test_concurrent_document_creation_operations() -> Result<()> {
    let fixture = ConcurrentTestFixture::new()?;
    let config = fixture.config;
    let results = Arc::new(Mutex::new(ConcurrentTestResults::new(true)));
    let barrier = Arc::new(Barrier::new(15)); // 15 concurrent document creations

    let concurrent_count = 15;
    let timeout_duration = Duration::from_secs(45);

    tracing::info!(
        concurrent_count,
        "Starting concurrent document creation operations test"
    );

    let mut join_set = JoinSet::new();

    // Prepare unique document content for each operation
    for i in 0..concurrent_count {
        let config = config.clone();
        let results = Arc::clone(&results);
        let barrier = Arc::clone(&barrier);

        join_set.spawn(async move {
            barrier.wait().await;

            let document_id = Uuid::new_v4();
            let file_name = format!("concurrent_test_doc_{}.md", document_id);
            let content = format!(
                "# Concurrent Test Document {}\n\nCreated at: {}\n\nContent for testing concurrent operations.",
                i,
                chrono::Utc::now().to_rfc3339()
            );

            let start_time = Instant::now();

            // Create the document file
            let file_path = std::path::PathBuf::from(config.kiln_path_opt().unwrap_or_else(|| ".".to_string())).join(&file_name);

            let result = tokio::task::spawn_blocking(move || {
                std::fs::write(&file_path, content)
                    .context("Failed to write document file")
            }).await;

            let duration = start_time.elapsed();

            let success = result.is_ok() && result.as_ref().unwrap().is_ok();
            let mut results_guard = results.lock().unwrap();
            results_guard.record_operation(success, Some(duration));

            match result {
                Ok(Ok(_)) => {
                    tracing::debug!(
                        document_index = i,
                        file_name = %file_name,
                        duration_ms = duration.as_millis(),
                        "Document created successfully"
                    );
                    Ok(())
                }
                Ok(Err(e)) => {
                    tracing::error!(
                        document_index = i,
                        file_name = %file_name,
                        error = %e,
                        "Document creation failed"
                    );
                    Err(e)
                }
                Err(join_error) => {
                    tracing::error!(
                        document_index = i,
                        error = %join_error,
                        "Document creation task panicked"
                    );
                    Err(anyhow::anyhow!("Task panicked: {}", join_error))
                }
            }
        });
    }

    // Wait for all operations to complete
    let test_start = Instant::now();
    while !join_set.is_empty() {
        if test_start.elapsed() > timeout_duration {
            join_set.abort_all();
            anyhow::bail!("Concurrent document creation timed out after {:?}", timeout_duration);
        }

        if let Some(join_result) = join_set.join_next().await {
            match join_result {
                Ok(task_result) => {
                    task_result.with_context(|| "Concurrent document creation task panicked")?;
                }
                Err(join_error) => {
                    tracing::warn!(error = %join_error, "Document creation task join failed");
                }
            }
        } else {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    // Analyze results
    let final_results = results.lock().unwrap();
    let success_rate = final_results.success_rate();

    tracing::info!(
        total_operations = final_results.total_operations,
        successful_operations = final_results.successful_operations,
        success_rate = format!("{:.2}%", success_rate * 100.0),
        "Concurrent document creation completed"
    );

    assert_eq!(final_results.total_operations, concurrent_count);
    assert!(success_rate >= 0.9, "Document creation success rate should be at least 90%");

    Ok(())
}

#[tokio::test]
async fn test_race_condition_metadata_updates() -> Result<()> {
    let fixture = ConcurrentTestFixture::new()?;
    let config = fixture.config.clone();
    let kiln_path = fixture.path().to_string();
    let results = Arc::new(Mutex::new(ConcurrentTestResults::new(true)));

    // Create a test document that we'll update concurrently
    let test_doc_path = std::path::PathBuf::from(&kiln_path).join("race_condition_test.md");
    let initial_content = "# Race Condition Test\n\nInitial content for testing concurrent metadata updates.";
    std::fs::write(&test_doc_path, initial_content)?;

    let barrier = Arc::new(Barrier::new(8)); // 8 concurrent metadata updates
    let concurrent_count = 8;

    tracing::info!(
        concurrent_count,
        "Starting race condition metadata updates test"
    );

    let mut join_set = JoinSet::new();

    for i in 0..concurrent_count {
        let _config = config.clone();
        let results = Arc::clone(&results);
        let barrier = Arc::clone(&barrier);
        let doc_path = test_doc_path.clone();

        join_set.spawn(async move {
            barrier.wait().await;

            let start_time = Instant::now();

            // Simulate metadata update operation by modifying file attributes
            let operation_id = Uuid::new_v4();

            let result = tokio::task::spawn_blocking(move || {
                // Read current content
                let mut current_content = std::fs::read_to_string(&doc_path)?;

                // Add update marker
                let update_marker = format!("\n\nUpdate from operation {} at {}",
                    operation_id, chrono::Utc::now().to_rfc3339());
                current_content.push_str(&update_marker);

                // Write back (this is where race conditions could occur)
                std::fs::write(&doc_path, current_content)?;

                // Simulate some processing time
                std::thread::sleep(Duration::from_millis(50 + (i as u64 % 200)));

                Ok::<(), anyhow::Error>(())
            }).await;

            let duration = start_time.elapsed();

            let success = result.is_ok() && result.as_ref().unwrap().as_ref().is_ok();
            let mut results_guard = results.lock().unwrap();
            results_guard.record_operation(success, Some(duration));

            match result {
                Ok(Ok(_)) => {
                    tracing::debug!(
                        operation_index = i,
                        operation_id = %operation_id,
                        duration_ms = duration.as_millis(),
                        "Metadata update succeeded"
                    );
                    Ok(())
                }
                Ok(Err(e)) => {
                    tracing::error!(
                        operation_index = i,
                        operation_id = %operation_id,
                        error = %e,
                        "Metadata update failed"
                    );
                    Err(e)
                }
                Err(join_error) => {
                    tracing::error!(
                        operation_index = i,
                        error = %join_error,
                        "Metadata update task panicked"
                    );
                    Err(anyhow::anyhow!("Task panicked: {}", join_error))
                }
            }
        });
    }

    // Wait for completion
    let test_start = Instant::now();
    let timeout_duration = Duration::from_secs(30);

    while !join_set.is_empty() {
        if test_start.elapsed() > timeout_duration {
            join_set.abort_all();
            anyhow::bail!("Metadata updates timed out");
        }

        if let Some(join_result) = join_set.join_next().await {
            match join_result {
                Ok(task_result) => {
                    task_result.with_context(|| "Metadata update task panicked")?;
                }
                Err(join_error) => {
                    tracing::warn!(error = %join_error, "Metadata update task join failed");
                }
            }
        } else {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    // Verify data integrity
    let final_content = std::fs::read_to_string(&test_doc_path)?;
    let update_count = final_content.matches("Update from operation").count();

    tracing::info!(
        expected_updates = concurrent_count,
        actual_updates = update_count,
        "Race condition test completed"
    );

    // We should have all updates present (no data loss)
    assert_eq!(update_count, concurrent_count,
              "All updates should be present, but found only {}", update_count);

    let final_results = results.lock().unwrap();
    let success_rate = final_results.success_rate();
    assert!(success_rate >= 0.8, "Metadata update success rate should be at least 80%");

    Ok(())
}

#[tokio::test]
async fn test_resource_contention_database_locks() -> Result<()> {
    let fixture = ConcurrentTestFixture::new()?;
    let config = fixture.config;
    let results = Arc::new(Mutex::new(ConcurrentTestResults::new(true)));

    // Test with higher concurrency to stress database locking
    let barrier = Arc::new(Barrier::new(25)); // 25 concurrent database operations
    let concurrent_count = 25;
    let timeout_duration = Duration::from_secs(60);

    tracing::info!(
        concurrent_count,
        "Starting resource contention database locks test"
    );

    let mut join_set = JoinSet::new();

    for i in 0..concurrent_count {
        let config = config.clone();
        let results = Arc::clone(&results);
        let barrier = Arc::clone(&barrier);

        join_set.spawn(async move {
            barrier.wait().await;

            // Mix of operations that would contend for database locks
            let operations = [
                vec!["search", "test"],
                vec!["search", "content"],
                vec!["search", "getting"],
                vec!["search", "started"],
            ];

            let operation_idx = i % operations.len();
            let operation_args = &operations[operation_idx];

            // Add some randomness to timing
            let delay = Duration::from_millis((i * 10) as u64 % 100);
            tokio::time::sleep(delay).await;

            let start_time = Instant::now();

            let result = run_cli_support(operation_args, &config).await;
            let duration = start_time.elapsed();

            let success = result.is_ok();
            let mut results_guard = results.lock().unwrap();
            results_guard.record_operation(success, Some(duration));

            if let Err(e) = result {
                Err(e.context(format!("CLI operation failed with args: {:?}", operation_args)))
            } else {
                Ok(())
            }
        });
    }

    // Wait for completion with extended timeout for lock contention
    let test_start = Instant::now();
    while !join_set.is_empty() {
        if test_start.elapsed() > timeout_duration {
            join_set.abort_all();
            anyhow::bail!("Database lock contention test timed out");
        }

        if let Some(join_result) = join_set.join_next().await {
            match join_result {
                Ok(task_result) => {
                    task_result.with_context(|| "Database contention task panicked")?;
                }
                Err(join_error) => {
                    tracing::warn!(error = %join_error, "Database contention task join failed");
                }
            }
        } else {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    let final_results = results.lock().unwrap();
    let success_rate = final_results.success_rate();

    tracing::info!(
        total_operations = final_results.total_operations,
        successful_operations = final_results.successful_operations,
        success_rate = format!("{:.2}%", success_rate * 100.0),
        max_duration_ms = final_results.max_duration.map(|d| d.as_millis()),
        "Database lock contention test completed"
    );

    // Under high contention, we still expect reasonable success rate
    assert_eq!(final_results.total_operations, concurrent_count);
    assert!(success_rate >= 0.7, "Success rate under contention should be at least 70%");

    // Maximum duration should be reasonable even under contention
    if let Some(max_duration) = final_results.max_duration {
        assert!(max_duration < Duration::from_secs(30),
                "Maximum duration under contention should be under 30 seconds, got {:?}", max_duration);
    }

    Ok(())
}

#[tokio::test]
async fn test_performance_scalability_concurrent_operations() -> Result<()> {
    let fixture = ConcurrentTestFixture::new()?;
    let config = fixture.config;

    // Test scalability with increasing concurrent operations
    let concurrency_levels = vec![5, 10, 25, 50];
    let mut performance_results = Vec::new();

    for &concurrent_count in &concurrency_levels {
        let results = Arc::new(Mutex::new(ConcurrentTestResults::new(true)));
        let barrier = Arc::new(Barrier::new(concurrent_count));

        tracing::info!(
            concurrent_count,
            "Testing scalability with concurrent operations"
        );

        let start_time = Instant::now();

        let mut join_set = JoinSet::new();

        for i in 0..concurrent_count {
            let config = config.clone();
            let results = Arc::clone(&results);
            let barrier = Arc::clone(&barrier);

            join_set.spawn(async move {
                barrier.wait().await;

                let start_time = Instant::now();

                let result = run_cli_support(&["search", "test"], &config).await;
                let duration = start_time.elapsed();

                let success = result.is_ok();
                let mut results_guard = results.lock().unwrap();
                results_guard.record_operation(success, Some(duration));

                if let Err(e) = result {
                    Err(e)
                } else {
                    Ok(())
                }
            });
        }

        // Wait for completion
        while !join_set.is_empty() {
            if let Some(join_result) = join_set.join_next().await {
                match join_result {
                    Ok(task_result) => {
                        task_result.with_context(|| "Scalability test task panicked")?;
                    }
                    Err(join_error) => {
                        tracing::warn!(error = %join_error, "Scalability test task join failed");
                    }
                }
            } else {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        }

        let total_time = start_time.elapsed();
        let final_results = results.lock().unwrap();

        performance_results.push((
            concurrent_count,
            final_results.success_rate(),
            total_time,
            final_results.average_duration,
        ));

        tracing::info!(
            concurrent_count,
            success_rate = format!("{:.2}%", final_results.success_rate() * 100.0),
            total_time_ms = total_time.as_millis(),
            average_operation_time_ms = final_results.average_duration.map(|d| d.as_millis()),
            "Scalability test completed for level"
        );
    }

    // Analyze scalability
    for (i, (count, success_rate, _total_time, avg_op_time)) in performance_results.iter().enumerate() {
        // Success rate should remain high even at higher concurrency
        assert!(success_rate >= &0.7,
                "Success rate at {} concurrent operations should be >= 70%, got {:.2}%",
                count, success_rate * 100.0);

        // Average operation time shouldn't degrade dramatically
        if i > 0 && avg_op_time.is_some() {
            let prev_avg = performance_results[i-1].3.unwrap_or(Duration::ZERO);
            let current_avg = avg_op_time.unwrap();
            let degradation_ratio = current_avg.as_secs_f64() / prev_avg.as_secs_f64();

            // Allow up to 3x degradation from 5 to 50 concurrent ops
            assert!(degradation_ratio < 3.0,
                    "Average operation time degraded too much: {:.2}x", degradation_ratio);
        }
    }

    // Overall throughput should increase with concurrency
    let initial_throughput = performance_results[0].0 as f64 / performance_results[0].2.as_secs_f64();
    let final_throughput = performance_results.last().unwrap().0 as f64 / performance_results.last().unwrap().2.as_secs_f64();

    tracing::info!(
        initial_throughput = format!("{:.2} ops/sec", initial_throughput),
        final_throughput = format!("{:.2} ops/sec", final_throughput),
        throughput_improvement = format!("{:.2}x", final_throughput / initial_throughput),
        "Scalability analysis completed"
    );

    // We expect some throughput improvement with higher concurrency
    assert!(final_throughput > initial_throughput * 0.5,
            "Throughput should not degrade significantly with higher concurrency");

    Ok(())
}

#[tokio::test]
async fn test_stress_high_concurrent_operations() -> Result<()> {
    let fixture = ConcurrentTestFixture::new()?;
    let config = fixture.config;

    // Stress test with very high concurrency
    let stress_concurrent_count = 100;
    let results = Arc::new(Mutex::new(ConcurrentTestResults::new(true)));
    let barrier = Arc::new(Barrier::new(stress_concurrent_count));
    let timeout_duration = Duration::from_secs(120); // 2 minutes timeout

    tracing::info!(
        stress_concurrent_count,
        "Starting stress test with high concurrent operations"
    );

    let start_time = Instant::now();

    let mut join_set = JoinSet::new();

    for i in 0..stress_concurrent_count {
        let config = config.clone();
        let results = Arc::clone(&results);
        let barrier = Arc::clone(&barrier);

        join_set.spawn(async move {
            barrier.wait().await;

            // Mix of different operation types for realistic stress
            let operations = [
                vec!["search", "test"],
                vec!["search", "content"],
                vec!["search", "getting"],
                vec!["search", "started"],
                vec!["search", "basic"],
            ];

            let operation_idx = i % operations.len();
            let operation_args = &operations[operation_idx];

            // Add small random delay to simulate real-world timing variations
            let delay = Duration::from_millis((i * 7) as u64 % 50);
            tokio::time::sleep(delay).await;

            let start_time = Instant::now();

            let result = run_cli_support(operation_args, &config).await;
            let duration = start_time.elapsed();

            let success = result.is_ok();
            let mut results_guard = results.lock().unwrap();
            results_guard.record_operation(success, Some(duration));

            if let Err(e) = result {
                Err(e.context(format!("CLI operation failed with args: {:?}", operation_args)))
            } else {
                Ok(())
            }
        });
    }

    // Track progress during stress test
    let mut completed = 0;
    while !join_set.is_empty() {
        if start_time.elapsed() > timeout_duration {
            join_set.abort_all();
            anyhow::bail!("Stress test timed out after {:?}", timeout_duration);
        }

        if let Some(join_result) = join_set.join_next().await {
            completed += 1;

            match join_result {
                Ok(task_result) => {
                    task_result.with_context(|| "Stress test task panicked")?;
                }
                Err(join_error) => {
                    tracing::warn!(error = %join_error, "Stress test task join failed");
                }
            }

            // Log progress periodically
            if completed % 20 == 0 {
                let elapsed = start_time.elapsed();
                tracing::info!(
                    completed_operations = completed,
                    total_operations = stress_concurrent_count,
                    elapsed_ms = elapsed.as_millis(),
                    "Stress test progress"
                );
            }
        } else {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    let total_time = start_time.elapsed();
    let final_results = results.lock().unwrap();
    let success_rate = final_results.success_rate();

    tracing::info!(
        total_operations = final_results.total_operations,
        successful_operations = final_results.successful_operations,
        failed_operations = final_results.failed_operations,
        success_rate = format!("{:.2}%", success_rate * 100.0),
        total_time_ms = total_time.as_millis(),
        average_duration_ms = final_results.average_duration.map(|d| d.as_millis()),
        max_duration_ms = final_results.max_duration.map(|d| d.as_millis()),
        throughput = format!("{:.2} ops/sec", stress_concurrent_count as f64 / total_time.as_secs_f64()),
        "Stress test completed"
    );

    // Stress test assertions (more lenient due to high load)
    assert_eq!(final_results.total_operations, stress_concurrent_count);
    assert!(success_rate >= 0.5, "Stress test success rate should be at least 50%");

    // Even under stress, operations shouldn't take too long
    if let Some(max_duration) = final_results.max_duration {
        assert!(max_duration < Duration::from_secs(60),
                "Maximum operation duration under stress should be under 60 seconds");
    }

    // System should handle high throughput
    let throughput = stress_concurrent_count as f64 / total_time.as_secs_f64();
    assert!(throughput > 1.0, "System should handle at least 1 operation per second under stress");

    Ok(())
}

#[tokio::test]
async fn test_concurrent_isolation_and_data_integrity() -> Result<()> {
    let fixture = ConcurrentTestFixture::new()?;
    let config = fixture.config.clone();
    let kiln_path = fixture.path().to_string();

    // Create multiple isolated workspaces within the same kiln
    let workspace_count = 5;
    let workspaces: Arc<RwLock<Vec<String>>> = Arc::new(RwLock::new(Vec::new()));

    // Initialize workspaces
    for i in 0..workspace_count {
        let workspace_id = format!("workspace_{}", i);
        let workspace_path = std::path::PathBuf::from(&kiln_path).join(&workspace_id);
        std::fs::create_dir_all(&workspace_path)?;

        // Create initial content in each workspace
        let content = format!("# Workspace {}\n\nInitial content for workspace {}", i, i);
        let file_path = workspace_path.join("content.md");
        std::fs::write(file_path, content)?;

        workspaces.write().await.push(workspace_id);
    }

    let results = Arc::new(Mutex::new(ConcurrentTestResults::new(true)));
    let barrier = Arc::new(Barrier::new(workspace_count * 3)); // 3 operations per workspace

    tracing::info!(
        workspace_count,
        "Starting concurrent isolation and data integrity test"
    );

    let mut join_set = JoinSet::new();

    // Spawn operations for each workspace
    for workspace_idx in 0..workspace_count {
        for op_idx in 0..3 {
            let config = config.clone();
            let results = Arc::clone(&results);
            let barrier = Arc::clone(&barrier);
            let workspaces = Arc::clone(&workspaces);

            join_set.spawn(async move {
                barrier.wait().await;

                let start_time = Instant::now();

                // Get the specific workspace
                let workspace_id = {
                    let workspaces_guard = workspaces.read().await;
                    workspaces_guard[workspace_idx].clone()
                };

                let workspace_path = std::path::PathBuf::from(config.kiln_path_opt().unwrap_or_else(|| ".".to_string()))
                    .join(&workspace_id);

                // Perform isolated operations
                let result = async move {
                    match op_idx {
                        0 => {
                            // Search operation
                            run_cli_support(&["search", "workspace"], &config).await
                        }
                        1 => {
                            // File creation in workspace
                            let file_name = format!("test_file_{}.md", Uuid::new_v4());
                            let content = format!("Test content created by operation {}", op_idx);
                            let file_path = workspace_path.join(file_name);

                            tokio::task::spawn_blocking(move || {
                                std::fs::write(file_path, content)
                                    .context("Failed to create workspace file")
                            }).await?;

                            Ok(common::CliCommandOutput {
                                stdout: "File created successfully".to_string(),
                                stderr: String::new(),
                            })
                        }
                        2 => {
                            // Read operation
                            let content_path = workspace_path.join("content.md");
                            let content = tokio::task::spawn_blocking(move || {
                                std::fs::read_to_string(content_path)
                                    .context("Failed to read workspace content")
                            }).await??;

                            Ok(common::CliCommandOutput {
                                stdout: content,
                                stderr: String::new(),
                            })
                        }
                        _ => unreachable!(),
                    }
                }.await;

                let duration = start_time.elapsed();

                let success = result.is_ok();
                let mut results_guard = results.lock().unwrap();
                results_guard.record_operation(success, Some(duration));

                if let Err(e) = result {
                    tracing::error!(
                        workspace_id = %workspace_id,
                        operation_index = op_idx,
                        error = %e,
                        "Isolation test operation failed"
                    );
                    Err(e)
                } else {
                    tracing::debug!(
                        workspace_id = %workspace_id,
                        operation_index = op_idx,
                        duration_ms = duration.as_millis(),
                        "Isolation test operation succeeded"
                    );
                    Ok(())
                }
            });
        }
    }

    // Wait for completion
    let test_start = Instant::now();
    let timeout_duration = Duration::from_secs(60);

    while !join_set.is_empty() {
        if test_start.elapsed() > timeout_duration {
            join_set.abort_all();
            anyhow::bail!("Isolation test timed out");
        }

        if let Some(join_result) = join_set.join_next().await {
            match join_result {
                Ok(task_result) => {
                    task_result.with_context(|| "Isolation test task panicked")?;
                }
                Err(join_error) => {
                    tracing::warn!(error = %join_error, "Isolation test task join failed");
                }
            }
        } else {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    // Verify workspace isolation - each workspace should have exactly the right number of files
    let workspaces_guard = workspaces.read().await;
    for (_i, workspace_id) in workspaces_guard.iter().enumerate() {
        let workspace_path = std::path::PathBuf::from(&kiln_path).join(workspace_id);
        let entries: Vec<_> = std::fs::read_dir(&workspace_path)?
            .filter_map(Result::ok)
            .collect();

        // Should have initial content.md + 1 new file = 2 files total
        assert_eq!(entries.len(), 2,
                  "Workspace {} should have exactly 2 files, found {}", workspace_id, entries.len());

        tracing::debug!(
            workspace_id = %workspace_id,
            file_count = entries.len(),
            "Workspace isolation verified"
        );
    }

    let final_results = results.lock().unwrap();
    let success_rate = final_results.success_rate();

    tracing::info!(
        total_operations = final_results.total_operations,
        successful_operations = final_results.successful_operations,
        success_rate = format!("{:.2}%", success_rate * 100.0),
        "Isolation and data integrity test completed"
    );

    assert_eq!(final_results.total_operations, workspace_count * 3);
    assert!(success_rate >= 0.9, "Isolation test success rate should be at least 90%");

    Ok(())
}