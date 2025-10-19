# Crucible TUI Architecture Design

> **Status**: Design Document
> **Date**: 2025-10-19
> **Purpose**: Architectural decisions and implementation patterns for the ratatui-based daemon interface

## Executive Summary

This document defines the terminal user interface architecture for the Crucible daemon. The TUI consists of three primary sections: a status header, a scrollable log window, and a REPL input area. The design prioritizes memory efficiency, responsive async updates, and clean separation of concerns between business logic and presentation.

## Core Architecture Principles

### 1. Actor-Based Concurrency Model

**Decision**: Use an actor-like pattern with message passing via tokio channels rather than shared state with locks.

**Rationale**:
- Eliminates lock contention in the UI rendering path
- Provides clear ownership boundaries (watcher → parser → indexer → UI)
- Enables backpressure handling when log buffer fills
- Simplifies reasoning about concurrent state updates

**Trade-offs**:
- Slightly higher memory overhead for message passing
- Potential for message queue buildup if UI can't keep up
- More boilerplate for channel setup

**Alternative Considered**: Arc<Mutex<AppState>> shared state
- Rejected: Mutex contention would block worker threads during UI redraws
- Rejected: Harder to test individual components in isolation

### 2. Ring Buffer for Log Storage

**Decision**: Use a fixed-size ring buffer (VecDeque) for log history with configurable capacity.

**Rationale**:
- Bounded memory usage (critical for long-running daemon)
- O(1) push/pop operations for log rotation
- Simple implementation without dependencies
- Natural fit for "last N lines" display requirement

**Implementation Details**:
```rust
struct LogBuffer {
    entries: VecDeque<LogEntry>,
    capacity: usize,
}

impl LogBuffer {
    fn push(&mut self, entry: LogEntry) {
        if self.entries.len() >= self.capacity {
            self.entries.pop_front(); // Drop oldest
        }
        self.entries.push_back(entry);
    }
}
```

**Trade-offs**:
- Fixed capacity means old logs are lost (mitigated by file logging)
- No compression (acceptable for 15-20 lines)

**Alternative Considered**: Persistent log viewer with paging
- Rejected: Adds complexity for minimal benefit (file logs serve this need)
- Rejected: Would require database or file I/O in rendering path

### 3. Unidirectional Data Flow

**Decision**: Adopt Elm Architecture pattern - events flow up, state flows down.

**Flow**:
```
User Input → Event → State Mutation → UI Re-render
     ↑                                      ↓
     └──────────── (cycle repeats) ─────────┘

Worker Thread Events:
File Change → Parser → DB Update → Log Event → State Mutation → UI Update
```

**Rationale**:
- Predictable state updates (all mutations in one place)
- Easy to add new event sources (MCP, HTTP, etc.)
- Testable: Event → Expected State Change
- No circular dependencies between components

### 4. Lazy Rendering with Dirty Flags

**Decision**: Track which UI sections need re-rendering with dirty flags, only update changed areas.

**Rationale**:
- Terminal rendering is expensive (avoid full redraws at 60fps)
- Most updates affect only logs or REPL, not header
- Enables efficient handling of high-frequency log events

**Implementation**:
```rust
struct RenderState {
    header_dirty: bool,
    logs_dirty: bool,
    repl_dirty: bool,
}
```

**Trade-offs**:
- Adds complexity to rendering logic
- Must carefully manage dirty flag lifecycle

**Alternative Considered**: Always full re-render
- Rejected: Wasteful for header which rarely changes
- Rejected: CPU usage spikes during high log volume

## System Architecture

### Component Diagram

