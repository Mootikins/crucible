# Deprecated Code Migration Plan

> Plan for migrating deprecated but actively-used code to unified systems

**Created**: 2025-01-17
**Status**: Ready for implementation

## Overview

Two deprecated systems are still actively used and need migration:

1. **embedding_events** (crucible-watch) → SessionEvent variants
2. **RingHandler** (crucible-rune) → Unified Handler trait

This document provides detailed migration plans for both.

---

## Part 1: EmbeddingEvent → SessionEvent Migration

### Summary

| Metric | Value |
|--------|-------|
| **Complexity** | Low |
| **Effort** | 1-2 days |
| **Files to change** | 2 active files |
| **Lines affected** | ~200 |
| **Breaking changes** | Internal only (no downstream impact) |

### Type Mapping

| Deprecated (crucible-watch) | Replacement (crucible-core) |
|----------------------------|----------------------------|
| `EmbeddingEvent` | `SessionEvent::EmbeddingRequested` |
| `EmbeddingEventMetadata` | Removed (encoded in entity_id) |
| `EmbeddingEventPriority` | `Priority` enum |
| `EmbeddingEventResult::success()` | `SessionEvent::EmbeddingStored` |
| `EmbeddingEventResult::failure()` | `SessionEvent::EmbeddingFailed` |
| `EventDrivenEmbeddingConfig` | Removed (config in embedding provider) |

### Files to Modify

#### 1. `crates/crucible-watch/src/handlers/indexing.rs` (PRIMARY)

**Current state**: Uses `EmbeddingEvent`, `EventDrivenEmbeddingConfig`, `create_embedding_metadata`

**Changes required**:
```rust
// REMOVE these imports:
use crate::embedding_events::{
    create_embedding_metadata, 
    EmbeddingEvent, 
    EventDrivenEmbeddingConfig
};

// REMOVE these fields from IndexingHandler:
embedding_config: EventDrivenEmbeddingConfig,
embedding_event_tx: Option<mpsc::UnboundedSender<EmbeddingEvent>>,

// REMOVE these methods:
pub fn with_embedding_config(config: EventDrivenEmbeddingConfig) -> Result<Self>
pub fn with_embedding_event_channel(self, tx: mpsc::UnboundedSender<EmbeddingEvent>) -> Self

// REFACTOR create_and_emit_embedding_event() to use SessionEvent:
// Before:
let metadata = create_embedding_metadata(&path, &trigger_event, file_size);
let embedding_event = EmbeddingEvent::new(path.clone(), trigger_event, content, metadata);
if let Some(tx) = &self.embedding_event_tx {
    tx.send(embedding_event)?;
}

// After:
let priority = match trigger_event {
    FileEventKind::Created => Priority::High,
    FileEventKind::Modified => Priority::Normal,
    FileEventKind::Deleted => Priority::Low,
    _ => Priority::Normal,
};
let event = SessionEvent::EmbeddingRequested {
    entity_id: format!("note:{}", path.display()),
    block_id: None,
    priority,
};
self.emitter.emit(event).await;
```

**Effort**: 2-3 hours

#### 2. `crates/crucible-watch/src/lib.rs`

**Changes required**:
```rust
// REMOVE from main exports:
#[allow(deprecated)]
pub use embedding_events::*;

// REMOVE from prelude:
pub use crate::embedding_events::{
    create_embedding_metadata, 
    determine_content_type, 
    determine_event_priority,
    generate_document_id, 
    EmbeddingEvent, 
    EmbeddingEventMetadata, 
    EmbeddingEventPriority,
    EmbeddingEventResult, 
    EventDrivenEmbeddingConfig,
};

// KEEP the module private for backward compat during transition:
mod embedding_events;
```

**Effort**: 30 minutes

#### 3. `crates/crucible-watch/src/embedding_events.rs`

**Action**: Keep as-is during transition, then delete after 1-2 releases

### Migration Steps

