# Add Event & Hook Systems: Reactive Automation and API Surface

## Status: Partially Implemented

This proposal has been partially implemented. This document reflects current status as of December 2025.

## Implementation Status

### ✅ Implemented

**Event System Core (crucible-core/crucible-rune):**
- `SessionEvent` enum with 20+ variants (file, note, tool, agent, streaming)
- `EventBus` with priority-based handler registration
- `EventRing` ring buffer for event storage
- `SharedEventBus` trait for async event emission
- Handler results with Continue/Cancel/SoftError/FatalError
- Event metadata (timestamps via UUIDs)

**Handler Infrastructure:**
- `Handler` struct with event type filtering and glob patterns
- Priority-based execution order
- Pre/Post semantics via priority (low numbers = early = "pre")
- `EventContext` for cross-handler state

**File Watching Events:**
- `FileChanged`, `FileDeleted`, `FileMoved` events
- `NoteParsed` event with block count
- Integration with `crucible-watch` file watcher

**Storage Events:**
- `EntityStored`, `EntityDeleted`, `BlocksUpdated` events
- `EmbeddingRequested`, `EmbeddingGenerated` events
- Storage handlers emit events (not the store itself)

**Streaming Events:**
- `TextDelta { delta, seq }` for token-by-token streaming
- `AgentResponded`, `AgentThinking` events
- `ToolCalled`, `ToolCompleted` events

### 🟡 Partially Implemented

**Web API (crucible-web):**
- Axum server with basic routes
- SSE endpoint for streaming
- Svelte 5 SPA frontend
- Missing: Full REST API, OpenAPI spec, rate limiting

### ❌ Not Implemented

**Persistent Event Store:**
- Events are in-memory ring buffer only
- No query by time/type/actor
- No event replay functionality

**Hook Configuration (Markdown-based):**
- No YAML frontmatter hook definitions
- No hook discovery from `.crucible/hooks/`
- No shell/HTTP action types

**Webhook System:**
- No webhook registry
- No delivery with retry logic
- No HMAC signature verification

**CLI Commands:**
- No `cru hooks list/add/remove`
- No `cru webhooks` management

## Why (Original Motivation)

Crucible needs an **event-driven architecture** to enable:

1. **Local automation** (hooks): Trigger behaviors when events occur
2. **External integration** (webhooks): Push events to external systems
3. **API access** (REST/WebSocket): Programmatic control of memory operations
4. **Observability**: Track what happens in the system
5. **Dual-purpose architecture**: Memory infrastructure + agent platform

## What Remains

### Phase 1: Persistent Event Store (Priority: Medium)

Store events to SurrealDB for:
- Audit trail
- Event replay
- Query by time/type/actor

```rust
// New in crucible-surrealdb
pub struct EventStore {
    client: SurrealClient,
}

impl EventStore {
    pub async fn store(&self, event: &SessionEvent) -> Result<()>;
    pub async fn query(&self, query: EventQuery) -> Result<Vec<SessionEvent>>;
    pub async fn replay_from(&self, seq: u64) -> impl Stream<Item = SessionEvent>;
}
```

### Phase 2: Markdown Hook Configuration (Priority: Low)

Allow users to define hooks in `.crucible/hooks/*.md`:

```markdown
---
name: auto-link-related
trigger:
  events: [NoteParsed]
  timing: Post
action:
  type: RuneScript
  script: auto_link.rn
---
```

### Phase 3: Full REST API (Priority: Medium)

Expand crucible-web with:
- `POST /api/v1/events/query` - Query events
- `POST /api/v1/hooks` - Register hook
- `GET /api/v1/hooks` - List hooks
- OpenAPI spec generation

### Phase 4: Webhook Delivery (Priority: Low)

External webhook system:
- Webhook registry in SurrealDB
- Delivery with exponential backoff retry
- HMAC signature verification
- Delivery status tracking

## Impact

### Affected Specs
- **event-hooks** (extends) - Add remaining features
- **apis** (extends) - Full REST/WebSocket API

### Affected Code

**Already Exists:**
- `crates/crucible-core/src/events/` - Event types, emitter, subscriber
- `crates/crucible-rune/src/event_bus.rs` - Event bus implementation
- `crates/crucible-rune/src/session.rs` - Ring buffer, session handle
- `crates/crucible-watch/src/handlers/` - File watching event handlers
- `crates/crucible-surrealdb/src/event_handlers/` - Storage event handlers

**To Be Added:**
- `crates/crucible-surrealdb/src/event_store.rs` - Persistent event storage
- `crates/crucible-hooks/` - Hook configuration and execution (new crate)
- `crates/crucible-web/src/routes/events.rs` - Event query API
- `crates/crucible-web/src/routes/hooks.rs` - Hook management API

## Success Criteria (Updated)

**Done:**
- [x] Core event types defined
- [x] Event bus with priority handlers
- [x] Ring buffer for in-memory events
- [x] File/Note/Entity events emit correctly
- [x] Streaming events (TextDelta) work
- [x] Storage handlers emit events

**Remaining:**
- [ ] Events persisted to SurrealDB
- [ ] Event query API
- [ ] Hook configuration in markdown
- [ ] REST API with OpenAPI
- [ ] Webhook delivery system

## Migration from Original Proposal

The original proposal was ambitious (12-week plan). Given current implementation:

1. **Event System**: 80% complete - focus on persistence
2. **Hook System**: 20% complete - Rune handlers exist, markdown config missing
3. **Webhook System**: 0% complete - defer to later
4. **HTTP API**: 30% complete - SSE works, REST needs expansion

Recommended priority:
1. **Add-terminal-tui** (new proposal) - Uses existing event system
2. **Event persistence** - Enables audit trail
3. **REST API expansion** - Enables external integrations
4. **Markdown hooks** - Nice to have
5. **Webhooks** - Defer
