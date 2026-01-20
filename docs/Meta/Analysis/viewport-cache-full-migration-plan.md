# ViewportCache Full Migration Plan

> **Status**: Ready for implementation  
> **Created**: 2026-01-20  
> **Scope**: Replace `items: VecDeque<ChatItem>` with `ViewportCache` in `InkChatApp`

## Executive Summary

Migrate `InkChatApp` from the current `items: VecDeque<ChatItem>` + `streaming: StreamingState` pattern to use `ViewportCache` as the single source of truth for chat content. This is a delete-not-deprecate migration.

**Goals:**
- Single source of truth for chat items (ViewportCache)
- Memory-efficient `Arc<str>` storage for all content
- Unified streaming through ViewportCache
- Delete `ChatItem`, `StreamingState`, and `items` field entirely

**Non-Goals:**
- Changing the Node-based rendering system (future work)
- Implementing full Span-based rendering pipeline (already done in crucible-ink)

---

## Current State Analysis

### InkChatApp Fields (relevant to migration)

```rust
pub struct InkChatApp {
    items: VecDeque<ChatItem>,        // DELETE: 512-item ring buffer
    streaming: StreamingState,         // DELETE: separate streaming state
    message_counter: usize,            // KEEP: ID generation
    // ... other fields unchanged
}
```

### ChatItem Enum (to be replaced)

```rust
pub enum ChatItem {
    Message { id: String, role: Role, content: String },
    ToolCall { id: String, name: String, args: String, result: String, complete: bool },
    ShellExecution { id: String, command: String, exit_code: i32, output_tail: Vec<String>, output_path: Option<PathBuf> },
}
```

### StreamingState (to be deleted)

```rust
struct StreamingState {
    content: String,
    active: bool,
}
```

### Usage Counts

| Pattern | Count | Location |
|---------|-------|----------|
| `self.items` | 35 | message handling, rendering, tests |
| `ChatItem::` | 24 | enum construction, pattern matching |
| `self.streaming.` | 12 | streaming state management |
| Tests using `app.items` | 15 | direct field access in tests |

---

## Target State

### ViewportCache (extended)

```rust
// In viewport_cache.rs
pub struct ViewportCache {
    items: VecDeque<CachedChatItem>,   // Was: messages: VecDeque<CachedMessage>
    streaming: Option<StreamingBuffer>,
    tool_streaming: Option<ToolStreamingBuffer>,  // NEW: for tool result streaming
    anchor: Option<ViewportAnchor>,
}

pub enum CachedChatItem {
    Message(CachedMessage),
    ToolCall(CachedToolCall),
    ShellExecution(CachedShellExecution),
}

pub struct CachedToolCall {
    pub id: String,
    pub name: Arc<str>,
    pub args: Arc<str>,
    pub result: String,  // Mutable during streaming
    pub complete: bool,
}

pub struct CachedShellExecution {
    pub id: String,
    pub command: Arc<str>,
    pub exit_code: i32,
    pub output_tail: Vec<Arc<str>>,
    pub output_path: Option<PathBuf>,
}
```

### InkChatApp (after migration)

```rust
pub struct InkChatApp {
    cache: ViewportCache,              // NEW: single source of truth
    message_counter: usize,
    // ... other fields unchanged
    // DELETED: items, streaming
}
```

---

## Migration Phases

### Phase A: Extend ViewportCache Types (foundation)

**Files:** `viewport_cache.rs`

1. Rename `CachedMessage` internal fields for clarity
2. Add `CachedToolCall` struct
3. Add `CachedShellExecution` struct  
4. Create `CachedChatItem` enum wrapping all three
5. Add `ToolStreamingBuffer` for tool result accumulation
6. Update `ViewportCache` to use `VecDeque<CachedChatItem>`
7. Add methods: `push_tool_call()`, `push_shell_execution()`, `update_tool_result()`, `complete_tool()`
8. Update `ContentSource` impl for new structure
9. Add tests for new types

