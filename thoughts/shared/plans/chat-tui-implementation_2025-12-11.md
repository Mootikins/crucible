---
date: 2025-12-11T16:30:00Z
author: Claude
topic: "Chat TUI Implementation Plan"
tags: [plan, implementation, tui, ratatui, chat]
status: active
---

# Chat TUI Implementation Plan

## Overview

Replace the current reedline-based chat with a ratatui inline viewport TUI that preserves terminal scrollback while providing fuzzy completion menus.

## Implementation Harness

**All implementation tasks MUST follow `/home/moot/crucible/.claude/harness/tdd-implementation.md`**

---

## Task Dependency Graph

```
                    ┌─────────────────┐
                    │ T1: Add deps    │
                    └────────┬────────┘
                             │
                    ┌────────▼────────┐
                    │ T2: Module      │
                    │ structure       │
                    └────────┬────────┘
                             │
        ┌────────────────────┼────────────────────┐
        │                    │                    │
        ▼                    ▼                    ▼
┌───────────────┐   ┌───────────────┐   ┌───────────────┐
│ T3: Inline    │   │ T4: ChatInput │   │ T5: Completion│
│ viewport      │   │ (tui-textarea)│   │ State+Filter  │
└───────┬───────┘   └───────┬───────┘   └───────┬───────┘
        │                   │                   │
        │           ┌───────┴───────┐           │
        │           │               │           │
        │           ▼               │           ▼
        │   ┌───────────────┐       │   ┌───────────────┐
        │   │ T6: Submit    │       │   │ T7: Completion│
        │   │ behavior      │       │   │ triggers      │
        │   └───────┬───────┘       │   └───────┬───────┘
        │           │               │           │
        └─────┬─────┘               │           │
              │                     │           │
              ▼                     │           ▼
      ┌───────────────┐             │   ┌───────────────┐
      │ T8: Event     │             │   │ T9: Completion│
      │ loop          │             │   │ popup render  │
      └───────┬───────┘             │   └───────┬───────┘
              │                     │           │
              │                     └─────┬─────┘
              │                           │
              ▼                           ▼
      ┌───────────────┐           ┌───────────────┐
      │ T10: Status   │           │ T11: Completion│
      │ bar widget    │           │ navigation    │
      └───────┬───────┘           └───────┬───────┘
              │                           │
              │                           ▼
              │                   ┌───────────────┐
              │                   │ T12: Multi-   │
              │                   │ select        │
              │                   └───────┬───────┘
              │                           │
              └─────────────┬─────────────┘
                            │
                            ▼
                    ┌───────────────┐
                    │ T13: insert_  │
                    │ before msgs   │
                    └───────┬───────┘
                            │
           ┌────────────────┼────────────────┐
           │                │                │
           ▼                ▼                ▼
   ┌───────────────┐ ┌───────────────┐ ┌───────────────┐
   │ T14: Command  │ │ T15: File     │ │ T16: Streaming│
   │ completion    │ │ completion    │ │ indicator     │
   └───────┬───────┘ └───────┬───────┘ └───────┬───────┘
           │                │                │
           └────────────────┼────────────────┘
                            │
                            ▼
                    ┌───────────────┐
                    │ T17: Agent    │
                    │ integration   │
                    └───────┬───────┘
                            │
                            ▼
                    ┌───────────────┐
                    │ T18: /clear   │
                    │ command       │
                    └───────────────┘
```

---

## Topologically Sorted Waves (Parallelizable)

### Wave 0: Setup (Sequential)
| ID | Task | Dependencies | Test |
|----|------|--------------|------|
| T1 | Add `tui-textarea = "0.6"` to Cargo.toml | None | Build check |
| T2 | Create `chat_tui/` module structure | T1 | Compiles |

### Wave 1: Core Components (3 parallel tracks)
| ID | Task | Dependencies | Test |
|----|------|--------------|------|
| T3 | Inline viewport setup (`Viewport::Inline`) | T2 | `test_viewport_setup` |
| T4 | ChatInput wrapper (tui-textarea) | T2 | `test_input_editing` |
| T5 | CompletionState + nucleo refilter | T2 | `test_fuzzy_filter` |

### Wave 2: Behaviors (2 parallel tracks)
| ID | Task | Dependencies | Test |
|----|------|--------------|------|
| T6 | Submit behavior (Enter/Ctrl+Enter) | T4 | `test_submit_behavior` |
| T7 | Completion triggers (`/`, `@`) | T4, T5 | `test_completion_triggers` |

### Wave 3: Event Loop + Popup (2 parallel tracks)
| ID | Task | Dependencies | Test |
|----|------|--------------|------|
| T8 | Event loop (poll-render cycle) | T3, T6 | `test_event_loop` |
| T9 | Completion popup rendering | T5, T7 | `test_popup_render` (manual) |

### Wave 4: Widgets (2 parallel tracks)
| ID | Task | Dependencies | Test |
|----|------|--------------|------|
| T10 | Status bar widget | T8 | `test_status_bar` |
| T11 | Completion navigation (up/down/enter/esc) | T9 | `test_completion_nav` |

### Wave 5: Multi-select
| ID | Task | Dependencies | Test |
|----|------|--------------|------|
| T12 | Multi-select with checkboxes | T11 | `test_multi_select` |

