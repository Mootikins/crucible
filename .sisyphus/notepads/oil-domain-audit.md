# Oil Module Domain Audit

## Summary

| Metric | Count |
|--------|-------|
| Total files audited | 71 |
| Pure UI files | 66 |
| Files with domain coupling | 5 |
| Severity | **Low** |

The oil module is **remarkably well-isolated**. Only 5 files (7%) have any domain imports, and those imports are limited to a small set of types. No `crucible_rig` or `crucible_daemon` imports exist in the oil module.

## Domain Imports

### crucible_core

#### `chat_app.rs` (Main TUI State)
```rust
use crucible_core::interaction::{
    AskRequest, AskResponse, InteractionRequest, InteractionResponse, PermAction, PermRequest,
    PermResponse,
};
```
- **Usage**: Interaction modal handling (Ask, Permission dialogs)
- **Severity**: Medium - Core UI state depends on interaction types
- **Lines**: 23-26, 3895-4044 (test code)

#### `chat_runner.rs` (Event Loop)
```rust
use crucible_core::events::SessionEvent;
use crucible_core::interaction::InteractionRequest;
use crucible_core::traits::chat::{AgentHandle, ChatChunk, ChatResult, SubagentEventType};
```
- **Usage**: Agent communication, session events, streaming chunks
- **Severity**: High - Event loop is tightly coupled to agent traits
- **Lines**: 10-12

#### `components/notification_area.rs`
```rust
use crucible_core::types::{Notification, NotificationKind};
```
- **Usage**: Notification rendering (toast, progress, warning)
- **Severity**: Low - Simple data types for display
- **Lines**: 21

#### `tests/chat_runner_tests.rs`
```rust
use crucible_core::traits::chat::{AgentHandle, ChatChunk, ChatError, ChatResult, ChatToolResult};
use crucible_core::types::acp::schema::{AvailableCommand, SessionModeState};
use crucible_core::traits::llm::TokenUsage;
```
- **Usage**: Mock agent implementation for testing
- **Severity**: N/A (test code)
- **Lines**: 13-14, 396

#### `tests/chat_app_snapshot_tests.rs`
```rust
use crucible_core::interaction::{AskRequest, InteractionRequest, PermRequest};
```
- **Usage**: Test fixtures for interaction modals
- **Severity**: N/A (test code)
- **Lines**: 332

### crucible_rig
**No imports found** - The oil module does not depend on crucible_rig.

### crucible_daemon
**No imports found** - The oil module does not depend on crucible_daemon.

## File-by-File Analysis

### Pure UI (No Changes Needed) - 66 files

These files have zero domain dependencies and are already reusable:

**Core Rendering:**
- `node.rs` - Rendering primitives (Node, BoxNode, TextNode, etc.)
- `style.rs` - Styling system (Color, Style, Border, etc.)
- `render.rs` - Node-to-string rendering
- `overlay.rs` - Overlay positioning and extraction
- `ansi.rs` - ANSI escape code handling
- `diff.rs` - Differential rendering

**Layout:**
- `layout/mod.rs` - Layout module
- `layout/flex.rs` - Flexbox-like layout
- `layout/tree.rs` - Layout tree

**Viewport & Graduation:**
- `viewport.rs` - Line clamping utilities
- `viewport_cache.rs` - Message caching (uses local `Role` enum, not domain)
- `graduation.rs` - Viewport-to-scrollback graduation
- `planning.rs` - Frame planning

**Components:**
- `components/input_area.rs` - Input field component
- `components/status_bar.rs` - Status bar (uses local `ChatMode`)
- `components/message_list.rs` - Message rendering (uses local cache types)
- `components/popup_overlay.rs` - Popup rendering

**Infrastructure:**
- `app.rs` - App trait (generic, no domain types)
- `component.rs` - Component trait
- `event.rs` - Event types (keyboard, mouse, tick)
- `focus.rs` - Focus management
- `terminal.rs` - Terminal abstraction
- `runtime.rs` - Runtime loop
- `runner.rs` - Generic runner
- `theme.rs` - Color/style themes
- `markdown.rs` - Markdown-to-Node rendering
- `taffy_layout.rs` - Taffy layout integration
- `composer.rs` - Node composition
- `output.rs` - Output handling

**Configuration:**
- `config/mod.rs` - Config module
- `config/value.rs` - Config values
- `config/presets.rs` - Config presets
- `config/overlay.rs` - Config overlay
- `config/shortcuts.rs` - Keyboard shortcuts
- `config/stack.rs` - Config stack

