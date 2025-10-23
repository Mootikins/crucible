//! ScriptEngine load testing benchmarks for Phase 6.5
//!
//! Tests concurrent tool execution performance under various load conditions

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};
use std::sync::Arc;
use tokio::runtime::Runtime;
use std::time::{Duration, Instant};
use futures::future::join_all;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Simulate ScriptEngine tool execution under load
struct MockScriptEngine {
    tool_count: AtomicUsize,
}

impl MockScriptEngine {
    fn new() -> Self {
        Self {
            tool_count: AtomicUsize::new(0),
        }
    }

    async fn execute_tool(&self, complexity: ToolComplexity, input_size: usize) -> String {
        let tool_id = self.tool_count.fetch_add(1, Ordering::Relaxed);

        // Simulate tool execution based on complexity
        match complexity {
            ToolComplexity::Simple => self.execute_simple_tool(tool_id, input_size).await,
            ToolComplexity::Medium => self.execute_medium_tool(tool_id, input_size).await,
            ToolComplexity::Complex => self.execute_complex_tool(tool_id, input_size).await,
        }
    }

    async fn execute_simple_tool(&self, tool_id: usize, input_size: usize) -> String {
        // Simple operation: basic string manipulation
        let input = format!("input_{}", tool_id);
        let mut result = String::with_capacity(input_size);

        for i in 0..input_size.min(100) {
            result.push_str(&format!("item_{}_{}", tool_id, i));
        }

        // Simulate some processing time
        tokio::time::sleep(Duration::from_micros(100)).await;

        result
    }

    async fn execute_medium_tool(&self, tool_id: usize, input_size: usize) -> String {
        // Medium operation: data transformation
        let data: Vec<i32> = (0..input_size.min(1000))
            .map(|i| (tool_id * 1000 + i) as i32)
            .collect();

        // Simulate data processing
        let processed: Vec<i32> = data.iter()
            .map(|x| x * 2 + 1)
            .filter(|x| x % 3 == 0)
            .take(input_size.min(500))
            .collect();

        // Add some async work
        tokio::task::yield_now().await;
        tokio::time::sleep(Duration::from_micros(500)).await;

        format!("tool_{}_processed_{}_items", tool_id, processed.len())
    }

    async fn execute_complex_tool(&self, tool_id: usize, input_size: usize) -> String {
        // Complex operation: multi-stage pipeline
        let stages = 3;
        let mut results = Vec::new();

        for stage in 0..stages {
            let stage_data: Vec<String> = (0..input_size.min(200))
                .map(|i| format!("stage_{}_item_{}_{}", stage, tool_id, i))
                .collect();

            // Simulate complex processing for each stage
            let handles: Vec<_> = stage_data.into_iter()
                .enumerate()
                .map(|(i, item)| {
                    tokio::spawn(async move {
                        // Simulate computation
                        let mut result = item;
                        for _ in 0..10 {
                            result = format!("processed_{}", result);
                            tokio::task::yield_now().await;
                        }
                        result
                    })
                })
                .collect();

            let stage_results = join_all(handles).await;
            results.push(format!("stage_{}_completed_{}", stage, stage_results.len()));

            // Simulate I/O or heavy computation
            tokio::time::sleep(Duration::from_millis(1)).await;
        }

        format!("complex_tool_{}_completed: {}", tool_id, results.join(", "))
    }
}

#[derive(Debug, Clone, Copy)]
enum ToolComplexity {
    Simple,
    Medium,
    Complex,
}

impl ToolComplexity {
    fn as_str(&self) -> &'static str {
        match self {
            ToolComplexity::Simple => "simple",
            ToolComplexity::Medium => "medium",
            ToolComplexity::Complex => "complex",
        }
    }
}

