# Notification Area for TUI

## Context

### Original Request
Add a unified notification area to Crucible's TUI that displays as a popup overlay, accessible via `:messages` command (vim-style).

### Interview Summary
**Key Discussions**:
- **Visual Design**: Popup anchored bottom-right, grows upward. Uses block characters: `▗` (top-left corner), `▄` (top edge), `▌` (left border), `▘` (notch where input meets popup). Same color as statusline.
- **Behavior**: Unified system (not separate toasts vs messages). `:messages` toggles visibility. Auto-dismiss for toasts (~3s), persistent for progress/warnings.
- **Scrolling**: Reserved space approach when visible, avoiding overlay conflicts with graduated content.
- **Test Strategy**: Soft TDD - interface tests first (RPC), then data→RPC tests, then RPC→TUI tests.

### Research Findings
**Existing Popup Pattern** (`crates/crucible-cli/src/tui/oil/`):
- `PopupOverlay` component in `components/popup_overlay.rs` provides reusable template
- State: `visible`, `selected`, `items`, `offset_from_bottom`
- Rendering: `overlay_from_bottom()` primitive positions from bottom
- Key pattern: `popup()` + `focusable()` + `overlay_from_bottom()`

**RPC Pattern** (`crates/crucible-daemon/`):
- dispatch.rs: Register method name
- server.rs: Add handler function
- agent_manager.rs: Business logic + event emission
- daemon-client/client.rs: Client method
- daemon-client/agent.rs: AgentHandle trait impl

**Existing Notification State** (Metis finding):
- `InkChatApp.notification: Option<(String, Instant)>` - 2s timeout
- `StatusBar.notification: Option<(String, Instant)>` - 5s timeout
- DUPLICATE STATE - must unify in new system

### Metis Review
**Identified Gaps** (addressed):
- Duplicate notification state → Replace both with NotificationArea
- Popup offset calculation → Independent overlay, doesn't affect existing popup
- Progress sources undefined → Manual emission for v1, sources added incrementally
- Test baseline → Soft TDD creates tests as we go

---

## Work Objectives

### Core Objective
Add a unified notification area component that replaces existing duplicate notification state, provides `:messages` toggle, and supports auto-dismissing toasts alongside persistent progress/warning notifications.

### Concrete Deliverables
- `NotificationArea` component in `components/notification_area.rs`
- `Notification` and `NotificationKind` types in `crucible-core`
- RPC methods: `session.add_notification`, `session.list_notifications`, `session.dismiss_notification`
- `:messages` command to toggle visibility
- Integration with `InkChatApp` replacing existing notification fields
- Interface and rendering tests

### Definition of Done
- [ ] `:messages` toggles notification popup on/off
- [ ] Toast notifications auto-dismiss after 3s
- [ ] Progress/warning notifications persist until dismissed
- [ ] `cargo nextest run -p crucible-cli notification` passes
- [ ] `cargo nextest run -p crucible-daemon notification` passes
- [ ] Existing `InkChatApp.notification` and `StatusBar.notification` removed

### Must Have
- Unified notification queue (single source of truth)
- Block character rendering: `▗▄▌▘`
- Auto-dismiss for toasts, persistent for progress/warnings
- `:messages` toggle command
- RPC interface for daemon↔CLI sync

### Must NOT Have (Guardrails)
- NO animation system (slide in/out) - static appearance for v1
- NO mouse interaction - keyboard only
- NO notification sounds
- NO notification grouping ("3 files saved")
- NO notification history beyond current session
- NO max_visible scrolling for v1 - just show recent N (5)
- NO separate toast vs message systems - unified only

---

## Verification Strategy (MANDATORY)

### Test Decision
- **Infrastructure exists**: YES (cargo-nextest, insta snapshots, harness)
- **User wants tests**: Soft TDD
- **Framework**: cargo-nextest + insta for snapshots

### Soft TDD Approach

**Phase 1: Interface Tests First**
- Define RPC interface contracts
- Write tests for RPC request/response shapes
- Mock daemon responses for client tests

