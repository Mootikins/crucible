# Memory Testing Framework

This document describes the comprehensive memory testing framework implemented for Crucible services to ensure memory efficiency and detect resource leaks.

## Overview

The memory testing framework provides automated testing capabilities for validating memory usage patterns, detecting leaks, and ensuring production-ready performance across all Crucible services:

- **ScriptEngine** - VM-per-execution pattern, script caching, context management
- **InferenceEngine** - Model loading, caching, LLM provider connections
- **DataStore** - Database connections, query caching, data storage
- **McpGateway** - Session management, tool registration, MCP connections

## Features

### 🔍 Memory Profiling
- Real-time memory usage tracking
- Heap, stack, cache, and connection memory analysis
- Arc/Mutex reference count monitoring
- Custom metrics collection

### 🧪 Test Scenarios
- **Idle Baseline** - Memory usage when services are idle
- **Single Operation** - Memory impact of individual operations
- **High Frequency Operations** - Memory under rapid request load
- **Large Data Processing** - Memory handling of large payloads
- **Concurrent Operations** - Memory with multiple simultaneous operations
- **Long-Running Stability** - Memory stability over extended periods
- **Resource Exhaustion** - Behavior when memory limits are approached
- **Cleanup Validation** - Memory returns to baseline after operations

### 🔬 Leak Detection
- Statistical analysis using linear regression
- Pattern recognition (linear, exponential, stepped, sporadic, cyclic)
- Confidence level calculation
- Suspected leak source identification

### 📊 Analysis & Reporting
- Comprehensive memory statistics
- Performance metrics correlation
- Threshold violation detection
- Actionable recommendations
- JSON export for further analysis

## Quick Start

### Installation

Add the memory testing feature to your `Cargo.toml`:

```toml
[dependencies]
crucible-services = { version = "0.1", features = ["memory-testing"] }
```

### Basic Usage

```rust
use crucible_services::memory_testing::*;
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create configuration
    let config = MemoryTestConfig::default();

    // Create test runner
    let runner = MemoryTestRunner::new(config).await?;

    // Run comprehensive tests for all services
    let results = runner.run_comprehensive_tests().await?;

    // Export results
    runner.export_results(&results, "memory_test_results.json").await?;

    Ok(())
}
```

### Running Specific Tests

```rust
// Test specific service and scenario
let test_data = HashMap::from([
    ("payload_size_mb".to_string(), 10.into()),
    ("iterations".to_string(), 100.into()),
]);

let result = runner.run_test_scenario(
    ServiceType::ScriptEngine,
    TestScenario::LargeDataProcessing,
    test_data,
).await?;

println!("Memory per operation: {:.2} MB",
         result.memory_stats.memory_per_operation / (1024.0 * 1024.0));
```

## Configuration

The memory testing framework is highly configurable through `MemoryTestConfig`:

### Test Durations

```rust
TestDurations {
    short_test_seconds: 300,      // 5 minutes - basic operations
    medium_test_seconds: 3600,    // 1 hour - load testing
    long_test_seconds: 28800,     // 8 hours - leak detection
    measurement_interval_ms: 1000, // 1 second between measurements
}
```

### Memory Thresholds

```rust
MemoryThresholds {
    max_baseline_memory_bytes: 100 * 1024 * 1024,  // 100MB
    max_memory_growth_rate: 1024 * 1024,            // 1MB/s
    max_memory_per_operation: 10 * 1024 * 1024,    // 10MB
    leak_threshold_bytes: 5 * 1024 * 1024,          // 5MB
    cleanup_timeout_seconds: 60,                    // 1 minute
}
```

### Load Testing

```rust
LoadTestingConfig {
    concurrent_operations: 100,
    operations_per_second: 1000,
    large_data_size_bytes: 100 * 1024 * 1024,      // 100MB
    max_payload_size_bytes: 10 * 1024 * 1024,       // 10MB
}
```

### Leak Detection

```rust
LeakDetectionConfig {
    enabled: true,
    sampling_interval_ms: 500,
    min_samples: 10,
    significance_threshold: 0.95,
    enable_pattern_analysis: true,
}
```

## Test Results

### Memory Statistics

Each test generates comprehensive memory statistics:

```rust
MemoryStatistics {
    baseline_memory_bytes: 52428800,     // 50MB baseline
    peak_memory_bytes: 104857600,        // 100MB peak
    average_memory_bytes: 75497472,      // 72MB average
    memory_growth_rate: 1024.0,          // 1KB/s growth rate
    memory_volatility: 0.15,             // 15% volatility
    cleanup_efficiency: 0.95,            // 95% cleanup efficiency
    memory_per_operation: 1048576.0,      // 1MB per operation
}
```

### Leak Detection

Advanced leak detection provides:

