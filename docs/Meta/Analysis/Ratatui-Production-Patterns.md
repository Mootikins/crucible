# Ratatui Production Patterns

Patterns from OpenAI Codex CLI - a production Ratatui application with streaming, async operations, and complex state management.

## Overview

Codex CLI is a production-quality terminal application using Ratatui for LLM-powered coding assistance. This document captures patterns that could improve Crucible's TUI.

## Component Architecture

### Renderable Trait

Codex uses a unified trait for all renderable components:

```rust
pub trait Renderable {
    fn render(&self, area: Rect, buf: &mut Buffer);
    fn desired_height(&self, width: u16) -> u16;
    fn cursor_pos(&self, _area: Rect) -> Option<(u16, u16)> { None }
}
```

**Key insight**: `desired_height()` is computed *before* rendering, enabling layout calculations without drawing. This is critical for:

- Content-based sizing for streaming text
- Scroll position calculations before render
- Layout negotiation between components

**Application to Crucible**: Add `desired_height()` to widget trait, precompute heights for streaming content.

### Event Separation

Codex separates terminal events from domain events:

```rust
// Terminal events - raw input
pub enum TuiEvent {
    Key(KeyEvent),
    Paste(String),
    Draw,
}

// Domain events - application logic
pub enum AppEvent {
    InsertHistoryCell(...),
    NewSession,
    OpenResumePicker,
    StreamingDelta(String),
    ToolCall { name: String, args: Value },
    // ... many more
}
```

**Benefits**:
- Cleaner separation of concerns
- Easier testing of domain logic
- Multiple sources can emit AppEvents

---

## Async/Sync Bridge

### FrameRequester Pattern

Ratatui's render loop is synchronous, but LLM streaming is async. Codex solves this with `FrameRequester`:

```rust
pub struct FrameRequester {
    frame_schedule_tx: mpsc::UnboundedSender<Instant>,
}

impl FrameRequester {
    pub fn schedule_frame(&self) {
        let _ = self.frame_schedule_tx.send(Instant::now());
    }
    
    pub fn schedule_frame_in(&self, dur: Duration) {
        let _ = self.frame_schedule_tx.send(Instant::now() + dur);
    }
}
```

**Actor pattern**: Dedicated `FrameScheduler` task coalesces requests:

- Multiple `schedule_frame()` calls → single draw
- Rate limited to 60 FPS
- `schedule_frame_in()` enables timed animations (spinners, shimmers)

**Application to Crucible**: Replace direct redraw calls with frame scheduling to avoid wasted renders during rapid streaming.

### Event Broker

`EventBroker` manages the crossterm event stream lifecycle:

```rust
pub struct EventBroker<S: EventSource = CrosstermEventSource> {
    state: Mutex<EventBrokerState<S>>,
}

impl EventBroker {
    pub fn pause_events(&self) {
        let mut state = self.state.lock().unwrap();
        *state = EventBrokerState::Paused;  // Drops underlying stream
    }
    
    pub fn resume_events(&self) {
        let mut state = self.state.lock().unwrap();
        *state = EventBrokerState::Start;  // Recreates stream
    }
}
```

**Solves**: Input stealing when spawning external processes (vim, editors, shell commands).

**Application to Crucible**: Implement pause/resume before spawning external editors or shell commands.

### Main Event Loop

Codex uses `tokio::select!` to poll multiple receivers:

```rust
loop {
    let control = select! {
        Some(event) = app_event_rx.recv() => {
            app.handle_event(tui, event).await?
        }
        Some(event) = active_thread_rx.recv() => {
            app.handle_active_thread_event(tui, event)?
        }
        Some(event) = tui_events.next() => {
            app.handle_tui_event(tui, event).await?
        }
    };
    match control {
        AppRunControl::Continue => {}
        AppRunControl::Exit(reason) => break reason,
    }
}
```

**Pattern**: Single `AppRunControl` return type enables clean exit handling.

---

## Event Handling

### KeyEventKind Filtering

Crossterm can emit both press and release events. Filter for press only:

```rust
match event {
    TuiEvent::Key(key_event) => {
        if key_event.kind != KeyEventKind::Press {
            return Ok(AppRunControl::Continue);
        }
        // Handle key...
    }
}
```

**Solves**: Double-firing on keypress (press + release events).

### Suspend Context (Ctrl+Z)

Graceful SIGTSTP handling:

```rust
pub fn suspend(&self, alt_screen_active: &Arc<AtomicBool>) -> Result<()> {
    if alt_screen_active.load(Ordering::Relaxed) {
        execute!(stdout(), DisableAlternateScroll);
        execute!(stdout(), LeaveAlternateScreen);
        self.set_resume_action(ResumeAction::RestoreAlt);
    } else {
        self.set_resume_action(ResumeAction::RealignInline);
    }
    let y = self.suspend_cursor_y.load(Ordering::Relaxed);
    execute!(stdout(), MoveTo(0, y), Show);
    unsafe { libc::kill(0, libc::SIGTSTP) };
}
```