### Wave 6: Message Display
| ID | Task | Dependencies | Test |
|----|------|--------------|------|
| T13 | insert_before() for messages | T8, T10 | `test_message_scrollback` |

### Wave 7: Data Sources (3 parallel tracks)
| ID | Task | Dependencies | Test |
|----|------|--------------|------|
| T14 | Slash command completion source | T12, T13 | `test_command_source` |
| T15 | File completion source (@file) | T12, T13 | `test_file_source` |
| T16 | Streaming indicator | T10, T13 | `test_streaming_indicator` |

### Wave 8: Integration
| ID | Task | Dependencies | Test |
|----|------|--------------|------|
| T17 | Connect to AgentHandle | T14, T15, T16 | Integration test |

### Wave 9: Commands
| ID | Task | Dependencies | Test |
|----|------|--------------|------|
| T18 | /clear command | T17 | `test_clear_session` |

---

## Execution Plan for Parallel Agents

### Wave 0 (Sequential - Main Agent)
```
T1 → T2
```

### Wave 1 (Spawn 3 Parallel Agents)
```
Agent A: T3 (viewport)
Agent B: T4 (input)
Agent C: T5 (completion state)
```

### Wave 2 (Spawn 2 Parallel Agents)
```
Agent A: T6 (submit) - needs T4 complete
Agent B: T7 (triggers) - needs T4, T5 complete
```

### Wave 3 (Spawn 2 Parallel Agents)
```
Agent A: T8 (event loop) - needs T3, T6 complete
Agent B: T9 (popup render) - needs T5, T7 complete
```

### Wave 4 (Spawn 2 Parallel Agents)
```
Agent A: T10 (status bar) - needs T8 complete
Agent B: T11 (navigation) - needs T9 complete
```

### Wave 5 (Single Agent)
```
Agent A: T12 (multi-select) - needs T11 complete
```

### Wave 6 (Single Agent)
```
Agent A: T13 (insert_before) - needs T8, T10 complete
```

### Wave 7 (Spawn 3 Parallel Agents)
```
Agent A: T14 (commands) - needs T12, T13 complete
Agent B: T15 (files) - needs T12, T13 complete
Agent C: T16 (streaming) - needs T10, T13 complete
```

### Wave 8-9 (Sequential)
```
T17 → T18
```

---

## Task Details

### T1: Add Dependencies
```toml
# In crates/crucible-cli/Cargo.toml
tui-textarea = { version = "0.6", features = ["crossterm"] }
```

### T2: Module Structure
```
crates/crucible-cli/src/chat_tui/
├── mod.rs           # pub mod declarations, run_chat_tui()
├── app.rs           # ChatApp state, RenderState
├── input.rs         # ChatInput (tui-textarea wrapper)
├── completion.rs    # CompletionState, CompletionItem
├── render.rs        # Viewport layout, widget dispatch
└── widgets/
    ├── mod.rs
    ├── status.rs    # Status bar
    └── popup.rs     # Completion popup
```

### T3: Inline Viewport Setup
```rust
pub fn setup_inline_terminal(height: u16) -> Result<Terminal<...>> {
    enable_raw_mode()?;
    let backend = CrosstermBackend::new(stdout());
    Terminal::with_options(backend, TerminalOptions {
        viewport: Viewport::Inline(height),
    })
}
```

### T4: ChatInput
```rust
pub struct ChatInput {
    textarea: TextArea<'static>,
}

impl ChatInput {
    pub fn new() -> Self;
    pub fn handle_key(&mut self, key: KeyEvent) -> Option<ChatAction>;
    pub fn content(&self) -> String;
    pub fn clear(&mut self);
    pub fn height(&self, width: u16) -> u16;
}
```

### T5: CompletionState
```rust
pub struct CompletionState {
    pub query: String,
    pub all_items: Vec<CompletionItem>,
    pub filtered_items: Vec<CompletionItem>,
    pub selected_index: usize,
    pub multi_select: bool,
    pub selections: HashSet<usize>,
}

impl CompletionState {
    pub fn refilter(&mut self);  // Uses nucleo-matcher
}
```

### T6-T18: See architecture doc
`thoughts/shared/research/chat-tui-architecture_2025-12-11-1600.md`

---

## Agent Dispatch Commands

### Wave 1 Example
```
// Dispatch 3 agents in parallel
Task(T3): "Implement inline viewport setup following TDD harness.
          Create test_viewport_setup first, then implement."

Task(T4): "Implement ChatInput with tui-textarea following TDD harness.
          Create test_input_editing first, then implement."

Task(T5): "Implement CompletionState with nucleo refilter following TDD harness.
          Create test_fuzzy_filter first, then implement."
```

---

## Success Criteria

- [ ] All 18 tasks pass their tests
- [ ] `cargo test -p crucible-cli chat_tui` all green
- [ ] `cargo clippy -p crucible-cli` no warnings
- [ ] Manual verification: chat works with scrollback preserved

## References

- Architecture: `thoughts/shared/research/chat-tui-architecture_2025-12-11-1600.md`
- TDD harness: `.claude/harness/tdd-implementation.md`
- Existing patterns: `crates/crucible-cli/src/tui/`
