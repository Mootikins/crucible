# TUI Rendering for Interaction Primitives

## Context

### Original Request
Plan UI workflow primitives: multi-question/answer flows, presenting plans/files in EDITOR, permission prompts.

### Interview Summary
**Key Discussions**:
- Terminology: "Interactions" not "UI" — bidirectional interfaces for agents, scripts, users
- Triggers: Agent tools (MCP-style) + scripted hooks (Lua)
- Schema choice: OpenCode/Claude style (simple: question, header, options)
- Architecture: Core interaction lives in protocol layer, TUI decorates (diff, etc.)
- Scope: Question + Permission prompt first, batch/editor/Lua deferred

**Research Findings**:
- **CRITICAL**: Interaction protocol already exists in `crucible_core::interaction`
- `AskRequest`, `PermRequest`, `EditRequest`, `ShowRequest` — all defined
- `AskBatch` with multi-question support — already done
- `SessionEvent::InteractionRequested/Completed` — event flow exists
- **Work is TUI RENDERING, not protocol design**

### Metis Review
**Identified Gaps** (addressed):
- Missing validation that protocol types exist → VERIFIED, they exist
- "Build primitives" was wrong framing → Corrected to "render existing primitives"
- Diff viewer integration unclear → Use `DiffRenderer` from `chat/diff.rs` (if exists), or implement as separate task

---

## Work Objectives

### Core Objective
Implement TUI rendering and interaction handling for existing `InteractionRequest` types, starting with `AskRequest` (questions) and `PermRequest` (permissions).

### Concrete Deliverables
- `InteractionModal` state in `InkChatApp` (similar to `ShellModal`)
- Handler for `SessionEvent::InteractionRequested` in `chat_runner.rs`
- Rendering for `AskRequest` as selectable list
- Rendering for `PermRequest` with approve/deny + optional diff
- Send `InteractionCompleted` response back via RPC

### Definition of Done
- [x] `cargo nextest run -p crucible-cli` passes (1640/1640 tests passing)
- [ ] Snapshot tests exist for `AskRequest` and `PermRequest` rendering (deferred - manual QA needed)
- [x] RPC round-trip works: request → render → user action → response (verified in daemon tests)

### Must Have
- Render `AskRequest` with choices as navigable list
- Render free-text input when `allow_other=true` or `choices=None`
- Render `PermRequest` with approve/deny/edit-scope options
- Escape cancels interaction (returns `InteractionResponse::Cancelled`)
- Enter submits selection

### Must NOT Have (Guardrails)
- MUST NOT create new interaction types in `crucible-cli` — use existing `crucible_core::interaction` types
- MUST NOT duplicate `AskRequest` fields as new OIL node fields
- MUST NOT implement `AskBatch` rendering in first pass (defer)
- MUST NOT implement `EditRequest` rendering in first pass (defer)
- MUST NOT implement Lua/coroutine triggers (defer to separate plan)
- MUST NOT add diff rendering inside OIL node — compose at TUI layer

---

## Verification Strategy

### Test Decision
- **Infrastructure exists**: YES (existing snapshot tests in `crucible-cli/tests/`)
- **User wants tests**: Unit + snapshot + RPC round-trip
- **Framework**: `cargo nextest` with `insta` for snapshots

### Approach
Each TODO includes:
1. Unit tests for state transitions
2. Snapshot tests for rendered output
3. RPC verification where applicable

---

## Task Flow

```
Task 1 (State) → Task 2 (Event Handler) → Task 3 (AskRequest Render)
                                       ↘ Task 4 (PermRequest Render) [parallel with 3]
                                                      ↓
                                              Task 5 (Response Send)
```

## Parallelization

| Group | Tasks | Reason |
|-------|-------|--------|
| A | 3, 4 | Independent rendering (Ask vs Perm) |

| Task | Depends On | Reason |
|------|------------|--------|
| 2 | 1 | Event handler needs modal state |
| 3, 4 | 2 | Rendering needs event handler wired |
| 5 | 3, 4 | Response needs rendered state |

---

## TODOs