**Phase 2: Data → RPC Tests**
- Notification struct serialization
- Queue management (add, dismiss, auto-expire)
- RPC handler correctness

**Phase 3: RPC → TUI Tests**
- NotificationArea rendering with mock data
- Snapshot tests for visual appearance
- `:messages` toggle behavior

---

## Task Flow

```
0 (types) → 1 (RPC interface tests) → 2 (RPC impl) → 3 (component) → 4 (integration) → 5 (cleanup)
                                           ↓
                                      3 (component) can start after types
```

## Parallelization

| Group | Tasks | Reason |
|-------|-------|--------|
| A | 2, 3 | RPC impl and component can proceed in parallel after interface tests |

| Task | Depends On | Reason |
|------|------------|--------|
| 1 | 0 | Interface tests need types |
| 2 | 1 | RPC impl validates against interface tests |
| 3 | 0 | Component needs types |
| 4 | 2, 3 | Integration needs both RPC and component |
| 5 | 4 | Cleanup after integration verified |

---

## TODOs

- [x] 0. Define Notification Types in crucible-core

  **What to do**:
  - Add `Notification` struct with id, kind, message, created_at
  - Add `NotificationKind` enum: `Toast`, `Progress { current, total }`, `Warning`
  - Add `NotificationQueue` struct with VecDeque, add/dismiss/expire methods
  - Derive Serialize/Deserialize for RPC transport

  **Must NOT do**:
  - No daemon-specific logic in core types
  - No TUI rendering in core types

  **Parallelizable**: NO (foundation for all other tasks)

  **References**:
  
  **Pattern References**:
  - `crates/crucible-core/src/session/types.rs` - Session struct patterns for serde derives
  - `crates/crucible-daemon/src/protocol.rs:95-204` - SessionEventMessage pattern for event types
  
  **Type References**:
  - `crates/crucible-core/src/types/` - Core type organization pattern

  **Acceptance Criteria**:

  **Test (interface contract)**:
  - [ ] `cargo nextest run -p crucible-core notification_types`
  - [ ] Test: Notification serializes to expected JSON shape
  - [ ] Test: NotificationKind variants all serialize correctly
  - [ ] Test: NotificationQueue add/dismiss/expire_old work correctly

  **Commit**: YES
  - Message: `feat(core): add notification types for TUI notification area`
  - Files: `crates/crucible-core/src/types/notification.rs`, `crates/crucible-core/src/types/mod.rs`

---

- [x] 1. Write RPC Interface Tests (contracts)

  **What to do**:
  - Create test file `crates/crucible-daemon/src/rpc/notification_tests.rs`
  - Define expected request/response shapes for:
    - `session.add_notification` - add notification to queue
    - `session.list_notifications` - get current notifications
    - `session.dismiss_notification` - remove by id
  - Tests should define the CONTRACT, impl comes in task 2

  **Must NOT do**:
  - No actual RPC implementation yet
  - No daemon state changes

  **Parallelizable**: NO (depends on task 0)

  **References**:
  
  **Pattern References**:
  - `crates/crucible-daemon/src/rpc/dispatch.rs:13-46` - Method registration pattern
  - `crates/crucible-daemon/src/server.rs:1079-1109` - Handler pattern (thinking_budget example)
  
  **Test References**:
  - `crates/crucible-daemon/tests/` - Existing RPC test patterns

  **Acceptance Criteria**:

  **Test (RED phase - tests exist but fail)**:
  - [ ] Test file created with contract tests
  - [ ] Tests compile but fail (methods not implemented)
  - [ ] `cargo nextest run -p crucible-daemon notification_rpc` shows failures

  **Commit**: YES
  - Message: `test(daemon): add RPC interface contract tests for notifications`
  - Files: `crates/crucible-daemon/src/rpc/notification_tests.rs`

---