```
[ ] 1. Update indexing.rs to emit SessionEvent::EmbeddingRequested
[ ] 2. Remove embedding_config and embedding_event_tx fields
[ ] 3. Remove with_embedding_config() and with_embedding_event_channel() methods
[ ] 4. Update lib.rs to remove public re-exports
[ ] 5. Run tests: cargo test -p crucible-watch
[ ] 6. Update EVENT_DRIVEN_EMBEDDING_TESTS_SUMMARY.md
[ ] 7. Mark embedding_events.rs for removal in changelog
```

---

## Part 2: RingHandler → Unified Handler Migration

### Summary

| Metric | Value |
|--------|-------|
| **Complexity** | High |
| **Effort** | 3-4 weeks |
| **Files to change** | 7 modules |
| **Lines affected** | ~6,200 |
| **Breaking changes** | Public API (deprecation period required) |

### Dependency Graph

```
handler.rs (LEAF - migrate first)
    ↓
    ├── persistence_handler.rs
    ├── logging_handler.rs
    ├── dependency_graph.rs
    │       ↓
    └── handler_chain.rs
            ↓
        handler_wiring.rs
            ↓
        linear_reactor.rs (ROOT - migrate last)
```

### Key Differences

| Aspect | RingHandler | Unified Handler |
|--------|------------|-----------------|
| Generic | `RingHandler<E>` | `Handler` (SessionEvent only) |
| Sequence | `seq: u64` param | Not passed |
| Event | `Arc<E>` | Owned `SessionEvent` |
| Result | `RingHandlerResult<()>` | `HandlerResult<SessionEvent>` |
| Lifecycle | `on_register()`/`on_unregister()` | Handled by Reactor |
| Priority | Implicit | Explicit `priority()` method |
| Filtering | None | `event_pattern()` glob |

### Files to Modify (in order)

#### Phase 1: Foundation (Week 1)

**1. `handler.rs`** - Mark deprecated, add re-exports
- Effort: 2-3 days
- Add `#[deprecated]` to module
- Keep as shim for backward compat

**2. `persistence_handler.rs`** - Implement Handler trait
- Effort: 3-4 days
- Change `impl RingHandler<SessionEvent>` → `impl Handler`
- Update handle() signature
- Update error handling
- ~300 lines of tests to update

**3. `logging_handler.rs`** - Implement Handler trait
- Effort: 3-4 days
- Same changes as persistence_handler
- ~200 lines of tests

#### Phase 2: Infrastructure (Week 2)

**4. `dependency_graph.rs`** - Remove generic parameter
- Effort: 4-5 days
- `HandlerGraph<E>` → `HandlerGraph`
- Update to use `Box<dyn Handler>`
- Critical: topo-sort correctness testing

**5. `handler_wiring.rs`** - Update bridge
- Effort: 3-4 days
- `EventBusRingHandler` → `EventBusHandler`
- Implement new Handler trait

**6. `handler_chain.rs`** - Update execution engine
- Effort: 5-6 days
- Remove generic parameter
- Update result handling
- ~300 lines of tests

#### Phase 3: Public API (Week 3)

**7. `linear_reactor.rs`** - Update public interface
- Effort: 5-7 days
- Update `add_handler()` signature
- Update all callers
- Integration testing

**8. `lib.rs`** - Update re-exports
- Effort: 1 day
- Add deprecation notices
- Point to crucible-core types

#### Phase 4: Testing (Week 4)

- Integration testing with Rune scripts
- Cross-language handler testing
- Documentation updates

### Signature Changes

```rust
// BEFORE (RingHandler)
#[async_trait]
impl RingHandler<SessionEvent> for MyHandler {
    fn name(&self) -> &str { "my_handler" }
    fn depends_on(&self) -> &[&str] { &[] }
    
    async fn handle(
        &self,
        ctx: &mut RingHandlerContext<SessionEvent>,
        event: Arc<SessionEvent>,
        seq: u64,
    ) -> RingHandlerResult<()> {
        // process event
        Ok(())
    }
}

// AFTER (Unified Handler)
#[async_trait]
impl Handler for MyHandler {
    fn name(&self) -> &str { "my_handler" }
    fn dependencies(&self) -> &[&str] { &[] }
    fn priority(&self) -> i32 { 50 }
    fn event_pattern(&self) -> &str { "*" }
    
    async fn handle(
        &self,
        ctx: &mut HandlerContext,
        event: SessionEvent,
    ) -> HandlerResult<SessionEvent> {
        // process event
        HandlerResult::ok(event)
    }
}
```

