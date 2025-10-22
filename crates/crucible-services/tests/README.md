# Crucible Services Test Suite

This directory contains the comprehensive test suite for the crucible-services crate, organized into clear categories for unit tests, integration tests, and specialized testing frameworks.

**🎯 Recent Update**: Phase 2 testing artifacts have been consolidated into a new organized structure while preserving all excellent integration tests and adding comprehensive unit test coverage.

## 🏗️ Architecture Overview

The test suite validates:

### Core Services
- **ScriptEngine** - Script execution with security sandboxing and caching
- **InferenceEngine** - LLM integration with multiple providers and performance optimization
- **DataStore** - Unified database interface with multiple backends and vector search
- **McpGateway** - MCP protocol connections and tool registration

### Event System Features
- **Event Publishing & Subscription** - Each service can publish and receive events
- **Cross-Service Communication** - Events flow correctly between services
- **Load Balancing** - Events distributed across service instances
- **Circuit Breaker** - Failed services isolated and recovered automatically
- **Event Priorities** - Critical, High, Normal, Low priority handling
- **Correlation Tracking** - Related events grouped across the system

## 📁 Test Organization

### 🏗️ Test Structure Overview

```
tests/
├── 📋 README.md                          # This file - test documentation
├── 📦 mod.rs                             # Test module organization
│
├── 🧪 Unit Tests
│   └── unit_tests.rs                     # Basic unit tests for core types
│   └── ../src/services/                  # NEW: Service-specific unit tests
│       ├── script_engine_tests.rs        # Script Engine unit tests
│       ├── data_store_tests.rs           # Data Store unit tests
│       ├── mcp_gateway_tests.rs          # MCP Gateway unit tests
│       └── inference_engine_tests.rs     # Inference Engine unit tests
│
├── 🔗 Integration Tests
│   ├── service_integration_tests.rs      # Service integration tests (original)
│   ├── consolidated_integration_tests.rs # NEW: Consolidated Phase 2 tests
│   └── integration_test_runner.rs        # Test execution framework
│
├── 📡 Event System Tests (Phase 1 - Preserved)
│   ├── event_core_tests.rs               # Core event functionality
│   ├── event_routing_integration_tests.rs # Event routing validation
│   ├── event_circuit_breaker_tests.rs    # Circuit breaker functionality
│   ├── event_concurrent_tests.rs         # Concurrency testing
│   ├── event_error_handling_tests.rs     # Error scenario testing
│   ├── event_filtering_tests.rs          # Event filtering logic
│   ├── event_load_balancing_tests.rs     # Load balancing validation
│   ├── event_performance_tests.rs        # Performance benchmarks
│   ├── event_property_based_tests.rs     # Property-based testing
│   └── event_test_utilities.rs           # Event testing helpers
│
├── 🔧 Test Utilities
│   ├── test_utilities.rs                 # Common test utilities
│   ├── mock_services.rs                  # Mock service implementations
│   ├── event_validation.rs               # Event validation helpers
│   └── performance_benchmarks.rs         # Performance testing tools
│
├── 📜 Legacy Phase 2 Tests (Deprecated)
│   ├── phase2_integration_tests.rs       # Original Phase 2 integration tests
│   ├── phase2_main_test.rs               # Phase 2 main test runner
│   ├── phase2_simple_validation.rs       # Simple validation tests
│   ├── phase2_test_runner.rs             # Phase 2 test execution
│   └── phase2_validation_tests.rs        # Phase 2 validation suite
│
└── 📊 Specialized Tests
    └── 🗂️ Subdirectories/
        ├── event_tests/                  # Additional event tests
        ├── integration_tests/            # Additional integration tests
        └── service_tests/                # Additional service tests
```

### 🎯 Key Changes in This Restructuring

✅ **Preserved**: All comprehensive integration tests from Phase 1 and Phase 2
✅ **Added**: Dedicated unit tests for each of the 4 core services
✅ **Consolidated**: Phase 2 tests into organized `consolidated_integration_tests.rs`
✅ **Organized**: Clear structure by functionality (unit, integration, performance)
✅ **Maintained**: Backward compatibility with existing test runners
✅ **Enhanced**: Better documentation and test categorization