- [x] 2. Implement RPC Methods for Notifications

  **What to do**:
  - Register methods in `dispatch.rs`: `session.add_notification`, `session.list_notifications`, `session.dismiss_notification`
  - Add handlers in `server.rs` following thinking_budget pattern
  - Add notification queue to session state in `agent_manager.rs`
  - Emit `SessionEventMessage` for notification changes
  - Add client methods in `daemon-client/src/client.rs`
  - Wire to `DaemonAgentHandle` in `daemon-client/src/agent.rs`

  **Must NOT do**:
  - No TUI integration yet
  - No auto-dismiss timer in daemon (TUI handles display timing)

  **Parallelizable**: YES (with task 3, after task 1)

  **References**:
  
  **Pattern References**:
  - `crates/crucible-daemon/src/rpc/dispatch.rs:13-46` - Add to METHODS array
  - `crates/crucible-daemon/src/server.rs:390-414` - Handler dispatch pattern
  - `crates/crucible-daemon/src/agent_manager.rs:445-487` - State update + event emit pattern
  - `crates/crucible-daemon-client/src/client.rs:959-990` - Client method pattern
  - `crates/crucible-daemon-client/src/agent.rs:309-315` - AgentHandle delegation pattern

  **Acceptance Criteria**:

  **Test (GREEN phase - interface tests pass)**:
  - [ ] `cargo nextest run -p crucible-daemon notification_rpc` PASSES
  - [ ] RPC roundtrip: add_notification → list_notifications shows it
  - [ ] RPC roundtrip: dismiss_notification → list_notifications removes it

  **Commit**: YES
  - Message: `feat(daemon): implement notification RPC methods`
  - Files: `dispatch.rs`, `server.rs`, `agent_manager.rs`, `client.rs`, `agent.rs`

---

- [x] 3. Create NotificationArea Component

  **What to do**:
  - Create `crates/crucible-cli/src/tui/oil/components/notification_area.rs`
  - Implement `NotificationArea` struct with state: notifications, visible, max_visible
  - Implement `Component` trait with `view()` method
  - Render using block characters: `▗` top-left, `▄` top edge, `▌` left border, `▘` notch
  - Use `overlay_from_bottom()` for positioning (offset = 1, above statusline)
  - Handle auto-dismiss timing for Toast kind (check created_at + 3s)

  **Must NOT do**:
  - No RPC calls in component (receives data via props)
  - No animation
  - No scrolling (just show max 5)

  **Parallelizable**: YES (with task 2, after task 0)

  **References**:
  
  **Pattern References**:
  - `crates/crucible-cli/src/tui/oil/components/popup_overlay.rs:8-108` - Component structure, builder pattern
  - `crates/crucible-cli/src/tui/oil/node.rs:272-277` - `overlay_from_bottom()` usage
  - `crates/crucible-cli/src/tui/oil/chat_app.rs:2740-2754` - Popup rendering pattern
  
  **Visual References**:
  - Block characters: `▗` U+2597, `▄` U+2584, `▌` U+258C, `▘` U+2598

  **Acceptance Criteria**:

  **Test (snapshot)**:
  - [ ] `cargo nextest run -p crucible-cli notification_area`
  - [ ] Snapshot test: empty notifications → Node::Empty
  - [ ] Snapshot test: single toast → renders with block border
  - [ ] Snapshot test: multiple notifications → stacks correctly
  - [ ] Snapshot test: progress notification → shows bar

  **Commit**: YES
  - Message: `feat(cli): add NotificationArea component with block character rendering`
  - Files: `crates/crucible-cli/src/tui/oil/components/notification_area.rs`, `components/mod.rs`

---