/// Benchmark concurrent tool execution with different concurrency levels
fn bench_concurrent_tool_execution(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let engine = Arc::new(MockScriptEngine::new());

    let mut group = c.benchmark_group("concurrent_tool_execution");

    // Test different concurrency levels
    for concurrency in [1, 5, 10, 20, 50].iter() {
        group.throughput(Throughput::Elements(*concurrency as u64));

        group.bench_with_input(
            BenchmarkId::new("simple_tools", concurrency),
            concurrency,
            |b, &concurrency| {
                b.to_async(&rt).iter(|| async {
                    let handles: Vec<_> = (0..concurrency)
                        .map(|_| {
                            let engine = Arc::clone(&engine);
                            tokio::spawn(async move {
                                engine.execute_tool(ToolComplexity::Simple, 100).await
                            })
                        })
                        .collect();

                    let results = join_all(handles).await;
                    let completed = results.len();

                    black_box(completed);
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("medium_tools", concurrency),
            concurrency,
            |b, &concurrency| {
                b.to_async(&rt).iter(|| async {
                    let handles: Vec<_> = (0..concurrency)
                        .map(|_| {
                            let engine = Arc::clone(&engine);
                            tokio::spawn(async move {
                                engine.execute_tool(ToolComplexity::Medium, 500).await
                            })
                        })
                        .collect();

                    let results = join_all(handles).await;
                    let completed = results.len();

                    black_box(completed);
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("complex_tools", concurrency),
            concurrency,
            |b, &concurrency| {
                b.to_async(&rt).iter(|| async {
                    let handles: Vec<_> = (0..concurrency)
                        .map(|_| {
                            let engine = Arc::clone(&engine);
                            tokio::spawn(async move {
                                engine.execute_tool(ToolComplexity::Complex, 200).await
                            })
                        })
                        .collect();

                    let results = join_all(handles).await;
                    let completed = results.len();

                    black_box(completed);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark sustained load over time
fn bench_sustained_load(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let engine = Arc::new(MockScriptEngine::new());

    let mut group = c.benchmark_group("sustained_load");

    // Test different load durations
    for duration in [1, 5, 10].iter() {
        group.bench_with_input(
            BenchmarkId::new("sustained_medium_load", duration),
            duration,
            |b, &duration_secs| {
                b.to_async(&rt).iter(|| async {
                    let start = Instant::now();
                    let duration = Duration::from_secs(duration_secs);
                    let mut total_operations = 0;

                    while start.elapsed() < duration {
                        // Execute a batch of operations
                        let batch_size = 10;
                        let handles: Vec<_> = (0..batch_size)
                            .map(|_| {
                                let engine = Arc::clone(&engine);
                                tokio::spawn(async move {
                                    engine.execute_tool(ToolComplexity::Medium, 300).await
                                })
                            })
                            .collect();

                        let results = join_all(handles).await;
                        total_operations += results.len();

                        // Small delay between batches
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }

                    black_box(total_operations);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark mixed workload execution
fn bench_mixed_workload(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let engine = Arc::new(MockScriptEngine::new());

    let mut group = c.benchmark_group("mixed_workload");

    // Test different workload distributions
    let workloads = vec![
        ("simple_heavy", (70, 20, 10)), // 70% simple, 20% medium, 10% complex
        ("balanced", (33, 33, 34)),    // Balanced distribution
        ("complex_heavy", (10, 30, 60)), // 10% simple, 30% medium, 60% complex
    ];

    for (name, (simple_pct, medium_pct, complex_pct)) in workloads {
        group.bench_function(name, |b| {
            b.to_async(&rt).iter(|| async {
                let total_operations = 100;
                let simple_count = (total_operations * simple_pct) / 100;
                let medium_count = (total_operations * medium_pct) / 100;
                let complex_count = total_operations - simple_count - medium_count;

                let mut handles = Vec::new();

                // Launch simple tools
                for _ in 0..simple_count {
                    let engine = Arc::clone(&engine);
                    handles.push(tokio::spawn(async move {
                        engine.execute_tool(ToolComplexity::Simple, 100).await
                    }));
                }

                // Launch medium tools
                for _ in 0..medium_count {
                    let engine = Arc::clone(&engine);
                    handles.push(tokio::spawn(async move {
                        engine.execute_tool(ToolComplexity::Medium, 300).await
                    }));
                }

                // Launch complex tools
                for _ in 0..complex_count {
                    let engine = Arc::clone(&engine);
                    handles.push(tokio::spawn(async move {
                        engine.execute_tool(ToolComplexity::Complex, 200).await
                    }));
                }

                let results = join_all(handles).await;
                let completed = results.len();

                black_box(completed);
            });
        });
    }

    group.finish();
}

/// Benchmark resource usage under load
fn bench_resource_usage_under_load(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let engine = Arc::new(MockScriptEngine::new());

    let mut group = c.benchmark_group("resource_usage_under_load");

    // Test memory usage patterns
    group.bench_function("memory_intensive_load", |b| {
        b.to_async(&rt).iter(|| async {
            let handles: Vec<_> = (0..20)
                .map(|i| {
                    let engine = Arc::clone(&engine);
                    tokio::spawn(async move {
                        // Execute memory-intensive operations
                        let result = engine.execute_tool(ToolComplexity::Complex, 1000).await;

                        // Allocate additional memory to simulate memory pressure
                        let mut data = Vec::with_capacity(1000);
                        for j in 0..1000 {
                            data.push(format!("tool_{}_data_{}", i, j));
                        }

                        (result, data.len())
                    })
                })
                .collect();

            let results = join_all(handles).await;
            let (tool_results, memory_sizes): (Vec<_>, Vec<_>) = results
                .into_iter()
                .map(|r| r.unwrap())
                .unzip();

            let total_memory = memory_sizes.iter().sum::<usize>();

            black_box((tool_results.len(), total_memory));
        });
    });

    group.finish();
}

/// Benchmark error handling under load
fn bench_error_handling_under_load(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("error_handling_under_load");

    // Test graceful degradation under high load
    group.bench_function("high_load_error_handling", |b| {
        b.to_async(&rt).iter(|| async {
            let engine = Arc::new(MockScriptEngine::new());
            let high_concurrency = 100;

            let handles: Vec<_> = (0..high_concurrency)
                .map(|i| {
                    let engine = Arc::clone(&engine);
                    tokio::spawn(async move {
                        // Simulate potential error conditions
                        if i % 10 == 0 {
                            // Simulate a timeout or error
                            tokio::time::sleep(Duration::from_millis(10)).await;
                            Err("Simulated timeout".to_string())
                        } else {
                            let result = engine.execute_tool(ToolComplexity::Medium, 200).await;
                            Ok(result)
                        }
                    })
                })
                .collect();

            let results = join_all(handles).await;
            let (successes, failures): (Vec<_>, Vec<_>) = results
                .into_iter()
                .map(|r| r.unwrap())
                .partition(Result::is_ok);

            let success_count = successes.len();
            let failure_count = failures.len();

            black_box((success_count, failure_count));
        });
    });

    group.finish();
}

criterion_group!(
    load_tests,
    bench_concurrent_tool_execution,
    bench_sustained_load,
    bench_mixed_workload,
    bench_resource_usage_under_load,
    bench_error_handling_under_load
);

criterion_main!(load_tests);