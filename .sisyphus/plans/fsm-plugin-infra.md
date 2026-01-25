# FSM Plugin Infrastructure

## Context

### Original Request
Enable FSM-driven agent discipline (Ralph loops, todo enforcer) in Crucible via Lua plugins. Focus on minimal Rust changes that unlock maximum Lua flexibility.

### Interview Summary
**Key Discussions**:
- Focus on unique value (FSM discipline), not commodities (LSP, AST-grep)
- Standardize on `cru` namespace (not both `cru` and `crucible`)
- Plaintext + minimal utils for hook output (not XML tags baked in)
- Inject as user message prefix (avoids cache invalidation from system prompt changes)

**Research Findings**:
- `cru.on()` at `handlers.rs:572` ignores the function parameter (prefixed `_handler`)
- `RuntimeHandler` struct only stores metadata, not the function reference
- `ScriptHandlerResult` has Transform/PassThrough/Cancel but no Inject variant
- `message_complete` event is emitted but not bridged to Lua handlers

---

## Work Objectives

### Core Objective
Enable Lua plugins to implement FSM workflow discipline (Ralph loops, todo enforcer) with targeted Rust changes and critical event flow wiring.

### Concrete Deliverables
- Fixed `cru.on()` that stores and can execute Lua function references
- `ScriptHandlerResult::Inject` variant for message injection
- Event dispatch loop routing daemon events to Lua handlers
- Inject message flow wiring injected content into conversation
- `cru.fmt()` string formatting utility
- Example todo enforcer plugin demonstrating the full pattern end-to-end

### Definition of Done
- [ ] `cargo test -p crucible-lua` passes
- [ ] `cargo clippy -p crucible-lua` has no warnings
- [ ] Can register a handler with `cru.on("turn:complete", fn)` and have it execute
- [ ] Handler can return `{ inject = { content = "..." } }` and it's recognized

### Must Have
- Function storage for runtime handlers (RegistryKey)
- Inject return type parsing
- `cru.fmt()` utility function

### Must NOT Have (Guardrails)
- DO NOT add XML/structured format conventions at Rust level
- DO NOT modify system prompt injection (use user message prefix only)
- DO NOT allow nested injection (max 1 inject per turn)
- DO NOT inject during active streaming

---

## Verification Strategy

### Test Decision
- **Infrastructure exists**: YES (crucible-lua has test infrastructure)
- **User wants tests**: YES (TDD for handler registration)
- **Framework**: Rust built-in tests

### TDD Workflow
Each TODO follows RED-GREEN-REFACTOR:
1. **RED**: Write failing test first
2. **GREEN**: Implement minimum code to pass
3. **REFACTOR**: Clean up while keeping green

---

## Task Flow

```
Task P (rename InkChatApp → OilChatTui) ← Do first to avoid conflicts
    ↓
Task 0 (docs) [parallel OK]
    ↓
Task 1 (function storage)
    ↓
Task 2 (execution method)
    ↓
Task 2.5 (event dispatch in daemon) ← Critical glue
    ↓
Task 3 (Inject variant) [parallel with 2.5]
    ↓
Task 3.5 (inject flow in daemon) ← Critical glue
    ↓
Task 4 (cru.fmt) [parallel with 3.5]
    ↓
Task 5 (E2E integration test)
```
Task 1 (RuntimeHandler function storage)
    ↓
Task 2 (Execute runtime handlers)
    ↓
Task 2.5 (Event dispatch loop) ← CRITICAL: Routes events to handlers
    ↓
Task 3 (ScriptHandlerResult::Inject) [can parallel with 2.5]
    ↓
Task 3.5 (Inject message flow) ← CRITICAL: Processes Inject results
    ↓
Task 4 (cru.fmt utility) [can parallel with 3.5]
    ↓