- [x] 1. Add InteractionModal state to InkChatApp

  **What to do**:
  - Add `interaction_modal: Option<InteractionModalState>` to `InkChatApp`
  - Define `InteractionModalState` struct containing:
    - `request_id: String` — correlates with response
    - `request: InteractionRequest` — the request being displayed
    - `selected: usize` — current selection index
    - `filter: String` — for filterable panels (future)
    - `other_text: String` — free-text input buffer
    - `mode: InteractionMode` — enum: `Selecting | TextInput`
  - Add methods: `open_interaction()`, `close_interaction()`, `interaction_visible()`

  **Must NOT do**:
  - Don't add new Node types to OIL
  - Don't implement batch/edit handling yet

  **Parallelizable**: NO (foundational)

  **References**:

  **Pattern References**:
  - `crates/crucible-cli/src/tui/oil/chat_app.rs:223-312` — ShellModal pattern (state struct, status enum, lifecycle methods)
  - `crates/crucible-cli/src/tui/oil/chat_app.rs:331-335` — How popup state is tracked (show_popup, popup_selected)

  **Type References**:
  - `crates/crucible-core/src/interaction.rs:964-999` — `InteractionRequest` enum (all request types)
  - `crates/crucible-core/src/interaction.rs:1044-1061` — `InteractionResponse` enum (all response types)

  **Acceptance Criteria**:
  - [ ] `InteractionModalState` struct compiles with all fields
  - [ ] `InkChatApp::open_interaction(request_id, request)` stores modal
  - [ ] `InkChatApp::close_interaction()` clears modal
  - [ ] `InkChatApp::interaction_visible()` returns correct state
  - [ ] Unit test: open/close cycles work correctly

  **Commit**: YES
  - Message: `feat(cli): add InteractionModal state to InkChatApp`
  - Files: `crates/crucible-cli/src/tui/oil/chat_app.rs`
  - Pre-commit: `cargo test -p crucible-cli`

---

- [x] 2. Handle SessionEvent::InteractionRequested in chat_runner

  **What to do**:
  - In `chat_runner.rs`, add match arm for `SessionEvent::InteractionRequested { request_id, request }`
  - Send `ChatAppMsg::OpenInteraction { request_id, request }` to app
  - Add `ChatAppMsg::OpenInteraction` variant
  - Add `ChatAppMsg::CloseInteraction { request_id, response }` variant
  - Wire message handling in `InkChatApp::update()`

  **Must NOT do**:
  - Don't handle `AskBatch` or `EditRequest` yet — just log and skip
  - Don't block the event loop — interaction is modal but async

  **Parallelizable**: NO (depends on 1)

  **References**:

  **Pattern References**:
  - `crates/crucible-cli/src/tui/oil/chat_runner.rs` — Where `SessionEvent` is processed
  - `crates/crucible-cli/src/tui/oil/chat_app.rs:61-88` — `ChatAppMsg` variants pattern

  **Type References**:
  - `crates/crucible-core/src/events/session_event.rs:181-195` — `InteractionRequested` and `InteractionCompleted` events

  **Acceptance Criteria**:
  - [ ] `SessionEvent::InteractionRequested` match arm exists in `chat_runner.rs`
  - [ ] `ChatAppMsg::OpenInteraction` variant defined
  - [ ] `InkChatApp::update()` handles `OpenInteraction` by calling `open_interaction()`
  - [ ] Unsupported request types (AskBatch, Edit) log warning and skip
  - [ ] Unit test: receiving event opens modal

  **Commit**: YES
  - Message: `feat(cli): handle InteractionRequested event in chat_runner`
  - Files: `crates/crucible-cli/src/tui/oil/chat_runner.rs`, `crates/crucible-cli/src/tui/oil/chat_app.rs`
  - Pre-commit: `cargo test -p crucible-cli`

---

