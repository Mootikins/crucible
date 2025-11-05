# Queue-Based Database Architecture Design

## Context

The current Crucible system uses a multi-threaded architecture where file processing threads make direct synchronous database operations. This creates lock contention with RocksDB and causes performance bottlenecks when processing large numbers of files concurrently.

## Critical Architectural Issue: RocksDB Lock Contention

**Problem**: Multiple threads accessing RocksDB simultaneously cause lock contention with "lock hold by current process" errors during file processing.

**Solution**: Implement **single-threaded database consumer** with simple transaction queue:
- File processing threads: Parse files and enqueue documents
- Database consumer: Single thread processes all database operations sequentially
- Simple diff-based approach: Consumer figures out what changed automatically

**Key Design Principle**: **SIMPLICITY FIRST** - The database layer should be smart about diffing, not the file processing layer. File processing just says "here's the new document state" and the consumer figures out what actually needs to change.

## Updated Architecture: Drastically Simplified with Workspace Sync

### Original Plan (Over-Engineered):
- 6 granular transaction types
- ProcessedDocument wrapper with ProcessingContext
- TransactionBuilder complex logic
- ProcessingFlags with boolean instructions
- Multiple statistics structures
- Document-level Merkle trees
- **Result: 2,677 lines, cognitive load 8/10**

### Simplified Architecture:
- 3 transaction types: Create, Update, Delete
- Just ParsedDocument - no wrapper needed
- Consumer does intelligent diffing automatically
- Single statistics structure
- **Workspace-level Merkle trees for sync compatibility**
- **Result: ~300 lines, cognitive load 4/10**

**Key Addition**: Workspace-level Merkle trees enable multi-device synchronization while maintaining the simplicity of parallel document processing.

## Goals / Non-Goals

**Goals:**
- Eliminate RocksDB lock contention by serializing database access
- Improve CPU utilization by preventing processing threads from blocking on I/O
- Maintain existing storage interfaces to preserve compatibility
- Provide better error handling and retry mechanisms
- Enable transaction batching and optimization opportunities

**Non-Goals:**
- Change the public storage API contracts
- Add new database backends or storage types
- Modify the file parsing logic or data structures
- Change the overall CLI workflow or user experience

## Decisions

### Decision: Maximum Simplicity
**Choice**: Use existing ParsedDocument directly with intelligent consumer diffing.

**Rationale**:
- **No wrapper types needed** - ParsedDocument already contains everything needed
- **Consumer intelligence** - Database layer figures out what changed automatically using existing Merkle tree infrastructure
- **Git-like diffing** - Just like Git, we give the new state and let the system figure out differences
- **Minimal learning curve** - Developers only need to understand existing types

**Simplified Design:**
- **ParsedDocument**: Existing type, no changes needed
- **3 transaction types**: Create, Update, Delete (standard CRUD)
- **Smart consumer**: Automatically detects what changed using Merkle tree diffing and updates accordingly

**Why this beats complex handoff types:**
- Zero new types to learn beyond basic transaction enum
- No processing metadata or flags to manage
- Consumer handles all complexity internally using existing diff infrastructure
- Follows proven patterns from version control systems
- Leverages existing sophisticated Merkle tree change detection

### Decision: Transaction Queue Architecture
**Choice**: Implement a bounded transaction queue with a single consumer thread.

**Rationale**:
- Single consumer eliminates RocksDB lock contention entirely
- Bounded queue provides natural backpressure
- Separation of concerns improves maintainability
- Allows for transaction batching and optimization

**Alternatives considered:**
- Database connection pool: Still suffers from RocksDB locking issues
- Semaphore limiting concurrent operations: Better but still allows contention
- Multiple database files: Adds complexity without solving core issue

### Decision: Transaction Granularity
**Choice**: Document-level CRUD transactions with intelligent diffing.

**Rationale**:
- **One transaction per document** - Simple and predictable
- **Consumer figures out sub-operations** - No need to specify what to update
- **Git-like model** - "Here's the new state, you figure out what changed"
- **Zero configuration** - No flags or instructions needed from file processing

**Consumer Diffing Logic:**
```rust
// For Update transactions, consumer automatically:
// 1. Read existing document
// 2. Compare with new ParsedDocument
// 3. Update only what changed (content, links, embeddings, tags)
// 4. Generate appropriate sub-operations automatically
```

**Alternatives considered:**
- Operation-level transactions: Too complex, requires processing layer knowledge
- Instruction-based updates: Requires flags and configuration, error-prone

### Decision: Queue Implementation
**Choice**: tokio mpsc channel with bounded capacity.

**Rationale**:
- Native async support with tokio runtime
- Built-in backpressure when queue is full
- Simple and reliable implementation
- Good performance characteristics

**Alternatives considered:**
- Custom ring buffer: More complex, marginal performance gain
- Database-backed queue: Overkill, adds unnecessary persistence

### Decision: Error Handling Strategy
**Choice**: Centralized retry logic with exponential backoff and dead-letter queue.

**Rationale**:
- Prevents transient database errors from blocking processing
- Provides visibility into persistent failures
- Allows for manual intervention on problematic operations
- Maintains system stability under adverse conditions

**Alternatives considered:**
- Immediate failure propagation: Too brittle, poor user experience
- Infinite retry: Can hide persistent problems and consume resources

## Risks / Trade-offs

