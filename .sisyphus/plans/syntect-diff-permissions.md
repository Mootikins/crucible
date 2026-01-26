# Pre-Approval Workflow with Inline Diff Preview

## Context

### Original Request
Thoroughly plan the entire syntect/file edit/diff work, including remaining work to create permission prompts and integrate them with tool calls. Include invariant testing, PTY testing, and focus on usability.

### Interview Summary
**Key Discussions**:
- Primary Goal: Pre-approval workflow (like Claude Code) for destructive tools
- Permission UX: Inline diff preview ABOVE input area, NOT full-screen modals
- Tools Requiring Permission: File writes, bash commands, deletes (all except read-only)
- Configurability: vim-style defaults via config + `:set` + Lua hooks
- Pattern Storage: `whitelists.d/` in `~/.config/crucible/` or `~/.local/share/crucible/`
- Test Strategy: RPC→TUI, Data→RPC, snapshots, integration tests

**Research Findings**:
- Syntect highlighter: `crates/crucible-cli/src/formatting/syntax.rs` (FULLY INTEGRATED)
- Diff rendering: `crates/crucible-cli/src/tui/oil/diff.rs` (TWO-LAYER ARCHITECTURE)
- Permission types: `crates/crucible-core/src/interaction.rs` (EXISTS BUT NOT WIRED)
- PreToolCall event: `crates/crucible-core/src/events/session_event.rs` (EXISTS BUT DOESN'T EMIT PermRequest)
- Test harness: `AppHarness`, `insta` snapshots, `expectrl` PTY tests

### Metis Review
**Identified Gaps** (addressed):
- Tool execution pause mechanism doesn't exist (~40% of work): ADDRESSED in Phase 1
- "Inline" definition unclear: RESOLVED — diff panel appears ABOVE input area
- Timeout behavior: RESOLVED — block forever until response
- Multiple permissions: RESOLVED — queue and show one at a time
- Denial feedback: RESOLVED — return ToolError::PermissionDenied

**Architectural Constraint** (critical):
Per AGENTS.md: "CLI/TUI IS A VIEW LAYER ONLY" — permission enforcement MUST happen daemon-side in `AgentManager`, not TUI-side.

---

## Work Objectives

### Core Objective
Create a pre-approval workflow where destructive tool calls (file writes, bash commands, deletes) require explicit user confirmation via an inline syntax-highlighted diff preview, with configurable allow-patterns for efficiency.

### Concrete Deliverables
1. Daemon-side permission gate in `AgentManager::execute_tool()` with async blocking
2. Inline diff preview component rendered above input area
3. Syntax-highlighted diffs using existing `SyntaxHighlighter`
4. Pattern storage system in `whitelists.d/` with per-project support
5. vim-style `:set` commands for runtime configuration
6. Comprehensive test suite (unit, snapshot, integration, PTY)

### Definition of Done
- [ ] Destructive tools ALWAYS prompt for permission (unless whitelisted)
- [ ] `cargo test -p crucible-cli --features test-utils` passes
- [ ] `cargo test -p crucible-daemon` passes
- [ ] Invariant tests pass (no writes without consent, escape = deny, diff accuracy)
- [ ] Snapshot tests capture all permission prompt states
- [ ] PTY E2E test verifies real terminal interaction

### Must Have
- Daemon-side blocking permission gate (not TUI-side)
- Inline diff preview above input area
- Syntax highlighting in diffs
- y/n/p keybindings (allow, deny, pattern)
- ToolError::PermissionDenied returned to LLM on denial
- Pattern persistence across sessions
- Queue for multiple pending permissions

### Must NOT Have (Guardrails)
- NO permission enforcement in TUI (daemon only)
- NO auto-approve by default (must be explicit whitelist)
- NO timeout-based auto-approve (security risk)
- NO wildcard patterns starting with `*` (too permissive)
- NO undo/rollback system (out of scope)
- NO web UI implementation (TUI only)
- NO full-screen modals (inline only)
- NO diffs larger than 500 lines without truncation

---

## Verification Strategy (MANDATORY)

### Test Decision
- **Infrastructure exists**: YES (three-tier system)
- **User wants tests**: TDD + comprehensive coverage
- **Framework**: Cargo test with nextest, insta for snapshots, expectrl for PTY

### Test Layers

| Layer | What | How |
|-------|------|-----|
| **Unit** | Permission gate logic, pattern matching | `AppHarness`, mock daemon |
| **Snapshot** | Diff preview rendering, modal states | `insta::assert_snapshot!` |
| **Integration** | RPC→TUI flow, Data→RPC flow | Mock daemon client |
| **PTY E2E** | Real terminal interaction | `expectrl`, `TuiTestBuilder` |
| **Invariant** | Security properties | Property-based tests |

### Invariant Tests (CRITICAL)

```rust
// These invariants MUST be tested and MUST pass
#[test] fn invariant_no_writes_without_consent() { ... }
#[test] fn invariant_escape_always_denies() { ... }
#[test] fn invariant_diff_accuracy() { ... }
#[test] fn invariant_pattern_persistence() { ... }
```

---

## Task Flow

```
Phase 1 (Core Infrastructure)
  ├── 1. Permission channel in daemon
  ├── 2. Wire PreToolCall to PermRequest
  ├── 3. Block execution until response
  └── 4. Return PermissionDenied on deny

Phase 2 (UI Components)
  ├── 5. Inline diff preview component
  ├── 6. Syntax-highlighted diff rendering
  ├── 7. Keybinding handlers (y/n/p/h)
  └── 8. Queue UI for multiple pending

Phase 3 (Pattern System)
  ├── 9. Pattern storage in whitelists.d/
  ├── 10. Pattern matching logic
  ├── 11. :set command integration
  └── 12. Lua configuration hooks

Phase 4 (Testing)
  ├── 13. Unit tests for permission gate
  ├── 14. Snapshot tests for diff preview
  ├── 15. Integration tests for full flow
  ├── 16. Invariant tests
  └── 17. PTY E2E tests
```

## Parallelization

| Group | Tasks | Reason |
|-------|-------|--------|
| A | 5, 6, 7, 8 | UI components after core is stable |
| B | 9, 10, 11, 12 | Pattern system after core is stable |
| C | 13, 14, 15, 16, 17 | Tests can be written alongside each phase |

| Task | Depends On | Reason |
|------|------------|--------|
| 2 | 1 | Need channel before wiring |
| 3 | 2 | Need PermRequest before blocking |
| 4 | 3 | Need blocking before deny handling |
| 5-8 | 4 | UI needs working permission flow |
| 9-12 | 4 | Patterns need working permission flow |
| 17 | 5-12 | PTY tests need complete feature |

---

## TODOs

### Phase 1: Core Infrastructure

- [ ] 1. Create permission response channel in daemon

  **What to do**:
  - Add `tokio::sync::oneshot` channel to `AgentManager` for permission responses
  - Define `PermissionRequest` and `PermissionResponse` types for internal daemon use
  - Create `pending_permissions: HashMap<PermissionId, oneshot::Sender<PermissionResponse>>` in session state
  - Implement `await_permission(request) -> PermissionResponse` method

  **Must NOT do**:
  - Do NOT put any permission logic in TUI code
  - Do NOT use unbounded channels (oneshot only)
  - Do NOT store permission state globally (session-scoped only)

  **Parallelizable**: NO (foundation for all other tasks)

  **References**:
  - `crates/crucible-daemon/src/agent_manager.rs` - AgentManager implementation
  - `crates/crucible-daemon/src/session_manager.rs` - Session state management
  - `crates/crucible-core/src/interaction.rs:PermRequest` - Permission request types
  - `crates/crucible-core/src/interaction.rs:PermResponse` - Permission response types

  **Acceptance Criteria**:
  - [ ] `PermissionId` type created for tracking pending permissions
  - [ ] `oneshot::channel` created per permission request
  - [ ] `await_permission()` blocks until response received
  - [ ] Unit test: `await_permission` returns when response sent

  **Commit**: YES
  - Message: `feat(daemon): add permission response channel for tool execution`
  - Files: `crates/crucible-daemon/src/agent_manager.rs`, `crates/crucible-daemon/src/session_manager.rs`

---

- [ ] 2. Wire PreToolCall event to emit PermissionRequest

  **What to do**:
  - Modify tool execution flow in `AgentManager::execute_tool()` to emit `PreToolCall` event
  - Create handler that converts `PreToolCall` to `PermissionRequest` for destructive tools
  - Categorize tools: `is_destructive(tool_name) -> bool` (write, bash, delete = true)
  - Emit `SessionEvent::InteractionRequested(Permission(request))` to TUI

  **Must NOT do**:
  - Do NOT prompt for read-only tools
  - Do NOT modify existing `PreToolCall` event type structure
  - Do NOT bypass permission for any tool the agent claims is "safe"

  **Parallelizable**: NO (depends on task 1)

  **References**:
  - `crates/crucible-daemon/src/agent_manager.rs:execute_tool()` - Tool execution entry point
  - `crates/crucible-core/src/events/session_event.rs:PreToolCall` - Pre-event definition (lines 118-123)
  - `crates/crucible-core/src/events/session_event.rs:InteractionRequested` - Interaction event
  - `crates/crucible-core/src/interaction.rs:PermAction` - Permission action types (Bash, Read, Write, Tool)

  **Acceptance Criteria**:
  - [ ] `is_destructive("write")` returns `true`
  - [ ] `is_destructive("bash")` returns `true`
  - [ ] `is_destructive("read_note")` returns `false`
  - [ ] Write tool emits `InteractionRequested(Permission(...))` before execution
  - [ ] Unit test: destructive tool triggers permission event

  **Commit**: YES
  - Message: `feat(daemon): emit permission requests for destructive tools`
  - Files: `crates/crucible-daemon/src/agent_manager.rs`

---

- [ ] 3. Implement execution blocking until permission response

  **What to do**:
  - In `execute_tool()`, after emitting permission request, await on oneshot channel
  - Handle response: `PermResponse::Allow` → continue execution, `PermResponse::Deny` → return error
  - Handle `PermResponse::AllowPattern` → store pattern, then continue execution
  - Implement infinite timeout (block forever until user responds)

  **Must NOT do**:
  - Do NOT add timeout-based auto-approve
  - Do NOT auto-deny on timeout
  - Do NOT continue execution if channel is dropped (treat as deny)

  **Parallelizable**: NO (depends on task 2)

  **References**:
  - `crates/crucible-daemon/src/agent_manager.rs:execute_tool()` - Where to add await
  - `crates/crucible-core/src/interaction.rs:PermResponse` - Response variants (Allow, Deny, AllowPattern)
  - Pattern: `tokio::select!` with channel recv vs cancellation

  **Acceptance Criteria**:
  - [ ] Tool execution blocks until permission response received
  - [ ] `PermResponse::Allow` allows execution to proceed
  - [ ] `PermResponse::Deny` prevents execution
  - [ ] Channel drop treated as deny (safety)
  - [ ] Unit test: execution waits for response
  - [ ] Unit test: allow response continues execution
  - [ ] Unit test: deny response blocks execution

  **Commit**: YES
  - Message: `feat(daemon): block tool execution until permission granted`
  - Files: `crates/crucible-daemon/src/agent_manager.rs`

---

- [ ] 4. Return ToolError::PermissionDenied on denial

  **What to do**:
  - When permission denied, return `ToolError::PermissionDenied(reason)` to LLM
  - Format reason clearly: "User denied permission to write to {path}"
  - Ensure error propagates through agent response stream
  - LLM receives error as tool result (not silent skip)

  **Must NOT do**:
  - Do NOT silently skip the tool call
  - Do NOT retry automatically
  - Do NOT expose internal details in error message

  **Parallelizable**: NO (depends on task 3)

  **References**:
  - `crates/crucible-core/src/traits/tools.rs:ToolError::PermissionDenied` - Error variant
  - `crates/crucible-daemon/src/agent_manager.rs` - Error return path
  - `crates/crucible-cli/src/tui/oil/chat_app.rs:ChatAppMsg::ToolResultError` - TUI error handling

  **Acceptance Criteria**:
  - [ ] Denial returns `ToolError::PermissionDenied` with clear message
  - [ ] Error reaches LLM as tool result
  - [ ] TUI displays error via `ToolResultError` message
  - [ ] Unit test: denial produces PermissionDenied error
  - [ ] Integration test: LLM receives error in context

  **Commit**: YES
  - Message: `feat(daemon): return PermissionDenied error to LLM on denial`
  - Files: `crates/crucible-daemon/src/agent_manager.rs`, `crates/crucible-core/src/traits/tools.rs`

---

### Phase 2: UI Components

- [ ] 5. Create inline diff preview component

  **What to do**:
  - Create `DiffPreview` component that renders ABOVE input area
  - Use existing `diff_to_node()` from `tui/oil/diff.rs` as foundation
  - Add collapsible state (keybind to hide/show diff)
  - Show file path, action type (write/create/delete), and diff content
  - For new files: show all lines as additions with "[new file]" header
  - For deletes: show all lines as deletions with "[deleting file]" header

  **Must NOT do**:
  - Do NOT create full-screen modal (inline only)
  - Do NOT show diffs larger than 500 lines without truncation
  - Do NOT add editing capabilities to diff (read-only view)

  **Parallelizable**: YES (with 6, 7, 8 after task 4)

  **References**:
  - `crates/crucible-cli/src/tui/oil/diff.rs:diff_to_node()` - Existing diff rendering
  - `crates/crucible-cli/src/tui/oil/diff.rs:diff_to_node_with_word_highlight()` - Word-level diffing
  - `crates/crucible-cli/src/tui/oil/chat_app.rs:render_ask_interaction()` - Pattern for inline rendering (lines 2584+)
  - `crates/crucible-cli/src/tui/oil/theme.rs:diff_*()` - Diff styling functions

  **Acceptance Criteria**:
  - [ ] DiffPreview component renders above input area
  - [ ] Shows file path and action type
  - [ ] New files show all green with "[new file]" header
  - [ ] Deleted files show all red with "[deleting file]" header
  - [ ] Keybind (h) collapses/expands diff
  - [ ] Long diffs (>500 lines) truncated with "... N more lines"
  - [ ] Snapshot test: diff preview rendering

  **Commit**: YES
  - Message: `feat(tui): add inline diff preview component for permission prompts`
  - Files: `crates/crucible-cli/src/tui/oil/components/diff_preview.rs`, `crates/crucible-cli/src/tui/oil/components/mod.rs`

---

- [ ] 6. Add syntax highlighting to diff preview

  **What to do**:
  - Integrate `SyntaxHighlighter` from `formatting/syntax.rs` with diff rendering
  - Detect language from file extension
  - Apply syntax highlighting to diff content (both old and new lines)
  - Preserve diff coloring (red/green) as background, syntax colors as foreground

  **Must NOT do**:
  - Do NOT highlight binary files
  - Do NOT require full file for highlighting (line-by-line is fine)
  - Do NOT add LSP or semantic highlighting (syntect only)

  **Parallelizable**: YES (with 5, 7, 8)

  **References**:
  - `crates/crucible-cli/src/formatting/syntax.rs:SyntaxHighlighter` - Existing highlighter
  - `crates/crucible-cli/src/formatting/syntax.rs:highlight()` - Highlighting method
  - `crates/crucible-cli/src/tui/oil/markdown.rs:render_highlighted_code()` - Integration pattern (line 540)
  - Syntect docs for incremental highlighting

  **Acceptance Criteria**:
  - [ ] Diff lines have syntax highlighting based on file extension
  - [ ] `.rs` files get Rust highlighting
  - [ ] `.ts` files get TypeScript highlighting
  - [ ] Unknown extensions get no syntax highlighting (plain text)
  - [ ] Diff colors (red/green) preserved as background
  - [ ] Snapshot test: syntax-highlighted diff

  **Commit**: YES
  - Message: `feat(tui): add syntax highlighting to diff preview`
  - Files: `crates/crucible-cli/src/tui/oil/components/diff_preview.rs`

---

- [ ] 7. Implement permission keybinding handlers

  **What to do**:
  - Add keybindings for permission response: y (allow), n (deny), p (pattern), h (toggle diff), Esc (deny)
  - Create `PermissionState` in `InkChatApp` to track current permission prompt
  - Send response via RPC when user presses y/n/p
  - Clear permission state and return to normal input after response
  - For 'p': prompt for pattern input (mini-input mode)

  **Must NOT do**:
  - Do NOT execute on any key other than y (allow requires explicit consent)
  - Do NOT allow typing in main input while permission pending
  - Do NOT auto-approve on Enter (explicit y required)

  **Parallelizable**: YES (with 5, 6, 8)

  **References**:
  - `crates/crucible-cli/src/tui/oil/chat_app.rs:handle_perm_key()` - Existing handler (lines 2117+)
  - `crates/crucible-cli/src/tui/oil/chat_app.rs:InteractionModalState` - Current state tracking
  - `crates/crucible-daemon-client/src/client.rs` - RPC methods for permission response
  - Pattern: Follow existing `handle_ask_key()` structure

  **Acceptance Criteria**:
  - [ ] 'y' sends `PermResponse::Allow` via RPC
  - [ ] 'n' sends `PermResponse::Deny` via RPC
  - [ ] 'Esc' sends `PermResponse::Deny` via RPC
  - [ ] 'p' enters pattern input mode
  - [ ] 'h' toggles diff visibility
  - [ ] Other keys are ignored (no accidental approval)
  - [ ] Unit test: each keybinding triggers correct response
  - [ ] Snapshot test: permission prompt with keybinding hints

  **Commit**: YES
  - Message: `feat(tui): implement permission keybinding handlers`
  - Files: `crates/crucible-cli/src/tui/oil/chat_app.rs`

---

- [ ] 8. Implement permission queue for multiple pending requests

  **What to do**:
  - Add `permission_queue: VecDeque<PermissionRequest>` to `InkChatApp`
  - When new permission arrives while one is pending, add to queue
  - After responding to current, show next from queue
  - Show queue indicator: "[1/3]" when multiple pending
  - Allow Escape to deny ALL (prompt: "Deny all N pending? [y/n]")

  **Must NOT do**:
  - Do NOT show all permissions at once (one at a time)
  - Do NOT batch approve without explicit user action
  - Do NOT lose queue on TUI restart (daemon owns queue)

  **Parallelizable**: YES (with 5, 6, 7)

  **References**:
  - `crates/crucible-cli/src/tui/oil/chat_app.rs:InteractionModalState` - Extend with queue
  - `crates/crucible-daemon/src/session_manager.rs` - Daemon-side queue management
  - Pattern: Similar to notification queue

  **Acceptance Criteria**:
  - [ ] Multiple permissions queue properly
  - [ ] Queue indicator shows position: "[1/3]"
  - [ ] After response, next permission shown automatically
  - [ ] Escape on last item denies it
  - [ ] Unit test: queue ordering preserved
  - [ ] Snapshot test: queue indicator rendering

  **Commit**: YES
  - Message: `feat(tui): add permission queue for multiple pending requests`
  - Files: `crates/crucible-cli/src/tui/oil/chat_app.rs`, `crates/crucible-daemon/src/session_manager.rs`

---

### Phase 3: Pattern System

- [ ] 9. Create pattern storage in whitelists.d/

  **What to do**:
  - Create `~/.config/crucible/whitelists.d/` directory structure
  - Define pattern file format: `<project-hash>.toml` per project
  - Pattern types: `bash_commands`, `file_paths`, `tool_names`
  - Implement `PatternStore` with load/save/merge operations
  - Support hot-reload on file changes (via `notify` crate)

  **Must NOT do**:
  - Do NOT store raw secrets in pattern files (sanitize bash args)
  - Do NOT allow patterns starting with `*` (too permissive)
  - Do NOT create project folder proliferation (central storage only)

  **Parallelizable**: YES (with 10, 11, 12 after task 4)

  **References**:
  - `crates/crucible-config/src/security.rs:ShellPolicy` - Existing pattern model
  - `crates/crucible-config/src/lib.rs` - Config loading patterns
  - XDG spec for config directory detection
  - Format inspiration: `/etc/sudoers.d/`

  **Pattern File Format**:
  ```toml
  # ~/.config/crucible/whitelists.d/<project-hash>.toml
  [bash_commands]
  allowed_prefixes = ["npm install", "cargo build", "git "]

  [file_paths]
  allowed_prefixes = ["src/", "tests/"]

  [tools]
  always_allow = ["read_note", "text_search"]
  ```

  **Acceptance Criteria**:
  - [ ] `whitelists.d/` created on first pattern save
  - [ ] Patterns load from correct project file
  - [ ] Invalid patterns rejected with clear error
  - [ ] Hot-reload works when file changes
  - [ ] Unit test: load/save round-trip
  - [ ] Unit test: merge multiple pattern sources

  **Commit**: YES
  - Message: `feat(config): add whitelists.d/ pattern storage`
  - Files: `crates/crucible-config/src/patterns.rs`, `crates/crucible-config/src/lib.rs`

---

- [ ] 10. Implement pattern matching logic

  **What to do**:
  - Create `PatternMatcher` that checks if tool call matches allow-pattern
  - Support prefix matching for paths and commands
  - Support exact matching for tool names
  - Integrate with permission flow: skip prompt if pattern matches
  - Priority: deny patterns > allow patterns (safety)

  **Must NOT do**:
  - Do NOT support regex (too complex, security risk)
  - Do NOT support glob wildcards beyond path prefix
  - Do NOT allow empty patterns to match everything

  **Parallelizable**: YES (with 9, 11, 12)

  **References**:
  - `crates/crucible-config/src/security.rs:ShellPolicy::is_allowed()` - Existing prefix matching
  - `crates/crucible-daemon/src/agent_manager.rs` - Integration point
  - Pattern: Follow `ShellPolicy.allowed_prefixes` matching

  **Acceptance Criteria**:
  - [ ] `matches("npm install foo", ["npm install"])` returns `true`
  - [ ] `matches("rm -rf /", ["npm install"])` returns `false`
  - [ ] `matches("src/lib.rs", ["src/"])` returns `true`
  - [ ] Empty pattern `""` never matches
  - [ ] Pattern `"*"` rejected on add
  - [ ] Unit test: all matching scenarios

  **Commit**: YES
  - Message: `feat(config): implement pattern matching for permissions`
  - Files: `crates/crucible-config/src/patterns.rs`

---

- [ ] 11. Add `:set` command for runtime configuration

  **What to do**:
  - Add `:set perm.<option>=<value>` command to TUI
  - Options: `perm.autoconfirm_session=true` (allow all for session)
  - Options: `perm.show_diff=true/false` (default diff visibility)
  - Send config changes to daemon via RPC
  - Changes take effect immediately, persist for session only

  **Must NOT do**:
  - Do NOT persist `:set` changes across restart (session only)
  - Do NOT allow `:set perm.autoconfirm_always=true` (too dangerous)
  - Do NOT modify config.toml from `:set` (manual edit only)

  **Parallelizable**: YES (with 9, 10, 12)

  **References**:
  - `crates/crucible-cli/src/tui/oil/chat_app.rs` - Command handling
  - `crates/crucible-daemon-client/src/client.rs` - RPC for config changes
  - `crates/crucible-core/src/config.rs` - Runtime config types
  - Vim `:set` documentation for UX inspiration

  **Acceptance Criteria**:
  - [ ] `:set perm.show_diff=false` hides diff by default
  - [ ] `:set perm.autoconfirm_session=true` skips prompts for session
  - [ ] `:set perm.autoconfirm_session=false` re-enables prompts
  - [ ] Changes apply immediately
  - [ ] Changes don't persist after restart
  - [ ] Unit test: `:set` parsing and application

  **Commit**: YES
  - Message: `feat(tui): add :set command for permission configuration`
  - Files: `crates/crucible-cli/src/tui/oil/chat_app.rs`, `crates/crucible-daemon-client/src/client.rs`

---

- [ ] 12. Add Lua configuration hooks

  **What to do**:
  - Add `crucible.permissions.on_request(callback)` Lua hook
  - Callback receives: tool name, args, file path
  - Callback can return: `{allow=true}`, `{deny=true}`, or nil (show prompt)
  - Load hooks from `~/.config/crucible/init.lua` or kiln's `.crucible/init.lua`
  - Hooks execute daemon-side, not TUI-side

  **Must NOT do**:
  - Do NOT execute Lua hooks in TUI (daemon only)
  - Do NOT allow Lua to bypass pattern safety checks
  - Do NOT block daemon event loop on slow Lua execution

  **Parallelizable**: YES (with 9, 10, 11)

  **References**:
  - `crates/crucible-lua/src/handlers.rs` - Lua handler registration
  - `crates/crucible-lua/src/session.rs` - Session-level handler execution
  - `crates/crucible-daemon/src/agent_manager.rs` - Integration point
  - Pattern: Follow existing `crucible.on("pre_tool_call", fn)` pattern

  **Acceptance Criteria**:
  - [ ] Lua hook receives tool info on permission check
  - [ ] `{allow=true}` skips prompt and allows
  - [ ] `{deny=true}` skips prompt and denies
  - [ ] `nil` return shows normal prompt
  - [ ] Hook timeout (1s) prevents blocking
  - [ ] Unit test: hook callback execution
  - [ ] Integration test: hook affects permission flow

  **Commit**: YES
  - Message: `feat(lua): add permissions.on_request hook for custom logic`
  - Files: `crates/crucible-lua/src/handlers.rs`, `crates/crucible-daemon/src/agent_manager.rs`

---

### Phase 4: Testing

- [ ] 13. Write unit tests for permission gate

  **What to do**:
  - Test `is_destructive()` for all tool categories
  - Test permission channel creation and response handling
  - Test `await_permission()` blocking behavior
  - Test `PermissionDenied` error propagation
  - Use mock daemon and mock channels

  **Must NOT do**:
  - Do NOT require real daemon for unit tests
  - Do NOT require real LLM for unit tests
  - Do NOT skip edge cases (channel drop, etc.)

  **Parallelizable**: YES (with 14, 15, 16, 17)

  **References**:
  - `crates/crucible-daemon/tests/` - Existing daemon tests
  - `crates/crucible-core/src/test_support/` - Test utilities
  - Pattern: Follow existing daemon test patterns

  **Acceptance Criteria**:
  - [ ] `is_destructive()` tests for all tool types
  - [ ] Permission channel creation test
  - [ ] Allow response test
  - [ ] Deny response test
  - [ ] Channel drop = deny test
  - [ ] Error propagation test
  - [ ] All tests pass with `cargo test -p crucible-daemon`

  **Commit**: YES
  - Message: `test(daemon): add unit tests for permission gate`
  - Files: `crates/crucible-daemon/tests/permission_tests.rs`

---

- [ ] 14. Write snapshot tests for diff preview

  **What to do**:
  - Snapshot test: empty diff (no changes)
  - Snapshot test: new file creation (all green)
  - Snapshot test: file deletion (all red)
  - Snapshot test: file modification (mixed red/green)
  - Snapshot test: long diff with truncation
  - Snapshot test: syntax-highlighted diff
  - Snapshot test: permission prompt with keybinding hints
  - Snapshot test: queue indicator "[1/3]"

  **Must NOT do**:
  - Do NOT use ANSI codes in snapshots (strip with `strip_ansi()`)
  - Do NOT test with real files (use fixture content)
  - Do NOT skip terminal size variations

  **Parallelizable**: YES (with 13, 15, 16, 17)

  **References**:
  - `crates/crucible-cli/src/tui/oil/tests/chat_app_snapshot_tests.rs` - Existing snapshots
  - `crates/crucible-cli/src/tui/oil/tests/helpers.rs` - Test helpers
  - Pattern: `insta::assert_snapshot!(render_and_strip(&app, 80))`

  **Acceptance Criteria**:
  - [ ] Snapshot: diff preview new file
  - [ ] Snapshot: diff preview modification
  - [ ] Snapshot: diff preview deletion
  - [ ] Snapshot: diff preview truncated
  - [ ] Snapshot: diff preview with syntax highlighting
  - [ ] Snapshot: permission prompt keybindings
  - [ ] Snapshot: queue indicator
  - [ ] All snapshots pass with `cargo test -p crucible-cli -- snapshot`

  **Commit**: YES
  - Message: `test(tui): add snapshot tests for diff preview and permission prompts`
  - Files: `crates/crucible-cli/src/tui/oil/tests/permission_snapshot_tests.rs`

---

- [ ] 15. Write integration tests for full permission flow

  **What to do**:
  - Test: RPC message triggers permission prompt in TUI
  - Test: TUI response reaches daemon via RPC
  - Test: Allow response continues tool execution
  - Test: Deny response returns error to LLM
  - Test: Pattern match skips prompt
  - Use mock daemon client with captured messages

  **Must NOT do**:
  - Do NOT require real daemon (use mocks)
  - Do NOT test LLM behavior (mock tool calls)
  - Do NOT rely on timing (use synchronization)

  **Parallelizable**: YES (with 13, 14, 16, 17)

  **References**:
  - `crates/crucible-cli/src/tui/oil/tests/chat_app_interaction_tests.rs` - Existing integration tests
  - `crates/crucible-daemon-client/src/test_support/` - Client mocks
  - Pattern: Follow existing chat_runner tests

  **Acceptance Criteria**:
  - [ ] RPC → TUI rendering test
  - [ ] TUI → RPC response test
  - [ ] Full allow flow test
  - [ ] Full deny flow test
  - [ ] Pattern skip test
  - [ ] Queue handling test
  - [ ] All tests pass with `cargo test -p crucible-cli -- integration`

  **Commit**: YES
  - Message: `test(tui): add integration tests for permission flow`
  - Files: `crates/crucible-cli/src/tui/oil/tests/permission_integration_tests.rs`

---

- [ ] 16. Write invariant tests for security properties

  **What to do**:
  - **Invariant 1**: No writes without consent - tool never executes if denied
  - **Invariant 2**: Escape always denies - Esc key never allows execution
  - **Invariant 3**: Diff accuracy - shown diff equals actual file change
  - **Invariant 4**: Pattern persistence - patterns survive session restart
  - Use property-based testing (proptest) for invariant 1 and 3
  - Use exhaustive testing for invariant 2 (all key codes)

  **Must NOT do**:
  - Do NOT mark invariant tests as `#[ignore]`
  - Do NOT skip edge cases (empty files, binary, etc.)
  - Do NOT assume happy path only

  **Parallelizable**: YES (with 13, 14, 15, 17)

  **References**:
  - `proptest` crate for property-based testing
  - `crates/crucible-cli/src/tui/oil/tests/generators.rs` - Existing generators
  - Pattern: `proptest!` macro with custom generators

  **Acceptance Criteria**:
  - [ ] `invariant_no_writes_without_consent` passes
  - [ ] `invariant_escape_always_denies` passes
  - [ ] `invariant_diff_accuracy` passes
  - [ ] `invariant_pattern_persistence` passes
  - [ ] Property tests run with reasonable iterations (1000+)
  - [ ] All invariant tests in CI

  **Commit**: YES
  - Message: `test(invariant): add security property tests for permissions`
  - Files: `crates/crucible-cli/src/tui/oil/tests/permission_invariant_tests.rs`

---

- [ ] 17. Write PTY E2E tests for real terminal interaction

  **What to do**:
  - Test: Permission prompt appears on destructive tool
  - Test: 'y' key allows and continues
  - Test: 'n' key denies and shows error
  - Test: Escape key denies
  - Test: Diff renders correctly in real terminal
  - Test: Queue indicator visible with multiple pending
  - Use `TuiTestBuilder` with built binary

  **Must NOT do**:
  - Do NOT run PTY tests in CI without built binary
  - Do NOT rely on real LLM (mock with scripted responses)
  - Do NOT use short timeouts (real terminals are slow)

  **Parallelizable**: NO (depends on all other tasks)

  **References**:
  - `tests/tui_e2e_harness.rs` - PTY harness
  - `tests/tui_e2e_tests.rs` - Existing PTY tests
  - Pattern: `TuiTestBuilder::new().command("chat").timeout(30).spawn()`

  **Acceptance Criteria**:
  - [ ] PTY test: permission prompt appears
  - [ ] PTY test: 'y' allows execution
  - [ ] PTY test: 'n' denies execution
  - [ ] PTY test: Escape denies execution
  - [ ] PTY test: diff visible in terminal
  - [ ] All PTY tests pass with `cargo test -- --ignored tui_e2e`

  **Commit**: YES
  - Message: `test(e2e): add PTY tests for permission workflow`
  - Files: `tests/tui_permission_e2e_tests.rs`

---

## Commit Strategy

| After Task | Message | Files | Verification |
|------------|---------|-------|--------------|
| 1 | `feat(daemon): add permission response channel` | `crates/crucible-daemon/src/agent_manager.rs`, `crates/crucible-daemon/src/session_manager.rs` | `cargo test -p crucible-daemon` |
| 2 | `feat(daemon): emit permission requests` | `crates/crucible-daemon/src/agent_manager.rs` | `cargo test -p crucible-daemon` |
| 3 | `feat(daemon): block tool execution` | `crates/crucible-daemon/src/agent_manager.rs` | `cargo test -p crucible-daemon` |
| 4 | `feat(daemon): return PermissionDenied` | `crates/crucible-daemon/src/agent_manager.rs`, `crates/crucible-core/src/traits/tools.rs` | `cargo test -p crucible-daemon` |
| 5 | `feat(tui): add diff preview component` | `crates/crucible-cli/src/tui/oil/components/diff_preview.rs` | `cargo test -p crucible-cli` |
| 6 | `feat(tui): add syntax highlighting` | `crates/crucible-cli/src/tui/oil/components/diff_preview.rs` | `cargo test -p crucible-cli` |
| 7 | `feat(tui): permission keybindings` | `crates/crucible-cli/src/tui/oil/chat_app.rs` | `cargo test -p crucible-cli` |
| 8 | `feat(tui): permission queue` | `crates/crucible-cli/src/tui/oil/chat_app.rs`, `crates/crucible-daemon/src/session_manager.rs` | `cargo test -p crucible-cli` |
| 9 | `feat(config): whitelists.d/ storage` | `crates/crucible-config/src/patterns.rs` | `cargo test -p crucible-config` |
| 10 | `feat(config): pattern matching` | `crates/crucible-config/src/patterns.rs` | `cargo test -p crucible-config` |
| 11 | `feat(tui): :set command` | `crates/crucible-cli/src/tui/oil/chat_app.rs` | `cargo test -p crucible-cli` |
| 12 | `feat(lua): permissions hook` | `crates/crucible-lua/src/handlers.rs`, `crates/crucible-daemon/src/agent_manager.rs` | `cargo test -p crucible-lua` |
| 13-17 | `test(*): permission tests` | test files | `cargo nextest run` |

---

## Success Criteria

### Verification Commands
```bash
# All tests pass
cargo nextest run --workspace

# Invariant tests specifically
cargo test -p crucible-cli invariant

# PTY tests (requires built binary)
cargo build --release
cargo test -- --ignored tui_permission_e2e

# Check for clippy warnings
cargo clippy --workspace -- -D warnings
```

### Final Checklist
- [ ] All "Must Have" present
- [ ] All "Must NOT Have" absent
- [ ] All unit tests pass
- [ ] All snapshot tests pass
- [ ] All integration tests pass
- [ ] All invariant tests pass
- [ ] PTY E2E tests pass
- [ ] No clippy warnings
- [ ] Documentation updated