```
┌──────────────────────────────────────────────────────────┐
│                     Main Thread                          │
│  ┌────────────────────────────────────────────────────┐  │
│  │  Event Loop (tokio::select!)                       │  │
│  │  - Crossterm events (keyboard, resize)             │  │
│  │  - Log channel receiver                            │  │
│  │  - REPL result channel receiver                    │  │
│  │  - Status update channel receiver                  │  │
│  └────────────────────┬───────────────────────────────┘  │
│                       ↓                                  │
│  ┌────────────────────────────────────────────────────┐  │
│  │  App State (single ownership)                      │  │
│  │  - LogBuffer (VecDeque<LogEntry>)                  │  │
│  │  - StatusBar (vault info, stats)                   │  │
│  │  - ReplState (input, history, cursor)              │  │
│  │  - RenderState (dirty flags)                       │  │
│  └────────────────────┬───────────────────────────────┘  │
│                       ↓                                  │
│  ┌────────────────────────────────────────────────────┐  │
│  │  Render Engine (ratatui)                           │  │
│  │  - Header widget (conditional render)              │  │
│  │  - Logs widget (with scrollback)                   │  │
│  │  - REPL widget (with syntax highlighting)          │  │
│  └────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────────┐
│                   Worker Threads                         │
│                                                          │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  │
│  │   Watcher    │→ │    Parser    │→ │   Indexer    │  │
│  │  (notify)    │  │ (frontmatter)│  │ (SurrealDB)  │  │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘  │
│         │                 │                 │          │
│         └─────────────────┴─────────────────┘          │
│                           ↓                            │
│                    Log Event Sender                    │
│                   (mpsc::Sender<LogEvent>)             │
└──────────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────────┐
│                   REPL Executor                          │
│  ┌────────────────────────────────────────────────────┐  │
│  │  Command Parser                                    │  │
│  │  - Built-in commands (:help, :stats, :quit)       │  │
│  │  - SurrealQL queries (via surrealdb client)       │  │
│  │  - Tool execution (:run <tool> <args>)            │  │
│  └────────────────────┬───────────────────────────────┘  │
│                       ↓                                  │
│         Result Sender (oneshot::Sender<ReplResult>)      │
└──────────────────────────────────────────────────────────┘
```

### Message Types

```rust
/// Events that trigger UI updates
enum UiEvent {
    /// User input from terminal
    Input(crossterm::event::Event),

    /// Log entry from worker threads
    Log(LogEntry),

    /// Status update (doc count, DB size, etc.)
    Status(StatusUpdate),

    /// REPL command execution result
    ReplResult(ReplResult),

    /// Terminal resize
    Resize { width: u16, height: u16 },

    /// Graceful shutdown request
    Shutdown,
}

/// Log entry structure
struct LogEntry {
    timestamp: chrono::DateTime<chrono::Utc>,
    level: tracing::Level,
    target: String, // Module path
    message: String,
    fields: HashMap<String, String>, // Structured fields
}

/// Status bar information
struct StatusUpdate {
    vault_path: Option<PathBuf>,
    db_type: Option<String>,
    doc_count: Option<u64>,
    db_size: Option<u64>,
}

/// REPL execution result
enum ReplResult {
    Success { output: String, duration: Duration },
    Error { message: String },
    Table { headers: Vec<String>, rows: Vec<Vec<String>> },
}
```

## State Management Design

### App State Structure

```rust
/// Main application state (single ownership, lives on main thread)
pub struct App {
    /// Current UI mode
    mode: AppMode,

    /// Log buffer (ring buffer)
    logs: LogBuffer,

    /// Status bar state
    status: StatusBar,

    /// REPL state
    repl: ReplState,

    /// Render optimization
    render_state: RenderState,

    /// Scroll state for log window
    log_scroll: ScrollState,

    /// Channel receivers (for event loop)
    log_rx: mpsc::Receiver<LogEntry>,
    status_rx: mpsc::Receiver<StatusUpdate>,
    repl_rx: mpsc::Receiver<ReplResult>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AppMode {
    /// Normal operation
    Running,
    /// REPL input active
    Input,
    /// Scrolling through logs
    Scrolling,
    /// Shutting down
    Exiting,
}

/// Log buffer with fixed capacity
pub struct LogBuffer {
    entries: VecDeque<LogEntry>,
    capacity: usize,
}

/// Status bar information
pub struct StatusBar {
    vault_path: PathBuf,
    db_type: String,
    doc_count: u64,
    db_size: u64,
    last_update: Instant,
}

/// REPL state management
pub struct ReplState {
    /// Current input buffer
    input: String,

    /// Cursor position in input
    cursor: usize,

    /// Command history (ring buffer)
    history: VecDeque<String>,
    history_capacity: usize,
    history_index: Option<usize>,

    /// Current execution state
    execution_state: ExecutionState,

    /// Last result
    last_result: Option<ReplResult>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExecutionState {
    Idle,
    Executing,
}

/// Render optimization state
struct RenderState {
    header_dirty: bool,
    logs_dirty: bool,
    repl_dirty: bool,
}

/// Log scroll state
struct ScrollState {
    /// Current scroll offset (0 = bottom/latest)
    offset: usize,
    /// Whether auto-scroll is enabled
    auto_scroll: bool,
}
```