## 🚀 Running Tests

### 📊 Quick Tests (CI/CD Friendly)

```bash
# Run unit tests only
cargo test --lib

# Run quick integration tests (NEW)
cargo test --test consolidated_integration_tests -- test_quick_consolidated_suite

# Run event system tests (fast)
cargo test --test event_core_tests

# Run service-specific unit tests (NEW)
cargo test --lib services::script_engine_tests
cargo test --lib services::data_store_tests
cargo test --lib services::mcp_gateway_tests
cargo test --lib services::inference_engine_tests
```

### 🔍 Comprehensive Tests

```bash
# Run all integration tests
cargo test --test "*integration_tests*"

# Run consolidated test suite (NEW - replaces Phase 2)
cargo test --test consolidated_integration_tests -- test_full_consolidated_tests

# Run event system tests (comprehensive)
cargo test --test "event_*_tests"

# Run all tests including legacy Phase 2
cargo test
```

### ⚡ Performance Tests

```bash
# Run performance benchmarks
cargo test --test performance_benchmarks

# Run event performance tests
cargo test --test event_performance_tests

# Run memory stress tests (feature-gated)
cargo test --features memory-testing --test memory_testing
```

### 🎯 Using the Test Runners

#### Original Test Runner
```bash
# Run all tests with default configuration
cargo test --test integration_test_runner

# This will execute:
# - Unit tests for mock services
# - Integration tests for event flows
# - Performance benchmarks (if enabled)
# - Stress tests (if enabled)
```

#### NEW: Consolidated Test Runner
```bash
# Run quick consolidated tests (ideal for CI/CD)
cargo test --test consolidated_integration_tests -- test_quick_consolidated_suite

# Run full consolidated tests (comprehensive validation)
cargo test --test consolidated_integration_tests -- test_full_consolidated_tests
```

### Test Configuration

You can customize test execution by modifying the `TestRunnerConfig` in `integration_test_runner.rs`:

```rust
let config = TestRunnerConfig {
    run_unit_tests: true,
    run_integration_tests: true,
    run_performance_tests: false,  // Enable for performance testing
    run_stress_tests: false,      // Enable for stress testing
    parallel_execution: true,
    verbose_output: true,
    save_results: true,
    test_timeout_seconds: 300,
};
```

## 📋 Test Categories

### 🧪 1. Unit Tests (`unit_tests.rs` + `src/services/`)

**Purpose**: Test individual functions, types, and methods in isolation.

**Coverage**:
- Error types and conversions
- Service configuration validation
- Type serialization/deserialization
- Basic functionality validation
- **NEW**: Service-specific unit tests for each core service

**Service-Specific Unit Tests**:
- `script_engine_tests.rs` - Script execution, security policies, validation
- `data_store_tests.rs` - Database operations, document management, queries
- `mcp_gateway_tests.rs` - Session management, protocol handling, tool registration
- `inference_engine_tests.rs` - Model management, inference operations, performance

**When to Run**: Every PR, local development

### 🔗 2. Service Integration Tests (`service_integration_tests.rs`)

**Purpose**: Test how services work together through the event system.

**Coverage**:
- Service registration and discovery
- Cross-service communication
- Event routing and delivery
- Service health monitoring
- Load balancing and circuit breakers

**When to Run**: Pre-merge, nightly builds

### 🎯 3. Consolidated Integration Tests (`consolidated_integration_tests.rs`)

**Purpose**: **NEW** - Comprehensive test suite that consolidates all Phase 2 work.

**Coverage**:
- Event system validation
- Service integration
- Cross-service workflows
- Performance testing (optional)
- Memory management (optional)
- Error handling and recovery

**Features**:
- **Quick Mode**: Fast tests for CI/CD (`run_quick_consolidated_tests()`)
- **Full Mode**: Comprehensive validation (`run_full_consolidated_tests()`)
- **Categorized Results**: Tests organized by category with detailed metrics
- **Recommendations**: Automated recommendations for improvements

