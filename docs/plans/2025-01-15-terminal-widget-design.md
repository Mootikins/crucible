# Terminal Widget Chat Interface Design

**Date:** 2025-01-15
**Status:** Approved
**Author:** Claude + moot

## Overview

Replace the full-screen ratatui TUI with a Claude Code-style terminal widget. Messages print to terminal scrollback while a minimal widget at the bottom handles input, status, and streaming responses.

## Architecture

### Terminal Model

**No alternate screen.** The interface stays in the normal terminal. Completed messages print to stdout and become permanent scrollback history.

**Bottom widget reservation.** Reserve the bottom N lines using cursor positioning. The widget redraws only this region, leaving scrollback untouched.

### Widget Layout (bottom to top)

```
│ (terminal scrollback - completed messages)  │
│ ...                                         │
├─────────────────────────────────────────────┤ ← widget boundary
│ [streaming response - dynamic, ~1/3 cap]    │
│ ─────────────────────────────────────────── │ ← dim separator
│ [input prompt - grows upward]               │
│ ─────────────────────────────────────────── │ ← dim separator
│ [status line: mode, etc.]                   │
└─────────────────────────────────────────────┘
```

**Minimum widget height:** 4 lines (status + 2 separators + 1 line input)

### Components

1. **Status line (1 line)** - Mode (Plan/Act/Auto), connection status. Dim styling.

2. **Lower separator (1 line)** - Dim horizontal rule (`─────`)

3. **Input prompt (1+ lines)** - Grows upward as user types. No hard cap.

4. **Upper separator (1 line)** - Dim horizontal rule

5. **Streaming area (0 to ~1/3 terminal height)** - Empty when idle. Grows as response streams. Caps at ~1/3 terminal height with internal scroll.

## Data Flow

### User Sends Message

1. User types, presses Enter
2. Print user message to stdout (scrollback)
3. Clear input area
4. Send through `AgentEventBridge` → Agent

### Agent Response (Streaming)

1. `TextDelta` events arrive from ring buffer
2. Append to streaming area buffer
3. Re-render streaming area (grows upward, caps at ~1/3)
4. **Incremental flush** on natural breaks:
   - Paragraph break (`\n\n`)
   - Code block end (` ``` `)
   - Streaming area 80% full
5. Flushed content prints to stdout, clears from streaming area

### Response Completes

1. `AgentResponded` event arrives
2. Flush remaining content to stdout
3. Clear streaming area
4. Widget shrinks to minimum height

### Tool Calls

Display evolves as information arrives:
- Initially: `"Running tool: ..."`
- Name known: `"Running tool: tool_name"`
- Params known: `"Running tool: tool_name(arg1=val1, ...)"` (truncated)

Tool calls flush to stdout with the surrounding response content.

## Edge Cases

### Terminal Resize
- Recalculate streaming cap (1/3 of new height)
- Enforce minimum widget height
- Re-render widget in new position
- Scrollback unaffected

### Long Input
- Let input grow unbounded
- User can submit and see it in scrollback

### Agent Error Mid-Stream
- Flush partial content to stdout with error marker
- Clear streaming area
- Show error in status line

### Cancel Streaming (Ctrl+C or ESC)
- Cancel current stream
- Flush partial response to stdout (marked as interrupted)
- Return to input mode

## Implementation Changes

### Keep
- `AgentEventBridge` - converts chunks to events, pushes to ring
- Event polling logic (~60fps)
- Mode tracking

### Change

| Current | New |
|---------|-----|
| `EnterAlternateScreen` | Normal terminal |
| Full-frame ratatui render | Bottom widget only |
| Messages in `Vec<DisplayMessage>` | Messages to stdout |
| `render()` draws everything | `render_widget()` bottom only |

### New Components
- `StreamingBuffer` - accumulates text, handles incremental flush detection
- `WidgetRenderer` - cursor positioning, draws reserved bottom lines
- Natural break detection logic

### File Changes
- `tui/runner.rs` - remove alternate screen, add stdout printing
- `tui/render.rs` - rewrite for bottom-widget-only
- `tui/state.rs` - remove message storage, add streaming buffer
- New: `tui/streaming.rs` - StreamingBuffer with flush logic

## Future Considerations (not in scope)

- Multi-session with session-managed scrollback
- Custom output formats (diffs, etc.)
- Tool result expansion/collapse