Task 5 (Integration test - full E2E pattern)
```

## Parallelization

| Group | Tasks | Reason |
|-------|-------|--------|
| A | 2.5, 3 | Independent: Event dispatch vs Inject parsing |
| B | 3.5, 4 | Independent: Inject flow vs fmt utility |

| Task | Depends On | Reason |
|------|------------|--------|
| 2 | 1 | Need stored functions to execute |
| 2.5 | 2 | Need execution method before dispatch loop |
| 3 | - | Can start after Task 1 (just enum + parsing) |
| 3.5 | 2.5, 3 | Need both dispatch and Inject variant |
| 5 | 3.5 | Integration test needs full E2E flow |

---

## TODOs

- [ ] P. Rename InkChatApp to OilChatTui (Prep Task)

  **What to do**:
  - Use LSP rename: `InkChatApp` → `OilChatTui`
  - Affects 17 files, ~258 usages
  - Update module re-export in `mod.rs`

  **Why**:
  - Clarifies this is TUI-specific (will later extract core interface components)
  - Aligns with OIL rendering system naming
  - Removes legacy "Ink" reference

  **How**:
  - Use `lsp_prepare_rename` to verify rename is valid
  - Use `lsp_rename` to perform the rename
  - Run `cargo test -p crucible-cli` to verify

  **Parallelizable**: NO (must be done first to avoid conflicts)

  **Acceptance Criteria**:
  - [ ] No `InkChatApp` references remain
  - [ ] `OilChatTui` used consistently
  - [ ] `cargo test -p crucible-cli` passes
  - [ ] `cargo clippy -p crucible-cli` has no new warnings

  **Commit**: YES
  - Message: `refactor(cli): rename InkChatApp to OilChatTui for clarity`
  - Files: All files in `crates/crucible-cli/src/tui/oil/`
  - Pre-commit: `cargo test -p crucible-cli`

---

- [ ] 0. Add CLI Architecture Documentation

  **What to do**:
  - Move `.sisyphus/drafts/cli-agents-md.md` to `crates/crucible-cli/AGENTS.md`
  - This documents the critical rule: CLI/TUI is view-only, no domain logic

  **Why**:
  - Prevents future mistakes of putting handler/inject logic in CLI
  - Establishes daemon as the owner of all domain logic
  - Ensures multi-client consistency

  **Parallelizable**: YES (independent, can be done anytime)

  **Acceptance Criteria**:
  - [ ] `crates/crucible-cli/AGENTS.md` exists
  - [ ] Draft file deleted

  **Commit**: YES
  - Message: `docs(cli): add AGENTS.md documenting view-only architecture`
  - Files: `crates/crucible-cli/AGENTS.md`

---

- [x] 1. Fix RuntimeHandler to Store Function Reference

  **What to do**:
  - Add `RegistryKey` to mlua imports in `handlers.rs`
  - Create new struct `RuntimeHandlerWithFunction` that includes `RegistryKey` (can't derive Clone/Debug)
  - Or: Add companion `HashMap<String, RegistryKey>` field to `LuaScriptHandlerRegistry`
  - Update `register_crucible_on_api` to store function via `lua.create_registry_value(handler)`
  - Remove underscore prefix from `_handler` parameter at line 572

  **Must NOT do**:
  - Don't modify `RuntimeHandler` struct directly if it breaks Clone/Debug derives elsewhere

  **Parallelizable**: NO (foundation for all other tasks)

  **References**:
  - `crates/crucible-lua/src/handlers.rs:572` - Current broken implementation
  - `crates/crucible-lua/src/handlers.rs:332-341` - RuntimeHandler struct
  - `crates/crucible-lua/src/handlers.rs:328-329` - runtime_handlers field
  - mlua docs: `Lua::create_registry_value()` stores value, returns `RegistryKey`
  - mlua docs: `Lua::registry_value::<Function>(&key)` retrieves stored function

  **Acceptance Criteria**:
  - [ ] Test: Register handler with `cru.on()`, verify function is stored (not just metadata)
  - [ ] Test: Retrieve stored function via registry key
  - [ ] `cargo test -p crucible-lua runtime_handler` → PASS

  **Commit**: YES
  - Message: `fix(lua): store function reference in cru.on() runtime handlers`
  - Files: `crates/crucible-lua/src/handlers.rs`
  - Pre-commit: `cargo test -p crucible-lua`

---

- [x] 2. Add Runtime Handler Execution Method

  **What to do**:
  - Add method `execute_runtime_handler(&self, lua: &Lua, name: &str, event: &SessionEvent) -> LuaResult<ScriptHandlerResult>`
  - Retrieve function from registry using stored key
  - Convert event to Lua table (reuse `session_event_to_lua`)
  - Call function with (ctx, event)
  - Parse result with `interpret_handler_result`

  **Must NOT do**:
  - Don't duplicate event conversion logic - reuse existing functions

  **Parallelizable**: NO (depends on Task 1)

  **References**:
  - `crates/crucible-lua/src/handlers.rs:167-215` - `LuaScriptHandler::execute()` pattern to follow
  - `crates/crucible-lua/src/handlers.rs:596-620` - `session_event_to_lua()` function
  - `crates/crucible-lua/src/handlers.rs:276-298` - `interpret_handler_result()` function

  **Acceptance Criteria**:
  - [ ] Test: Register handler, execute it, verify it receives event and returns result
  - [ ] Test: Handler returning `{ cancel = true }` produces `ScriptHandlerResult::Cancel`
  - [ ] `cargo test -p crucible-lua execute_runtime` → PASS

  **Commit**: YES
  - Message: `feat(lua): add execution method for runtime-registered handlers`
  - Files: `crates/crucible-lua/src/handlers.rs`
  - Pre-commit: `cargo test -p crucible-lua`

---

- [x] 2.5. Add Event Dispatch Loop in Daemon (CRITICAL - Routes Events to Handlers)

  **What to do**:
  - In daemon's `agent_manager.rs`, after emitting `message_complete`:
    1. Create `SessionEvent::Custom { name: "turn:complete", payload }`
    2. Get session's registered handlers: `session.runtime_handlers_for("turn:complete")`
    3. Execute each handler in priority order with event
    4. Collect results (Transform/Cancel/Inject/PassThrough)
  - Handlers are **session-scoped** (registered per-session, not global)
  - Add method `runtime_handlers_for(&self, event_type: &str) -> Vec<&RuntimeHandler>` to session/registry
  - Execute handlers in priority order (lower priority number = runs first)

  **Architecture Note**:
  - CLI/TUI are just presentation layers - they don't run handler logic
  - Daemon owns session state including registered handlers
  - This ensures all clients see consistent behavior

  **Must NOT do**:
  - Don't process Inject results here (that's Task 3.5)
  - Don't put handler execution in CLI - daemon owns this
  - Don't bridge all events yet - just `turn:complete` for MVP

  **Parallelizable**: YES (with Task 3)

  **References**:
  - `crates/crucible-daemon/src/agent_manager.rs:317` - Where `message_complete` is emitted
  - `crates/crucible-daemon/src/session_manager.rs` - Session state management
  - `crates/crucible-lua/src/handlers.rs:run_handler_chain()` - Pattern for handler execution

  **Error Handling**:
  - Log errors but continue to next handler (isolation)
  - Don't let one failing handler break the chain
  - Pattern: `tracing::error!(handler = %name, error = %e, "Handler failed")`

  **Acceptance Criteria**:
  - [ ] Test: Register handler on session, fire event, verify handler executes
  - [ ] Test: Multiple handlers execute in priority order
  - [ ] Test: Handler error doesn't crash other handlers
  - [ ] Test: Handlers are session-scoped (different sessions have different handlers)
  - [ ] `cargo test -p crucible-daemon event_dispatch` → PASS

  **Commit**: YES
  - Message: `feat(daemon): add event dispatch loop for session Lua handlers`
  - Files: `crates/crucible-daemon/src/agent_manager.rs`, `crates/crucible-lua/src/handlers.rs`
  - Pre-commit: `cargo test -p crucible-daemon`

---

- [ ] 3. Add ScriptHandlerResult::Inject Variant

  **What to do**:
  - Add new variant to `ScriptHandlerResult` enum at line 66:
    ```rust
    /// Handler wants to inject a follow-up message
    Inject {
        /// Content to inject
        content: String,
        /// Where to inject: "user_prefix" (default), "user_suffix"
        position: String,
    },
    ```
  - Update `interpret_handler_result()` to check for `inject` key in returned table:
    ```rust
    // Check for inject convention: {inject={content="...", position="..."}}
    if let Ok(inject_table) = t.get::<Table>("inject") {
        let content = inject_table.get::<String>("content")?;
        let position = inject_table.get::<String>("position")
            .unwrap_or_else(|_| "user_prefix".to_string());
        return Ok(ScriptHandlerResult::Inject { content, position });
    }
    ```

  **Must NOT do**:
  - Don't add XML formatting at Rust level
  - Don't implement the actual injection into message flow (that's event bridging, Phase 2)

  **Parallelizable**: YES (with Task 4)

  **References**:
  - `crates/crucible-lua/src/handlers.rs:59-74` - ScriptHandlerResult enum
  - `crates/crucible-lua/src/handlers.rs:276-298` - interpret_handler_result function
  - `.sisyphus/drafts/lua-api-conventions.md:101-108` - Inject return convention spec

  **Acceptance Criteria**:
  - [ ] Test: Handler returns `{inject={content="test"}}`, result is `Inject{content:"test", position:"user_prefix"}`
  - [ ] Test: Handler returns `{inject={content="test", position="user_suffix"}}`, position is preserved
  - [ ] `cargo test -p crucible-lua inject` → PASS

  **Commit**: YES
  - Message: `feat(lua): add Inject variant to ScriptHandlerResult for message injection`
  - Files: `crates/crucible-lua/src/handlers.rs`
  - Pre-commit: `cargo test -p crucible-lua`

---

- [ ] 3.5. Implement Inject Message Flow in Daemon (CRITICAL - Processes Inject Results)

  **What to do**:
  - When event dispatch (Task 2.5) returns `ScriptHandlerResult::Inject`:
    1. Queue the injected content as pending message in session state
    2. After current response completes, auto-send injected message to LLM
    3. Mark injected messages with `is_continuation: true` metadata
  - Add `pending_injection: Option<InjectedMessage>` state to `SessionAgent`
  - In agent message flow, check for pending injection after response completes
  - Emit event so CLI knows injection is happening (for UI feedback)

  **Architecture Note**:
  - Daemon processes inject results and sends to LLM
  - CLI receives events about injection (can show "Continuing..." in UI)
  - This keeps all conversation logic in daemon

  **Must NOT do**:
  - Don't allow nested injection (one inject per turn max)
  - Don't inject during active streaming (wait for completion)
  - Don't skip the pending check if multiple handlers return Inject (last one wins)

  **Parallelizable**: NO (depends on Tasks 2.5 and 3)

  **References**:
  - `crates/crucible-daemon/src/agent_manager.rs` - Agent message flow
  - `crates/crucible-core/src/session/types.rs` - SessionAgent struct
  - `.sisyphus/drafts/lua-api-conventions.md:101-108` - Inject convention spec

  **Infinite Loop Prevention**:
  - Pass `is_continuation: true` in event context for injected messages
  - Handlers MUST check: `if ctx.is_continuation then return nil end`
  - Document this pattern in plugin guidelines

  **Acceptance Criteria**:
  - [ ] Test: Handler returns `{inject={content="Continue"}}` → daemon sends "Continue" to LLM
  - [ ] Test: Injected messages include `is_continuation` context
  - [ ] Test: Second inject in same turn replaces first (no queue buildup)
  - [ ] Test: Injection only happens after response completes
  - [ ] Test: CLI receives injection event for UI feedback
  - [ ] `cargo test -p crucible-daemon inject_flow` → PASS

  **Commit**: YES
  - Message: `feat(daemon): implement inject message flow for FSM handlers`
  - Files: `crates/crucible-daemon/src/agent_manager.rs`, `crates/crucible-core/src/session/types.rs`
  - Pre-commit: `cargo test -p crucible-daemon`

---

- [ ] 4. Add cru.fmt() String Formatting Utility

  **What to do**:
  - Add Lua code that implements `cru.fmt(template, vars)` for string interpolation
  - Can be added via `lua.load()` in the initialization, or in a Lua stdlib file
  - Implementation:
    ```lua
    function cru.fmt(template, vars)
      vars = vars or {}
      return (template:gsub("{(%w+)}", function(key)
        local val = vars[key]
        if val ~= nil then
          return tostring(val)
        end
        return "{" .. key .. "}"
      end))
    end
    ```
  - Register on the `cru` table (or `crucible` table, but prefer `cru`)

  **Must NOT do**:
  - Don't add complex template features (conditionals, loops) - keep it simple

  **Parallelizable**: YES (with Task 3)

  **References**:
  - `crates/crucible-lua/src/executor.rs` - Where Lua globals are set up
  - `.sisyphus/drafts/lua-api-conventions.md:42-64` - cru.fmt() spec

  **Acceptance Criteria**:
  - [ ] Test: `cru.fmt("Hello {name}", {name="world"})` → "Hello world"
  - [ ] Test: `cru.fmt("Missing {key}", {})` → "Missing {key}" (preserves missing)
  - [ ] Test: `cru.fmt("Count: {n}", {n=42})` → "Count: 42" (numbers work)
  - [ ] `cargo test -p crucible-lua fmt` → PASS

  **Commit**: YES
  - Message: `feat(lua): add cru.fmt() string interpolation utility`
  - Files: `crates/crucible-lua/src/executor.rs` or new Lua stdlib file
  - Pre-commit: `cargo test -p crucible-lua`

---

- [ ] 5. Integration Test: Todo Enforcer Pattern

  **What to do**:
  - Create test that demonstrates the full pattern:
    1. Register handler with `cru.on("turn:complete", fn)`
    2. Handler checks event.response for incomplete todos
    3. Handler returns `{inject={content="Continue..."}}` if todos found
    4. Verify result is `ScriptHandlerResult::Inject`
  - This validates the entire chain works together

  **Must NOT do**:
  - Don't implement actual event bridging (that's daemon work, Phase 2)
  - Don't create a real plugin file yet (just in-test Lua code)

  **Parallelizable**: NO (depends on Tasks 2, 3)

  **References**:
  - `.sisyphus/drafts/lua-api-conventions.md:275-301` - Todo enforcer example
  - `.sisyphus/drafts/minimal-fsm-gaps.md:259-285` - Proof of concept code

  **Acceptance Criteria**:
  - [ ] Test demonstrates: register → execute → inject return
  - [ ] Test uses `cru.fmt()` for message formatting
  - [ ] `cargo test -p crucible-lua todo_enforcer_pattern` → PASS

  **Commit**: YES
  - Message: `test(lua): add integration test for FSM handler pattern`
  - Files: `crates/crucible-lua/src/handlers.rs` (test module)
  - Pre-commit: `cargo test -p crucible-lua`

---

## Commit Strategy

| After Task | Message | Files | Verification |
|------------|---------|-------|--------------|
| P | `refactor(cli): rename InkChatApp to OilChatTui for clarity` | tui/oil/*.rs | `cargo test -p crucible-cli` |
| 0 | `docs(cli): add AGENTS.md documenting view-only architecture` | AGENTS.md | - |
| 1 | `fix(lua): store function reference in cru.on() runtime handlers` | handlers.rs | `cargo test -p crucible-lua` |
| 2 | `feat(lua): add execution method for runtime-registered handlers` | handlers.rs | `cargo test -p crucible-lua` |
| 2.5 | `feat(daemon): add event dispatch loop for session Lua handlers` | agent_manager.rs, handlers.rs | `cargo test -p crucible-daemon` |
| 3 | `feat(lua): add Inject variant to ScriptHandlerResult` | handlers.rs | `cargo test -p crucible-lua` |
| 3.5 | `feat(daemon): implement inject message flow for FSM handlers` | agent_manager.rs, session/types.rs | `cargo test -p crucible-daemon` |
| 4 | `feat(lua): add cru.fmt() string interpolation utility` | executor.rs | `cargo test -p crucible-lua` |
| 5 | `test(lua): add integration test for FSM handler pattern` | handlers.rs | `cargo test -p crucible-lua` |

---

## Success Criteria

### Verification Commands
```bash
cargo test -p crucible-lua                    # All tests pass
cargo clippy -p crucible-lua -- -D warnings   # No warnings
cargo doc -p crucible-lua --no-deps           # Docs build
```

### Final Checklist
- [ ] `InkChatApp` renamed to `OilChatTui` (no legacy references)
- [ ] CLI architecture documented in `crates/crucible-cli/AGENTS.md`
- [ ] `cru.on()` stores function reference (not just metadata)
- [ ] Runtime handlers can be executed and return results
- [ ] Event dispatch routes `turn:complete` events to handlers (in daemon)
- [ ] `ScriptHandlerResult::Inject` variant exists and is parsed
- [ ] Inject flow sends injected messages to agent after response (in daemon)
- [ ] `cru.fmt()` works for string interpolation
- [ ] Integration test demonstrates full E2E pattern (register → event → inject → agent)
- [ ] No clippy warnings
- [ ] All tests pass

---

## Phase 2 (Future Work)

After this plan is complete, Phase 2 will add:
- Bridge additional events: `AwaitingInput` → `session:idle`, `PreLlmCall` → `message:before_send`
- `cru.state` module for persistent state (can be pure Lua)
- Session context enrichment (transcript, todos, state in handler ctx)
- `cru.once()` / `cru.off()` API for handler management
- Namespace standardization (`cru` everywhere, deprecate `crucible`)

This plan provides the **complete MVP** for Ralph loops and todo enforcer.

---

## Hidden Dependencies & Risks

### Session-Scoped Handlers (Correct Architecture)
Handlers are **session-scoped** and run in the daemon:
- All clients connected to the same session see the same handler behavior
- Handler registration is part of session state
- Injected messages are visible to all clients (consistent experience)
- Injected messages include `source: handler:name` metadata for debugging

### Error Isolation
If handler 3 of 5 fails:
- Log error, continue to next handler
- Don't let one failure break the chain
- Return partial results (e.g., if handler 2 returned Inject, use it even if 4 fails)

### Infinite Loop Prevention
Risk: Handler always returns Inject → infinite loop
- Pass `is_continuation: true` in event context
- Handlers check `if ctx.is_continuation then return nil end`
- Max 1 inject per turn (no nesting)