### State Update Patterns

```rust
impl App {
    /// Handle incoming event and update state
    fn handle_event(&mut self, event: UiEvent) -> anyhow::Result<()> {
        match event {
            UiEvent::Input(input) => self.handle_input(input)?,
            UiEvent::Log(entry) => self.handle_log(entry),
            UiEvent::Status(update) => self.handle_status(update),
            UiEvent::ReplResult(result) => self.handle_repl_result(result),
            UiEvent::Resize { width, height } => self.handle_resize(width, height),
            UiEvent::Shutdown => self.mode = AppMode::Exiting,
        }
        Ok(())
    }

    /// Update log buffer and mark dirty
    fn handle_log(&mut self, entry: LogEntry) {
        self.logs.push(entry);

        // Auto-scroll to latest if enabled
        if self.log_scroll.auto_scroll {
            self.log_scroll.offset = 0;
        }

        self.render_state.logs_dirty = true;
    }

    /// Update status bar
    fn handle_status(&mut self, update: StatusUpdate) {
        self.status.apply_update(update);
        self.render_state.header_dirty = true;
    }

    /// Process keyboard input
    fn handle_input(&mut self, event: crossterm::event::Event) -> anyhow::Result<()> {
        use crossterm::event::{Event, KeyCode, KeyModifiers};

        match event {
            Event::Key(key) => match (key.code, key.modifiers) {
                // Ctrl+C - quit
                (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                    self.mode = AppMode::Exiting;
                }

                // Ctrl+D - quit (Unix convention)
                (KeyCode::Char('d'), KeyModifiers::CONTROL) => {
                    self.mode = AppMode::Exiting;
                }

                // Enter - submit command
                (KeyCode::Enter, _) if self.mode == AppMode::Input => {
                    self.submit_command()?;
                }

                // Up/Down - history navigation
                (KeyCode::Up, _) if self.mode == AppMode::Input => {
                    self.repl.history_prev();
                    self.render_state.repl_dirty = true;
                }
                (KeyCode::Down, _) if self.mode == AppMode::Input => {
                    self.repl.history_next();
                    self.render_state.repl_dirty = true;
                }

                // Page Up/Down - scroll logs
                (KeyCode::PageUp, _) => {
                    self.log_scroll.scroll_up(10);
                    self.log_scroll.auto_scroll = false;
                    self.render_state.logs_dirty = true;
                }
                (KeyCode::PageDown, _) => {
                    self.log_scroll.scroll_down(10);
                    if self.log_scroll.offset == 0 {
                        self.log_scroll.auto_scroll = true;
                    }
                    self.render_state.logs_dirty = true;
                }

                // Char input
                (KeyCode::Char(c), _) => {
                    self.repl.insert_char(c);
                    self.render_state.repl_dirty = true;
                }

                // Backspace
                (KeyCode::Backspace, _) => {
                    self.repl.delete_char();
                    self.render_state.repl_dirty = true;
                }

                // Left/Right - cursor movement
                (KeyCode::Left, _) => {
                    self.repl.move_cursor_left();
                    self.render_state.repl_dirty = true;
                }
                (KeyCode::Right, _) => {
                    self.repl.move_cursor_right();
                    self.render_state.repl_dirty = true;
                }

                _ => {}
            },

            Event::Resize(width, height) => {
                // Mark all sections dirty on resize
                self.render_state.mark_all_dirty();
            }

            _ => {}
        }

        Ok(())
    }
}
```

## Event Loop Architecture

### Async Event Handling with Tokio

**Decision**: Use tokio::select! for multiplexed async event handling rather than blocking I/O or callbacks.

**Rationale**:
- Non-blocking: Worker threads never block UI updates
- Composable: Easy to add new event sources (HTTP server, IPC, etc.)
- Cancellation-safe: Clean shutdown with drop guards
- Integrates with existing tokio-based components (watcher, DB client)

**Implementation Pattern**:

