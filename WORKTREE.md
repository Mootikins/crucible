# Worktree: feat/chat-ux-improvements

**Goal**: Replace reedline chat with ratatui inline viewport TUI
**Status**: Implementation complete (all Waves done)

## Architecture Decision

**Approach:** Ratatui inline viewport (`Viewport::Inline(8)`) instead of alternate screen

- Preserves terminal scrollback (user preference)
- Agent responses pushed up via `terminal.insert_before()`
- Fixed 8-line viewport at bottom for input + status + completion
- Fuzzy completion menus for `/commands`, `@files`, `@agents`

## Implementation Harness

**All tasks follow:** `.claude/harness/tdd-implementation.md`

- Write test first (red)
- Implement to pass (green)
- 3 retries max, then escalate to debugger agent

## Progress (Topological Waves)

### Wave 0: Setup [DONE]
| Task | Status | Test |
|------|--------|------|
| T1: Add tui-textarea dependency | [x] | Build check |
| T2: Create chat_tui module structure | [x] | Compiles |

### Wave 1: Core Components [DONE]
| Task | Status | Test |
|------|--------|------|
| T3: Inline viewport setup | [x] | `test_viewport_*` (12 tests) |
| T4: ChatInput with tui-textarea | [x] | `test_input_*` (7 tests) |
| T5: CompletionState with nucleo | [x] | `test_fuzzy_*` (15 tests) |

### Wave 2: Behaviors [DONE]
| Task | Status | Test |
|------|--------|------|
| T6: Submit behavior | [x] | `test_submit_message` |
| T7: Completion triggers | [x] | `test_*_completion_trigger` |

### Wave 3: Event Loop + Popup [DONE]
| Task | Status | Test |
|------|--------|------|
| T8: Event loop | [x] | `test_event_loop_*` (7 tests) |
| T9: Completion popup render | [x] | `test_popup_*` (10 tests) |

### Wave 4: Widgets [DONE]
| Task | Status | Test |
|------|--------|------|
| T10: Status bar widget | [x] | `test_status_*` (7 tests) |
| T11: Completion navigation | [x] | `test_completion_nav_*` (13 tests) |

### Wave 5: Multi-select [DONE]
| Task | Status | Test |
|------|--------|------|
| T12: Multi-select checkboxes | [x] | `test_multi_select_*` (12 tests) |

### Wave 6: Messages [DONE]
| Task | Status | Test |
|------|--------|------|
| T13: insert_before for messages | [x] | `test_message_*` (18 tests) |

### Wave 7: Data Sources [DONE]
| Task | Status | Test |
|------|--------|------|
| T14: Command completion source | [x] | `test_command_source` |
| T15: File completion source | [x] | `test_file_source_*` (8 tests) |
| T16: Streaming indicator | [x] | `test_status_*` (streaming) |

### Wave 8: Integration [DONE]
| Task | Status | Test |
|------|--------|------|
| T17: Connect to AgentHandle | [x] | `test_agent_*`, `test_handle_key_with_agent_*` (9 tests) |

### Wave 9: Commands [DONE]
| Task | Status | Test |
|------|--------|------|
| T18: /clear command | [x] | `test_clear_*`, `test_exit_*`, `test_quit_*` (8 tests) |

## Key Files

**Created:**
- `crates/crucible-cli/src/chat_tui/mod.rs` - Entry point, viewport setup
- `crates/crucible-cli/src/chat_tui/app.rs` - ChatApp state, event handling
- `crates/crucible-cli/src/chat_tui/input.rs` - tui-textarea wrapper
- `crates/crucible-cli/src/chat_tui/completion.rs` - Fuzzy completion with nucleo
- `crates/crucible-cli/src/chat_tui/render.rs` - Viewport rendering
- `crates/crucible-cli/src/chat_tui/widgets/popup.rs` - Completion popup
- `crates/crucible-cli/src/chat_tui/widgets/status.rs` - Status bar with streaming indicator
- `crates/crucible-cli/src/chat_tui/sources.rs` - Completion data sources

**Reuse patterns from:**
- `crates/crucible-cli/src/tui/mod.rs` - Event loop
- `crates/crucible-cli/src/tui/app.rs` - Dirty flags, state
- `crates/crucible-cli/src/interactive.rs` - FuzzyPicker (nucleo)

## Test Summary (179 tests passing)

```bash
cargo test -p crucible-cli chat_tui
```

Tests cover:
- Viewport configuration and bounds
- Input editing, cursor movement, multi-line
- Fuzzy filtering, ranking, performance
- Completion triggers and wrap-around navigation
- Submit behavior and mode switching
- Event loop, quit handling, message sending
- Popup rendering, positioning, checkboxes
- Status bar rendering, modes, streaming
- Ctrl+J/K navigation, Tab/Enter confirmation
- Command completion source conversion
- File completion source with extension filtering
- Agent integration with channel-based communication
- Local command handling (/clear, /exit, /quit)
- **KeyBindings**: resolution, layering, defaults (15 tests)
- **Convert**: crossterm to KeyPattern (14 tests)
- **CompletionSource**: injection, hot-reload (6 tests)

## References

- Architecture: `thoughts/shared/research/chat-tui-architecture_2025-12-11-1600.md`
- Implementation plan: `thoughts/shared/plans/chat-tui-implementation_2025-12-11.md`
- TDD harness: `.claude/harness/tdd-implementation.md`

## Success Criteria

- [x] Chat preserves terminal scrollback (uses Viewport::Inline)
- [x] Inline viewport stays at bottom (Viewport::Inline(8))
- [x] `/command` fuzzy completion works (unit tested)
- [x] `@file` multi-select completion works (unit tested)
- [x] Status bar shows mode + streaming
- [x] `/clear` resets conversation context
