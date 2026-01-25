# ask_user Tool + Interaction Modal Redesign

## Context

### Original Request
Build an `ask_user` tool for internal agents that triggers UI interactions, AND redesign the interaction modal UI to match input box styling with multi-select support.

### Interview Summary
**Key Discussions**:
- This is the foundation for future interactive features (diffs, permissions, Lua API)
- Approach: Dedicated `AskUserTool` struct (opt-in, not modifying all tools)
- Location: `crucible-rig` for tool, `crucible-cli` for UI
- Tool name: `ask_user` (matches Lua API naming)
- Test strategy: Integration test + manual QA + snapshots for UI

**Research Findings**:
- Lua `ask_user()` pattern works: register → emit InteractionRequested → wait
- `InteractionRegistry` exists in `crucible-core/src/interaction_registry.rs`
- `AskRequest`/`AskResponse` types exist in `crucible-core/src/interaction.rs`
- TUI already handles `InteractionRequest::Ask` in `chat_runner.rs:655-656`
- Input box uses `▄`/`▀` borders with `INPUT_BG` color

### Metis Review
**Critical Finding**: `blocking_recv()` would deadlock - Rig tools run async, must use `rx.await`

**Identified Gaps** (addressed):
- Async vs blocking: Use `tokio::sync::oneshot` with `.await`, NOT `blocking_recv()`
- Event callback wiring: Daemon needs to provide `push_event` to InteractionContext
- Response format: Return structured JSON so LLM can parse result
- Tool gating: Only attach when `has_interaction_context()` is true

---

## Work Objectives

### Core Objective
Enable internal agents to ask users questions during execution by creating an `ask_user` tool, AND provide a polished modal UI with multi-select, "Other" textarea, and multi-question tabs.

### Concrete Deliverables

**Backend (Tasks 1-6)**:
- `InteractionContext` type in `crucible-core`
- `AskUserTool` implementation in `crucible-rig`
- `WorkspaceContext` extended with `with_interaction_context()`
- Daemon wiring to create and pass InteractionContext
- Tool correctly gated on context availability

**Frontend (Tasks 7-12)**:
- Updated `render_ask_interaction()` with new visual design
- Multi-select checkbox support in state and rendering
- "Other" inline textarea with preserved text
- Multi-question tab bar UI
- Ctrl+C cancel handler

### Definition of Done
- [ ] `ask_user` tool appears in agent tool list when context provided
- [ ] Tool emits `SessionEvent::InteractionRequested` with `Ask` variant
- [ ] Tool awaits response via oneshot channel and returns structured result
- [ ] Integration test validates full flow
- [ ] Modal has `▄▄▄` / `▀▀▀` borders matching input box
- [ ] Multi-select checkboxes work (`[ ]` / `[x]`)
- [ ] "Other" textarea preserves text when deselected
- [ ] Multi-question tab bar navigation works
- [ ] Ctrl+C cancels modal
- [ ] All snapshot tests pass

### Must Have
- Async waiting via `rx.await` (NOT blocking_recv)
- Structured JSON output for LLM to parse
- `▄▄▄` / `▀▀▀` borders matching input box
- `[ ]` / `[x]` for multi-select
- Esc and Ctrl+C cancel

### Must NOT Have (Guardrails)
- Don't use `blocking_recv()` (will deadlock async runtime)
- Don't modify existing tools
- Mutex between "Other" and regular options
- Fancy Unicode checkboxes (use `[ ]`/`[x]`)
- Permission modal changes (separate task)

---

## Verification Strategy

### Test Decision
- **Infrastructure exists**: YES (InteractionRegistry, event types, snapshots)
- **User wants tests**: Integration test + manual QA + snapshots
- **Framework**: `cargo nextest`, `insta` snapshots

### Integration Test Approach
Test with mock registry and channel for backend.

### UI Verification
Snapshot tests for visual design, manual QA via `test_interaction` RPC.

---

## Task Flow

```
BACKEND:
Task 1 (InteractionContext) → Task 2 (AskUserTool) → Task 3 (WorkspaceContext)
                                                              ↓
                                                    Task 4 (Daemon wiring)
                                                              ↓
                                                    Task 5 (Tool attachment)
                                                              ↓
                                                    Task 6 (Integration test)

FRONTEND (can run in parallel with backend after Task 1):
Task 7 (Modal state) → Task 8 (Single-select render) → Task 9 (Multi-select)
                                                              ↓
                                                    Task 10 (Other textarea)
                                                              ↓
                                                    Task 11 (Multi-question tabs)
                                                              ↓
                                                    Task 12 (Ctrl+C handler)
```

