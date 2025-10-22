# Crucible Performance Testing Suite

This comprehensive performance testing suite validates the architectural improvements between the current DataCoordinator approach and the new centralized daemon approach for event coordination in Crucible.

## 🎯 Objectives

### Primary Goals
1. **Baseline Performance** - Measure current DataCoordinator performance characteristics
2. **Centralized Daemon Performance** - Test new CrucibleCore with event routing capabilities
3. **Comparison Analysis** - Identify performance improvements or regressions
4. **Resource Usage** - Analyze memory, CPU, and latency patterns
5. **Scalability Testing** - Determine performance limits and bottlenecks

### Test Scenarios
- **Event Throughput** - Events processed per second
- **Memory Usage** - Baseline memory consumption and growth patterns
- **Latency** - Event routing and processing latency
- **Concurrent Load** - Performance with multiple services processing events
- **Resource Efficiency** - CPU usage and scalability patterns
- **Error Recovery** - Performance during failure scenarios

## 📁 Benchmark Structure

```
benches/
├── daemon_performance.rs      # Core performance comparison benchmarks
├── load_testing.rs           # Realistic load testing scenarios
├── memory_profiling.rs       # Memory usage and leak detection
├── comparison_analysis.rs    # Comparative analysis and reporting
├── scalability_testing.rs    # Scalability limits and bottleneck identification
├── run_performance_tests.sh  # Automated test runner
└── README.md                 # This documentation
```

## 🚀 Quick Start

### Prerequisites
- Rust 1.75+ with Criterion benchmarking
- Sufficient system resources (8GB+ RAM recommended)
- Unix-like environment (Linux/macOS)

### Running Tests

#### Quick Test (5-10 minutes)
```bash
# Run a subset of benchmarks for fast feedback
./benches/run_performance_tests.sh quick
```

#### Full Test Suite (30-60 minutes)
```bash
# Run comprehensive performance tests
./benches/run_performance_tests.sh full
```

#### Individual Benchmark Groups
```bash
# Run specific benchmark groups
cargo bench --bench daemon_performance
cargo bench --bench load_testing
cargo bench --bench memory_profiling
cargo bench --bench comparison_analysis
cargo bench --bench scalability_testing
```

#### Custom Benchmark Runs
```bash
# Run specific benchmarks with custom parameters
cargo bench --bench daemon_performance -- --profile-time 30
cargo bench --bench load_testing -- --sample-size 100
```

## 📊 Benchmark Categories

### 1. Daemon Performance (`daemon_performance.rs`)

**Purpose**: Direct performance comparison between DataCoordinator and centralized daemon approaches.

**Key Metrics**:
- Events per second throughput
- Average/P95/P99 latency
- Memory usage patterns
- CPU utilization
- Success/error rates

**Test Scenarios**:
- Light load: 100 events, 3 services, 512B payload
- Medium load: 1,000 events, 10 services, 1KB payload
- Heavy load: 10,000 events, 50 services, 4KB payload

### 2. Load Testing (`load_testing.rs`)

**Purpose**: Realistic event pattern testing with various load scenarios.

**Event Patterns**:
- **Steady Load**: Constant event rate over time
- **Burst Patterns**: Short high-intensity bursts
- **Ramp-up Load**: Gradual load increase
- **Spike Patterns**: Sudden load spikes
- **Mixed Workloads**: Realistic event type distribution

**Realistic Event Types**:
- Filesystem events (30%): File create, modify, delete, move
- Database events (25%): Record CRUD operations
- External events (20%): API calls, webhooks, notifications
- MCP events (10%): Tool calls, resource requests
- Service events (10%): Health checks, status changes
- System events (5%): Metrics, configuration changes

### 3. Memory Profiling (`memory_profiling.rs`)

**Purpose**: Comprehensive memory usage analysis and leak detection.

**Features**:
- Custom memory allocator tracking
- Real-time memory usage monitoring
- Allocation/deallocation pattern analysis
- Memory leak detection
- Resource efficiency metrics

**Memory Test Scenarios**:
- Variable payload sizes (1KB - 64KB)
- High-frequency event processing
- Long-running stability tests
- Memory pressure conditions

### 4. Comparison Analysis (`comparison_analysis.rs`)

**Purpose**: Automated comparative analysis with detailed reporting.

**Analysis Features**:
- Performance improvement calculations
- Statistical significance testing
- Automated recommendation generation
- Bottleneck identification
- Comprehensive report generation

**Report Contents**:
- Executive summary
- Performance comparison tables
- Key findings and insights
- Strengths/weaknesses analysis
- Bottleneck identification
- Actionable recommendations

### 5. Scalability Testing (`scalability_testing.rs`)

**Purpose**: Identify performance limits and scaling characteristics.

**Scalability Dimensions**:
- **Throughput Scalability**: Maximum events per second
- **Concurrency Scalability**: Performance under concurrent load
- **Memory Scalability**: Memory usage scaling with load
- **Latency Scalability**: Latency characteristics under load

**Bottleneck Detection**:
- CPU bottlenecks
- Memory constraints
- Thread contention
- I/O limitations
- Network constraints

## 📈 Interpreting Results

### Performance Metrics

