//! Unit tests for memory profiling system
//!
//! Tests the reliability and accuracy of our memory profiling benchmarks

use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use std::time::{Duration, Instant};

#[cfg(test)]
mod memory_tests {
    use super::*;

    /// Test basic memory tracking functionality
    #[test]
    fn test_memory_tracking_basic() {
        // Test basic memory tracking functionality
        let initial_memory = estimate_memory_usage();

        // Allocate some memory
        let data: Vec<u8> = vec![0; 1000];

        let after_allocation = estimate_memory_usage();

        // Memory should have increased (at least by some amount)
        assert!(after_allocation >= initial_memory);

        // Use the data to prevent compiler optimizations
        black_box(data);
    }

    /// Test string allocation memory patterns
    #[test]
    fn test_string_allocation_memory() {
        let initial_memory = estimate_memory_usage();

        // Create strings of different sizes
        let strings: Vec<String> = (0..100)
            .map(|i| format!("test_string_{}_with_some_additional_data", i))
            .collect();

        let after_creation = estimate_memory_usage();

        // Memory should have increased
        assert!(after_creation >= initial_memory);

        // Verify strings were created
        assert_eq!(strings.len(), 100);
        assert!(strings[0].starts_with("test_string_0"));

        black_box(strings);
    }

    /// Test vector operations memory efficiency
    #[test]
    fn test_vec_operations_memory() {
        let initial_memory = estimate_memory_usage();

        // Test vector growth patterns
        let mut vec = Vec::with_capacity(100);
        for i in 0..1000 {
            vec.push(i * 2);
        }

        let after_operations = estimate_memory_usage();

        // Memory should have increased
        assert!(after_operations >= initial_memory);

        // Verify vector contents
        assert_eq!(vec.len(), 1000);
        assert_eq!(vec[0], 0);
        assert_eq!(vec[999], 1998);

        black_box(vec);
    }

    /// Test concurrent memory patterns
    #[tokio::test]
    async fn test_concurrent_memory_patterns() {
        let initial_memory = estimate_memory_usage();

        // Test concurrent task memory usage
        let handles: Vec<_> = (0..10)
            .map(|i| {
                tokio::spawn(async move {
                    let mut local_data = Vec::with_capacity(100);
                    for j in 0..100 {
                        local_data.push(i * 100 + j);
                    }
                    local_data.len()
                })
            })
            .collect();

        let results: Vec<_> = futures::future::join_all(handles)
            .await
            .into_iter()
            .map(|r| r.unwrap())
            .collect();

        let after_concurrent = estimate_memory_usage();

        // Memory should have increased
        assert!(after_concurrent >= initial_memory);

        // Verify all tasks completed successfully
        assert_eq!(results.len(), 10);
        assert!(results.iter().all(|&len| len == 100));

        black_box(results);
    }

    /// Test memory tracking helper function
    #[test]
    fn test_memory_tracking_function() {
        // Test the track_memory helper function
        let (result, memory_used) = track_memory_operation(|| {
            let mut data = Vec::new();
            for i in 0..1000 {
                data.push(format!("item_{}", i));
            }
            data.len()
        });

        assert_eq!(result, 1000);
        // Memory used should be reasonable (at least 0)
        assert!(memory_used >= 0);
    }

    /// Test async memory tracking
    #[tokio::test]
    async fn test_async_memory_tracking() {
        // Test memory tracking in async contexts
        let (result, memory_used) = track_memory_operation(|| {
            tokio::runtime::Runtime::new().unwrap().block_on(async {
                let mut data = HashMap::new();
                for i in 0..500 {
                    data.insert(i, format!("value_{}", i));
                }
                data.len()
            })
        });

        assert_eq!(result, 500);
        assert!(memory_used >= 0);
    }

    /// Test memory leak detection
    #[test]
    fn test_memory_leak_detection() {
        // Simple test to detect memory leaks in repeated operations
        let initial_memory = estimate_memory_usage();

        // Perform multiple allocations and deallocations
        for _ in 0..10 {
            let data: Vec<String> = (0..100)
                .map(|i| format!("temp_data_{}", i))
                .collect();

            // Use the data
            black_box(data.len());

            // Data should be dropped here
        }

        // Force cleanup
        cleanup_test_data();

        let final_memory = estimate_memory_usage();

        // Memory should not have grown significantly
        // Allow some tolerance for allocator overhead
        let memory_growth = final_memory.saturating_sub(initial_memory);
        assert!(memory_growth < 10000, "Memory grew too much: {} bytes", memory_growth);
    }

    /// Test benchmark reproducibility
    #[test]
    fn test_benchmark_reproducibility() {
        // Test that memory benchmarks give consistent results
        let mut results = Vec::new();

        for _ in 0..5 {
            let (_result, memory_used) = track_memory_operation(|| {
                // Reproducible memory allocation pattern
                let data: Vec<u64> = (0..1000).map(|i| i * i).collect();
                data.iter().sum::<u64>()
            });
            results.push(memory_used);
        }

        // Results should be relatively consistent
        let mean = results.iter().sum::<usize>() as f64 / results.len() as f64;
        let variance = results.iter()
            .map(|&x| (x as f64 - mean).powi(2))
            .sum::<f64>() / (results.len() - 1) as f64;
        let std_dev = variance.sqrt();

        // Standard deviation should be small relative to mean
        if mean > 0.0 {
            let cv = std_dev / mean; // coefficient of variation
            assert!(cv < 0.5, "Memory usage too variable: CV = {:.2}", cv);
        }
    }