**Commands:**
- `commands/mod.rs` - Command module
- `commands/set.rs` - Set command

**Other:**
- `mod.rs` - Module exports
- `example.rs` - Example app (uses local types)
- `agent_selection.rs` - Agent selection enum (local)
- `lua_view.rs` - Lua view
- `test_harness.rs` - Test harness

**Tests (26 files):**
All test files in `tests/` except `chat_runner_tests.rs` and `chat_app_snapshot_tests.rs` are pure UI tests.

### Adapter Layer (Needs Refactoring) - 3 files

#### `chat_app.rs` - Main TUI State
**Domain types used:**
- `InteractionRequest` - For modal dialogs
- `InteractionResponse` - For modal responses
- `AskRequest`, `AskResponse` - Ask dialog types
- `PermRequest`, `PermResponse`, `PermAction` - Permission dialog types

**Coupling points:**
1. `InteractionModalState.request: InteractionRequest` (line ~272)
2. `ChatAppMsg::OpenInteraction { request: InteractionRequest }` (line ~117)
3. `ChatAppMsg::CloseInteraction { response: InteractionResponse }` (line ~119)
4. Pattern matching on `InteractionRequest` variants for rendering

**Recommendation:**
- Create `trait InteractionDisplay` in oil with methods like `title()`, `choices()`, `allows_other()`
- Create adapter in `crucible-cli/src/tui/adapters/` that implements trait for `InteractionRequest`
- Replace direct `InteractionRequest` usage with trait object

#### `components/notification_area.rs` - Notification Display
**Domain types used:**
- `Notification` - Notification data
- `NotificationKind` - Toast/Progress/Warning enum

**Coupling points:**
1. `notifications: Vec<(Notification, Instant)>` (line 58)
2. Pattern matching on `NotificationKind` for icon/style selection

**Recommendation:**
- Create `trait NotificationDisplay` in oil with methods like `message()`, `kind()`, `progress()`
- Create adapter in `crucible-cli/src/tui/adapters/` that implements trait for `Notification`
- This is a simple extraction - the types are data-only

### Domain-Heavy (Major Extraction) - 1 file

#### `chat_runner.rs` - Event Loop
**Domain types used:**
- `SessionEvent` - Session lifecycle events
- `InteractionRequest` - Interaction handling
- `AgentHandle` trait - Agent communication
- `ChatChunk` - Streaming response chunks
- `ChatResult` - Result type for chat operations
- `SubagentEventType` - Subagent lifecycle events

**Coupling points:**
1. `run_with_factory<F, Fut, A>` where `A: AgentHandle` (line 116)
2. `event_loop<A: AgentHandle>` (line 173)
3. `process_message<A: AgentHandle>` (line 441)
4. `process_action<A: AgentHandle>` (line 474)
5. `handle_session_command` (line 585)
6. `handle_session_event` (line 651)
7. Direct streaming from `BoxStream<'static, ChatResult<ChatChunk>>`

**Recommendation:**
This file should be **split**:
1. Keep generic event loop in oil (`runner.rs` already exists)
2. Move agent-specific logic to `crucible-cli/src/tui/chat_runner.rs`
3. Oil provides `trait StreamHandler` for processing generic stream events
4. Crucible-cli implements `StreamHandler` for `ChatChunk`

## Local Domain Types (Already Decoupled)

The oil module defines its own local types that mirror domain concepts:

### `viewport_cache.rs`
- `Role` enum (User, Assistant, System) - local, not from crucible_core
- `CachedMessage` - local message representation
- `CachedToolCall` - local tool call representation
- `CachedChatItem` - enum of Message/ToolCall
- `StreamSegment` - streaming buffer segments

### `chat_app.rs`
- `ChatAppMsg` - UI message enum (already domain-agnostic except for `InteractionRequest`)
- `ChatItem` - local item representation
- `ChatMode` - Normal/Plan/Auto enum
- `InputMode` - Normal/Command/Shell enum

These local types are **good design** - they decouple the UI from domain types.

## Recommendations

### Phase 1: Define Pure Interfaces (2-4 hours)

Create traits in `crates/crucible-cli/src/tui/oil/traits/`:

