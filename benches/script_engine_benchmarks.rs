//! ScriptEngine performance benchmarks
//!
//! These benchmarks measure the performance of tool execution, VM instantiation,
//! and scripting operations in the simplified ScriptEngine architecture.

use criterion::{black_box, criterion_group, BenchmarkId, Criterion, Throughput};
use std::sync::Arc;
use tokio::runtime::Runtime;
use std::time::Duration;

use crate::benchmark_utils::{
    TestDataGenerator, BenchmarkConfig, ResourceMonitor, ToolComplexity,
    ConcurrencyLevels, run_async_benchmark
};

/// Benchmark ScriptEngine tool execution for different tool complexities
fn bench_tool_execution(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let config = BenchmarkConfig::default();

    let mut group = c.benchmark_group("script_engine_tool_execution");

    for complexity in [ToolComplexity::Simple, ToolComplexity::Medium, ToolComplexity::Complex] {
        let name = format!("tool_execution_{}", complexity.as_str());

        group.bench_function(&name, |b| {
            b.to_async(&rt).iter(|| async {
                let monitor = ResourceMonitor::new();

                // Simulate tool execution based on complexity
                let result = match complexity {
                    ToolComplexity::Simple => execute_simple_tool().await,
                    ToolComplexity::Medium => execute_medium_tool().await,
                    ToolComplexity::Complex => execute_complex_tool().await,
                };

                black_box(result);
                black_box(monitor.elapsed());
            });
        });
    }

    group.finish();
}

/// Benchmark ScriptEngine VM instantiation and teardown
fn bench_vm_instantiation(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("script_engine_vm_instantiation");

    group.bench_function("vm_instantiation_simple", |b| {
        b.to_async(&rt).iter(|| async {
            let monitor = ResourceMonitor::new();

            // Simulate VM instantiation
            let vm = instantiate_vm_simple().await;

            black_box(vm);
            black_box(monitor.elapsed());
        });
    });

    group.bench_function("vm_instantiation_with_scripts", |b| {
        b.to_async(&rt).iter(|| async {
            let monitor = ResourceMonitor::new();

            // Simulate VM instantiation with pre-loaded scripts
            let vm = instantiate_vm_with_scripts().await;

            black_box(vm);
            black_box(monitor.elapsed());
        });
    });

    group.finish();
}

/// Benchmark concurrent tool execution
fn bench_concurrent_execution(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("script_engine_concurrent_execution");

    for concurrency in [
        ConcurrencyLevels::SINGLE,
        ConcurrencyLevels::LOW,
        ConcurrencyLevels::MEDIUM,
    ] {
        group.throughput(Throughput::Elements(concurrency as u64));
        group.bench_with_input(
            BenchmarkId::new("concurrent_tools", concurrency),
            &concurrency,
            |b, &concurrency| {
                b.to_async(&rt).iter(|| async {
                    let monitor = ResourceMonitor::new();

                    // Execute tools concurrently
                    let results = execute_concurrent_tools(concurrency).await;

                    black_box(results);
                    black_box(monitor.elapsed());
                });
            },
        );
    }

    group.finish();
}

/// Benchmark script loading and compilation
fn bench_script_loading(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let data_gen = TestDataGenerator::new().unwrap();

    let mut group = c.benchmark_group("script_engine_script_loading");

    for script_size in [1, 10, 100] { // KB
        group.throughput(Throughput::Bytes(script_size as u64 * 1024));
        group.bench_with_input(
            BenchmarkId::new("load_and_compile", script_size),
            &script_size,
            |b, &script_size| {
                b.to_async(&rt).iter(|| async {
                    let monitor = ResourceMonitor::new();

                    // Generate test script of specified size
                    let script = generate_test_script(script_size);

                    // Load and compile script
                    let compiled = load_and_compile_script(&script).await;

                    black_box(compiled);
                    black_box(monitor.elapsed());
                });
            },
        );
    }

    group.finish();
}

/// Benchmark tool registry performance
fn bench_tool_registry(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let config = BenchmarkConfig::default();

    let mut group = c.benchmark_group("script_engine_tool_registry");

    // Benchmark tool lookup
    group.bench_function("tool_lookup", |b| {
        b.to_async(&rt).iter(|| async {
            let monitor = ResourceMonitor::new();

            let tool = lookup_tool("file_system_read").await;

            black_box(tool);
            black_box(monitor.elapsed());
        });
    });

    // Benchmark batch tool registration
    for tool_count in [10, 100, 1000] {
        group.throughput(Throughput::Elements(tool_count));
        group.bench_with_input(
            BenchmarkId::new("batch_registration", tool_count),
            &tool_count,
            |b, &tool_count| {
                b.to_async(&rt).iter(|| async {
                    let monitor = ResourceMonitor::new();

                    let tools = generate_test_tools(tool_count);
                    let registry = register_tools_batch(tools).await;

                    black_box(registry);
                    black_box(monitor.elapsed());
                });
            },
        );
    }

    group.finish();
}

