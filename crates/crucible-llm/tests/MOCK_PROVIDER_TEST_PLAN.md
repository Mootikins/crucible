# MockEmbeddingProvider Test Implementation Plan

## Overview

This document describes the comprehensive test suite for `MockEmbeddingProvider` that uses pre-generated deterministic fixtures instead of algorithmic generation. The tests are currently **failing** and will pass once the `FixtureBasedMockProvider` is properly implemented.

## Current Status

✅ **Tests Created**: All tests are written and properly documented
❌ **Implementation Missing**: `FixtureBasedMockProvider` doesn't implement `EmbeddingProvider` trait
❌ **Mock Module Access**: Need to enable `test-utils` feature for mock access
❌ **Method Implementation**: All trait methods need implementation

## Test Structure

### 1. Fixture Data Architecture (`EmbeddingFixtures`)

The `EmbeddingFixtures` struct provides:

- **Pre-generated embeddings**: Deterministic vectors for common test texts
- **Model information**: Structured metadata for different embedding models
- **Batch fixtures**: Pre-computed batch responses for consistency
- **Model dimensions**: Expected dimensions for different models

### 2. Target Implementation (`FixtureBasedMockProvider`)

The `FixtureBasedMockProvider` struct should:

- Load fixtures from `EmbeddingFixtures`
- Implement the `EmbeddingProvider` trait
- Return deterministic data based on fixtures
- Support different models (nomic-embed-text-v1.5, text-embedding-3-small)
- Handle unknown texts gracefully

## Test Coverage

### ✅ Single Text Embedding Tests
- Basic embedding generation with fixtures
- Deterministic behavior verification
- Model-specific embeddings
- Unicode and special character handling
- Empty string handling

### ✅ Batch Processing Tests
- Multi-text batch generation
- Large batch performance
- Mixed known/unknown text handling
- Concurrent access patterns

### ✅ Model Information Tests
- Model metadata from fixtures
- Dimension verification
- Provider information
- Health check functionality

### ✅ Configuration Integration Tests
- Integration with `EmbeddingConfig`
- crucible-config compatibility
- Model selection based on configuration

### ✅ Error Handling Tests
- Unknown text handling
- Batch error scenarios
- Graceful degradation

### ✅ Performance and Concurrency Tests
- Concurrent access safety
- Large batch processing
- Response time verification

## Implementation Requirements

### 1. Enable Mock Module Access

```rust
// In Cargo.toml or test configuration
[features]
test-utils = []  # Already present

// Run tests with:
cargo test -p crucible-llm --features test-utils
```

### 2. Implement EmbeddingProvider Trait

```rust
#[async_trait]
impl EmbeddingProvider for FixtureBasedMockProvider {
    async fn embed(&self, text: &str) -> EmbeddingResult<EmbeddingResponse> {
        // Look up text in fixtures
        // Return fixture data or generate deterministic fallback
    }

    async fn embed_batch(&self, texts: Vec<String>) -> EmbeddingResult<Vec<EmbeddingResponse>> {
        // Process batch using fixtures
        // Handle mixed known/unknown texts
    }

    fn model_name(&self) -> &str {
        &self.model_name
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn provider_name(&self) -> &str {
        "mock-fixture"
    }

    async fn health_check(&self) -> EmbeddingResult<bool> {
        Ok(true) // Always healthy for fixture-based provider
    }

    async fn list_models(&self) -> EmbeddingResult<Vec<ModelInfo>> {
        // Return model info from fixtures
    }
}
```

### 3. Fixture Lookup Strategy

- **Known texts**: Return exact fixture data
- **Unknown texts**: Generate deterministic fallback based on text hash
- **Batch processing**: Process each text individually or use batch fixtures
- **Model information**: Return structured metadata from fixtures

### 4. Error Handling Approach

- **Unknown texts**: Either return deterministic embeddings or graceful error
- **Invalid inputs**: Return appropriate `EmbeddingError` variants
- **Configuration errors**: Validate model names and dimensions

## Running the Tests

### Current State (Expected Failures)

```bash
# Tests should fail with compilation errors
cargo test -p crucible-llm --features test-utils

# Expected errors:
# - E0432: mock module not found
# - E0599: trait methods not implemented
# - Missing async keyword on test functions
```

### After Implementation

```bash
# Tests should pass
cargo test -p crucible-llm --features test-utils

# All 16 test cases should pass:
# - Single text embedding tests
# - Batch processing tests
# - Model information tests
# - Configuration integration tests
# - Error handling tests
# - Performance and concurrency tests
```

## Design Benefits

### 1. Deterministic Behavior
- Pre-generated fixtures ensure identical results across test runs
- No algorithmic variation between executions
- Consistent test behavior regardless of environment

### 2. Test Isolation
- No external dependencies or network calls
- No real embedding provider usage
- Eliminates test pollution from real services

### 3. Performance
- Fast test execution with pre-computed data
- No network latency or processing delays
- Suitable for CI/CD environments

### 4. Comprehensive Coverage
- Tests all aspects of the `EmbeddingProvider` trait
- Covers edge cases and error scenarios
- Validates integration with configuration system

## Integration with Existing System

### 1. Compatibility with crucible-surrealdb
The new mock provider should integrate seamlessly with the existing `EmbeddingThreadPool` in `crucible-surrealdb/src/embedding_pool.rs`.

### 2. Configuration System Integration
Should work with `EmbeddingConfig` from `crucible-llm/src/embeddings/config.rs` and integrate with the broader `crucible-config` system.

### 3. Provider Factory Pattern
Should be creatable through the existing `create_provider()` function or a new fixture-specific factory.

## Next Steps

1. **Implement the trait methods** for `FixtureBasedMockProvider`
2. **Enable test-utils feature** for mock module access
3. **Add fixture loading logic** for robust data management
4. **Run comprehensive tests** to verify all functionality
5. **Integrate with existing embedding system** components

## Files Created

- `/home/moot/crucible/crates/crucible-llm/tests/mock_embedding_provider_tests.rs` - Comprehensive test suite
- `/home/moot/crucible/crates/crucible-llm/tests/MOCK_PROVIDER_TEST_PLAN.md` - This documentation

The failing tests provide a clear specification for exactly how the MockEmbeddingProvider should behave with deterministic fixture data.