```rust
pub async fn run_tui(
    log_rx: mpsc::Receiver<LogEntry>,
    status_rx: mpsc::Receiver<StatusUpdate>,
    repl_executor: Arc<ReplExecutor>,
    config: Config,
) -> anyhow::Result<()> {
    // Setup terminal
    let mut terminal = setup_terminal()?;

    // Setup crossterm event stream
    let mut event_stream = EventStream::new();

    // Initialize app state
    let mut app = App::new(log_rx, status_rx, config);

    // Main event loop
    loop {
        // Render UI if any section is dirty
        if app.render_state.is_dirty() {
            terminal.draw(|frame| {
                render(&mut app, frame);
            })?;
            app.render_state.clear_dirty();
        }

        // Wait for next event (select! is cancellation-safe)
        tokio::select! {
            // Keyboard/mouse input
            Some(Ok(event)) = event_stream.next() => {
                app.handle_event(UiEvent::Input(event))?;
            }

            // Log entry from worker threads
            Some(log_entry) = app.log_rx.recv() => {
                app.handle_event(UiEvent::Log(log_entry))?;
            }

            // Status update
            Some(status) = app.status_rx.recv() => {
                app.handle_event(UiEvent::Status(status))?;
            }

            // REPL result
            Some(result) = app.repl_rx.recv() => {
                app.handle_event(UiEvent::ReplResult(result))?;
            }

            // Shutdown signal
            _ = tokio::signal::ctrl_c() => {
                app.mode = AppMode::Exiting;
            }
        }

        // Exit on quit command
        if app.mode == AppMode::Exiting {
            break;
        }
    }

    // Cleanup
    restore_terminal(terminal)?;

    Ok(())
}
```

### Crossterm Event Handling

**Decision**: Use crossterm's async EventStream rather than blocking read_event().

**Rationale**:
- Non-blocking: Allows concurrent processing of worker events
- Integrates with tokio::select!
- Supports terminal resize events
- Handles signals gracefully

**Setup**:

```rust
use crossterm::{
    event::{EventStream, Event, KeyCode, KeyModifiers},
    terminal::{enable_raw_mode, disable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    execute,
};
use futures::StreamExt;

fn setup_terminal() -> anyhow::Result<Terminal<CrosstermBackend<std::io::Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;

    Ok(terminal)
}

fn restore_terminal(mut terminal: Terminal<CrosstermBackend<std::io::Stdout>>) -> anyhow::Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}
```

## Widget Composition

### Layout Structure

**Decision**: Use ratatui's Layout with Constraint::Percentage for responsive sizing.

**Layout Breakdown**:
```
┌─────────────────────────────────────────┐
│ Header (1 line, fixed)                  │ Constraint::Length(1)
├─────────────────────────────────────────┤
│                                         │
│ Logs (flexible, ~70%)                   │ Constraint::Percentage(70)
│                                         │
│                                         │
├─────────────────────────────────────────┤
│                                         │
│ REPL (flexible, ~30%)                   │ Constraint::Percentage(30)
│                                         │
└─────────────────────────────────────────┘
```

**Implementation**:

```rust
use ratatui::{
    layout::{Constraint, Direction, Layout},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    Frame,
};

fn render(app: &mut App, frame: &mut Frame) {
    // Main layout: Header | Logs | REPL
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),      // Header
            Constraint::Percentage(70), // Logs
            Constraint::Percentage(30), // REPL
        ])
        .split(frame.area());

    // Render header (always, but cheap)
    if app.render_state.header_dirty {
        render_header(app, frame, chunks[0]);
    }

    // Render logs
    if app.render_state.logs_dirty {
        render_logs(app, frame, chunks[1]);
    }

    // Render REPL
    if app.render_state.repl_dirty {
        render_repl(app, frame, chunks[2]);
    }
}
```

### Header Widget

**Design**: Single-line status bar with vault info and stats.

```rust
fn render_header(app: &App, frame: &mut Frame, area: Rect) {
    let status = &app.status;

    // Format: "Crucible v0.1.0 | /path/to/vault | SurrealDB | 43 docs | 2.3MB"
    let text = format!(
        "Crucible v{} | {} | {} | {} docs | {}",
        env!("CARGO_PKG_VERSION"),
        status.vault_path.display(),
        status.db_type,
        status.doc_count,
        format_bytes(status.db_size),
    );

    let header = Paragraph::new(text)
        .style(Style::default()
            .bg(Color::Blue)
            .fg(Color::White)
            .add_modifier(Modifier::BOLD));

    frame.render_widget(header, area);
}
```

