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
- [x] `:messages` toggles notification popup on/off
- [x] Toast notifications auto-dismiss after 3s
- [x] Progress/warning notifications persist until dismissed
- [x] `cargo nextest run -p crucible-cli notification` passes (20/20)
- [x] `cargo nextest run -p crucible-daemon notification` passes (8/8)
- [x] Existing `InkChatApp.notification` and `StatusBar.notification` removed

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

- [x] 0. Define Notification Types in crucible-core ✅
- [x] 1. Write RPC Interface Tests (contracts) ✅
- [x] 2. Implement RPC Methods for Notifications ✅
- [x] 3. Create NotificationArea Component ✅
- [x] 4. Integrate NotificationArea into InkChatApp ✅
- [x] 5. Remove Legacy Notification State ✅

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
cargo nextest run notification  # ✅ 35 tests pass

# Full CLI test suite (no regressions)  
cargo nextest run -p crucible-cli  # ✅ 1556 tests pass

# Full daemon test suite
cargo nextest run -p crucible-daemon  # ✅ 377 tests pass
```

### Final Checklist
- [x] `:messages` toggles notification popup
- [x] Toasts auto-dismiss after 3s
- [x] Progress/warnings persist
- [x] Block characters render correctly: `▗▄▌▘`
- [x] Legacy `notification` fields removed
- [x] All tests pass
- [x] No critical clippy warnings

## PLAN COMPLETE ✅

All tasks completed. Ready for code review.
