# Lua Config Hierarchy Implementation

## Context

### Original Request
Implement configuration hierarchy for Crucible that respects:
- TOML for security/structure (project-level)
- Lua for behavior/customization (user/kiln-level)
- Session API for runtime tweaks

### Interview Summary
**Key Discussions**:
- **Nomenclature**: Project/Repo = canonical code definition, Workspace = runtime instance, Kiln = knowledge store, Session = time-bounded conversation
- **Config domains**: Security (shell whitelist, provider restrictions) is TOML-only, never overridable by Lua
- **Defaults timing**: Session creation only - running sessions unchanged
- **API design**: Hook-based (`crucible.on_session_start(fn)`) chosen over declarative tables

**Research Findings**:
- `ConfigLoader` in `config.rs` already loads user + kiln init.lua in correct order
- `SessionManager` exists but lacks hook storage
- Session API (`session_api.rs`) provides runtime configuration via channels
- CLI owns Lua execution - daemon doesn't run Lua (security boundary)

### Metis Review
**Identified Gaps** (addressed):
- **Hook storage location**: Resolved → Store in `LuaExecutor` (not `SessionManager`)
- **Hook timing**: Resolved → CLI fires hooks after session creation, before first message
- **Multi-hook support**: Resolved → Both user and kiln can register hooks, fired in order
- **Error handling**: Resolved → Hook errors logged but don't block session creation

---

## Work Objectives

### Core Objective
Add `crucible.on_session_start(fn)` hook API so user/kiln init.lua can set session defaults (temperature, max_tokens, thinking_budget) at session creation time.

### Concrete Deliverables
- `crucible.on_session_start(fn)` Lua function for hook registration
- Hook storage in `LuaExecutor` 
- Hook firing mechanism in CLI chat command
- `:config show` TUI command to inspect effective configuration

### Definition of Done
- [x] User can define `crucible.on_session_start(function(s) s.temperature = 0.5 end)` in `~/.config/crucible/init.lua`
- [x] New sessions start with temperature 0.5 (or whatever the hook sets)
- [x] Kiln init.lua can override user defaults
- [x] `:config show` displays current values and their sources