**API additions:**
```rust
impl ViewportCache {
    pub fn push_item(&mut self, item: CachedChatItem);
    pub fn push_tool_call(&mut self, id: String, name: &str, args: &str);
    pub fn append_tool_result(&mut self, name: &str, delta: &str);
    pub fn complete_tool(&mut self, name: &str);
    pub fn push_shell_execution(&mut self, ...);
    pub fn items(&self) -> impl Iterator<Item = &CachedChatItem>;
    pub fn find_tool_mut(&mut self, name: &str) -> Option<&mut CachedToolCall>;
}
```

### Phase B: Add ViewportCache to InkChatApp (dual-write)

**Files:** `chat_app.rs`

1. Add `cache: ViewportCache` field to `InkChatApp`
2. Initialize in `Default` impl
3. Update `push_item()` to write to BOTH `items` AND `cache`
4. Update streaming handlers to use BOTH
5. All tests should still pass (dual-write ensures compatibility)

**This phase is a checkpoint - tests must pass before proceeding.**

### Phase C: Migrate Message Handlers (switch reads)

**Files:** `chat_app.rs`

1. Update `on_message(ChatAppMsg::TextDelta)` to use `cache.append_streaming()`
2. Update `on_message(ChatAppMsg::ToolCall)` to use `cache.push_tool_call()`
3. Update `on_message(ChatAppMsg::ToolResultDelta)` to use `cache.append_tool_result()`
4. Update `on_message(ChatAppMsg::ToolResultComplete)` to use `cache.complete_tool()`
5. Update `finalize_streaming()` to use `cache.complete_streaming()`
6. Update `add_user_message()` and `add_system_message()` to use cache
7. Update `load_previous_messages()` to populate cache

**After this phase, `items` is write-only (still dual-writing but not reading).**

### Phase D: Migrate Rendering (switch iteration)

**Files:** `chat_app.rs`

1. Update `render_items()` to iterate `self.cache.items()`
2. Update `render_item()` signature to take `&CachedChatItem`
3. Adapt pattern matching for new enum structure
4. Update `render_streaming()` to use `cache.streaming_content()`
5. Update `is_streaming()` to use `cache.is_streaming()`
6. Update `format_session_for_export()` to use cache

**After this phase, `items` is unused (only dual-written).**

### Phase E: Delete Old Code (cleanup)

**Files:** `chat_app.rs`, `viewport_cache.rs`

1. Remove `items: VecDeque<ChatItem>` field
2. Remove `streaming: StreamingState` field
3. Delete `ChatItem` enum entirely
4. Delete `StreamingState` struct
5. Remove dual-write code from all handlers
6. Update all tests to use cache API
7. Remove unused imports

**This is the point of no return.**

### Phase F: Test Updates and Verification

**Files:** `chat_app.rs` (test module)

1. Update `test_app_init()` - check `cache.item_count()` instead of `items.is_empty()`
2. Update `test_user_message()` - use cache getters
3. Update `test_streaming_flow()` - use cache streaming API
4. Update `test_tool_call_flow()` - use cache tool API
5. Update `test_clear_repl_command()` - use `cache.clear()`
6. Update `test_items_ring_buffer_evicts_oldest()` - use cache
7. Add new tests for CachedToolCall and CachedShellExecution
8. Run full test suite: `cargo test -p crucible-cli`

---

## Detailed Task Breakdown

### Phase A Tasks (8 tasks)

| ID | Task | Est. Lines |
|----|------|------------|
| A1 | Add `CachedToolCall` struct with Arc<str> fields | +25 |
| A2 | Add `CachedShellExecution` struct | +20 |
| A3 | Create `CachedChatItem` enum | +15 |
| A4 | Add `ToolStreamingBuffer` for result accumulation | +30 |
| A5 | Refactor `ViewportCache` to use `VecDeque<CachedChatItem>` | +40 |
| A6 | Add tool-specific methods to ViewportCache | +60 |
| A7 | Update `ContentSource` impl | +10 |
| A8 | Add tests for new types | +80 |

