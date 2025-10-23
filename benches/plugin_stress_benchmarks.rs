//! Plugin System Stress Testing Benchmarks
//!
//! Comprehensive Criterion benchmarks for plugin system stress testing
//! with multiple long-running processes and resource management validation.

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};
use std::time::Duration;
use std::sync::Arc;
use tokio::runtime::Runtime;

mod plugin_stress_testing_framework;
use plugin_stress_testing_framework::{
    PluginSystemStressTester, PluginStressTestConfig, PluginProcessType
};

/// Benchmark concurrent plugin execution with varying concurrency levels
fn benchmark_concurrent_plugin_execution(c: &mut Criterion) {
    let runtime = Runtime::new().unwrap();
    let stress_tester = Arc::new(PluginSystemStressTester::new());

    let mut group = c.benchmark_group("plugin_concurrent_execution");
    group.measurement_time(Duration::from_secs(30));
    group.sample_size(20);

    for concurrent_plugins in [5, 10, 25, 50, 100].iter() {
        group.throughput(Throughput::Elements(*concurrent_plugins as u64));
        group.bench_with_input(
            BenchmarkId::new("concurrent_plugins", concurrent_plugins),
            concurrent_plugins,
            |b, &concurrent_plugins| {
                b.to_async(&runtime).iter(|| async {
                    let config = PluginStressTestConfig {
                        name: format!("Concurrent Plugin Test - {} plugins", concurrent_plugins),
                        duration: Duration::from_secs(10),
                        concurrent_plugins,
                        long_running_processes: 0,
                        process_lifetime: Duration::from_secs(5),
                        memory_pressure_mb: 100,
                        cpu_pressure_percent: 50.0,
                        failure_injection_rate: 0.0,
                        resource_isolation_test: false,
                    };

                    let result = stress_tester.run_stress_test(config).await;
                    black_box(result);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark long-running plugin processes
fn benchmark_long_running_processes(c: &mut Criterion) {
    let runtime = Runtime::new().unwrap();
    let stress_tester = Arc::new(PluginSystemStressTester::new());

    let mut group = c.benchmark_group("plugin_long_running_processes");
    group.measurement_time(Duration::from_secs(60));
    group.sample_size(10);

    for process_count in [5, 10, 20].iter() {
        for lifetime in [Duration::from_secs(30), Duration::from_secs(60), Duration::from_secs(120)].iter() {
            group.bench_with_input(
                BenchmarkId::new("long_running", format!("{}_processes_{:?}", process_count, lifetime)),
                &(process_count, *lifetime),
                |b, &(process_count, lifetime)| {
                    b.to_async(&runtime).iter(|| async {
                        let config = PluginStressTestConfig {
                            name: format!("Long-running Process Test - {} processes for {:?}", process_count, lifetime),
                            duration: lifetime + Duration::from_secs(10),
                            concurrent_plugins: process_count / 2,
                            long_running_processes: process_count,
                            process_lifetime: lifetime,
                            memory_pressure_mb: 200,
                            cpu_pressure_percent: 70.0,
                            failure_injection_rate: 0.0,
                            resource_isolation_test: false,
                        };

                        let result = stress_tester.run_stress_test(config).await;
                        black_box(result);
                    });
                },
            );
        }
    }

    group.finish();
}

/// Benchmark memory-intensive plugin operations
fn benchmark_memory_intensive_plugins(c: &mut Criterion) {
    let runtime = Runtime::new().unwrap();
    let stress_tester = Arc::new(PluginSystemStressTester::new());

    let mut group = c.benchmark_group("plugin_memory_intensive");
    group.measurement_time(Duration::from_secs(45));
    group.sample_size(15);

    for memory_pressure in [50, 100, 200, 500].iter() {
        group.bench_with_input(
            BenchmarkId::new("memory_pressure_mb", memory_pressure),
            memory_pressure,
            |b, &memory_pressure| {
                b.to_async(&runtime).iter(|| async {
                    let config = PluginStressTestConfig {
                        name: format!("Memory Intensive Test - {} MB", memory_pressure),
                        duration: Duration::from_secs(20),
                        concurrent_plugins: 10,
                        long_running_processes: 5,
                        process_lifetime: Duration::from_secs(15),
                        memory_pressure_mb: memory_pressure,
                        cpu_pressure_percent: 60.0,
                        failure_injection_rate: 0.0,
                        resource_isolation_test: false,
                    };

                    let result = stress_tester.run_stress_test(config).await;
                    black_box(result);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark CPU-intensive plugin operations
fn benchmark_cpu_intensive_plugins(c: &mut Criterion) {
    let runtime = Runtime::new().unwrap();
    let stress_tester = Arc::new(PluginSystemStressTester::new());

    let mut group = c.benchmark_group("plugin_cpu_intensive");
    group.measurement_time(Duration::from_secs(45));
    group.sample_size(15);

    for cpu_pressure in [25.0, 50.0, 75.0, 90.0].iter() {
        group.bench_with_input(
            BenchmarkId::new("cpu_pressure_percent", cpu_pressure),
            cpu_pressure,
            |b, &cpu_pressure| {
                b.to_async(&runtime).iter(|| async {
                    let config = PluginStressTestConfig {
                        name: format!("CPU Intensive Test - {:.1}%", cpu_pressure),
                        duration: Duration::from_secs(20),
                        concurrent_plugins: 8,
                        long_running_processes: 4,
                        process_lifetime: Duration::from_secs(12),
                        memory_pressure_mb: 150,
                        cpu_pressure_percent: cpu_pressure,
                        failure_injection_rate: 0.0,
                        resource_isolation_test: false,
                    };

                    let result = stress_tester.run_stress_test(config).await;
                    black_box(result);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark resource isolation between plugins
fn benchmark_resource_isolation(c: &mut Criterion) {
    let runtime = Runtime::new().unwrap();
    let stress_tester = Arc::new(PluginSystemStressTester::new());

    let mut group = c.benchmark_group("plugin_resource_isolation");
    group.measurement_time(Duration::from_secs(40));
    group.sample_size(12);

    for plugin_groups in [5, 10, 15].iter() {
        group.bench_with_input(
            BenchmarkId::new("isolation_groups", plugin_groups),
            plugin_groups,
            |b, &plugin_groups| {
                b.to_async(&runtime).iter(|| async {
                    let config = PluginStressTestConfig {
                        name: format!("Resource Isolation Test - {} groups", plugin_groups),
                        duration: Duration::from_secs(15),
                        concurrent_plugins: plugin_groups * 3,
                        long_running_processes: plugin_groups,
                        process_lifetime: Duration::from_secs(10),
                        memory_pressure_mb: 300,
                        cpu_pressure_percent: 80.0,
                        failure_injection_rate: 0.0,
                        resource_isolation_test: true,
                    };

                    let result = stress_tester.run_stress_test(config).await;
                    black_box(result);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark failure injection and recovery
fn benchmark_failure_recovery(c: &mut Criterion) {
    let runtime = Runtime::new().unwrap();
    let stress_tester = Arc::new(PluginSystemStressTester::new());

    let mut group = c.benchmark_group("plugin_failure_recovery");
    group.measurement_time(Duration::from_secs(35));
    group.sample_size(18);

    for failure_rate in [0.05, 0.1, 0.2, 0.3].iter() {
        group.bench_with_input(
            BenchmarkId::new("failure_rate", failure_rate),
            failure_rate,
            |b, &failure_rate| {
                b.to_async(&runtime).iter(|| async {
                    let config = PluginStressTestConfig {
                        name: format!("Failure Recovery Test - {:.1}% failure rate", failure_rate * 100.0),
                        duration: Duration::from_secs(12),
                        concurrent_plugins: 20,
                        long_running_processes: 5,
                        process_lifetime: Duration::from_secs(8),
                        memory_pressure_mb: 120,
                        cpu_pressure_percent: 55.0,
                        failure_injection_rate: failure_rate,
                        resource_isolation_test: false,
                    };

                    let result = stress_tester.run_stress_test(config).await;
                    black_box(result);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark mixed workload scenarios
fn benchmark_mixed_workloads(c: &mut Criterion) {
    let runtime = Runtime::new().unwrap();
    let stress_tester = Arc::new(PluginSystemStressTester::new());

    let mut group = c.benchmark_group("plugin_mixed_workloads");
    group.measurement_time(Duration::from_secs(50));
    group.sample_size(12);

    let workload_scenarios = vec![
        ("light_mixed", 10, 5, Duration::from_secs(15), 50, 30.0),
        ("medium_mixed", 25, 10, Duration::from_secs(20), 150, 60.0),
        ("heavy_mixed", 50, 20, Duration::from_secs(25), 300, 85.0),
    ];

    for (scenario_name, concurrent, long_running, lifetime, memory, cpu) in workload_scenarios {
        group.bench_with_input(
            BenchmarkId::new("mixed_workload", scenario_name),
            &(concurrent, long_running, lifetime, memory, cpu),
            |b, &(concurrent, long_running, lifetime, memory, cpu)| {
                b.to_async(&runtime).iter(|| async {
                    let config = PluginStressTestConfig {
                        name: format!("Mixed Workload Test - {}", scenario_name),
                        duration: lifetime + Duration::from_secs(10),
                        concurrent_plugins: concurrent,
                        long_running_processes: long_running,
                        process_lifetime: lifetime,
                        memory_pressure_mb: memory,
                        cpu_pressure_percent: cpu,
                        failure_injection_rate: 0.05,
                        resource_isolation_test: true,
                    };

                    let result = stress_tester.run_stress_test(config).await;
                    black_box(result);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark plugin lifecycle management
fn benchmark_plugin_lifecycle(c: &mut Criterion) {
    let runtime = Runtime::new().unwrap();
    let stress_tester = Arc::new(PluginSystemStressTester::new());

    let mut group = c.benchmark_group("plugin_lifecycle_management");
    group.measurement_time(Duration::from_secs(30));
    group.sample_size(20);

    for plugin_count in [20, 50, 100, 200].iter() {
        group.throughput(Throughput::Elements(*plugin_count as u64));
        group.bench_with_input(
            BenchmarkId::new("lifecycle_operations", plugin_count),
            plugin_count,
            |b, &plugin_count| {
                b.to_async(&runtime).iter(|| async {
                    let config = PluginStressTestConfig {
                        name: format!("Plugin Lifecycle Test - {} plugins", plugin_count),
                        duration: Duration::from_secs(8),
                        concurrent_plugins: plugin_count,
                        long_running_processes: 0,
                        process_lifetime: Duration::from_secs(2),
                        memory_pressure_mb: 80,
                        cpu_pressure_percent: 40.0,
                        failure_injection_rate: 0.0,
                        resource_isolation_test: false,
                    };

                    let result = stress_tester.run_stress_test(config).await;
                    black_box(result);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark stress test scalability
fn benchmark_stress_test_scalability(c: &mut Criterion) {
    let runtime = Runtime::new().unwrap();
    let stress_tester = Arc::new(PluginSystemStressTester::new());

    let mut group = c.benchmark_group("plugin_stress_scalability");
    group.measurement_time(Duration::from_secs(90));
    group.sample_size(8);

    let scalability_levels = vec![
        ("small_scale", 10, 5, Duration::from_secs(20), 50, 25.0),
        ("medium_scale", 50, 25, Duration::from_secs(40), 200, 60.0),
        ("large_scale", 100, 50, Duration::from_secs(60), 500, 85.0),
        ("extreme_scale", 200, 100, Duration::from_secs(90), 1000, 95.0),
    ];

    for (scale_name, concurrent, long_running, duration, memory, cpu) in scalability_levels {
        group.bench_with_input(
            BenchmarkId::new("scalability_level", scale_name),
            &(concurrent, long_running, duration, memory, cpu),
            |b, &(concurrent, long_running, duration, memory, cpu)| {
                b.to_async(&runtime).iter(|| async {
                    let config = PluginStressTestConfig {
                        name: format!("Scalability Test - {}", scale_name),
                        duration,
                        concurrent_plugins: concurrent,
                        long_running_processes: long_running,
                        process_lifetime: duration / 2,
                        memory_pressure_mb: memory,
                        cpu_pressure_percent: cpu,
                        failure_injection_rate: 0.1,
                        resource_isolation_test: true,
                    };

                    let result = stress_tester.run_stress_test(config).await;
                    black_box(result);
                });
            },
        );
    }

    group.finish();
}

/// Comprehensive stress test benchmark
fn benchmark_comprehensive_stress_test(c: &mut Criterion) {
    let runtime = Runtime::new().unwrap();
    let stress_tester = Arc::new(PluginSystemStressTester::new());

    let mut group = c.benchmark_group("plugin_comprehensive_stress");
    group.measurement_time(Duration::from_secs(120));
    group.sample_size(5);

    group.bench_function("enterprise_stress_test", |b| {
        b.to_async(&runtime).iter(|| async {
            let config = PluginStressTestConfig {
                name: "Enterprise-grade Plugin Stress Test".to_string(),
                duration: Duration::from_secs(60),
                concurrent_plugins: 75,
                long_running_processes: 30,
                process_lifetime: Duration::from_secs(45),
                memory_pressure_mb: 750,
                cpu_pressure_percent: 90.0,
                failure_injection_rate: 0.15,
                resource_isolation_test: true,
            };

            let result = stress_tester.run_stress_test(config).await;
            black_box(result);
        });
    });

    group.finish();
}

criterion_group!(
    plugin_stress_benchmarks,
    benchmark_concurrent_plugin_execution,
    benchmark_long_running_processes,
    benchmark_memory_intensive_plugins,
    benchmark_cpu_intensive_plugins,
    benchmark_resource_isolation,
    benchmark_failure_recovery,
    benchmark_mixed_workloads,
    benchmark_plugin_lifecycle,
    benchmark_stress_test_scalability,
    benchmark_comprehensive_stress_test
);

criterion_main!(plugin_stress_benchmarks);