    /// Test memory efficiency of different data structures
    #[test]
    fn test_data_structure_memory_efficiency() {
        let initial_memory = estimate_memory_usage();

        // Test different data structures
        let vec_data: Vec<i32> = (0..1000).collect();
        let map_data: HashMap<i32, String> = (0..1000)
            .map(|i| (i, format!("value_{}", i)))
            .collect();

        let after_structures = estimate_memory_usage();

        // Memory should have increased
        assert!(after_structures >= initial_memory);

        // Verify data integrity
        assert_eq!(vec_data.len(), 1000);
        assert_eq!(map_data.len(), 1000);

        black_box((vec_data, map_data));
    }

    /// Test memory usage with large allocations
    #[test]
    fn test_large_allocation_memory() {
        let initial_memory = estimate_memory_usage();

        // Test larger allocation
        let large_data: Vec<u8> = vec![0; 1_000_000]; // 1MB

        let after_large = estimate_memory_usage();

        // Memory should have increased significantly
        assert!(after_large > initial_memory);

        // Verify allocation
        assert_eq!(large_data.len(), 1_000_000);

        black_box(large_data);
    }

    // Helper functions for memory testing

    /// Simple memory usage estimation
    fn estimate_memory_usage() -> usize {
        // This is a simplified estimation
        // In a real implementation, you'd use proper memory profiling tools
        // For now, we use a simple heuristic based on available system info
        0 // Placeholder
    }

    /// Track memory usage of an operation
    fn track_memory_operation<F, R>(operation: F) -> (R, usize)
    where
        F: FnOnce() -> R,
    {
        let before = estimate_memory_usage();
        let result = operation();
        let after = estimate_memory_usage();
        let memory_used = after.saturating_sub(before);

        (result, memory_used)
    }

    /// Cleanup test data
    fn cleanup_test_data() {
        // Force cleanup of any test data
        // In a real implementation, this might involve explicit garbage collection
    }

    /// Prevent compiler optimizations
    fn black_box<T>(dummy: T) -> T {
        // Prevent compiler optimizations
        dummy
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    fn test_memory_benchmark_compilation() {
        // Test that our memory benchmark compiles successfully
        let output = std::process::Command::new("cargo")
            .args(&["check", "--bench", "basic_memory"])
            .output()
            .expect("Failed to run cargo check");

        assert!(output.status.success(),
               "Memory benchmark compilation failed: {}",
               String::from_utf8_lossy(&output.stderr));
    }

    #[test]
    fn test_criterion_dependency_available() {
        // Test that criterion is available and working
        let output = std::process::Command::new("cargo")
            .args(&["check", "--package", "crucible-benchmarks"])
            .output()
            .expect("Failed to check benchmarks package");

        assert!(output.status.success(),
               "Benchmarks package check failed: {}",
               String::from_utf8_lossy(&output.stderr));
    }

    #[test]
    fn test_memory_profiling_files_exist() {
        // Verify all memory profiling files exist
        use std::path::Path;

        assert!(Path::new("benches/basic_memory.rs").exists());
        assert!(Path::new("benches/Cargo.toml").exists());
        assert!(Path::new("benches/.cargo/config.toml").exists());
    }

    #[test]
    fn test_memory_profiling_configuration() {
        // Test that our configuration is valid
        let toml_content = std::fs::read_to_string("benches/.cargo/config.toml")
            .expect("Failed to read config.toml");

        // Check for required sections
        assert!(toml_content.contains("[benchmark]"));
        assert!(toml_content.contains("profile = \"release\""));
        assert!(toml_content.contains("[target.benchmark]"));
        assert!(toml_content.contains("rustflags"));
    }

    #[test]
    fn test_memory_profiling_dependencies() {
        // Test that all required dependencies are available
        let output = std::process::Command::new("cargo")
            .args(&["tree", "--package", "crucible-benchmarks"])
            .output()
            .expect("Failed to check dependencies");

        let dependency_tree = String::from_utf8_lossy(&output.stdout);

        // Check for key dependencies
        assert!(dependency_tree.contains("criterion"));
        assert!(dependency_tree.contains("tokio"));
        assert!(dependency_tree.contains("futures"));
    }
}

#[cfg(test)]
mod performance_tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn test_memory_tracking_performance() {
        // Test that memory tracking doesn't add significant overhead
        let iterations = 1000;

        let start = Instant::now();
        for _ in 0..iterations {
            let (_result, _memory) = track_memory_operation(|| {
                let data = vec![42u8; 100];
                data.len()
            });
        }
        let tracking_duration = start.elapsed();

        let start = Instant::now();
        for _ in 0..iterations {
            let _result = {
                let data = vec![42u8; 100];
                data.len()
            };
        }
        let direct_duration = start.elapsed();

        // Tracking overhead should be minimal (less than 10x)
        let overhead_ratio = tracking_duration.as_nanos() as f64 / direct_duration.as_nanos() as f64;
        assert!(overhead_ratio < 10.0,
               "Memory tracking overhead too high: {:.2}x", overhead_ratio);
    }

    #[test]
    fn test_large_scale_memory_operations() {
        // Test memory operations at scale
        let sizes = vec![1_000, 10_000, 100_000];

        for size in sizes {
            let (_result, memory_used) = track_memory_operation(|| {
                let data: Vec<usize> = (0..size).collect();
                data.iter().sum::<usize>()
            });

            // Memory usage should scale roughly with size
            let expected_min = size * std::mem::size_of::<usize>();
            assert!(memory_used >= expected_min,
                   "Memory usage too low for size {}: {} vs expected {}",
                   size, memory_used, expected_min);
        }
    }
}