### Risk: Queue Overflow
**Mitigation**: Bounded queue with backpressure prevents memory exhaustion. Processing threads block or skip when queue is full.

### Risk: Single Point of Failure
**Mitigation**: Database consumer thread has comprehensive error handling and automatic recovery. Health monitoring detects issues early.

### Risk: Increased Latency
**Mitigation**: Queue processing is prioritized and batched for efficiency. Latency is offset by improved throughput and reliability.

### Trade-off: Memory Usage vs. Performance
**Decision**: Accept moderate memory usage for queued transactions in exchange for significantly better throughput and reliability.

### Trade-off: Complexity vs. Maintainability
**Decision**: **RADICAL SIMPLIFICATION** over architectural complexity.

**Previous assumption**: More complexity = better maintainability
**Reality**: Simplicity = better maintainability, fewer bugs, easier onboarding

**Learning curve comparison:**
- **Complex version**: 12 concepts, 2,677 lines, cognitive load 8/10
- **Simple version**: 4 concepts, ~300 lines, cognitive load 4/10

**The simple version maintains all benefits while being dramatically easier to understand and maintain.**

## Migration Plan

### Phase 1: Core Infrastructure & Layer Separation
1. **Create transaction data structures and queue implementation** ✅
2. **Implement database consumer thread with basic operation handling**
3. **Add core layer handoff types** (`ProcessedDocument`, `DocumentProcessingJob`)
4. **Create database transaction builder API**
5. **Add comprehensive logging and metrics**
6. **Create configuration system for queue behavior**

### Phase 2: Layer Integration & Pipeline Refactoring
1. **Refactor file processing to use handoff architecture**
2. **Implement transaction builder integration**
3. **Modify process_single_file_internal() to use new flow**
4. **Implement result handling and error propagation**
5. **Add backpressure management**
6. **Update change detection for queued operations**
7. **Update read operations strategy for queue consistency**

### Phase 3: Reliability & Optimization
1. **Add transaction batching and optimization**
2. **Implement advanced error handling and retry logic**
3. **Add monitoring and diagnostic tools**
4. **Performance tuning and configuration optimization**
5. **Implement dependency resolution and transaction ordering**

### Phase 4: Testing & Validation
1. **Comprehensive testing of all failure scenarios**
2. **Performance benchmarking against current implementation**
3. **Layer separation validation and testing**
4. **Documentation updates and migration guides**
5. **Gradual rollout with monitoring**
6. **Integration testing with existing functionality**

## Open Questions

### Queue Capacity Configuration
- What should be the default queue size?
- How should queue capacity scale with system resources?
- Should different operation types have different queue priorities?

### Transaction Ordering
- Should transactions maintain insertion order or be reordered for optimization?
- How to handle dependencies between transactions?
- What heuristics should be used for transaction batching?

### Error Handling Policies
- How many retry attempts before giving up?
- What backoff strategy should be used?
- How should dead-letter transactions be handled?

### Performance Monitoring
- What metrics should be collected for queue performance?
- How to detect when the queue is becoming a bottleneck?
- What alert thresholds should be established?

## Architecture Diagram

```
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│ Processing      │    │ Transaction      │    │ Database        │
│ Thread 1        ├───▶│ Queue            ├───▶│ Consumer Thread │
├─────────────────┤    │ (bounded mpsc)   │    │ (single thread) │
│ Processing      │    └──────────────────┘    └─────────────────┘
│ Thread 2        │                                    │
├─────────────────┤                                    ▼
│ Processing      │                          ┌─────────────────┐
│ Thread N        │                          │ RocksDB         │
└─────────────────┘                          └─────────────────┘
```

## Implementation Notes

### Simplified Transaction Types (CRUD)

The simplified architecture uses only three transaction types:

1. **Create**: Insert a new document with full ParsedDocument data
2. **Update**: Replace an existing document, letting the consumer detect what changed
3. **Delete**: Remove a document entirely

Each transaction contains:
- Unique transaction identifier
- Document identifier (path-based or content hash-based)
- ParsedDocument with all content, links, tags, and metadata
- Transaction timestamp for ordering

The consumer handles all complexity of determining what actually needs to be updated based on the difference between the existing and incoming document state.

### Consumer Diffing Logic with Workspace Merkle Trees

The database consumer handles change detection at both document and workspace levels:

1. **Document-level processing**: Update individual document in database
2. **Workspace tree management**: Update the workspace-level Merkle tree with new document hash
3. **Change detection**: Compare workspace trees to detect what changed across the entire workspace
4. **Sync compatibility**: Enable workspace-level diffing for multi-device synchronization
5. **Conflict detection**: Identify when concurrent changes conflict at the workspace level

**Critical Architecture Decision**: Merkle trees operate at the workspace level, not individual document level. This provides:
- **Sync compatibility** across devices and branches
- **Global change detection** for the entire workspace
- **Conflict resolution** for concurrent modifications
- **Efficient incremental sync** by transferring only changed content blocks

This approach allows parallel document processing while maintaining workspace-level consistency and sync capabilities.

### Queue Configuration (Simplified)
- Default capacity: 1000 transactions
- Backpressure: Block processing threads when queue full
- Single priority level (all operations treated equally)
- Timeout: 30 seconds per transaction

### Error Handling (Simplified)
- Retry up to 3 times with exponential backoff
- Log failures with document ID and error type
- Continue processing other documents on failure
- No complex dead-letter queue needed