#### Throughput (Events/Second)
- **Good**: > 1,000 events/sec for medium load
- **Acceptable**: 500-1,000 events/sec
- **Poor**: < 500 events/sec

#### Latency
- **Excellent**: < 10ms average
- **Good**: 10-50ms average
- **Acceptable**: 50-200ms average
- **Poor**: > 200ms average

#### Memory Usage
- **Efficient**: < 100MB for medium load
- **Acceptable**: 100-500MB
- **Poor**: > 500MB

#### CPU Usage
- **Efficient**: < 50% for medium load
- **Acceptable**: 50-80%
- **Poor**: > 80%

### Success Criteria

#### DataCoordinator Baseline
- Should handle current production load effectively
- Maintain stability under normal conditions
- Show predictable scaling behavior

#### Centralized Daemon Target
- Equal or better performance than DataCoordinator
- Improved memory efficiency
- Better scalability characteristics
- Enhanced resilience under load

## 🔧 Configuration and Customization

### Environment Variables

```bash
# Optimize for performance testing
export RUSTFLAGS="-C target-cpu=native"

# Increase stack size if needed
export RUST_MIN_STACK=8388608

# Enable detailed logging
export RUST_LOG=debug
```

### Custom Test Scenarios

To add custom test scenarios, modify the benchmark configuration in the relevant files:

```rust
// Example: Custom load test scenario
let custom_config = LoadTestConfig {
    pattern: EventPattern::Steady {
        events_per_second: 2000,
        duration_secs: 60,
    },
    payload_size_range: (2048, 8192),
    concurrent_services: 20,
    enable_failures: true,
    failure_rate: 0.05,
};
```

### Benchmark Tuning

```toml
# Cargo.toml additions for benchmark tuning
[bench]
profile = "release" # Use release optimizations

[[bench]]
name = "daemon_performance"
harness = false
```

## 📋 Results Analysis

### Automated Report Generation

The comparison analysis module automatically generates comprehensive reports:

```bash
# Reports are saved to benchmark_results/
ls benchmark_results/
# comprehensive_report_YYYYMMDD_HHMMSS.md
# daemon_performance_YYYYMMDD_HHMMSS.json
# scalability_results_YYYYMMDD_HHMMSS.json
```

### Key Performance Indicators

1. **Performance Improvement (%)**
   ```text
   ((CD_Throughput - DC_Throughput) / DC_Throughput) * 100
   ```

2. **Memory Efficiency (%)**
   ```text
   (Deallocated_Bytes / Allocated_Bytes) * 100
   ```

3. **Scalability Factor**
   ```text
   Actual_Performance_Improvement / Expected_Improvement
   ```

4. **Error Rate (%)**
   ```text
   (Failed_Events / Total_Events) * 100
   ```

## 🐛 Troubleshooting

### Common Issues

#### High Variance in Results
- Close other applications
- Run tests multiple times
- Check for system resource contention
- Use consistent test environments

#### Memory Allocation Errors
- Increase available memory
- Check for memory leaks in test code
- Reduce payload sizes for testing

#### Timeout Issues
- Increase timeout values in test configuration
- Reduce load intensity
- Check for system I/O bottlenecks

#### Compilation Errors
- Ensure all dependencies are up to date
- Check Rust version compatibility
- Verify feature flags are enabled

### Performance Tips

1. **System Preparation**
   - Close unnecessary applications
   - Disable power saving features
   - Use dedicated test machines when possible

2. **Test Consistency**
   - Run tests multiple times
   - Use same hardware configuration
   - Monitor system resources during tests

3. **Result Validation**
   - Check for outliers in results
   - Verify statistical significance
   - Cross-reference with system metrics

## 📚 Advanced Usage

### Custom Memory Allocators

The memory profiling module includes a custom allocator for detailed tracking:

```rust
// Enable custom allocator
#[global_allocator]
static GLOBAL_ALLOC: TrackingAllocator = TrackingAllocator::new();

// Get memory statistics
let stats = GLOBAL_ALLOC.get_stats();
println!("Peak memory: {} KB", stats.peak_usage / 1024);
```

### Real-time Monitoring

Monitor system resources during benchmark runs:

```bash
# CPU and memory monitoring
htop

# I/O monitoring
iotop

# Network monitoring (if applicable)
nethogs
```

### CI/CD Integration

Add performance testing to your CI pipeline:

```yaml
# .github/workflows/performance.yml
name: Performance Tests
on: [push, pull_request]
jobs:
  benchmark:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      - uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
      - name: Run benchmarks
        run: ./benches/run_performance_tests.sh quick
```

## 🤝 Contributing

### Adding New Benchmarks

1. Create new benchmark file in `benches/`
2. Follow existing patterns and naming conventions
3. Add comprehensive documentation
4. Update this README
5. Add tests for benchmark correctness

### Performance Regression Testing

Set up automated performance regression detection:

1. Establish baseline measurements
2. Configure acceptable variance thresholds
3. Implement automated reporting
4. Set up alerts for regressions

## 📄 License

This performance testing suite is part of the Crucible project and follows the same licensing terms.

---

**For questions or issues with performance testing, please refer to the main project documentation or create an issue in the repository.**