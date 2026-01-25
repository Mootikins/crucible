# ACP Permission → TUI Integration

## Context

### Original Request
Connect the ACP agent permission system to the TUI interaction modal so that when agents request permission for sensitive operations, users see and respond to the permission modal instead of auto-approval.

### Problem Statement
Currently, `CrucibleClient::request_permission()` auto-approves in "act" mode without showing UI. The permission modal exists and renders correctly, but nothing triggers it during normal agent operation.

### Current Architecture

```
Agent calls tool
       ↓
CrucibleClient::request_permission()
       ↓
Auto-approve (act mode) / Auto-reject (read-only)  ← NO UI SHOWN
       ↓
Tool executes or fails
```

### Target Architecture

```
Agent calls tool
       ↓
CrucibleClient::request_permission()
       ↓
Emit InteractionRequested event
       ↓
Daemon routes to TUI via event channel
       ↓
TUI shows PermissionModal ← USER SEES THIS
       ↓
User presses y/n/Esc
       ↓
TUI emits InteractionCompleted
       ↓
Daemon routes back to ACP client
       ↓
request_permission() returns with user's decision
       ↓
Tool executes or fails based on user choice
```

---

## Work Objectives

### Core Objective
Enable real user approval for agent permission requests by connecting the ACP permission flow to the existing TUI modal infrastructure.

### Concrete Deliverables
- Modified `CrucibleClient` that emits/waits for interactions
- Daemon-side routing for ACP↔TUI interaction events
- Working e2e flow: agent requests permission → user sees modal → response controls execution

### Definition of Done
- [ ] Permission modal appears when agent calls sensitive tool
- [ ] User can approve (y), deny (n), or cancel (Esc)
- [ ] Agent receives correct response and acts accordingly
- [ ] Works in both daemon and embedded storage modes
- [ ] Integration test validates full flow