### Logs Widget

**Design**: Scrollable list of log entries with color-coded levels.

```rust
fn render_logs(app: &App, frame: &mut Frame, area: Rect) {
    let log_entries: Vec<ListItem> = app.logs.entries()
        .skip(app.log_scroll.offset)
        .map(|entry| {
            let level_style = match entry.level {
                tracing::Level::ERROR => Style::default().fg(Color::Red),
                tracing::Level::WARN => Style::default().fg(Color::Yellow),
                tracing::Level::INFO => Style::default().fg(Color::Green),
                tracing::Level::DEBUG => Style::default().fg(Color::Cyan),
                tracing::Level::TRACE => Style::default().fg(Color::Gray),
            };

            // Format: "12:34:56 INFO  Indexed file.md (23ms)"
            let timestamp = entry.timestamp.format("%H:%M:%S");
            let content = vec![
                Span::styled(format!("{} ", timestamp), Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{:<5} ", entry.level), level_style),
                Span::raw(&entry.message),
            ];

            ListItem::new(Line::from(content))
        })
        .collect();

    let logs_list = List::new(log_entries)
        .block(Block::default()
            .borders(Borders::TOP)
            .title(format!(" Logs ({} buffered) ", app.logs.len())));

    frame.render_widget(logs_list, area);
}
```

### REPL Widget

**Design**: Multi-line input area with command history and result display.

```rust
fn render_repl(app: &App, frame: &mut Frame, area: Rect) {
    // Split REPL area: result display (70%) | input (30%)
    let repl_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(70), // Last result
            Constraint::Percentage(30), // Input
        ])
        .split(area);

    // Render last result
    if let Some(result) = &app.repl.last_result {
        render_repl_result(result, frame, repl_chunks[0]);
    }

    // Render input area
    render_repl_input(&app.repl, frame, repl_chunks[1]);
}

fn render_repl_result(result: &ReplResult, frame: &mut Frame, area: Rect) {
    match result {
        ReplResult::Success { output, duration } => {
            let text = format!("{}\n\n({}ms)", output, duration.as_millis());
            let widget = Paragraph::new(text)
                .block(Block::default()
                    .borders(Borders::TOP)
                    .title(" Result "))
                .wrap(ratatui::widgets::Wrap { trim: false });

            frame.render_widget(widget, area);
        }

        ReplResult::Error { message } => {
            let widget = Paragraph::new(message.clone())
                .style(Style::default().fg(Color::Red))
                .block(Block::default()
                    .borders(Borders::TOP)
                    .title(" Error "));

            frame.render_widget(widget, area);
        }

        ReplResult::Table { headers, rows } => {
            // Render as formatted table (use tabled or manual formatting)
            render_table(headers, rows, frame, area);
        }
    }
}

fn render_repl_input(repl: &ReplState, frame: &mut Frame, area: Rect) {
    let prompt = "> ";
    let input_text = format!("{}{}", prompt, repl.input);

    let input = Paragraph::new(input_text)
        .block(Block::default()
            .borders(Borders::TOP)
            .title(match repl.execution_state {
                ExecutionState::Idle => " REPL (SurrealQL | :help) ",
                ExecutionState::Executing => " Executing... ",
            }));

    frame.render_widget(input, area);

    // Position cursor
    if repl.execution_state == ExecutionState::Idle {
        let cursor_x = area.x + prompt.len() as u16 + repl.cursor as u16;
        let cursor_y = area.y + 1; // Account for block border
        frame.set_cursor(cursor_x, cursor_y);
    }
}
```

## Performance Optimizations

### 1. Minimize Allocations in Hot Path

**Pattern**: Pre-allocate buffers and reuse where possible.

```rust
// Bad: Allocates on every render
fn render_logs_bad(app: &App, frame: &mut Frame, area: Rect) {
    let items: Vec<ListItem> = app.logs.entries()
        .map(|e| ListItem::new(format!("{}", e))) // Allocates String
        .collect();
}

// Good: Reuse String buffer
fn render_logs_good(app: &App, frame: &mut Frame, area: Rect) {
    let mut buffer = String::with_capacity(256);
    let items: Vec<ListItem> = app.logs.entries()
        .map(|e| {
            buffer.clear();
            write!(&mut buffer, "{}", e).unwrap();
            ListItem::new(buffer.clone()) // Only clone final result
        })
        .collect();
}
```

