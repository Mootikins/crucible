# Critical Architecture Issue: Mock Masquerading as Production Client

## Problem

The `SurrealClient` in `multi_client.rs` is **actually a mock** (in-memory HashMap with custom SQL parser), not a real SurrealDB client. This is a fundamental naming/architecture inversion.

### What It Should Be

```
SurrealClient (production)
  └── Wraps surrealdb::Surreal<Db>
  └── Used by all kiln operations
  └── Supports file/memory/remote backends

MockSurrealClient (testing)
  └── Simple in-memory mock for unit tests
  └── Or just use surrealdb::Surreal<Mem> directly
```

### What It Currently Is

```
SurrealClient (IS THE MOCK!)
  └── In-memory HashMap
  └── Custom SQL parser
  └── Complex query emulation
  └── Used in PRODUCTION CODE

No Real Client Exists!
```

## Impact

- **Production code uses a mock** - All kiln operations run against HashMap, not real DB
- **Mock complexity spiraled** - Trying to emulate SurrealDB led to complex SQL parsing
- **Testing is backwards** - Can't test against real DB because there's no real client
- **Performance unknown** - Haven't validated actual SurrealDB performance
- **Limited functionality** - Mock only supports subset of SurrealQL

## Files Affected

Core dependencies on current mock "SurrealClient":
- `src/kiln_integration.rs` - All kiln database operations
- `src/kiln_processor.rs` - Document processing pipeline
- `src/kiln_scanner.rs` - Kiln scanning and indexing
- `src/kiln_pipeline_connector.rs` - Pipeline coordination
- `src/embedding_pipeline.rs` - Embedding storage

## Refactoring Plan

### Phase 1: Create Real SurrealClient (HIGH PRIORITY)

Create `src/surreal_client.rs`:
```rust
pub struct SurrealClient {
    db: surrealdb::Surreal<surrealdb::engine::local::Db>,
    config: SurrealDbConfig,
}

impl SurrealClient {
    pub async fn new_file(path: &str) -> Result<Self> { ... }
    pub async fn new_memory() -> Result<Self> { ... }
    pub async fn new_remote(url: &str) -> Result<Self> { ... }

    // Expose real SurrealDB API - no custom SQL parsing!
    pub async fn query(&self, sql: &str) -> Result<Response> { ... }
    pub async fn create<T>(&self, table: &str) -> Result<T> { ... }
    // ... other SurrealDB SDK methods
}
```

### Phase 2: Rename Mock

Rename `multi_client.rs` → `mock_client.rs`:
```rust
pub struct MockSurrealClient {  // Renamed!
    storage: Arc<RwLock<MockStorage>>,
    // Keep simple - basic CRUD only
}
```

**Or better**: Just use `surrealdb::Surreal<Mem>` in tests directly

### Phase 3: Update All Usages

Update imports in:
- kiln_integration.rs
- kiln_processor.rs
- kiln_scanner.rs
- kiln_pipeline_connector.rs
- embedding_pipeline.rs

Change from:
```rust
use crate::SurrealClient;  // Was the mock
```

To:
```rust
use crate::SurrealClient;  // Now the real client
```

### Phase 4: Update Tests

Tests currently in `archived-mock/`:
- Rewrite to use real `SurrealClient` with in-memory backend
- Or use `surrealdb::Surreal<Mem>` directly
- Only use mocks for true unit tests if absolutely necessary

## Current Workaround

The mock is still in place (`multi_client.rs`) because:
1. Removing it breaks all kiln operations
2. Creating a real client is a separate large task
3. Tests using the mock have been archived to `archived-mock/`

## Next Steps

1. **Immediate**: Document this issue (✓ this file)
2. **Short-term**: Create real `SurrealClient` implementation
3. **Medium-term**: Migrate all code to use real client
4. **Long-term**: Restore tests with real SurrealDB

## Historical Context

This issue was discovered 2025-10-27 while fixing embedding_storage_tests after a graph relations refactor. Tests were failing because the mock couldn't handle complex graph traversal queries. Attempts to extend the mock revealed it had grown too complex, at which point we realized the fundamental architecture inversion.

## Related Files

- `archived-mock/README.md` - Detailed explanation of archived tests
- `archived-mock/multi_client.rs` - The mock (currently still in src/)
- `archived-mock/embedding_storage_tests.rs` - Tests that need rewriting
