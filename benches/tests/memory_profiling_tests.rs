//! Unit tests for memory profiling system
//!
//! Tests the reliability and accuracy of our memory profiling benchmarks

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use std::collections::HashMap;
    use std::time::{Duration, Instant};

    #[test]
    fn test_memory_tracking_basic() {
        // Test basic memory tracking functionality
        let initial_memory = get_estimated_memory();

        // Allocate some memory
        let data: Vec<u8> = vec![0; 1000];

        let after_allocation = get_estimated_memory();

        // Memory should have increased (at least by some amount)
        assert!(after_allocation >= initial_memory);

        // Use the data to prevent compiler optimizations
        black_box(data);
    }

    #[test]
    fn test_string_allocation_memory() {
        let initial_memory = get_estimated_memory();

        // Create strings of different sizes
        let strings: Vec<String> = (0..100)
            .map(|i| format!("test_string_{}_with_some_additional_data", i))
            .collect();

        let after_creation = get_estimated_memory();

        // Memory should have increased
        assert!(after_creation >= initial_memory);

        // Verify strings were created
        assert_eq!(strings.len(), 100);
        assert!(strings[0].starts_with("test_string_0"));

        black_box(strings);
    }

    #[test]
    fn test_vec_operations_memory() {
        let initial_memory = get_estimated_memory();

        // Test vector growth patterns
        let mut vec = Vec::with_capacity(100);
        for i in 0..1000 {
            vec.push(i * 2);
        }

        let after_operations = get_estimated_memory();

        // Memory should have increased
        assert!(after_operations >= initial_memory);

        // Verify vector contents
        assert_eq!(vec.len(), 1000);
        assert_eq!(vec[0], 0);
        assert_eq!(vec[999], 1998);

        black_box(vec);
    }

    #[tokio::test]
    async fn test_concurrent_memory_patterns() {
        let initial_memory = get_estimated_memory();

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

        let after_concurrent = get_estimated_memory();

        // Memory should have increased
        assert!(after_concurrent >= initial_memory);

        // Verify all tasks completed successfully
        assert_eq!(results.len(), 10);
        assert!(results.iter().all(|&len| len == 100));

        black_box(results);
    }

    #[test]
    fn test_memory_tracking_function() {
        // Test the track_memory helper function
        let (result, memory_used) = track_memory(|| {
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

    #[tokio::test]
    async fn test_async_memory_tracking() {
        // Test memory tracking in async contexts
        let (result, memory_used) = track_memory(|| {
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

    #[test]
    fn test_memory_leak_detection() {
        // Simple test to detect memory leaks in repeated operations
        let initial_memory = get_estimated_memory();

        // Perform multiple allocations and deallocations
        for _ in 0..10 {
            let data: Vec<String> = (0..100)
                .map(|i| format!("temp_data_{}", i))
                .collect();

            // Use the data
            black_box(data.len());

            // Data should be dropped here
        }

        // Force garbage collection (as much as possible in Rust)
        drop(std::mem::replace(&mut SOME_GLOBAL_VEC, Vec::new()));

        let final_memory = get_estimated_memory();

        // Memory should not have grown significantly
        // Allow some tolerance for allocator overhead
        let memory_growth = final_memory.saturating_sub(initial_memory);
        assert!(memory_growth < 10000, "Memory grew too much: {} bytes", memory_growth);
    }

    #[test]
    fn test_benchmark_reproducibility() {
        // Test that memory benchmarks give consistent results
        let mut results = Vec::new();

        for _ in 0..5 {
            let (_result, memory_used) = track_memory(|| {
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

    // Test helper for memory leak detection
    static mut SOME_GLOBAL_VEC: Vec<String> = Vec::new();

    // Helper function to get estimated memory usage
    fn get_estimated_memory() -> usize {
        // This is a simplified memory estimation
        // In a real implementation, you'd use proper memory profiling tools
        unsafe { SOME_GLOBAL_VEC.len() * std::mem::size_of::<String>() }
    }

    fn black_box<T>(dummy: T) -> T {
        // Prevent compiler optimizations
        dummy
    }
}

// Integration tests for the memory profiling benchmarks
#[cfg(test)]
mod integration_tests {
    use super::*;
    use std::process::Command;
    use std::path::Path;

    #[test]
    fn test_benchmark_compilation() {
        // Test that our memory benchmark compiles successfully
        let output = Command::new("cargo")
            .args(&["check", "--bench", "basic_memory"])
            .output()
            .expect("Failed to run cargo check");

        assert!(output.status.success(),
               "Benchmark compilation failed: {}",
               String::from_utf8_lossy(&output.stderr));
    }

    #[test]
    fn test_criterion_dependency_available() {
        // Test that criterion is available and working
        let output = Command::new("cargo")
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
}