### 2. Batch Log Updates

**Pattern**: Drain channel in batches to reduce render calls.

```rust
// In event loop:
tokio::select! {
    Some(log_entry) = app.log_rx.recv() => {
        // Drain all available logs in one go
        let mut logs = vec![log_entry];
        while let Ok(entry) = app.log_rx.try_recv() {
            logs.push(entry);
        }

        // Batch update
        for log in logs {
            app.logs.push(log);
        }

        app.render_state.logs_dirty = true;
    }
}
```

### 3. Debounce High-Frequency Events

**Pattern**: Rate-limit status updates to avoid render spam.

```rust
// In App struct:
struct App {
    // ...
    last_status_update: Instant,
    status_update_throttle: Duration, // e.g., 100ms
}

// In event handler:
fn handle_status(&mut self, update: StatusUpdate) {
    let now = Instant::now();
    if now.duration_since(self.last_status_update) < self.status_update_throttle {
        return; // Drop update
    }

    self.status.apply_update(update);
    self.render_state.header_dirty = true;
    self.last_status_update = now;
}
```

## Integration with Worker Threads

### Tracing → TUI Log Bridge

**Design**: Custom tracing subscriber that forwards events to UI channel.

```rust
use tracing_subscriber::{layer::SubscriberExt, Layer};

/// Custom layer that sends log events to TUI
struct TuiLayer {
    sender: mpsc::Sender<LogEntry>,
}

impl<S> Layer<S> for TuiLayer
where
    S: tracing::Subscriber,
{
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        // Extract event metadata
        let metadata = event.metadata();
        let mut visitor = LogVisitor::default();
        event.record(&mut visitor);

        let entry = LogEntry {
            timestamp: chrono::Utc::now(),
            level: *metadata.level(),
            target: metadata.target().to_string(),
            message: visitor.message,
            fields: visitor.fields,
        };

        // Send to UI (non-blocking, drop if full)
        let _ = self.sender.try_send(entry);
    }
}

// Setup in main:
fn setup_logging(log_tx: mpsc::Sender<LogEntry>) -> anyhow::Result<()> {
    let tui_layer = TuiLayer { sender: log_tx };
    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::fs::File::create("~/.crucible/daemon.log")?);

    tracing_subscriber::registry()
        .with(tui_layer)
        .with(file_layer)
        .init();

    Ok(())
}
```

### Watcher → Parser → DB Pipeline

**Integration Points**:

```rust
// In watcher thread:
async fn watch_loop(
    vault_path: PathBuf,
    log_tx: mpsc::Sender<LogEntry>,
    status_tx: mpsc::Sender<StatusUpdate>,
) {
    // ... notify setup ...

    loop {
        match rx.recv().await {
            Some(Ok(events)) => {
                for event in events {
                    tracing::info!("File changed: {:?}", event.path); // → TUI logs

                    // Parse and index
                    let doc = parse_markdown(&event.path).await?;
                    index_document(&db, doc).await?;

                    // Update stats
                    let stats = db.get_stats().await?;
                    status_tx.send(StatusUpdate {
                        doc_count: Some(stats.total_docs),
                        db_size: Some(stats.size_bytes),
                        ..Default::default()
                    }).await?;
                }
            }
            Some(Err(e)) => {
                tracing::error!("Watcher error: {}", e); // → TUI logs
            }
            None => break,
        }
    }
}
```

## Error Handling

### Error Boundary Design

**Principle**: Worker thread errors should not crash the UI. UI should display error state and continue running.

```rust
// In event loop:
tokio::select! {
    Some(log_entry) = app.log_rx.recv() => {
        // Infallible: log buffer can't fail
        app.handle_event(UiEvent::Log(log_entry)).unwrap();
    }

    Some(result) = app.repl_rx.recv() => {
        // REPL errors are part of result enum
        app.handle_event(UiEvent::ReplResult(result)).unwrap();
    }
}

// Worker threads send errors as log events:
async fn parser_worker() {
    if let Err(e) = parse_file().await {
        tracing::error!("Parse failed: {}", e); // Shows in TUI
        // Continue running, don't crash
    }
}
```

### Graceful Degradation