**When to Run**:
- Quick mode: Every PR, CI/CD
- Full mode: Pre-release, comprehensive validation

### 📡 4. Event System Tests (Phase 1 - Preserved)

**Purpose**: Comprehensive testing of the event system foundation.

**Coverage**:
- **Basic Event Flow Tests** - Service lifecycle, script execution, CRUD operations, MCP sessions
- **Cross-Service Communication Tests** - Event routing, service discovery, priority handling
- **Load Balancing Tests** - Round-robin distribution, multiple instances, health checks
- **Circuit Breaker Tests** - Failure thresholds, recovery mechanisms, half-open states
- **Performance Tests** - Throughput, latency, memory usage, concurrent processing
- **Stress Tests** - High concurrency, resource exhaustion, long-running operations

**When to Run**: Pre-merge, when event system changes

### 📜 5. Legacy Phase 2 Tests (Deprecated)

**Purpose**: Original Phase 2 integration work (being phased out).

**Status**: **DEPRECATED** - Being replaced by `consolidated_integration_tests.rs`

**Migration**: Use `consolidated_integration_tests.rs` for new development.

### 2. Cross-Service Communication Tests

Test communication between services:

- **Event Routing** - Events routed to correct target services
- **Service Discovery** - Services discover each other through events
- **Priority Handling** - Critical events processed first
- **Correlation Tracking** - Related events properly grouped

### 3. Load Balancing Tests

Validate event distribution across service instances:

- **Round-Robin Load Balancing** - Events distributed evenly
- **Multiple Service Instances** - Events routed to multiple instances
- **Instance Health** - Failed instances avoided

### 4. Circuit Breaker Tests

Test failure handling and recovery:

- **Failure Threshold** - Circuit breaker opens after failures
- **Recovery Mechanism** - Circuit breaker closes after recovery
- **Half-Open State** - Gradual recovery testing

### 5. Performance Tests

Measure system performance under load:

- **Event Throughput** - Events processed per second
- **Latency Measurements** - P50, P95, P99 latencies
- **Memory Usage** - Memory consumption under load
- **Concurrent Processing** - Multiple concurrent event streams

### 6. Stress Tests

Test system limits and robustness:

- **High Concurrency** - Many simultaneous operations
- **Resource Exhaustion** - Behavior under resource pressure
- **Long-Running Operations** - Stability over extended periods

## 🔧 Test Utilities

### Mock Services

The test suite includes comprehensive mock implementations:

```rust
// Mock Event Router with configurable failure simulation
let event_router = MockEventRouter::new();

// Mock Script Engine for testing script execution
let script_engine = MockScriptEngine::new();

// Mock Data Store for testing database operations
let data_store = MockDataStore::new();

// Mock Inference Engine for testing AI operations
let inference_engine = MockInferenceEngine::new();

// Mock MCP Gateway for testing MCP protocol
let mcp_gateway = MockMcpGateway::new();
```

### Event Factory

Create test events easily:

```rust
use test_utilities::EventFactory;

// Script execution event
let event = EventFactory::create_script_execution_event("test_script", "print('hello')");

// Document creation event
let doc = TestDataFactory::create_test_document("doc1", "Title", "Content");
let event = EventFactory::create_document_creation_event("test_db", &doc);

// Cross-service event
let event = EventFactory::create_cross_service_event("test_event", targets);
```

### Performance Tracking

Measure performance during tests:

```rust
use test_utilities::PerformanceTracker;

let tracker = PerformanceTracker::new();

let result = tracker.measure("test_operation", || async {
    // Your test operation here
    perform_operation().await
}).await;
```

## 📊 Performance Benchmarks

### Available Benchmarks

1. **Throughput Test** - High-volume event processing
2. **Latency Test** - Low-latency event processing
3. **Multi-Service Test** - Coordination between services
4. **Load Balancing Test** - Distribution performance
5. **Stress Test** - Maximum system load

### Running Benchmarks

