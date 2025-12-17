# Terminal TUI: Event-Based Chat Interface

## Why

The current CLI chat is stubbed after removing reedline/ratatui dependencies. Users need an interactive chat experience that:

1. **Streams responses** - See LLM output token-by-token as it generates
2. **Handles long operations** - Show progress for tool calls, searches, embeddings
3. **Supports rich display** - Markdown rendering, syntax highlighting, tool call visualization
4. **Integrates with event system** - Uses existing `SessionEvent` infrastructure in `crucible-rune`

### Previous Approaches (Superseded)

- **chat-interface** proposal: Designed around reedline REPL, now removed
- **chat-improvements** proposal: Depended on reedline-based framework

This proposal consolidates both into a single event-driven design using ratatui.

### Why Event-Based

The `crucible-rune` crate already provides:
- `EventRing<SessionEvent>` - Ring buffer for all events
- `SessionHandle` - API for sending events (`handle.message()`, `handle.thinking()`)
- `Session::run()` - Async event loop with reactor pattern
- `TextDelta` events - Streaming response chunks with sequence numbers

Building the TUI on this foundation means:
- Single source of truth for conversation state
- Easy integration with browser UI (same events via SSE)
- Handlers can observe TUI interactions
- Future: distributed sessions, replays, testing

## What Changes

### TUI Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Terminal (crossterm)                      │
├─────────────────────────────────────────────────────────────┤
│  ┌─────────────────────────────────────────────────────┐    │
│  │              Message History (scrollable)           │    │
│  │  ┌─────────────────────────────────────────────┐   │    │
│  │  │ User: What files handle auth?               │   │    │
│  │  └─────────────────────────────────────────────┘   │    │
│  │  ┌─────────────────────────────────────────────┐   │    │
│  │  │ Assistant: [streaming...]                    │   │    │
│  │  │ Looking at src/auth... ▋                    │   │    │
│  │  └─────────────────────────────────────────────┘   │    │
│  └─────────────────────────────────────────────────────┘    │
│  ┌─────────────────────────────────────────────────────┐    │
│  │ [Plan] > |                                          │    │
│  └─────────────────────────────────────────────────────┘    │
│  ─────────────────────────────────────────────────────────  │
│  Mode: Plan │ Tokens: 1.2k/8k │ Context: 5 notes │ ⏳ Tool  │
└─────────────────────────────────────────────────────────────┘
```

### Core Components

**1. TuiState - UI State Machine**
```rust
struct TuiState {
    // Display state
    messages: Vec<DisplayMessage>,
    scroll_offset: usize,

    // Input state
    input_buffer: String,
    cursor_position: usize,

    // Streaming state
    streaming: Option<StreamingResponse>,

    // Mode & status
    mode: ChatMode,
    pending_tools: Vec<ToolCallInfo>,

    // Event tracking
    last_seen_seq: u64,
}
```

**2. Event Bridge - Terminal Input → SessionEvent**
```rust
// Crossterm events become SessionEvents
match crossterm_event {
    Event::Key(KeyEvent { code: KeyCode::Enter, .. }) => {
        let input = state.take_input();
        handle.message(input).await;
    }
    Event::Key(KeyEvent { code: KeyCode::Char('j'), modifiers: CONTROL }) => {
        state.input_buffer.push('\n');
    }
    // ...
}
```

**3. Ring Buffer Polling - SessionEvent → Display**
```rust
// Poll for new events and update display
let new_events = session.ring()
    .iter_from(state.last_seen_seq);

for event in new_events {
    match event {
        SessionEvent::TextDelta { delta, seq } => {
            state.streaming.as_mut()?.push(delta);
            state.last_seen_seq = seq;
        }
        SessionEvent::AgentResponded { content, .. } => {
            state.finalize_streaming(content);
        }
        SessionEvent::ToolCalled { name, args, .. } => {
            state.pending_tools.push(ToolCallInfo { name, args });
        }
        // ...
    }
}
```

**4. Render Loop - Ratatui Drawing**
```rust
loop {
    // 1. Poll terminal events (non-blocking)
    if crossterm::event::poll(Duration::from_millis(16))? {
        handle_input(&mut state, &handle).await;
    }

    // 2. Poll ring buffer for new SessionEvents
    poll_events(&mut state, &session);

    // 3. Render current state
    terminal.draw(|frame| {
        render_messages(frame, &state);
        render_input(frame, &state);
        render_status(frame, &state);
    })?;
}
```

### New Session Helpers

Add to `crucible-rune/src/session.rs`:

```rust
impl Session {
    /// Get recent messages for display (filters MessageReceived + AgentResponded)
    pub fn recent_messages(&self, limit: usize) -> Vec<&SessionEvent>;