### Must Have
- Hook registration function (`on_session_start`)
- Hook firing on session creation
- Support for both user and kiln hooks (user first, kiln can override)
- Error isolation (hook failure doesn't crash session)

### Must NOT Have (Guardrails)
- **No project init.lua** - Projects use TOML only for security
- **No Lua-based security overrides** - Shell whitelist, provider restrictions are TOML-only
- **No hot-reload** - Require CLI restart to pick up init.lua changes
- **No implicit model switching** - Model remains read-only in hooks
- **No daemon-side Lua execution** - CLI is the only Lua executor
- **No async hooks** - Hooks execute synchronously at session start

---

## Verification Strategy (MANDATORY)

### Test Decision
- **Infrastructure exists**: YES (cargo test, unit tests throughout codebase)
- **User wants tests**: TDD
- **Framework**: cargo test (Rust) + inline Lua tests

### TDD Workflow

Each TODO follows RED-GREEN-REFACTOR:

**Task Structure:**
1. **RED**: Write failing test first
2. **GREEN**: Implement minimum code to pass
3. **REFACTOR**: Clean up while keeping green

**Test Commands:**
- Unit tests: `cargo test -p crucible-lua hook`
- Integration: `cargo test -p crucible-cli config`
- Full suite: `cargo nextest run --profile ci`

---

## Task Flow

```
Task 1 (Hook storage) 
    → Task 2 (Hook registration API)
        → Task 3 (Hook firing mechanism)
            → Task 4 (CLI integration)
                → Task 5 (:config show command)
```

All tasks are sequential - each depends on the previous.

## Parallelization

| Task | Depends On | Reason |
|------|------------|--------|
| 1 | - | Foundation |
| 2 | 1 | Needs storage before API |
| 3 | 2 | Needs API before firing |
| 4 | 3 | Needs firing before CLI wiring |
| 5 | 4 | Needs working hooks before inspection |

---

## TODOs

- [x] 1. Add hook storage to LuaExecutor

  **What to do**:
  - Add `on_session_start_hooks: Vec<mlua::RegistryKey>` field to `LuaExecutor`
  - Store hooks in Lua registry (survives across calls)
  - Provide method to add hooks: `fn add_session_start_hook(&self, key: RegistryKey)`
  - Provide method to get hooks: `fn session_start_hooks(&self) -> &[RegistryKey]`

  **Test (RED first)**:
  ```rust
  #[test]
  fn test_hook_storage_empty_by_default() {
      let executor = LuaExecutor::new().unwrap();
      assert!(executor.session_start_hooks().is_empty());
  }
  ```

  **Must NOT do**:
  - Don't store hooks in SessionManager (it's session-specific, hooks are global)
  - Don't use raw Function references (they don't survive scope changes)

  **Parallelizable**: NO (foundation task)

  **References**:

  **Pattern References**:
  - `crates/crucible-lua/src/executor.rs:23-28` - LuaExecutor struct definition (add new field here)
  - `crates/crucible-lua/src/executor.rs:30-65` - LuaExecutor::new() (initialize hooks vec here)

  **API/Type References**:
  - mlua::RegistryKey - Used to store Lua values that survive function calls
  - `self.lua.create_registry_value(func)` - Creates registry key from function

  **Test References**:
  - `crates/crucible-lua/src/executor.rs:328-407` - Existing executor tests (follow this pattern)

  **Acceptance Criteria**:
  - [ ] Test file: `crates/crucible-lua/src/executor.rs` (inline tests)
  - [ ] `cargo test -p crucible-lua hook_storage` → PASS
  - [ ] `LuaExecutor` has `on_session_start_hooks` field
  - [ ] Methods `add_session_start_hook()` and `session_start_hooks()` exist

  **Commit**: YES
  - Message: `feat(lua): add hook storage to LuaExecutor`
  - Files: `crates/crucible-lua/src/executor.rs`
  - Pre-commit: `cargo test -p crucible-lua`

---