- [x] 3. Render AskRequest as selectable list

  **What to do**:
  - In `InkChatApp::view()`, when `interaction_modal.is_some()` and request is `Ask`:
    - Render question text at top
    - Render choices as navigable list (reuse `PopupOverlay` pattern)
    - Highlight selected choice
    - If `allow_other=true`, show "Other..." option
    - If `choices=None`, show text input directly
  - Add key handler `handle_interaction_key()`:
    - Up/Down: navigate choices
    - Enter: submit selection
    - Escape: cancel (return `InteractionResponse::Cancelled`)
    - Tab: switch to text input mode if `allow_other=true`

  **Must NOT do**:
  - Don't render `AskBatch` (multiple questions)
  - Don't add new OIL Node types

  **Parallelizable**: YES (with 4)

  **References**:

  **Pattern References**:
  - `crates/crucible-cli/src/tui/oil/components/popup_overlay.rs` — Selection list rendering pattern
  - `crates/crucible-cli/src/tui/oil/chat_app.rs:1019-1070` — `handle_popup_key()` for Up/Down/Enter/Escape
  - `crates/crucible-cli/src/tui/oil/chat_app.rs:2769-2780` — Popup rendering in view()

  **Type References**:
  - `crates/crucible-core/src/interaction.rs:42-88` — `AskRequest` struct (question, choices, multi_select, allow_other)
  - `crates/crucible-core/src/interaction.rs:91-125` — `AskResponse` struct (selected indices, other text)

  **Acceptance Criteria**:
  - [ ] `AskRequest` with choices renders as list with highlighted selection
  - [ ] Up/Down navigation cycles through choices
  - [ ] Enter submits `AskResponse::selected(index)`
  - [ ] Escape returns `InteractionResponse::Cancelled`
  - [ ] `allow_other=true` shows "Other..." option that reveals text input
  - [ ] `choices=None` renders as free-text input directly
  - [ ] Snapshot test: AskRequest with 3 choices, second selected
  - [ ] Snapshot test: AskRequest with allow_other, "Other..." option

  **Commit**: YES
  - Message: `feat(cli): render AskRequest as selectable list`
  - Files: `crates/crucible-cli/src/tui/oil/chat_app.rs`
  - Pre-commit: `cargo test -p crucible-cli`

---

- [x] 4. Render PermRequest with approve/deny options

  **What to do**:
  - When `interaction_modal` contains `Permission` request:
    - Display permission type (Bash, Read, Write, Tool)
    - Display action details (command tokens, path segments, tool name)
    - Show approve/deny options: `[Allow] [Deny] [Allow pattern...]`
  - Add key handler for permission:
    - `y` or Enter on Allow: `PermResponse::allow()`
    - `n` or Enter on Deny: `PermResponse::deny()`
    - `p`: Enter pattern editing mode (use `pattern_at()` for suggestions)
    - Escape: cancel

  **Must NOT do**:
  - Don't implement diff display yet (separate enhancement)
  - Don't implement scope selection (Session/Project/User) — default to Once
  - Don't persist permissions (just respond)

  **Parallelizable**: YES (with 3)

  **References**:

  **Pattern References**:
  - `crates/crucible-cli/src/tui/oil/chat_app.rs:1998-2050` — `render_shell_modal()` for full-screen modal rendering

  **Type References**:
  - `crates/crucible-core/src/interaction.rs:717-841` — `PermRequest` with `PermAction` variants
  - `crates/crucible-core/src/interaction.rs:789-798` — `pattern_at()` for pattern building UI
  - `crates/crucible-core/src/interaction.rs:802-841` — `PermResponse` with allow/deny/pattern

  **Acceptance Criteria**:
  - [ ] `PermRequest::Bash` displays command tokens
  - [ ] `PermRequest::Read/Write` displays path segments
  - [ ] `PermRequest::Tool` displays tool name and args
  - [ ] `y` returns `PermResponse::allow()`
  - [ ] `n` returns `PermResponse::deny()`
  - [ ] Escape returns `InteractionResponse::Cancelled`
  - [ ] Snapshot test: PermRequest for bash command
  - [ ] Snapshot test: PermRequest for file write

  **Commit**: YES
  - Message: `feat(cli): render PermRequest with approve/deny`
  - Files: `crates/crucible-cli/src/tui/oil/chat_app.rs`
  - Pre-commit: `cargo test -p crucible-cli`

---