## Parallelization

| Task | Depends On | Reason |
|------|------------|--------|
| 1 | - | Foundation type |
| 2 | 1 | Uses InteractionContext |
| 3 | 1 | Uses InteractionContext |
| 4 | 1, 3 | Needs both context and WorkspaceContext |
| 5 | 2, 3 | Needs tool and context getter |
| 6 | 4, 5 | Needs full wiring |
| 7 | - | Can start immediately (UI state) |
| 8 | 7 | Depends on state changes |
| 9 | 8 | Depends on render base |
| 10 | 9 | Depends on multi-select |
| 11 | 10 | Depends on Other |
| 12 | 7 | Only needs state (parallel with 8-11) |

**Parallel Groups**:
- Tasks 2, 3, 7 can run in parallel after Task 1
- Task 12 can run in parallel with Tasks 8-11

---

## TODOs

### BACKEND TASKS (1-6)

- [x] 1. Create InteractionContext type in crucible-core

  **What to do**:
  - Create new file `crucible-core/src/interaction_context.rs`
  - Define `InteractionContext` struct with:
    - `registry: Arc<Mutex<InteractionRegistry>>`
    - `push_event: Arc<dyn Fn(SessionEvent) + Send + Sync>`
  - Implement `new()` constructor
  - Export from `crucible-core/src/lib.rs`

  **Must NOT do**:
  - Don't modify `InteractionRegistry` itself
  - Don't create new event types

  **Parallelizable**: NO (foundation for tasks 2, 3)

  **References**:
  - `crucible-lua/src/ask.rs:684-691` - LuaAskContext pattern to follow
  - `crucible-core/src/interaction_registry.rs` - Registry we wrap
  - `crucible-core/src/events/session_event.rs` - SessionEvent type

  **Acceptance Criteria**:
  - [ ] `InteractionContext` struct exists with registry + push_event
  - [ ] `InteractionContext::new(registry, push_event)` constructor works
  - [ ] Type exported from `crucible_core::InteractionContext`
  - [ ] Verify: `cargo check -p crucible-core`

  **Commit**: YES
  - Message: `feat(core): add InteractionContext for tool-triggered interactions`

---

- [x] 2. Create AskUserTool implementation

  **What to do**:
  - Create `AskUserTool` struct in `crucible-rig/src/workspace_tools.rs` (or new file)
  - Implement `rig::tool::Tool` trait for `AskUserTool`
  - Tool takes `AskRequest` as input (question, choices, multi_select, allow_other)
  - Tool returns `AskResponse` as output (selected indices or other text)
  - Implementation:
    1. Generate request ID (UUID)
    2. Create `tokio::sync::oneshot` channel
    3. Register receiver with InteractionRegistry
    4. Emit `SessionEvent::InteractionRequested` via push_event
    5. Await response via `rx.await`
    6. Return structured JSON response

  **Must NOT do**:
  - Don't use `blocking_recv()` (deadlocks async runtime)
  - Don't support AskBatch (v1 uses simple AskRequest)

  **Parallelizable**: YES (with 3, 7)

  **References**:
  - `crucible-lua/src/ask.rs:715-750` - Blocking wait pattern (adapt for async)
  - `crucible-rig/src/workspace_tools.rs:200-250` - Example tool implementation (BashTool)
  - `crucible-core/src/interaction.rs:42-88` - AskRequest/AskResponse types

  **Tool Schema** (for LLM):
  ```json
  {
    "name": "ask_user",
    "description": "Ask the user a question and wait for their response",
    "parameters": {
      "question": { "type": "string", "description": "The question to ask" },
      "choices": { "type": "array", "items": { "type": "string" }, "description": "Optional list of choices" },
      "multi_select": { "type": "boolean", "description": "Allow multiple selections" },
      "allow_other": { "type": "boolean", "description": "Allow free-text input" }
    }
  }
  ```

  **Acceptance Criteria**:
  - [ ] `AskUserTool` struct exists with `InteractionContext`
  - [ ] Implements `rig::tool::Tool` trait
  - [ ] Uses `rx.await` not `blocking_recv()`
  - [ ] Returns structured JSON: `{"selected": [0], "other": null}`
  - [ ] Verify: `cargo check -p crucible-rig`

  **Commit**: YES
  - Message: `feat(rig): add AskUserTool for agent-user interactions`