### Phase B Tasks (4 tasks)

| ID | Task | Est. Lines |
|----|------|------------|
| B1 | Add `cache: ViewportCache` field to InkChatApp | +5 |
| B2 | Initialize cache in Default impl | +3 |
| B3 | Update `push_item()` for dual-write | +20 |
| B4 | Verify all existing tests pass | +0 |

### Phase C Tasks (7 tasks)

| ID | Task | Est. Lines |
|----|------|------------|
| C1 | Migrate TextDelta handler | +5, -5 |
| C2 | Migrate ToolCall handler | +5, -10 |
| C3 | Migrate ToolResultDelta handler | +5, -15 |
| C4 | Migrate ToolResultComplete handler | +3, -8 |
| C5 | Migrate finalize_streaming() | +5, -10 |
| C6 | Migrate add_user_message/add_system_message | +10, -10 |
| C7 | Migrate load_previous_messages() | +15, -10 |

### Phase D Tasks (6 tasks)

| ID | Task | Est. Lines |
|----|------|------------|
| D1 | Update render_items() to use cache.items() | +5, -3 |
| D2 | Update render_item() for CachedChatItem | +30, -25 |
| D3 | Update render_streaming() | +5, -5 |
| D4 | Update is_streaming() | +1, -1 |
| D5 | Update format_session_for_export() | +20, -15 |
| D6 | Verify rendering tests pass | +0 |

### Phase E Tasks (7 tasks)

| ID | Task | Est. Lines |
|----|------|------------|
| E1 | Remove `items` field | -1 |
| E2 | Remove `streaming` field | -1 |
| E3 | Delete `ChatItem` enum | -35 |
| E4 | Delete `StreamingState` struct | -10 |
| E5 | Remove dual-write code | -30 |
| E6 | Clean up unused imports | -5 |
| E7 | Final code review pass | +0 |

### Phase F Tasks (8 tasks)

| ID | Task | Est. Lines |
|----|------|------------|
| F1 | Update test_app_init | +3, -3 |
| F2 | Update test_user_message | +5, -5 |
| F3 | Update test_streaming_flow | +8, -8 |
| F4 | Update test_tool_call_flow | +15, -15 |
| F5 | Update test_clear_repl_command | +3, -3 |
| F6 | Update test_items_ring_buffer | +10, -10 |
| F7 | Add CachedToolCall tests | +40 |
| F8 | Add CachedShellExecution tests | +30 |

---

## Risk Mitigation

### Checkpoint Strategy

Each phase ends with all tests passing. If tests fail:
1. Fix within the phase if possible
2. Revert phase and reassess if not

### Rollback Points

- **After Phase B**: Can revert to items-only by removing cache field
- **After Phase C**: Can revert reads but keep dual-write
- **After Phase E**: No rollback (committed to new architecture)

### Test Coverage

Before starting:
```bash
cargo test -p crucible-cli --lib 'chat_app' -- --list 2>&1 | grep -c 'test'
# Should show ~25 tests
```

After each phase, this count should remain stable or increase.

---

## Estimated Impact

| Metric | Before | After | Change |
|--------|--------|-------|--------|
| `chat_app.rs` LOC | 2629 | ~2550 | -79 |
| `viewport_cache.rs` LOC | 450 | ~650 | +200 |
| Memory per message | `String` (heap) | `Arc<str>` (shared) | Reduced |
| Streaming buffers | 2 (content + tools) | 2 (unified in cache) | Cleaner |
| Test count | 25 | ~30 | +5 |

---

## Success Criteria

1. All existing tests pass
2. `ChatItem` enum deleted
3. `StreamingState` struct deleted  
4. `items` field deleted
5. Single `ViewportCache` manages all chat content
6. Memory usage same or better
7. No functional regressions in TUI behavior