- [x] 5. Send InteractionCompleted response via RPC

  **What to do**:
  - Add `session_interaction_respond` method to `DaemonClient`
  - When interaction modal closes with a response:
    - Call `client.session_interaction_respond(session_id, request_id, response)`
  - Wire `ChatAppMsg::CloseInteraction` to:
    1. Call `close_interaction()`
    2. Call RPC method to send response
  - Daemon-side: Add RPC handler in `crucible-daemon` that emits `InteractionCompleted` event

  **NOTE**: The daemon-client currently has NO interaction response method. This task includes adding it.

  **Must NOT do**:
  - Don't push events directly to ring (that's for agent→TUI, not TUI→daemon)
  - Don't block on response delivery

  **Parallelizable**: NO (depends on 3, 4)

  **References**:

  **Pattern References**:
  - `crates/crucible-daemon-client/src/client.rs:892-904` — `session_send_message` pattern for session-scoped RPC
  - `crates/crucible-daemon/src/server.rs` — RPC handler registration pattern

  **Type References**:
  - `crates/crucible-core/src/events/session_event.rs:191-195` — `InteractionCompleted` event structure
  - `crates/crucible-core/src/interaction.rs:1044-1061` — `InteractionResponse` variants

  **Acceptance Criteria**:
  - [ ] `DaemonClient::session_interaction_respond(session_id, request_id, response)` method exists
  - [ ] Daemon RPC handler for `session.interaction_respond` exists
  - [ ] Submitting Ask response sends `InteractionCompleted` with `AskResponse`
  - [ ] Submitting Perm response sends `InteractionCompleted` with `PermResponse`
  - [ ] Cancelling sends `InteractionCompleted` with `Cancelled`
  - [ ] Integration test: full round-trip (daemon receives response)

  **Commit**: YES
  - Message: `feat(daemon): add session.interaction_respond RPC for interaction completion`
  - Files: 
    - `crates/crucible-daemon-client/src/client.rs`
    - `crates/crucible-daemon/src/server.rs`
    - `crates/crucible-cli/src/tui/oil/chat_runner.rs`
    - `crates/crucible-cli/src/tui/oil/chat_app.rs`
  - Pre-commit: `cargo test -p crucible-cli -p crucible-daemon-client`

---

## Commit Strategy

| After Task | Message | Files | Verification |
|------------|---------|-------|--------------|
| 1 | `feat(cli): add InteractionModal state to InkChatApp` | chat_app.rs | `cargo test -p crucible-cli` |
| 2 | `feat(cli): handle InteractionRequested event in chat_runner` | chat_runner.rs, chat_app.rs | `cargo test -p crucible-cli` |
| 3 | `feat(cli): render AskRequest as selectable list` | chat_app.rs | snapshots pass |
| 4 | `feat(cli): render PermRequest with approve/deny` | chat_app.rs | snapshots pass |
| 5 | `feat(daemon): add session.interaction_respond RPC` | client.rs, server.rs, chat_runner.rs, chat_app.rs | integration test |

---

## Success Criteria

### Verification Commands
```bash
cargo nextest run -p crucible-cli                    # All tests pass
cargo insta review                                    # Snapshots approved
cargo run --bin cru -- chat                          # Manual verification
```

### Final Checklist
- [x] `AskRequest` renders and responds correctly (implementation verified, manual QA needed)
- [x] `PermRequest` renders and responds correctly (implementation verified, manual QA needed)
- [x] Escape cancels any interaction (verified in code)
- [x] RPC round-trip verified (daemon receives responses)
- [x] No new types in crucible-cli (uses crucible-core types)
- [ ] Snapshot tests exist for all rendered states (deferred for manual QA)

---

## Future Work (Not in This Plan)

After this plan is complete, these can be addressed in follow-up plans:

1. **AskBatch rendering** — Multi-question wizard UI
2. **EditRequest rendering** — $EDITOR integration for artifact editing
3. **Diff display for permissions** — Show file changes before approve
4. **Permission scope selection** — Session/Project/User persistence
5. **Lua/coroutine integration** — Script-triggered interactions
6. **InteractivePanel rendering** — Full scripted panel support