**Pattern**: Cache cursor position, save viewport state, deliver SIGTSTP, restore on SIGCONT.

---

## View Management

### Overlay System

Modals and popups use an overlay pattern:

```rust
pub(crate) enum Overlay {
    Transcript(TranscriptOverlay),
    Static(StaticOverlay),
}

impl Overlay {
    pub(crate) fn handle_event(&mut self, tui: &mut Tui, event: TuiEvent) -> Result<()> {
        match self {
            Overlay::Transcript(o) => o.handle_event(tui, event),
            Overlay::Static(o) => o.handle_event(tui, event),
        }
    }
    
    pub(crate) fn is_done(&self) -> bool { ... }
}
```

Main app stores `Option<Overlay>`, routes events to overlay when active.

### Clear Before Overlay

Always clear overlay area before rendering content:

```rust
fn render(&mut self, area: Rect, buf: &mut Buffer) {
    Clear.render(area, buf);  // Clear first
    // ... render overlay content
}
```

---

## Streaming Text Display

### Newline-Gated Rendering

Only render after newlines to avoid expensive partial-line renders:

```rust
pub(crate) struct StreamController {
    state: StreamState,
}

impl StreamController {
    pub(crate) fn push(&mut self, delta: &str) -> bool {
        self.state.collector.push_delta(delta);
        if delta.contains('\n') {
            let newly_completed = self.state.collector.commit_complete_lines();
            if !newly_completed.is_empty() {
                self.state.enqueue(newly_completed);
                return true;  // Signal: render these lines
            }
        }
        false  // No newline yet, don't render
    }
}
```

**Key insight**: Incomplete lines stay in buffer, only complete lines trigger render.

### Incremental Markdown Rendering

`MarkdownStreamCollector` caches rendering state:

```rust
pub(crate) fn commit_complete_lines(&mut self) -> Vec<Line<'static>> {
    let last_newline_idx = self.buffer.rfind('\n');
    let source = match last_newline_idx {
        Some(idx) => self.buffer[..=idx].to_string(),
        None => return Vec::new(),  // No complete line
    };
    
    markdown::append_markdown(&source, self.width, &mut self.rendered);
    
    let out = self.rendered[self.committed_line_count..complete_line_count].to_vec();
    self.committed_line_count = complete_line_count;
    out  // Only newly completed lines
}
```

**Pattern**: Track `committed_line_count`, only re-render new content.

---

## Progress Indicators

### StatusIndicatorWidget

Animated status with scheduled redraws:

```rust
pub(crate) struct StatusIndicatorWidget {
    header: String,
    details: Option<String>,
    elapsed_running: Duration,
    last_resume_at: Instant,
    is_paused: bool,
    frame_requester: FrameRequester,  // For animation scheduling
    animations_enabled: bool,
}
```

Uses `frame_requester.schedule_frame_in(Duration::from_millis(100))` for spinner animation.

### Shimmer Effect

Subtle animation for streaming text - characters get highlight that moves across text.

---

## Terminal Safety

### Panic Hook Restoration

Always restore terminal state on panic:

```rust
fn set_panic_hook() {
    let hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        let _ = restore();  // Ignore errors, we're already failing
        hook(panic_info);
    }));
}
```

### Custom Terminal Drop

Restore cursor on drop:

```rust
impl<B: Backend + Write> Drop for Terminal<B> {
    fn drop(&mut self) {
        if self.hidden_cursor {
            if let Err(err) = self.show_cursor() {
                eprintln!("Failed to show cursor: {err}");
            }
        }
    }
}
```

---

## Comparison to Crucible

| Aspect | Codex | Crucible |
|--------|-------|----------|
| **Event routing** | TuiEvent + AppEvent (separate) | Direct handler calls |
| **Frame scheduling** | FrameRequester with rate limiting | Immediate redraws |
| **Async bridge** | FrameScheduler actor task | Channels without coalescing |
| **Streaming** | Newline-gated + cached | Immediate rendering |
| **Screen management** | Overlay enum | Direct state changes |
| **External processes** | EventBroker pause/resume | No explicit pause |

---

## Implementation Priorities

### High Priority

1. **Add FrameRequester** - Coalesce redraws, cap at 60fps
2. **Add `desired_height()`** - Enable content-based layout
3. **Implement newline gating** - Reduce render cost during streaming
4. **Add panic hook** - Always restore terminal

### Medium Priority

5. **Separate TuiEvent/AppEvent** - Cleaner architecture
6. **Implement Overlay system** - Better popup management
7. **Add EventBroker pause** - For external process safety

### Low Priority

8. **Add shimmer animation** - Visual polish
9. **Implement suspend context** - Ctrl+Z support
