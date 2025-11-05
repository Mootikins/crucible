# Design Document: Batch-Aware Database Consistency

## Context

The Crucible system supports concurrent database operations but previously lacked consistency guarantees when batch processing was enabled. The challenge was that metadata reads could return stale data while files were being processed in batches, creating race conditions between the database state and in-flight operations.

Key constraints:
- Must maintain existing concurrent read performance (RwLock-based)
- Cannot block existing database operations
- Need to integrate with existing EventDrivenEmbeddingProcessor
- Must provide different consistency levels for different use cases

## Goals / Non-Goals

**Goals:**
- Provide queue-aware database reads with configurable consistency
- Maintain existing concurrent read performance
- Enable integration between file watching and batch processing
- Support easy migration from existing SurrealClient usage
- Ensure thread-safe pending operation tracking

**Non-Goals:**
- Modify core SurrealDB transaction handling
- Change existing database schema
- Implement distributed transactions
- Add network-level consistency guarantees

## Decisions

### Decision 1: Wrapper Pattern over Core Modification
**What**: Create BatchAwareSurrealClient as a wrapper around existing SurrealClient
**Why**:
- Preserves existing behavior for unchanged code
- Allows gradual migration
- Doesn't risk breaking concurrent read performance
- Easier to test and maintain

**Alternatives considered:**
- Modify SurrealClient directly (too invasive, risk of breaking changes)
- Create new database interface (unnecessary complexity)

### Decision 2: Three-Level Consistency Model
**What**: Eventual, ReadAfterWrite, and Strong consistency levels
**Why**:
- Covers all common use cases from performance to correctness
- Follows established database consistency patterns
- Provides clear trade-offs for developers

**Alternatives considered:**
- Single consistency level (too restrictive)
- Complex fine-grained controls (over-engineering)

### Decision 3: Event Processor Trait Integration
**What**: Define EventProcessor trait for batch system integration
**Why**:
- Decouples batch-aware client from specific batch implementations
- Allows testing with mock processors
- Supports future batch system changes

**Alternatives considered:**
- Direct coupling to EventDrivenEmbeddingProcessor (too tight)
- No batch system integration (limited usefulness)

### Decision 4: Pending Operation Index by File Path
**What**: Track pending operations indexed by file path for efficient lookups
**Why**:
- Most consistency checks are file-specific
- O(1) lookup for file pending status
- Minimal memory overhead for typical workloads

**Alternatives considered:**
- Global pending operation list (inefficient lookups)
- No operation tracking (no consistency guarantees)

## Risks / Trade-offs

**Risk**: Performance overhead of consistency checking
**Mitigation**:
- Consistency checking is optional and configurable
- Uses efficient data structures (HashMap indexing)
- Minimal impact on hot path operations

**Risk**: Increased complexity for developers
**Mitigation**:
- Simple extension trait migration: `client.batch_aware()`
- Clear documentation and examples
- Sensible defaults (Eventual consistency)

**Trade-off**: Memory usage vs. consistency tracking
**Decision**: Accept modest memory increase for significant consistency benefits
**Impact**: ~few KB per pending operation, negligible for typical workloads

## Migration Plan

### Phase 1: Gradual Adoption
1. Existing code continues unchanged
2. New features can opt-in to batch-aware clients
3. Critical CLI commands migrate first

### Phase 2: Default Migration
1. Update CLI creation to use batch-aware clients by default
2. Existing behavior preserved through Eventual consistency
3. Applications can opt-up to stronger consistency as needed

### Phase 3: Enhanced Integration
1. More batch systems implement EventProcessor trait
2. Additional consistency features based on usage patterns
3. Performance optimizations based on real-world data

## Open Questions

- Should we add consistency metrics for monitoring? (Future enhancement)
- Do we need database-level consistency events? (Under consideration)
- Can we extend this to other database backends? (Architecture supports it)