---

- [x] 3. Extend WorkspaceContext with interaction support

  **What to do**:
  - Add `interaction_context: Option<Arc<InteractionContext>>` field to `WorkspaceContext`
  - Add `with_interaction_context(ctx: Arc<InteractionContext>) -> Self` builder method
  - Add `has_interaction_context() -> bool` getter
  - Add `interaction_context() -> Option<Arc<InteractionContext>>` getter

  **Parallelizable**: YES (with 2, 7)

  **References**:
  - `crucible-rig/src/workspace_tools.rs:67-70` - `with_background_spawner` pattern
  - `crucible-rig/src/workspace_tools.rs:99-102` - `has_background_spawner` pattern

  **Acceptance Criteria**:
  - [ ] `WorkspaceContext` has `interaction_context` field
  - [ ] `with_interaction_context()` builder works
  - [ ] `has_interaction_context()` returns true when set
  - [ ] Verify: `cargo check -p crucible-rig`

  **Commit**: YES
  - Message: `feat(rig): extend WorkspaceContext with InteractionContext support`

---

- [x] 4. Wire InteractionContext in daemon agent creation

  **What to do**:
  - In `crucible-daemon/src/agent_factory.rs`, create `InteractionContext`
  - Get `InteractionRegistry` from session manager (or create new)
  - Create `push_event` callback that routes to session's event channel
  - Pass `InteractionContext` to `WorkspaceContext` via `with_interaction_context()`

  **Parallelizable**: NO (depends on 1, 3)

  **References**:
  - `crucible-daemon/src/agent_factory.rs:100-103` - Where WorkspaceContext is created
  - `crucible-daemon/src/session_manager.rs` - Where event channels live

  **Wiring Pattern**:
  ```rust
  let registry = Arc::new(Mutex::new(InteractionRegistry::new()));
  let event_tx = session.event_sender.clone();
  let push_event: Arc<dyn Fn(SessionEvent) + Send + Sync> = Arc::new(move |event| {
      let _ = event_tx.send(event);
  });
  let interaction_ctx = Arc::new(InteractionContext::new(registry, push_event));
  
  let mut ws_ctx = WorkspaceContext::new(workspace)
      .with_interaction_context(interaction_ctx);
  ```

  **Acceptance Criteria**:
  - [ ] `InteractionContext` created in agent factory
  - [ ] Registry shared per-session
  - [ ] Event callback routes to session's event channel
  - [ ] Verify: `cargo check -p crucible-daemon`

  **Commit**: YES
  - Message: `feat(daemon): wire InteractionContext to agent creation`

---

- [x] 5. Add AskUserTool to attach_tools function

  **What to do**:
  - In `crucible-rig/src/agent.rs:attach_tools()`, add `AskUserTool` when context available
  - Gate on `ctx.has_interaction_context()`
  - Add to relevant match arms (read-write modes with interaction)

  **Must NOT do**:
  - Don't add to read-only modes (plan mode)
  - Don't add unconditionally

  **Parallelizable**: NO (depends on 2, 3)

  **References**:
  - `crucible-rig/src/agent.rs:85-155` - `attach_tools` function with match arms
  - `crucible-rig/src/agent.rs:115-127` - Background spawner conditional pattern

  **Acceptance Criteria**:
  - [ ] `AskUserTool` added when `has_interaction_context()` is true
  - [ ] Tool NOT added when context is None
  - [ ] Tool NOT added in read-only (plan) mode
  - [ ] Verify: `cargo check -p crucible-rig`

  **Commit**: YES
  - Message: `feat(rig): attach AskUserTool when InteractionContext available`

---