- [x] 4. Integrate NotificationArea into InkChatApp

  **What to do**:
  - Add `notification_area: NotificationArea` field to `InkChatApp`
  - Add `ChatAppMsg::ToggleMessages` for `:messages` command
  - Add `ChatAppMsg::AddNotification(Notification)` for incoming notifications
  - Wire `:messages` command in REPL command handling
  - Add `render_notification_area()` call in view between input and status
  - Subscribe to notification events from daemon
  - Handle auto-dismiss tick (check expired notifications on each render)

  **Must NOT do**:
  - Don't remove old notification fields yet (task 5)
  - Don't add progress emission sources yet

  **Parallelizable**: NO (depends on tasks 2 and 3)

  **References**:
  
  **Pattern References**:
  - `crates/crucible-cli/src/tui/oil/chat_app.rs:61-81` - ChatAppMsg enum
  - `crates/crucible-cli/src/tui/oil/chat_app.rs:330-355` - State fields
  - `crates/crucible-cli/src/tui/oil/chat_runner.rs:472-505` - Message handling
  - `crates/crucible-cli/src/tui/oil/chat_app.rs:2740-2754` - Overlay in view()

  **Acceptance Criteria**:

  **Manual verification (TUI)**:
  - [ ] Run `cargo run -- chat`
  - [ ] Type `:messages` → notification area appears (empty or with test data)
  - [ ] Type `:messages` again → notification area hides
  - [ ] Trigger a notification (e.g., session save) → appears in area
  - [ ] Wait 3s for toast → auto-dismisses

  **Test (integration)**:
  - [ ] `cargo nextest run -p crucible-cli messages_command`
  - [ ] Test: `:messages` toggles `notification_area.visible`

  **Commit**: YES
  - Message: `feat(cli): integrate NotificationArea with InkChatApp and :messages command`
  - Files: `chat_app.rs`, `chat_runner.rs`

---

- [x] 5. Remove Legacy Notification State

  **What to do**:
  - Remove `notification: Option<(String, Instant)>` from `InkChatApp`
  - Remove `notification: Option<(String, Instant)>` from `StatusBar`
  - Update all call sites to use `NotificationArea.add()` instead
  - Update status bar to show notification badge `[N]` when unread count > 0
  - Verify no regressions in existing notification behavior

  **Must NOT do**:
  - Don't change notification content/text
  - Don't change what triggers notifications

  **Parallelizable**: NO (depends on task 4)

  **References**:
  
  **Pattern References**:
  - `crates/crucible-cli/src/tui/oil/chat_app.rs:341` - InkChatApp.notification field
  - `crates/crucible-cli/src/tui/oil/components/status_bar.rs:18` - StatusBar.notification field
  
  **Search for call sites**:
  - `rg "\.notification\s*=" crates/crucible-cli/`
  - `rg "notification:" crates/crucible-cli/`

  **Acceptance Criteria**:

  **Test (no regressions)**:
  - [ ] `cargo nextest run -p crucible-cli` - all existing tests pass
  - [ ] `cargo build -p crucible-cli` - no warnings about unused fields

  **Manual verification**:
  - [ ] Session save → notification appears in area
  - [ ] Model switch → notification appears in area
  - [ ] Badge shows count when notifications present

  **Commit**: YES
  - Message: `refactor(cli): remove legacy notification state, use unified NotificationArea`
  - Files: `chat_app.rs`, `status_bar.rs`, call sites

---

## Commit Strategy

| After Task | Message | Files | Verification |
|------------|---------|-------|--------------|
| 0 | `feat(core): add notification types` | core types | `cargo nextest run -p crucible-core` |
| 1 | `test(daemon): add notification RPC contract tests` | daemon tests | Tests compile, fail |
| 2 | `feat(daemon): implement notification RPC methods` | daemon + client | Contract tests pass |
| 3 | `feat(cli): add NotificationArea component` | cli component | Snapshot tests pass |
| 4 | `feat(cli): integrate NotificationArea with :messages` | cli integration | Manual + integration tests |
| 5 | `refactor(cli): remove legacy notification state` | cli cleanup | All tests pass |

---

## Success Criteria

### Verification Commands
```bash
# All notification tests pass
cargo nextest run notification

# Full CLI test suite (no regressions)  
cargo nextest run -p crucible-cli

# Full daemon test suite
cargo nextest run -p crucible-daemon

# Manual smoke test
cargo run -- chat
# Then: :messages, observe toggle, trigger notifications
```

### Final Checklist
- [ ] `:messages` toggles notification popup
- [ ] Toasts auto-dismiss after 3s
- [ ] Progress/warnings persist
- [ ] Block characters render correctly: `▗▄▌▘`
- [ ] Legacy `notification` fields removed
- [ ] All tests pass
- [ ] No clippy warnings