### Must Have
- Blocking wait in `request_permission()` until user responds
- Timeout handling (don't block forever)
- Cancel/interrupt handling (Ctrl+C during wait)

### Must NOT Have (Guardrails)
- Don't change the existing modal rendering (already working)
- Don't break read-only mode (should still auto-reject)
- Don't require UI for headless/batch operations (need escape hatch)

---

## Research Findings

### Existing Infrastructure

| Component | Location | Status |
|-----------|----------|--------|
| `InteractionRegistry` | `crucible-core/src/interaction_registry.rs` | ✅ Exists - correlates request/response |
| `InteractionRequest::Permission` | `crucible-core/src/interaction.rs` | ✅ Exists |
| `SessionEvent::InteractionRequested` | `crucible-core/src/events/session_event.rs` | ✅ Exists |
| `SessionEvent::InteractionCompleted` | `crucible-core/src/events/session_event.rs` | ✅ Exists |
| TUI modal rendering | `crucible-cli/src/tui/oil/chat_app.rs` | ✅ Exists |
| `session.test_interaction` RPC | `crucible-daemon/src/server.rs:1079` | ✅ Works for testing |
| Lua `ask_user()` | `crucible-lua/src/ask.rs:715` | ✅ Working pattern to follow |

### Key Insight: Follow Lua Pattern

The Lua `ask_user()` implementation shows the pattern:
```rust
// 1. Register with registry to get a receiver
let rx = registry.register(id);

// 2. Push InteractionRequested event
(self.push_event)(SessionEvent::InteractionRequested { ... });

// 3. Wait for response (blocking)
let response = rx.blocking_recv()?;
```

The ACP client needs similar infrastructure.

---

## Verification Strategy

### Test Decision
- **Infrastructure exists**: YES (interaction registry, events)
- **User wants tests**: Integration tests + manual QA
- **Framework**: `cargo nextest` + PTY tests

### TDD Approach

**RED**: Write integration test that:
1. Starts daemon + TUI
2. Triggers agent tool call requiring permission
3. Expects modal to appear
4. Sends 'y' key
5. Verifies tool executed

**GREEN**: Implement until test passes

**REFACTOR**: Clean up, add timeout handling, edge cases

---

## Task Flow

```
Task 1 (Core types) → Task 2 (ACP client) → Task 3 (Daemon routing) → Task 4 (Integration test)
                                                     ↓
                                              Task 5 (Timeout/cancel handling)
```

---

## TODOs

- [ ] 1. Add interaction channel infrastructure to ACP client context

  **What to do**:
  - Add `InteractionRegistry` (or channel pair) to `CrucibleClient` struct
  - Add `event_sender` callback/channel for emitting events
  - Ensure client can both emit events AND receive responses

  **Files to modify**:
  - `crates/crucible-acp/src/acp_client.rs` - Add fields to struct
  - `crates/crucible-acp/src/lib.rs` - Export new types if needed

  **References**:
  - `crucible-lua/src/ask.rs:680-750` - Pattern for registry + event sender
  - `crucible-core/src/interaction_registry.rs` - Registry implementation

  **Acceptance Criteria**:
  - [ ] `CrucibleClient` has `interaction_registry: Arc<Mutex<InteractionRegistry>>`
  - [ ] `CrucibleClient` has `push_event: Box<dyn Fn(SessionEvent) + Send + Sync>`
  - [ ] Constructor accepts these dependencies

  **Commit**: YES
  - Message: `feat(acp): add interaction channel infrastructure to client`

- [ ] 2. Implement blocking permission request with UI

  **What to do**:
  - Modify `request_permission()` to:
    1. Create `PermRequest` from ACP `RequestPermissionRequest`
    2. Register with interaction registry
    3. Emit `InteractionRequested` event
    4. Block waiting for response
    5. Convert response back to ACP format
  - Keep auto-reject for read-only mode (no change)
  - Add configurable "headless" mode that auto-approves without UI

  **Files to modify**:
  - `crates/crucible-acp/src/acp_client.rs:133-169` - `request_permission()` method

  **References**:
  - `crucible-lua/src/ask.rs:715-750` - Blocking wait pattern
  - `crucible-core/src/interaction.rs:688-841` - PermRequest types

  **Acceptance Criteria**:
  - [ ] `request_permission()` emits `SessionEvent::InteractionRequested`
  - [ ] Method blocks until `InteractionCompleted` received
  - [ ] Returns correct `RequestPermissionResponse` based on user choice
  - [ ] Read-only mode still auto-rejects (no UI)

  **Commit**: YES
  - Message: `feat(acp): implement blocking permission request with UI integration`

- [ ] 3. Wire up daemon to route interactions to/from ACP agent

  **What to do**:
  - When creating ACP agent in `AgentManager`, pass interaction infrastructure
  - Route `InteractionCompleted` events back to the correct agent
  - Handle multiple concurrent permission requests (different request_ids)

  **Files to modify**:
  - `crates/crucible-daemon/src/agent_manager.rs` - Pass channels to ACP client
  - `crates/crucible-daemon/src/server.rs` - Handle `InteractionCompleted` routing

  **References**:
  - `crucible-daemon/src/server.rs:1079-1143` - test_interaction shows event emission
  - `crucible-daemon-client/src/agent.rs` - Event routing pattern

  **Acceptance Criteria**:
  - [ ] `AgentManager::create_acp_agent()` passes interaction infrastructure
  - [ ] `InteractionCompleted` events route to correct waiting agent
  - [ ] Multiple concurrent requests work (different sessions/request_ids)

  **Commit**: YES
  - Message: `feat(daemon): route permission interactions to/from ACP agents`

- [ ] 4. Add timeout and cancellation handling

  **What to do**:
  - Add configurable timeout for permission requests (default: 60s)
  - Handle Ctrl+C / session termination during wait
  - Return appropriate error/cancellation on timeout

  **Files to modify**:
  - `crates/crucible-acp/src/acp_client.rs` - Add timeout to blocking_recv
  - `crates/crucible-core/src/interaction.rs` - Add timeout config if needed

  **References**:
  - `tokio::time::timeout` for async timeout
  - `std::sync::mpsc::Receiver::recv_timeout` for sync timeout

  **Acceptance Criteria**:
  - [ ] Permission request times out after configurable duration
  - [ ] Timeout returns `RequestPermissionOutcome::Cancelled`
  - [ ] Session termination cancels pending requests

  **Commit**: YES
  - Message: `feat(acp): add timeout and cancellation for permission requests`

- [ ] 5. Add integration test for full flow

  **What to do**:
  - Create integration test that validates full e2e flow
  - Use daemon client + test_interaction or actual tool call
  - Verify modal appears, responds correctly

  **Files to modify**:
  - `crates/crucible-daemon-client/tests/integration.rs` - Add permission flow test

  **Test scenario**:
  ```rust
  #[tokio::test]
  async fn test_permission_request_shows_modal_and_responds() {
      // 1. Start daemon
      // 2. Create session with ACP agent
      // 3. Trigger tool that requires permission
      // 4. Verify InteractionRequested event received
      // 5. Send InteractionCompleted with allowed=true
      // 6. Verify tool executed successfully
  }
  ```

  **Acceptance Criteria**:
  - [ ] Integration test passes
  - [ ] Test covers approve flow (y)
  - [ ] Test covers deny flow (n)
  - [ ] Test covers cancel flow (Esc)

  **Commit**: YES
  - Message: `test(daemon): add integration test for permission request flow`

- [ ] 6. Add headless/batch mode escape hatch

  **What to do**:
  - Add `--auto-approve` / `--headless` flag for non-interactive use
  - When set, skip UI and auto-approve (like current behavior)
  - Useful for CI, scripts, batch operations

  **Files to modify**:
  - `crates/crucible-cli/src/commands/chat.rs` - Add CLI flag
  - `crates/crucible-acp/src/acp_client.rs` - Check flag before showing UI

  **Acceptance Criteria**:
  - [ ] `cru chat --auto-approve` auto-approves all permissions
  - [ ] Default behavior shows UI
  - [ ] Flag documented in `--help`

  **Commit**: YES
  - Message: `feat(cli): add --auto-approve flag for headless permission handling`

---

## Success Criteria

### Verification Commands
```bash
# Run integration test
cargo nextest run -p crucible-daemon-client permission_request

# Manual test: Start TUI, trigger tool, verify modal appears
cru chat --internal
# Type message that triggers tool call
# Verify permission modal appears
# Press 'y' and verify tool executes
```

### Final Checklist
- [ ] Permission modal appears for sensitive tool calls
- [ ] User decision (y/n/Esc) controls execution
- [ ] Timeout prevents infinite blocking
- [ ] Headless mode available for automation
- [ ] Integration test validates full flow
- [ ] No regression in read-only mode