    /// Get pending tool calls (ToolCalled without matching ToolCompleted)
    pub fn pending_tools(&self) -> Vec<&SessionEvent>;

    /// Check if currently streaming (has TextDelta without AgentResponded)
    pub fn is_streaming(&self) -> bool;

    /// Cancel current operation (sends interrupt signal)
    pub fn cancel(&self) -> Result<()>;
}
```

### Slash Commands

Built on existing `SlashCommandRegistry`:

| Command | Action |
|---------|--------|
| `/plan` | Switch to Plan mode (read-only) |
| `/act` | Switch to Act mode (write-enabled) |
| `/auto` | Switch to AutoApprove mode |
| `/mode` | Cycle modes |
| `/search <query>` | Search knowledge base |
| `/context [n]` | Show/set context window |
| `/compact` | Compact conversation history |
| `/cancel` | Cancel current operation |
| `/exit` | Exit chat |

### Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `Enter` | Send message |
| `Ctrl+J` | Insert newline |
| `Ctrl+C` | Cancel current / Exit (double) |
| `Shift+Tab` | Cycle mode |
| `Up/Down` | Scroll history |
| `PgUp/PgDn` | Page scroll |
| `Ctrl+L` | Clear screen |

## Impact

### Affected Specs
- **chat-interface** (supersedes) - This proposal replaces it
- **chat-improvements** (supersedes) - Features merged into this proposal
- **cli** (extends) - New `cru chat` interactive mode

### Affected Code

**New Files:**
```
crates/crucible-cli/src/tui/
├── mod.rs           # Module exports
├── state.rs         # TuiState struct
├── input.rs         # Crossterm event handling
├── render.rs        # Ratatui drawing
├── messages.rs      # Message display formatting
├── streaming.rs     # TextDelta accumulation
└── status.rs        # Status bar rendering
```

**Modified Files:**
- `crates/crucible-cli/src/chat/session.rs` - Integrate TUI loop
- `crates/crucible-rune/src/session.rs` - Add helper methods
- `crates/crucible-cli/Cargo.toml` - Add ratatui, crossterm

### Dependencies
```toml
ratatui = "0.29"
crossterm = "0.28"
```

### Migration Path

1. **Phase 1**: Basic TUI with message display, input, streaming
2. **Phase 2**: Rich features (markdown, syntax highlighting, tool visualization)
3. **Phase 3**: Session management (multiple sessions, inbox integration)

## Open Questions

1. **Widget reuse**: Should message rendering be shared with browser UI?
2. **Theming**: Support user-configurable colors/styles?
3. **Mouse support**: Enable mouse scrolling and selection?

---

## Amendment: Dynamic Capabilities from Agent

*See internal-agent-system proposal for full ACP capabilities flow*

### Modes from Registry (Not Hardcoded)

The TUI queries `ModeRegistry` for available modes instead of using hardcoded `ChatMode` enum:

```rust
// OLD: Hardcoded enum
enum ChatMode { Plan, Act, AutoApprove }

// NEW: Dynamic from agent
let modes = session.mode_registry().list_all();
let current = session.mode_registry().current();

// Status line
render_status_line(current.name, mode_color(&current.id));
```

Agent advertises modes in `NewSessionResponse.modes` (ACP spec). Internal agents provide default Plan/Act/Auto modes.

### Commands from Registry

Slash commands merge agent-provided + reserved client commands:

```rust
// Agent commands from AvailableCommandsUpdate notification
// Client reserved commands: /exit, /help, /crucible:*

let commands = session.command_registry().list_all();
for cmd in commands {
    // Agent commands shown first, then /crucible:* namespaced
}
```

### Conflict Resolution

When agent registers a command that conflicts with client reserved:
- Agent gets the bare name (`/search`)
- Client command namespaced (`/crucible:search`)

Reserved commands that cannot be shadowed: `/exit`, `/quit`, `/help`

### Impact on TUI

| Component | Change |
|-----------|--------|
| Status line | Query `mode_registry().current()` for mode name/icon |
| Mode switching | Validate against `mode_registry().find(id)` |
| Help display | Merge `command_registry().list_all()` |
| Tab cycling | Iterate `mode_registry().available_modes` |

### Widget Rendering

Mode display logic moves from hardcoded to registry lookup:

```rust
fn mode_icon(mode_id: &SessionModeId) -> &'static str {
    match mode_id.as_str() {
        "plan" => "📖",
        "act" => "✏️",
        "auto" => "⚡",
        _ => "●",  // Custom agent modes
    }
}

fn mode_color(mode_id: &SessionModeId) -> Color {
    match mode_id.as_str() {
        "plan" => Color::Cyan,
        "act" => Color::Yellow,
        "auto" => Color::Red,
        _ => Color::White,  // Custom agent modes
    }
}
```