- [x] 2. Implement crucible.on_session_start() Lua API

  **What to do**:
  - Create `register_hooks_module()` function in new file `crates/crucible-lua/src/hooks.rs`
  - Register `crucible.on_session_start(fn)` that stores function in executor's hook list
  - Hook function receives session object as argument: `function(session) session.temperature = 0.5 end`
  - Multiple calls append hooks (don't replace)

  **Test (RED first)**:
  ```rust
  #[test]
  fn test_on_session_start_registers_hook() {
      let executor = LuaExecutor::new().unwrap();
      executor.lua().load(r#"
          crucible.on_session_start(function(s) end)
      "#).exec().unwrap();
      assert_eq!(executor.session_start_hooks().len(), 1);
  }
  ```

  **Must NOT do**:
  - Don't validate hook signature at registration (validate at call time)
  - Don't execute hooks immediately - just store them

  **Parallelizable**: NO (depends on task 1)

  **References**:

  **Pattern References**:
  - `crates/crucible-lua/src/session_api.rs:324-346` - `register_session_module()` pattern for adding crucible.* functions
  - `crates/crucible-lua/src/config.rs:67-214` - `register_statusline_namespace()` for module registration pattern

  **API/Type References**:
  - `mlua::Function` - Type for Lua functions
  - `lua.create_function()` - Creates Rust-backed Lua function

  **Documentation References**:
  - mlua docs: Function handling and registry

  **Acceptance Criteria**:
  - [ ] New file: `crates/crucible-lua/src/hooks.rs`
  - [ ] `cargo test -p crucible-lua on_session_start` → PASS
  - [ ] Lua code `crucible.on_session_start(fn)` works
  - [ ] Multiple hooks can be registered

  **Commit**: YES
  - Message: `feat(lua): add crucible.on_session_start() hook registration`
  - Files: `crates/crucible-lua/src/hooks.rs`, `crates/crucible-lua/src/lib.rs`, `crates/crucible-lua/src/executor.rs`
  - Pre-commit: `cargo test -p crucible-lua`

---

- [x] 3. Implement hook firing mechanism

  **What to do**:
  - Add `fire_session_start_hooks(&self, session: &Session) -> Result<(), LuaError>` to `LuaExecutor`
  - Iterate through stored hooks, call each with session object
  - Log errors but continue (hook failure shouldn't block session)
  - Return Ok even if some hooks fail (isolation)

  **Test (RED first)**:
  ```rust
  #[test]
  fn test_fire_hooks_calls_registered_hooks() {
      let executor = LuaExecutor::new().unwrap();
      executor.lua().load(r#"
          test_called = false
          crucible.on_session_start(function(s) 
              test_called = true
          end)
      "#).exec().unwrap();
      
      let session = Session::new("test".to_string());
      session.bind(Box::new(MockRpc::new()));
      executor.fire_session_start_hooks(&session).unwrap();
      
      let called: bool = executor.lua().load("return test_called").eval().unwrap();
      assert!(called);
  }
  ```

  **Must NOT do**:
  - Don't panic on hook errors
  - Don't skip remaining hooks if one fails
  - Don't modify session outside of hook execution

  **Parallelizable**: NO (depends on task 2)

  **References**:

  **Pattern References**:
  - `crates/crucible-lua/src/executor.rs:210-231` - `execute_lua()` for calling Lua functions
  - `crates/crucible-lua/src/session_api.rs:192-227` - Session UserData impl for passing session to Lua

  **API/Type References**:
  - `self.lua.registry_value::<Function>(&key)` - Retrieve function from registry
  - `func.call::<_, ()>(session)` - Call function with session argument

  **Test References**:
  - `crates/crucible-lua/src/session_api.rs:416-460` - Tests using MockRpc pattern

  **Acceptance Criteria**:
  - [ ] Method `fire_session_start_hooks()` exists on `LuaExecutor`
  - [ ] `cargo test -p crucible-lua fire_hooks` → PASS
  - [ ] Hooks receive session object and can modify it
  - [ ] Hook errors are logged but don't crash

  **Commit**: YES
  - Message: `feat(lua): implement hook firing mechanism`
  - Files: `crates/crucible-lua/src/executor.rs`
  - Pre-commit: `cargo test -p crucible-lua`

---

- [x] 4. Wire hooks into CLI chat command

  **What to do**:
  - After session creation in `chat.rs`, call `executor.fire_session_start_hooks(&session)`
  - Pass session object (must be bound to RPC first)
  - Fire hooks before first user message is processed
  - Log which hooks were fired (debug level)

  **Test (RED first)**:
  Integration test in `tests/` directory:
  ```rust
  #[tokio::test]
  async fn test_init_lua_hook_sets_temperature() {
      // Create temp config dir with init.lua
      // init.lua: crucible.on_session_start(function(s) s.temperature = 0.3 end)
      // Start chat session
      // Verify temperature is 0.3
  }
  ```

  **Must NOT do**:
  - Don't fire hooks before session is bound to RPC
  - Don't fire hooks after first message (too late)
  - Don't block on hook execution (should be fast)

  **Parallelizable**: NO (depends on task 3)

  **References**:

  **Pattern References**:
  - `crates/crucible-cli/src/commands/chat.rs` - Chat command entry point
  - `crates/crucible-cli/src/tui/oil/chat_runner.rs` - Event loop where session is managed

  **API/Type References**:
  - `LuaExecutor::fire_session_start_hooks()` - From task 3
  - `Session` from `crucible_lua::session_api`

  **Documentation References**:
  - `AGENTS.md:Daemon Architecture` - CLI owns Lua execution, daemon doesn't

  **Acceptance Criteria**:
  - [ ] Integration test passes
  - [ ] `cargo test -p crucible-cli init_lua_hook` → PASS
  - [ ] User's `~/.config/crucible/init.lua` hooks fire on session start
  - [ ] Kiln's `.crucible/init.lua` hooks fire after user hooks
  - [ ] Debug log shows "Fired N session_start hooks"

  **Commit**: YES
  - Message: `feat(cli): fire session_start hooks on chat session creation`
  - Files: `crates/crucible-cli/src/commands/chat.rs`, `crates/crucible-cli/tests/` (new test)
  - Pre-commit: `cargo test -p crucible-cli`

---

- [x] 5. Implement :config show TUI command

  **What to do**:
  - Add `:config show` command to TUI
  - Display current session configuration values
  - Show source of each value (default / user init.lua / kiln init.lua / session API)
  - Format: `temperature: 0.5 (from: user init.lua)`

  **Test (RED first)**:
  ```rust
  #[test]
  fn test_config_show_command_displays_values() {
      let mut app = InkChatApp::default();
      // Set up session with known values
      app.handle_command(":config show");
      // Verify output contains expected format
  }
  ```

  **Must NOT do**:
  - Don't allow `:config set` (use `:set` for that)
  - Don't show security settings (those are TOML-only)
  - Don't show values for disconnected sessions

  **Parallelizable**: NO (depends on task 4 for meaningful sources)

  **References**:

  **Pattern References**:
  - `crates/crucible-cli/src/tui/oil/chat_app.rs` - TUI command handling
  - Existing `:set` command implementation for similar pattern

  **API/Type References**:
  - Session properties: `temperature`, `max_tokens`, `thinking_budget`, `mode`

  **Documentation References**:
  - `.sisyphus/drafts/nomenclature.md:117-125` - Config domain ownership

  **Acceptance Criteria**:
  - [ ] `:config show` command exists
  - [ ] `cargo test -p crucible-cli config_show` → PASS
  - [ ] Output shows all session-configurable values
  - [ ] Source tracking shows where each value came from

  **Manual Verification**:
  - [ ] Start `cru chat`
  - [ ] Type `:config show`
  - [ ] Verify output shows temperature, max_tokens, thinking_budget, mode
  - [ ] Each line shows the source (default/user/kiln/session)

  **Commit**: YES
  - Message: `feat(tui): add :config show command for configuration inspection`
  - Files: `crates/crucible-cli/src/tui/oil/chat_app.rs`
  - Pre-commit: `cargo test -p crucible-cli`

---

## Commit Strategy

| After Task | Message | Files | Verification |
|------------|---------|-------|--------------|
| 1 | `feat(lua): add hook storage to LuaExecutor` | executor.rs | `cargo test -p crucible-lua hook` |
| 2 | `feat(lua): add crucible.on_session_start() hook registration` | hooks.rs, lib.rs, executor.rs | `cargo test -p crucible-lua on_session_start` |
| 3 | `feat(lua): implement hook firing mechanism` | executor.rs | `cargo test -p crucible-lua fire_hooks` |
| 4 | `feat(cli): fire session_start hooks on chat session creation` | chat.rs, tests/ | `cargo test -p crucible-cli init_lua_hook` |
| 5 | `feat(tui): add :config show command for configuration inspection` | chat_app.rs | `cargo test -p crucible-cli config_show` |

---

## Success Criteria

### Verification Commands
```bash
# Run all related tests
cargo test -p crucible-lua hook
cargo test -p crucible-cli config

# Full CI check
cargo nextest run --profile ci
```

### Final Checklist
- [x] User can set defaults in `~/.config/crucible/init.lua`
- [x] Kiln can override in `{kiln}/.crucible/init.lua`
- [x] New sessions start with configured defaults
- [x] `:config show` displays effective configuration
- [x] Hook errors don't crash session creation
- [x] All tests pass
- [x] No security settings exposed to Lua
