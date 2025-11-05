# Crucible CLI Integration Tests

This directory contains integration tests for the Crucible CLI components.

## Test Files

### `tui_integration.rs` - TUI Event Loop Integration Tests

**Status:** RED phase (all tests fail with `todo!()` - ready for implementation)

**Test Count:** 17 tests

**Purpose:** Verify TUI event loop correctly integrates with CLI components (logs, REPL, status updates)

**Documentation:** See `TUI_INTEGRATION_TEST_SUMMARY.md` for detailed test descriptions and implementation guidance.

### Running TUI Integration Tests

```bash
# Run all TUI tests
cargo test -p crucible-cli --test tui_integration

# Run specific test
cargo test -p crucible-cli --test tui_integration test_tui_initialization

# Run with output visible
cargo test -p crucible-cli --test tui_integration -- --nocapture

# Compile tests without running (check for errors)
cargo test -p crucible-cli --test tui_integration --no-run
```

### Test Categories

1. **Event Loop Tests (4)** - TUI initialization, tick handling, keyboard input, shutdown
2. **Log Display Tests (3)** - Log rendering, scrolling, buffer limits
3. **REPL Integration Tests (3)** - Input routing, output display, command execution
4. **State Synchronization Tests (2)** - Status updates, concurrent event handling
5. **Advanced Tests (5)** - Resize handling, history navigation, render optimization, throttling, backpressure

### TDD Workflow

These tests follow Test-Driven Development:

**RED Phase (Current):**
- All tests compile but fail with `todo!()` panics
- Each `todo!()` has a descriptive message explaining what to implement

**GREEN Phase (Next):**
- Replace `todo!()` calls with actual implementations
- Make each test pass one at a time
- Run tests frequently: `cargo test -p crucible-cli --test tui_integration`

**REFACTOR Phase (After passing):**
- Clean up test code
- Extract common patterns into helpers
- Optimize for readability and maintainability

### Implementation Guide

Start with the simplest tests and work up to more complex ones:

1. `test_tui_initialization` - Just check default state
2. `test_log_buffer_displays_messages` - Add one log entry
3. `test_tui_handles_key_events` - Send keyboard events
4. `test_repl_input_routing` - Verify REPL buffer updates
5. Continue with remaining tests...

Each test has detailed comments explaining:
- What behavior is being tested
- Expected outcomes
- Implementation notes

### Test Helpers

Helper functions reduce boilerplate:

- `create_test_terminal()` - Creates ratatui TestBackend (headless testing)
- `create_test_channels()` - Sets up log/status event channels
- `create_test_app()` - Creates App with test configuration
- `create_test_log()` - Creates test LogEntry instances
- `buffer_content()` - Extracts text from terminal buffer (to be implemented)

### Dependencies

All dependencies already in `Cargo.toml`:

```toml
[dependencies]
ratatui = "0.29"      # TestBackend for headless testing
crossterm = "0.29"    # Event types
tokio = { ... }       # Async runtime

[dev-dependencies]
tokio-test = "0.4"    # Async test utilities
```

No additional dependencies needed.

### Other Test Files

- `integration_test.rs` - General CLI integration tests
- `test_chat.rs` - Chat REPL tests
- `test_backend.rs` - Backend integration tests
- `large_dataset_performance_tests.rs` - Large dataset performance testing suite

### Large Dataset Performance Tests (`large_dataset_performance_tests.rs`)

A comprehensive performance testing suite that validates the CLI system with 1000+ documents and establishes baseline performance metrics for semantic search scalability, database operations, and resource utilization patterns.

#### Features

- **Large Dataset Generation**: Creates realistic test datasets with 1000+ documents across diverse content types
- **Search Performance Testing**: Validates semantic search response times and result quality with large document sets
- **Database Performance Scaling**: Tests performance across different storage backends and dataset sizes
- **Resource Utilization Monitoring**: Monitors memory, CPU, and I/O patterns during operations
- **Scalability Benchmarks**: Tests performance scaling from 100 → 500 → 1000 → 2000+ documents

#### Running the Performance Tests

```bash
# Run all performance tests
cargo test -p crucible-cli --test large_dataset_performance_tests

# Run specific test categories
cargo test -p crucible-cli test_large_dataset_generation --test large_dataset_performance_tests
cargo test -p crucible-cli test_performance_threshold_validation --test large_dataset_performance_tests

# Run the complete performance suite manually
cargo run -p crucible-cli --test large_dataset_performance_tests
```

#### Performance Reports

The test suite generates comprehensive reports:
- `large_dataset_performance_report.json` - Machine-readable metrics
- `large_dataset_performance_report.md` - Human-readable summary

#### Performance Thresholds

- Maximum search time: 5 seconds
- Maximum memory usage: 1GB
- Minimum processing rate: 10 documents/second
- Maximum error rate: 5%

### CI/CD Notes

Tests use `ratatui::backend::TestBackend` for headless testing - they do NOT require a real terminal. This makes them CI-friendly and suitable for automated testing environments.

All tests are async (`#[tokio::test]`) to match production TUI event loop behavior.

### Troubleshooting

**Issue:** Tests panic with "not yet implemented"
**Solution:** This is expected in RED phase - implement the `todo!()` calls

**Issue:** "unused function" warnings
**Solution:** These are expected until tests are implemented (helpers not yet used)

**Issue:** Test hangs or times out
**Solution:** Check for blocking operations in async tests, use `tokio::time::timeout`

**Issue:** Channel errors (send/receive)
**Solution:** Ensure channel capacity is sufficient, check sender isn't dropped

### Next Steps

1. Read `TUI_INTEGRATION_TEST_SUMMARY.md` for detailed test specifications
2. Start implementing tests one at a time (replace `todo!()` calls)
3. Run tests after each change: `cargo test -p crucible-cli --test tui_integration test_name`
4. Once all tests pass, move to refactoring phase

### Questions?

See:
- `/home/moot/crucible/crates/crucible-cli/src/tui/` - TUI implementation code
- `/home/moot/crucible/docs/TUI_ARCHITECTURE.md` - TUI architecture documentation
- `TUI_INTEGRATION_TEST_SUMMARY.md` - Detailed test specifications
