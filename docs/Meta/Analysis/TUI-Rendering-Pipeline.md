# TUI Rendering Pipeline

Architecture analysis of Crucible's Oil-based TUI rendering system, including the graduation mechanism for streaming content.

## Overview

The TUI uses an **immediate-mode** rendering approach where the UI is rebuilt each frame from application state. This is combined with a **graduation system** that transitions stable content from the active viewport to terminal stdout (scrollback).

## Rendering Layers

```
┌─────────────────────────────────────┐
│  STDOUT (graduated content)         │ ← Historical, no longer re-rendered
├─────────────────────────────────────┤
│                                     │
│  VIEWPORT (active content)          │ ← Re-rendered each frame
│  ┌─────────────────────────────────┐│
│  │ MessageList                     ││
│  │ ├─ Graduated blocks             ││ ← Streaming content that settled
│  │ ├─ In-progress text + spinner   ││ ← Active streaming
│  │ └─ Tool calls                   ││
│  ├─────────────────────────────────┤│
│  │ InputArea                       ││
│  ├─────────────────────────────────┤│
│  │ StatusBar                       ││
│  └─────────────────────────────────┘│
│                                     │
│  POPUP LAYER (when active)          │ ← Overlays viewport
│                                     │
└─────────────────────────────────────┘
```

## Graduation System

### Purpose

The graduation system solves a key problem: **re-rendering a growing conversation is expensive**. As the chat history grows, re-rendering everything each frame becomes slow.

Solution: **Graduate** stable content from the viewport to stdout. Graduated content:
- Is written once to terminal scrollback
- Never re-rendered
- Can be scrolled via terminal scroll

### Two-Layer Graduation

#### 1. Viewport Graduation (`graduation.rs`)

Static `Node::Static` content graduates to stdout:

```rust
pub struct GraduationState {
    graduated_keys: VecDeque<String>,  // Max 256 keys, FIFO eviction
}
```

**Key functions:**
- `plan_graduation()` — Identifies static nodes to graduate (pure, readonly)
- `commit_graduation()` — Marks keys as graduated (state change)
- `format_stdout_delta()` — Builds output string for graduated content

#### 2. Streaming Graduation (`viewport_cache.rs`)

Within the viewport, streaming content graduates block-by-block:

```rust
pub struct StreamingBuffer {
    graduated_blocks: Vec<String>,  // Completed paragraphs
    in_progress: String,            // Active streaming text
    segments: Vec<StreamSegment>,   // Text, Thinking, ToolCall, Subagent
}
```

**Graduation point detection:**
- Looks for `\n\n` (double newline) as paragraph boundary
- Preserves code blocks (doesn't split inside ``` fences)
- Moves completed paragraphs to `graduated_blocks`

### Execution Order (Critical)

From `planning.rs` `FramePlanner::plan()`:

```
1. plan_graduation()       ← Identify content to graduate (pure scan)
2. format_stdout_delta()   ← Build stdout output string
3. commit_graduation()     ← Mark keys as graduated (state change)
4. render_with_filter()    ← Render viewport, filtering graduated keys
```

**Why this order matters:** Content must be written to stdout BEFORE being filtered from viewport. Otherwise content would "flash" — disappearing from viewport before appearing in stdout.

### Invariants

1. **XOR Placement** — Content appears in exactly stdout OR viewport, never both
2. **Monotonic** — Graduated count never decreases
3. **Atomic** — Graduation commits before viewport filtering (no flash)
4. **Stable** — Resize operations preserve content (no loss during height changes)

## ViewportCache

The `ViewportCache` manages conversation state for efficient rendering:

```rust
pub struct ViewportCache {
    items: VecDeque<CachedChatItem>,    // Message history
    graduated_ids: HashSet<String>,      // IDs that have graduated
    streaming: Option<StreamingBuffer>,  // Active streaming state
    streaming_start_index: usize,        // Where streaming began
}
```

### Key Methods

| Method | Purpose |
|--------|---------|
| `push_message()` | Add user/assistant message |
| `push_tool_call()` | Start tool call |
| `push_tool_result()` | Complete tool call |
| `mark_graduated()` | Track graduated content |
| `ungraduated_items()` | Iterator for viewport rendering |
| `streaming_graduated_content()` | Access graduated blocks |

## Frame Rendering

### Event Loop

```
Terminal Event (key, resize, etc.)
    ↓
ChatRunner::handle_event()
    ↓
InkChatApp::update() → ChatAppMsg
    ↓
State mutation
    ↓
InkChatApp::view() → Node tree
    ↓
Layout (taffy flexbox)
    ↓
Render (buffer with styles)
    ↓
Diff (minimal terminal writes)
    ↓
Graduation (plan → format → commit → filter)
    ↓
Output (viewport + stdout delta)
```

### Frame Planner

The `FramePlanner` orchestrates frame rendering:

```rust
pub struct FramePlanner {
    graduation: GraduationState,
}

impl FramePlanner {
    pub fn plan(&mut self, view: &Node, width: u16, height: u16) -> FrameSnapshot {
        // 1. Plan graduation (pure)
        let to_graduate = self.graduation.plan_graduation(view);
        
        // 2. Format stdout delta
        let stdout = self.graduation.format_stdout_delta(&to_graduate);
        
        // 3. Commit graduation (state change)
        self.graduation.commit_graduation(to_graduate);
        
        // 4. Render with filter
        let viewport = render_with_filter(view, &self.graduation);
        
        FrameSnapshot { stdout, viewport }
    }
}
```

## Testing

### Unit Tests

Located in `crates/crucible-cli/src/tui/oil/tests/`:

- `graduation_tests.rs` — Basic graduation behavior
- `graduation_invariant_tests.rs` — Property-based invariant testing
- `streaming_tests.rs` — Streaming graduation scenarios

### Key Test Properties

```rust
// XOR placement: content in stdout XOR viewport
proptest! {
    fn content_appears_exactly_once(messages in arb_messages()) {
        // ... verify no duplication
    }
}

// Monotonic graduation
proptest! {
    fn graduated_count_never_decreases(events in arb_events()) {
        // ... verify monotonicity
    }
}
```

## Performance Considerations

1. **Graduation threshold** — Don't graduate too eagerly (causes flicker) or too late (slow render)
2. **Batch updates** — Multiple events in one frame reduce render calls
3. **Incremental layout** — Taffy caches layout when possible
4. **Diff algorithm** — Only write changed cells to terminal

## Files

| File | Purpose |
|------|---------|
| `graduation.rs` | Core graduation logic |
| `planning.rs` | Frame orchestration |
| `viewport_cache.rs` | Viewport state management |
| `chat_app.rs` | Application state + view |
| `chat_runner.rs` | Event loop + message handling |
| `runtime.rs` | TUI runtime wrapper |
| `terminal.rs` | Terminal I/O |

## See Also

- [[Help/TUI/Component Architecture]] — Component system overview
- [[Help/TUI/Index]] — TUI reference
- [[Help/Extending/Scripted UI]] — Lua/Fennel UI building
