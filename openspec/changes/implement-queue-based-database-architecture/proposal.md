# Implement Queue-Based Database Architecture

## Why

The current Crucible architecture experiences RocksDB lock contention when multiple processing threads attempt concurrent database operations, causing "lock hold by current process" errors and reduced performance. The current tightly-coupled design where file processing threads make direct synchronous database calls creates I/O bottlenecks and prevents optimal resource utilization.

Core problems this addresses:
- **Database Lock Contention**: Multiple threads competing for RocksDB file locks cause failures
- **Resource Inefficiency**: CPU-bound file parsing threads block on I/O operations
- **Scalability Limits**: Performance degrades under high concurrency due to database bottlenecks
- **Transaction Coordination**: No centralized coordination between database operations

## What Changes

- **BREAKING**: Replace direct database calls from processing threads with queued transactions
- **Add Dedicated Database Thread**: Single consumer thread for all database operations
- **Implement Transaction Queue**: Bounded queue with backpressure handling
- **Separate Processing Concerns**: CPU-bound parsing separated from I/O-bound database operations
- **Transaction Batching**: Ability to batch multiple operations for better performance
- **Error Isolation**: Centralized error handling and retry logic for database operations

## Impact

- **Affected specs**:
  - `file-processing` - Modify processing pipeline to use queued transactions
  - `database-consistency` - Add transaction queue management and coordination
- **Affected code**:
  - `crates/crucible-surrealdb/src/kiln_processor.rs` - Replace direct DB calls with transaction queuing
  - `crates/crucible-cli/src/common/kiln_processor.rs` - Update integrated processing workflow
- **Architecture Changes**:
  - Single-threaded database access eliminates lock contention
  - Queue-based transaction processing with configurable limits
  - Clear separation between CPU-intensive parsing and I/O-intensive persistence
- **Performance Impact**:
  - Eliminated database lock contention errors
  - Better CPU utilization by preventing processing thread I/O blocks
  - Improved scalability for large file sets
  - Configurable queue limits for memory management

## Timeline

- **Phase 1** (2-3 hours): Implement transaction queue infrastructure and database consumer thread
- **Phase 2** (2-3 hours): Modify file processing pipeline to use queued transactions
- **Phase 3** (1-2 hours): Add transaction batching and error handling
- **Phase 4** (1-2 hours): Testing, performance validation, and configuration tuning
- **Total**: 6-10 hours of development work

## Benefits

- **Eliminated Lock Contention**: Single database thread prevents RocksDB conflicts
- **Better Resource Utilization**: Processing threads focus on CPU work, never block on I/O
- **Improved Scalability**: Architecture scales better with large file sets and high concurrency
- **Cleaner Separation**: Clear boundaries between parsing logic and persistence logic
- **Transaction Optimization**: Opportunities for batching and operation reordering
- **Better Error Handling**: Centralized retry logic and error recovery mechanisms