/// Benchmark memory usage during tool execution
fn bench_memory_usage(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("script_engine_memory_usage");

    group.bench_function("memory_peak_simple_tool", |b| {
        b.to_async(&rt).iter(|| async {
            let monitor = ResourceMonitor::new();

            // Execute tool that uses significant memory
            let result = execute_memory_intensive_tool(50).await; // 50MB

            black_box(result);
            black_box(monitor.memory_diff());
        });
    });

    group.bench_function("memory_gc_pressure", |b| {
        b.to_async(&rt).iter(|| async {
            let monitor = ResourceMonitor::new();

            // Execute many tools to create GC pressure
            for _ in 0..100 {
                let _tool = execute_simple_tool().await;
            }

            black_box(monitor.memory_diff());
        });
    });

    group.finish();
}

/// Benchmark tool error handling performance
fn bench_error_handling(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("script_engine_error_handling");

    group.bench_function("error_handling_overhead", |b| {
        b.to_async(&rt).iter(|| async {
            let monitor = ResourceMonitor::new();

            // Execute tool that will fail
            let result = execute_failing_tool().await;

            black_box(result);
            black_box(monitor.elapsed());
        });
    });

    group.bench_function("error_recovery", |b| {
        b.to_async(&rt).iter(|| async {
            let monitor = ResourceMonitor::new();

            // Execute tool, handle error, and retry
            let result = execute_tool_with_retry().await;

            black_box(result);
            black_box(monitor.elapsed());
        });
    });

    group.finish();
}

// Mock implementations for benchmarking

async fn execute_simple_tool() -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    // Simulate simple tool execution (file read, basic calculation)
    tokio::time::sleep(Duration::from_micros(100)).await;
    Ok("simple_result".to_string())
}

async fn execute_medium_tool() -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    // Simulate medium complexity tool (data processing, transformation)
    tokio::time::sleep(Duration::from_micros(500)).await;
    Ok("medium_result".to_string())
}

async fn execute_complex_tool() -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    // Simulate complex tool (AI integration, large data processing)
    tokio::time::sleep(Duration::from_micros(2000)).await;
    Ok("complex_result".to_string())
}

async fn instantiate_vm_simple() -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
    // Simulate VM instantiation overhead
    tokio::time::sleep(Duration::from_micros(50)).await;
    Ok(42) // VM instance ID
}

async fn instantiate_vm_with_scripts() -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
    // Simulate VM instantiation with pre-loaded scripts
    tokio::time::sleep(Duration::from_micros(200)).await;
    Ok(42)
}

async fn execute_concurrent_tools(concurrency: usize) -> Vec<Result<String, Box<dyn std::error::Error + Send + Sync>>> {
    let mut handles = Vec::with_capacity(concurrency);

    for i in 0..concurrency {
        let handle = tokio::spawn(async move {
            // Simulate work that scales with concurrency
            tokio::time::sleep(Duration::from_micros((i * 10) as u64)).await;
            Ok(format!("tool_{}_result", i))
        });
        handles.push(handle);
    }

    let mut results = Vec::with_capacity(concurrency);
    for handle in handles {
        results.push(handle.await.unwrap());
    }

    results
}

fn generate_test_script(size_kb: usize) -> String {
    let base_script = r#"
// Simple Rune script for benchmarking
pub fn main(input) {
    // Process input
    let result = input * 2;
    return result;
}

"#;

    // Pad with comments to reach desired size
    let padding_size = (size_kb * 1024).saturating_sub(base_script.len());
    let padding = "// ".repeat(padding_size / 4);

    format!("{}{}", base_script, padding)
}

async fn load_and_compile_script(script: &str) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
    // Simulate script compilation
    tokio::time::sleep(Duration::from_micros(script.len() as u64 / 10)).await;
    Ok(true)
}

async fn lookup_tool(tool_name: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    // Simulate tool lookup in registry
    tokio::time::sleep(Duration::from_micros(10)).await;
    Ok(format!("tool_definition_for_{}", tool_name))
}

async fn generate_test_tools(count: usize) -> Vec<String> {
    (0..count).map(|i| format!("tool_{}", i)).collect()
}

async fn register_tools_batch(tools: Vec<String>) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
    // Simulate batch registration
    tokio::time::sleep(Duration::from_micros(tools.len() as u64)).await;
    Ok(tools.len())
}

async fn execute_memory_intensive_tool(size_mb: usize) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    // Simulate memory-intensive operation
    let data: Vec<u8> = vec![0; size_mb * 1024 * 1024];
    tokio::time::sleep(Duration::from_micros(1000)).await;
    Ok(data)
}

async fn execute_failing_tool() -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    // Simulate tool execution that fails
    tokio::time::sleep(Duration::from_micros(100)).await;
    Err("Tool execution failed".into())
}

async fn execute_tool_with_retry() -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    // Simulate tool execution with retry logic
    for attempt in 1..=3 {
        tokio::time::sleep(Duration::from_micros(50)).await;
        if attempt == 3 {
            return Ok("success_after_retry".to_string());
        }
    }
    unreachable!()
}

pub fn script_engine_benchmarks(c: &mut Criterion) {
    bench_tool_execution(c);
    bench_vm_instantiation(c);
    bench_concurrent_execution(c);
    bench_script_loading(c);
    bench_tool_registry(c);
    bench_memory_usage(c);
    bench_error_handling(c);
}