- [x] 6. Add integration test for full flow

  **What to do**:
  - Create integration test that validates full ask_user flow
  - Test scenarios:
    1. Tool emits correct event
    2. Tool receives response and returns it
    3. Tool handles cancellation gracefully

  **Parallelizable**: NO (depends on 4, 5)

  **References**:
  - Test in `crucible-rig/tests/ask_user_tool.rs`

  **Test Pattern**:
  ```rust
  #[tokio::test]
  async fn test_ask_user_tool_flow() {
      let registry = Arc::new(Mutex::new(InteractionRegistry::new()));
      let (event_tx, mut event_rx) = tokio::sync::mpsc::channel(10);
      let push_event: Arc<dyn Fn(SessionEvent) + Send + Sync> = Arc::new(move |event| {
          let _ = event_tx.try_send(event);
      });
      let ctx = Arc::new(InteractionContext::new(registry.clone(), push_event));
      let tool = AskUserTool::new(ctx);
      
      // Spawn tool call
      let handle = tokio::spawn(async move {
          tool.call(AskRequest::new("Which option?").choices(["A", "B"])).await
      });
      
      // Receive event, complete interaction, verify result
      // ...
  }
  ```

  **Acceptance Criteria**:
  - [ ] Test validates event emission
  - [ ] Test validates response handling
  - [ ] Test validates cancellation handling
  - [ ] Verify: `cargo nextest run -p crucible-rig ask_user`

  **Commit**: YES
  - Message: `test(rig): add integration test for AskUserTool flow`

---

### FRONTEND TASKS (7-12)

- [x] 7. Extend InteractionModalState for multi-select and multi-question

  **What to do**:
  - Add `checked: HashSet<usize>` for multi-select tracking
  - Add `current_question: usize` for multi-question navigation
  - Add `other_text_preserved: bool` to track if "Other" was previously entered
  - Update `AskRequest` handling to detect multi_select mode

  **Parallelizable**: YES (can start with Task 1)

  **References**:
  - `crates/crucible-cli/src/tui/oil/chat_app.rs:2730-2780` - InteractionModalState
  - `crates/crucible-core/src/interaction.rs:100-150` - AskRequest struct

  **Acceptance Criteria**:
  - [ ] State struct compiles with new fields
  - [ ] Unit test: toggle checkbox updates `checked` set

  **Commit**: YES
  - Message: `feat(tui): extend InteractionModalState for multi-select`

---

- [x] 8. Redesign single-select render with new visual style

  **What to do**:
  - Add `▄▄▄` top border, `▀▀▀` bottom border (use INPUT_BG color)
  - Render question as bold header with mode-colored block
  - Number options: `1.` `2.` `3.`
  - Add dim description below each option
  - Keep `>` cursor for selection
  - Add help text at bottom

  **Parallelizable**: NO (depends on 7)

  **References**:
  - `crates/crucible-cli/src/tui/oil/chat_app.rs:2871-2872` - input box borders
  - `crates/crucible-cli/src/tui/oil/colors.rs` - INPUT_BG color
  - `crates/crucible-cli/src/tui/oil/chat_app.rs:2950-3050` - current render_ask_interaction

  **Acceptance Criteria**:
  - [ ] Snapshot test: `ask_single_select_styled`
  - [ ] Visual: borders, numbered options, dim descriptions visible

  **Commit**: YES
  - Message: `feat(tui): redesign single-select modal with input box styling`

---

- [x] 9. Implement multi-select checkbox rendering and toggle

  **What to do**:
  - Detect `multi_select: true` in AskRequest
  - Render `[ ]` or `[x]` prefix based on `checked` set
  - Space key toggles current item in `checked`
  - Enter confirms all checked items
  - Add `a` (select all) and `n` (select none) shortcuts

  **Parallelizable**: NO (depends on 8)

  **References**:
  - `crates/crucible-cli/src/tui/oil/chat_app.rs:3100-3200` - handle_ask_key
  - `crates/crucible-core/src/interaction.rs:AskRequest::multi_select`

  **Acceptance Criteria**:
  - [ ] Snapshot test: `ask_multi_select_none_checked`
  - [ ] Snapshot test: `ask_multi_select_some_checked`
  - [ ] Unit test: Space toggles checkbox state
  - [ ] Unit test: `a` selects all, `n` selects none

  **Commit**: YES
  - Message: `feat(tui): add multi-select checkbox support`

---