### Error Mapping

```rust
// BEFORE
RingHandlerError::NonFatal { handler, message }
RingHandlerError::Fatal { handler, message }

// AFTER
HandlerResult::SoftError { event, error: message }
HandlerResult::FatalError(EventError::other(message))
```

### Migration Checklist

```
Week 1: Foundation
[ ] Mark handler.rs as deprecated
[ ] Migrate persistence_handler.rs to Handler trait
[ ] Migrate logging_handler.rs to Handler trait
[ ] Run tests after each migration

Week 2: Infrastructure
[ ] Migrate dependency_graph.rs (remove generics)
[ ] Extensive topo-sort testing
[ ] Migrate handler_wiring.rs
[ ] Migrate handler_chain.rs
[ ] Run integration tests

Week 3: Public API
[ ] Migrate linear_reactor.rs
[ ] Update all callers
[ ] Update lib.rs re-exports
[ ] Add deprecation warnings

Week 4: Polish
[ ] Full integration testing
[ ] Rune script compatibility testing
[ ] Documentation updates
[ ] Changelog entry
[ ] Release notes
```

### Missing Features to Add (Optional)

The unified Handler system currently lacks:

1. **Lifecycle hooks** (`on_register`/`on_unregister`) - Consider adding to Handler trait if needed
2. **Handler introspection** - `RingHandlerInfo` equivalent
3. **Priority stability** - Stable sort for deterministic ordering

These can be added during or after migration if needed.

---

## Recommended Execution Order

### Option A: Parallel Execution (2 developers)

- **Developer 1**: EmbeddingEvent migration (1-2 days)
- **Developer 2**: RingHandler migration Week 1-2 (foundation + infrastructure)
- **Both**: RingHandler migration Week 3-4 (public API + testing)

### Option B: Sequential Execution (1 developer)

1. EmbeddingEvent migration (Days 1-2)
2. RingHandler migration (Days 3-20)
3. Integration testing (Days 21-25)

### Recommended Branch Strategy

```bash
# Main migration branch
git checkout -b feat/unify-event-systems

# Sub-branches for each phase
git checkout -b feat/embedding-event-migration
git checkout -b feat/handler-migration-phase1
git checkout -b feat/handler-migration-phase2
git checkout -b feat/handler-migration-phase3
```

---

## Success Criteria

### EmbeddingEvent Migration Complete When:

- [ ] No code imports from `crucible_watch::embedding_events`
- [ ] `IndexingHandler` emits `SessionEvent::EmbeddingRequested`
- [ ] All tests pass
- [ ] `embedding_events.rs` is private (not exported)

### RingHandler Migration Complete When:

- [ ] No code uses `RingHandler` trait directly
- [ ] All handlers implement `crucible_core::events::Handler`
- [ ] `LinearReactor` accepts `Box<dyn Handler>`
- [ ] All tests pass (1,465+ lines)
- [ ] Rune scripts work with new system
- [ ] Documentation updated

---

## Risks and Mitigations

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|-----------|
| Topo-sort regression | Medium | High | Extensive unit tests; compare outputs |
| Event processing semantics change | Medium | High | Snapshot tests; preserve behavior |
| Rune script breakage | Low | Medium | Test existing scripts; provide examples |
| Public API confusion | Medium | Low | Clear deprecation warnings; migration guide |

---

## References

- `crucible-core/src/events/handler.rs` - Unified Handler trait
- `crucible-core/src/events/reactor.rs` - Unified Reactor
- `crucible-core/src/events/session_event.rs` - SessionEvent with embedding variants
- `crucible-rune/src/handler.rs` - Deprecated RingHandler (migration source)
- `crucible-watch/src/embedding_events.rs` - Deprecated embedding types