```rust
LeakDetectionResult {
    leak_detected: true,
    leak_rate: 2048.0,                   // 2KB/s leak rate
    confidence: 0.98,                    // 98% confidence
    pattern_analysis: Some(LeakPatternAnalysis {
        pattern_type: LeakPatternType::Linear,
        growth_characteristics: GrowthCharacteristics {
            rate: 2048.0,
            acceleration: 0.0,
            consistency: 0.95,
        },
        operation_correlation: 0.85,
        time_patterns: vec![...],
    }),
    suspected_sources: vec![
        "Cache memory accumulation".to_string(),
        "Arc/Mutex reference cycle leaks".to_string(),
    ],
}
```

### Performance Metrics

```rust
PerformanceMetrics {
    operations_per_second: 150.5,
    average_response_time_ms: 45.2,
    p95_response_time_ms: 120.0,
    error_rate: 0.01,                   // 1% error rate
    throughput: 15728640.0,             // 15MB/s
    resource_utilization: ResourceUtilization {
        cpu_utilization: 0.35,           // 35% CPU
        memory_utilization: 0.72,        // 72% memory
        connection_utilization: 0.20,    // 20% connections
        cache_hit_rate: 0.85,            // 85% cache hit rate
    },
}
```

### Threshold Violations

```rust
ThresholdViolation {
    violation_type: ViolationType::MemoryLeak,
    threshold: 5242880.0,                // 5MB threshold
    actual: 6291456.0,                  // 6MB actual
    severity: ViolationSeverity::High,
    description: "Memory leak detected: 6MB/s (confidence: 98%)".to_string(),
}
```

## Service-Specific Testing

### ScriptEngine

Tests focus on:
- VM memory allocation and deallocation
- Script cache efficiency
- Context memory management
- Compilation and execution memory patterns

### InferenceEngine

Tests focus on:
- Model loading and unloading
- LLM provider connection memory
- Request/response buffer management
- Batch operation memory efficiency

### DataStore

Tests focus on:
- Database connection pooling
- Query result memory usage
- Cache memory management
- Large dataset handling

### McpGateway

Tests focus on:
- Session memory management
- Tool registration memory
- MCP protocol buffer handling
- Concurrent session resource usage

## Integration with CI/CD

### GitHub Actions Example

```yaml
name: Memory Testing

on: [push, pull_request]

jobs:
  memory-test:
    runs-on: ubuntu-latest

    steps:
    - uses: actions/checkout@v2

    - name: Setup Rust
      uses: actions-rs/toolchain@v1
      with:
        toolchain: stable

    - name: Run Memory Tests
      run: |
        cargo test --features memory-testing --release --bin memory_testing_example

    - name: Upload Results
      uses: actions/upload-artifact@v2
      with:
        name: memory-test-results
        path: memory_test_results.json
```

### Docker Integration

```dockerfile
FROM rust:1.70 as builder

WORKDIR /app
COPY . .
RUN cargo build --features memory-testing --release

FROM ubuntu:22.04
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/memory_testing_example /usr/local/bin/

CMD ["memory_testing_example"]
```

## Best Practices

### Test Configuration

1. **Development**: Use shorter durations and more frequent measurements
2. **Staging**: Use production-like durations and comprehensive scenarios
3. **Production**: Run long-running stability tests during maintenance windows

### Threshold Setting

1. **Baseline**: Set based on actual service startup memory
2. **Growth Rate**: Consider expected memory growth patterns
3. **Per Operation**: Measure actual memory usage for typical operations
4. **Leak Detection**: Set sensitive enough to catch issues but avoid false positives

### Monitoring Integration

```rust
// Integrate with existing monitoring
async fn continuous_memory_monitoring() {
    let config = MemoryTestConfig {
        test_durations: TestDurations {
            short_test_seconds: 60,
            // ... other config
        },
        // ... other config
    };

    let runner = MemoryTestRunner::new(config).await?;

    // Run tests every hour
    let mut interval = tokio::time::interval(Duration::from_secs(3600));

    loop {
        interval.tick().await;

        if let Ok(results) = runner.run_comprehensive_tests().await {
            // Send to monitoring system
            send_to_monitoring_system(&results).await;
        }
    }
}
```

## Troubleshooting

### Common Issues

1. **Permission Denied**: Ensure the process has permission to read `/proc/self/status` (Linux)
2. **High Memory Usage**: Check for other processes running on the same system
3. **Inconsistent Results**: Run tests on dedicated systems with minimal background noise
4. **False Positives**: Adjust thresholds based on actual service behavior patterns

### Debug Information

Enable detailed logging:

```rust
use tracing_subscriber::fmt;

fmt()
    .with_max_level(tracing::Level::DEBUG)
    .with_target(false)
    .init();
```

### Performance Impact

Memory testing has minimal performance impact:
- Measurements taken asynchronously
- Configurable sampling intervals
- Non-blocking operations
- Efficient data structures

## Contributing

When adding new test scenarios or improving the framework:

1. Follow existing code patterns
2. Add comprehensive tests
3. Update documentation
4. Consider cross-platform compatibility
5. Ensure backward compatibility

## License

This memory testing framework is part of the Crucible project and follows the same license terms.