- [x] 10. Implement "Other" inline textarea

  **What to do**:
  - Detect `allow_other: true` in AskRequest
  - Render "Other" as last option with hint text
  - When selected/checked, show inline textarea
  - Text preserved when deselected (rendered dim)
  - Tab or Enter on "Other" in single-select focuses textarea
  - In multi-select, checking "Other" focuses textarea

  **Parallelizable**: NO (depends on 9)

  **References**:
  - `crates/crucible-cli/src/tui/oil/chat_app.rs:InteractionModalState::text_input`
  - Existing text input handling in `handle_ask_key`

  **Acceptance Criteria**:
  - [ ] Snapshot test: `ask_other_empty`
  - [ ] Snapshot test: `ask_other_with_text`
  - [ ] Snapshot test: `ask_other_deselected_preserved`
  - [ ] Unit test: selecting "Other" enables text input
  - [ ] Unit test: deselecting preserves text

  **Commit**: YES
  - Message: `feat(tui): add Other textarea with text preservation`

---

- [ ] 11. Implement multi-question tab bar (BLOCKED - requires AskBatch refactoring)

  **What to do**:
  - Detect multiple questions in AskBatch
  - Render tab bar below top border: `Tab1 (colored)  Tab2 (dim)  Tab3 (dim)`
  - Tab/Shift+Tab navigates between questions
  - Track selections per question
  - Enter on last question submits all

  **Parallelizable**: NO (depends on 10)

  **References**:
  - `crates/crucible-core/src/interaction.rs:AskBatch` - batched questions
  - Tab styling similar to statusline mode blocks

  **Acceptance Criteria**:
  - [ ] Snapshot test: `ask_batch_tab_bar`
  - [ ] Snapshot test: `ask_batch_second_tab_active`
  - [ ] Unit test: Tab advances to next question
  - [ ] Unit test: Shift+Tab goes to previous

  **Commit**: YES
  - Message: `feat(tui): add multi-question tab bar`

---

- [x] 12. Add Ctrl+C cancel handler

  **What to do**:
  - Handle Ctrl+C in `handle_ask_key` and `handle_perm_key`
  - Close modal and send Cancelled response
  - Ensure no character insertion from Ctrl+C

  **Parallelizable**: YES (with 8-11, only needs 7)

  **References**:
  - `crates/crucible-cli/src/tui/oil/chat_app.rs:handle_ask_key`
  - Existing Esc cancel handling

  **Acceptance Criteria**:
  - [ ] Unit test: Ctrl+C closes modal
  - [ ] Unit test: Ctrl+C does not insert 'c' character

  **Commit**: YES
  - Message: `fix(tui): add Ctrl+C to cancel interaction modal`

---

## Commit Strategy

| After Task | Message | Verification |
|------------|---------|--------------|
| 1 | `feat(core): add InteractionContext` | cargo check |
| 2 | `feat(rig): add AskUserTool` | cargo check |
| 3 | `feat(rig): extend WorkspaceContext` | cargo check |
| 4 | `feat(daemon): wire InteractionContext` | cargo check |
| 5 | `feat(rig): attach AskUserTool` | cargo check |
| 6 | `test(rig): integration test` | cargo nextest |
| 7 | `feat(tui): extend InteractionModalState` | unit tests |
| 8 | `feat(tui): redesign single-select modal` | snapshot |
| 9 | `feat(tui): add multi-select checkbox support` | snapshot + unit |
| 10 | `feat(tui): add Other textarea` | snapshot + unit |
| 11 | `feat(tui): add multi-question tab bar` | snapshot + unit |
| 12 | `fix(tui): add Ctrl+C to cancel` | unit test |

---

## Success Criteria

### Verification Commands
```bash
# Build all affected crates
cargo check -p crucible-core -p crucible-rig -p crucible-daemon -p crucible-cli

# Run all tests
cargo nextest run -p crucible-rig ask_user
cargo nextest run -p crucible-cli

# Review snapshots
cargo insta review

# Manual test
cru chat
# Have agent use ask_user tool, verify modal appears with new styling
```

### Final Checklist
- [ ] `ask_user` tool appears in agent tool list
- [ ] Tool emits correct SessionEvent
- [ ] Tool awaits and returns response
- [ ] Modal has `▄▄▄` / `▀▀▀` borders
- [ ] Multi-select checkboxes work
- [ ] "Other" text preserved when deselected
- [ ] Multi-question tabs work
- [ ] Ctrl+C cancels modal
- [ ] All snapshot tests pass
- [ ] Integration test passes