```rust
use performance_benchmarks::{BenchmarkRunner, BenchmarkConfigs};

let mut runner = BenchmarkRunner::new().await?;

// Run specific benchmark
let result = runner.run_benchmark(BenchmarkConfigs::throughput_test()).await?;

// Run all benchmarks
let mut suite = BenchmarkSuite::new().await?;
let results = suite.run_all_benchmarks().await?;
```

## 🎯 Test Scenarios

### End-to-End Workflow Test

Tests a complete workflow spanning all services:

1. **Document Creation** in DataStore
2. **Script Processing** by ScriptEngine
3. **Embedding Generation** by InferenceEngine
4. **MCP Registration** by McpGateway

### Concurrent Service Coordination

Tests multiple services working simultaneously:

- Multiple concurrent tasks
- Correlation ID tracking
- Event ordering validation
- Resource usage monitoring

## 📈 Test Results

### Success Metrics

- **Event Processing Rate** - Events processed per second
- **Latency** - P50, P95, P99 response times
- **Success Rate** - Percentage of successful operations
- **Resource Usage** - Memory and CPU consumption
- **Error Rate** - Percentage of failed operations

### Result Files

Test results are saved to JSON files with timestamps:

```json
{
  "timestamp": "2024-01-01T12:00:00Z",
  "config": { ... },
  "summary": {
    "total_tests": 42,
    "passed_tests": 40,
    "failed_tests": 2,
    "success_rate": 0.952
  },
  "results": [ ... ]
}
```

## 🐛 Troubleshooting

### Common Issues

1. **Test Timeouts** - Increase `test_timeout_seconds` in configuration
2. **Memory Issues** - Reduce concurrent tasks or event counts
3. **Flaky Tests** - Check for race conditions in test setup
4. **Mock Service Failures** - Verify mock service configuration

### Debug Mode

Enable verbose output for detailed test execution:

```rust
let config = TestRunnerConfig {
    verbose_output: true,
    parallel_execution: false,  // Sequential for easier debugging
    save_results: true,
    ..Default::default()
};
```

### Performance Profiling

For performance investigation:

1. Enable performance tests
2. Use the performance tracker
3. Analyze the generated metrics
4. Check memory usage patterns

## 🔄 Continuous Integration

### CI Configuration

Example GitHub Actions workflow:

```yaml
name: Integration Tests

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest

    steps:
    - uses: actions/checkout@v3

    - name: Run Integration Tests
      run: |
        cargo test --test integration_test_runner --verbose

    - name: Upload Test Results
      uses: actions/upload-artifact@v3
      if: always()
      with:
        name: test-results
        path: test_results_*.json
```

### Performance Baselines

Establish performance baselines and monitor for regressions:

```bash
# Run performance tests and save baseline
cargo test --test performance_benchmarks -- --save-results

# Compare against baseline in CI
cargo test --test performance_benchmarks -- --compare-baseline
```

## 📚 Best Practices

### Writing Tests

1. **Use Mock Services** - Don't rely on external dependencies
2. **Isolate Tests** - Each test should be independent
3. **Cleanup Resources** - Clear state between tests
4. **Assert Clearly** - Make test assertions explicit
5. **Handle Timeouts** - Set appropriate timeouts for async operations

### Performance Testing

1. **Warm Up** - Include warmup phases for JIT compilation
2. **Measure Multiple Runs** - Run tests multiple times for accuracy
3. **Vary Load** - Test with different concurrency levels
4. **Monitor Resources** - Track memory and CPU usage
5. **Document Baselines** - Keep records of performance baselines

### Event Validation

1. **Check Event Types** - Validate event types and payloads
2. **Verify Correlations** - Ensure related events are linked
3. **Check Ordering** - Validate event sequences when required
4. **Monitor Timeouts** - Check for timely event processing
5. **Validate Routing** - Ensure events reach correct targets

## 🚀 Contributing

When adding new tests:

1. **Follow Naming Conventions** - Use descriptive test names
2. **Add Documentation** - Explain what the test validates
3. **Include Performance Metrics** - Track relevant performance data
4. **Handle Cleanup** - Ensure proper resource cleanup
5. **Update Documentation** - Keep this README current

For questions or issues with the integration tests, please refer to the main project documentation or create an issue in the repository.