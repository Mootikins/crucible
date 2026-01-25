---
description: TUI component system architecture and rendering pipeline
tags:
  - tui
  - architecture
  - components
  - oil
status: implemented
---

# Component Architecture

The TUI uses a composable component architecture built on the **Oil** renderer — a React-like immediate-mode UI system with flexbox layout (via taffy).

## Core Concepts

### Oil Renderer

Oil is Crucible's terminal rendering library, providing:

- **Immediate-mode rendering** — UI rebuilt each frame from state
- **Node-based composition** — `Node` tree defines UI structure  
- **Flexbox layout** — Powered by taffy for CSS-like layouts
- **Streaming graduation** — Content transitions from viewport to stdout

```rust
// Basic Oil node construction
let view = Node::col(vec![
    Node::text("Hello").bold(),
    Node::row(vec![
        Node::badge("OK").fg(Color::Green),
        Node::spacer(),
        Node::text("Status"),
    ]),
]);
```

### Rendering Pipeline

```
State Change → Node Tree → Layout (taffy) → Render (buffer) → Terminal Output
```

1. **State Change**: User input or agent event modifies `InkChatApp` state
2. **Node Tree**: `view()` method builds node tree from current state
3. **Layout**: Taffy calculates flex positions and sizes
4. **Render**: Nodes render to terminal buffer with styles
5. **Output**: Diff algorithm writes minimal changes to terminal

## Components

Components are modules in `crates/crucible-cli/src/tui/oil/components/`:

### MessageList

Renders conversation history with markdown formatting.

**Location:** `components/message_list.rs`

**Features:**
- Markdown rendering with syntax highlighting
- Tool call display with collapsible results
- Streaming content with spinner
- Thinking block display (toggle with `Alt+T`)

### InputArea

Text input with readline-style editing.

**Location:** `components/input_area.rs`

**Features:**
- Multi-line input with cursor
- Emacs keybindings (Ctrl+A/E/W/U/K)
- Command prefix detection (`/`, `@`, `[[`, `:`, `!`)

### StatusBar

Mode indicator and status display.

**Location:** `components/status_bar.rs`

**Features:**
- Mode display (Normal, Plan, Auto)
- Model name and token count
- Notification display
- Thinking token count (when visible)

### PopupOverlay

Autocomplete popup for commands and references.

**Location:** `components/popup_overlay.rs`

**Features:**
- Fuzzy filtering
- Keyboard navigation
- Multiple popup kinds (Command, File, Agent, Note, Model)

## Graduation System

The TUI uses a two-layer "graduation" system for streaming content:

### Viewport Graduation

Static content graduates from the viewport to terminal stdout (scrollback):

```
┌─────────────────────────┐
│  STDOUT (graduated)     │ ← Historical content, no longer re-rendered
├─────────────────────────┤
│                         │
│  VIEWPORT (active)      │ ← Live content, re-rendered each frame
│                         │
└─────────────────────────┘
```

**Key files:**
- `graduation.rs` — `GraduationState`, `plan_graduation()`, `commit_graduation()`
- `planning.rs` — `FramePlanner` orchestrates graduation-first rendering

### Streaming Graduation

Within the viewport, streaming content graduates block-by-block:

```rust
pub struct StreamingBuffer {
    graduated_blocks: Vec<String>,  // Completed paragraphs
    in_progress: String,            // Active streaming text
}
```

Paragraphs (split at `\n\n`) graduate to `graduated_blocks` while new content streams into `in_progress`.

**Key files:**
- `viewport_cache.rs` — `ViewportCache`, `StreamingBuffer`

### Invariants

1. **XOR Placement** — Content in stdout OR viewport, never both
2. **Monotonic** — Graduated count never decreases
3. **Atomic** — Graduation commits before viewport filtering (no flash)

## ViewportCache

The `ViewportCache` manages conversation state for efficient rendering:

```rust
pub struct ViewportCache {
    items: VecDeque<CachedChatItem>,
    graduated_ids: HashSet<String>,
    streaming: Option<StreamingBuffer>,
}
```

**Key methods:**
- `push_message()` — Add user/assistant message
- `push_tool_call()` / `push_tool_result()` — Tool call lifecycle
- `mark_graduated()` — Track graduated content
- `ungraduated_items()` — Iterator for viewport rendering

## Event Flow

Events propagate through the system:

```
Terminal Event
    ↓
ChatRunner::handle_event()
    ↓
InkChatApp::update() → ChatAppMsg
    ↓
State mutation + view rebuild
    ↓
Terminal::draw()
```

### ChatAppMsg

High-level messages that modify app state:

```rust
pub enum ChatAppMsg {
    SendMessage(String),
    StreamChunk(String),
    ToolCallStart { id, name },
    ToolCallComplete { id, result },
    SetModel(String),
    SetThinkingBudget(Option<u32>),
    ToggleThinking,
    // ...
}
```

## Theming

The `theme.rs` module defines consistent styling:

```rust
pub struct Theme {
    pub user_prefix: Style,
    pub assistant_prefix: Style,
    pub thinking_border: Style,
    pub tool_name: Style,
    // ...
}
```

Access via `Theme::default()` or configure via `:set` commands.

## Testing

Components are tested via:

1. **Unit tests** — State transitions, event handling
2. **Snapshot tests** — Visual output with insta
3. **Property tests** — Graduation invariants with proptest

Example snapshot test:

```rust
#[test]
fn test_status_bar_normal_mode() {
    let app = InkChatApp::default();
    let node = app.view();
    insta::assert_snapshot!(render_to_string(&node, 80, 24));
}
```

## See Also

- [[Help/TUI/Index]] — TUI overview
- [[Help/TUI/Keybindings]] — Keyboard shortcuts
- [[Help/TUI/Commands]] — REPL commands (`:set`, `:model`, etc.)
- [[Help/Extending/Scripted UI]] — Lua/Fennel UI building with `cru.oil`