**Strategy**: Show partial state when subsystems fail.

```rust
// Header shows "Unknown" if stats unavailable
fn render_header(app: &App, frame: &mut Frame, area: Rect) {
    let doc_count = app.status.doc_count
        .map(|c| c.to_string())
        .unwrap_or_else(|| "?".to_string());

    let text = format!("... | {} docs | ...", doc_count);
    // ...
}
```

## Testing Strategy

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_buffer_respects_capacity() {
        let mut buffer = LogBuffer::new(3);
        buffer.push(log_entry("A"));
        buffer.push(log_entry("B"));
        buffer.push(log_entry("C"));
        buffer.push(log_entry("D")); // Should evict "A"

        assert_eq!(buffer.len(), 3);
        assert_eq!(buffer.entries()[0].message, "B");
    }

    #[test]
    fn repl_history_navigation() {
        let mut repl = ReplState::new(10);
        repl.add_history("SELECT * FROM notes");
        repl.add_history(":help");

        repl.history_prev();
        assert_eq!(repl.input, ":help");

        repl.history_prev();
        assert_eq!(repl.input, "SELECT * FROM notes");
    }
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_event_loop_shutdown() {
    let (log_tx, log_rx) = mpsc::channel(100);
    let (status_tx, status_rx) = mpsc::channel(10);

    // Spawn TUI in background
    let handle = tokio::spawn(async move {
        run_tui(log_rx, status_rx, config).await
    });

    // Send shutdown signal
    tokio::signal::ctrl_c().await.unwrap();

    // Verify clean shutdown
    let result = handle.await.unwrap();
    assert!(result.is_ok());
}
```

## Configuration

```rust
/// TUI-specific configuration
#[derive(Debug, Clone, serde::Deserialize)]
pub struct TuiConfig {
    /// Log buffer capacity (number of lines)
    #[serde(default = "default_log_capacity")]
    pub log_capacity: usize,

    /// REPL history capacity
    #[serde(default = "default_history_capacity")]
    pub history_capacity: usize,

    /// Status update throttle (milliseconds)
    #[serde(default = "default_status_throttle")]
    pub status_throttle_ms: u64,

    /// Log/REPL split ratio (percentage for logs)
    #[serde(default = "default_split_ratio")]
    pub log_split_ratio: u16,
}

fn default_log_capacity() -> usize { 20 }
fn default_history_capacity() -> usize { 100 }
fn default_status_throttle_ms() -> u64 { 100 }
fn default_split_ratio() -> u16 { 70 }
```

## Future Enhancements

### Potential Improvements (Post-PoC)

1. **Syntax Highlighting**: Use syntect for SurrealQL syntax highlighting in REPL
2. **Autocomplete**: Tab completion for commands and SurrealQL keywords
3. **Multi-pane**: Split view for concurrent query results
4. **Log Filtering**: Filter logs by level/module in real-time
5. **Themes**: Configurable color schemes
6. **Mouse Support**: Click to focus panes, scroll with mouse wheel

### Performance Monitoring

```rust
// Add performance metrics to status bar
struct StatusBar {
    // ...
    render_time_ms: f64,
    event_queue_depth: usize,
}
```

## References

- [ratatui Documentation](https://docs.rs/ratatui)
- [Tokio Select Macro](https://docs.rs/tokio/latest/tokio/macro.select.html)
- [Crossterm Events](https://docs.rs/crossterm/latest/crossterm/event/index.html)
- [Tracing Subscriber Layers](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/layer/index.html)

## Appendix: Complete Type Definitions

See `/home/moot/crucible/crates/crucible-cli/src/tui/` for full implementation (to be created).

**Recommended File Structure**:
```
crates/crucible-cli/src/tui/
├── mod.rs              # Public API, run_tui()
├── app.rs              # App state struct
├── events.rs           # UiEvent enum, event handling
├── widgets/
│   ├── mod.rs
│   ├── header.rs       # Header widget
│   ├── logs.rs         # Logs widget
│   └── repl.rs         # REPL widget
├── log_buffer.rs       # LogBuffer implementation
├── repl_state.rs       # ReplState implementation
└── tracing_layer.rs    # TuiLayer for tracing integration
```

---

**Document Status**: Ready for Implementation Review
**Next Steps**: Skeleton code implementation, integration with crucible-watch