```rust
// traits/interaction.rs
pub trait InteractionDisplay {
    fn title(&self) -> &str;
    fn description(&self) -> Option<&str>;
    fn choices(&self) -> &[String];
    fn allows_other(&self) -> bool;
    fn is_multi_select(&self) -> bool;
}

// traits/notification.rs
pub trait NotificationDisplay {
    fn id(&self) -> &str;
    fn message(&self) -> &str;
    fn kind(&self) -> NotificationKind; // Keep local enum
    fn progress(&self) -> Option<(usize, usize)>;
}

// traits/stream.rs
pub trait StreamEvent {
    fn text_delta(&self) -> Option<&str>;
    fn thinking_delta(&self) -> Option<&str>;
    fn tool_call(&self) -> Option<(&str, &str)>; // (name, args)
    fn tool_result(&self) -> Option<(&str, &str)>; // (name, result)
    fn is_complete(&self) -> bool;
}
```

### Phase 2: Create Adapter Layer (2-4 hours)

In `crates/crucible-cli/src/tui/adapters/`:

```rust
// adapters/interaction.rs
impl InteractionDisplay for crucible_core::interaction::InteractionRequest {
    fn title(&self) -> &str { ... }
    fn choices(&self) -> &[String] { ... }
    // ...
}

// adapters/notification.rs
impl NotificationDisplay for crucible_core::types::Notification {
    fn message(&self) -> &str { &self.message }
    fn kind(&self) -> oil::NotificationKind {
        match self.kind {
            crucible_core::types::NotificationKind::Toast => oil::NotificationKind::Toast,
            // ...
        }
    }
}

// adapters/stream.rs
impl StreamEvent for crucible_core::traits::chat::ChatChunk {
    fn text_delta(&self) -> Option<&str> {
        if self.delta.is_empty() { None } else { Some(&self.delta) }
    }
    // ...
}
```

### Phase 3: Refactor Coupled Files (4-8 hours)

1. **notification_area.rs**: Replace `Notification` with `impl NotificationDisplay`
2. **chat_app.rs**: Replace `InteractionRequest` with `impl InteractionDisplay`
3. **chat_runner.rs**: Split into:
   - `oil/runner.rs` - Generic event loop with trait bounds
   - `crucible-cli/src/tui/chat_runner.rs` - Crucible-specific implementation

### Phase 4: Move Tests (2-4 hours)

- Move `chat_runner_tests.rs` to `crucible-cli/src/tui/tests/`
- Keep oil tests pure (no domain imports)

## Estimated Effort

| Phase | Hours | Risk |
|-------|-------|------|
| Phase 1: Define Interfaces | 2-4 | Low |
| Phase 2: Create Adapters | 2-4 | Low |
| Phase 3: Refactor Files | 4-8 | Medium |
| Phase 4: Move Tests | 2-4 | Low |
| **Total** | **10-20** | **Low-Medium** |

## Current Assessment

**Severity: Low**

The oil module is already well-designed with minimal domain coupling:
- Only 5 files (7%) have domain imports
- No `crucible_rig` or `crucible_daemon` dependencies
- Local types (`CachedMessage`, `Role`, etc.) already provide abstraction
- Most coupling is in `chat_runner.rs` which is the natural boundary

**Recommendation**: Proceed with extraction. The low coupling makes this a straightforward refactoring task. Start with Phase 1 (define traits) to establish the interface, then incrementally refactor.

## Appendix: Import Locations

### crucible_core imports by file

| File | Line | Import |
|------|------|--------|
| `chat_app.rs` | 23-26 | `interaction::{AskRequest, AskResponse, InteractionRequest, InteractionResponse, PermAction, PermRequest, PermResponse}` |
| `chat_runner.rs` | 10 | `events::SessionEvent` |
| `chat_runner.rs` | 11 | `interaction::InteractionRequest` |
| `chat_runner.rs` | 12 | `traits::chat::{AgentHandle, ChatChunk, ChatResult, SubagentEventType}` |
| `components/notification_area.rs` | 21 | `types::{Notification, NotificationKind}` |
| `tests/chat_runner_tests.rs` | 13 | `traits::chat::{AgentHandle, ChatChunk, ChatError, ChatResult, ChatToolResult}` |
| `tests/chat_runner_tests.rs` | 14 | `types::acp::schema::{AvailableCommand, SessionModeState}` |
| `tests/chat_runner_tests.rs` | 396 | `traits::llm::TokenUsage` |
| `tests/chat_app_snapshot_tests.rs` | 332 | `interaction::{AskRequest, InteractionRequest, PermRequest}` |

### External crate imports (non-crucible)

The oil module also imports from:
- `crossterm` - Terminal handling
- `markdown_it` - Markdown parsing
- `tokio` - Async runtime
- `futures` - Stream handling
- `anyhow` - Error handling
- `tracing` - Logging
- `insta` - Snapshot testing

These are appropriate dependencies for a TUI module.
