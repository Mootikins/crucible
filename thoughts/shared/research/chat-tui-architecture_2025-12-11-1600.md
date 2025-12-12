---
date: 2025-12-11T16:00:00Z
researcher: Claude
topic: "Chat TUI Architecture - Inline Viewport with Fuzzy Completion"
tags: [research, architecture, tui, ratatui, chat, completion, inline-viewport]
status: complete
---

# Chat TUI Architecture: Inline Viewport Design

## Executive Summary

This document outlines the architecture for Crucible's new chat TUI using **ratatui's inline viewport mode**. This approach preserves terminal scrollback while providing a widget-based input area with fuzzy completion menus.

**Key Decision:** Use `Viewport::Inline(n)` instead of alternate screen, enabling normal terminal scrollback above a fixed-height ratatui viewport at the bottom.

## Architecture Overview

```
┌─────────────────────────────────────┐
│     Terminal Scrollback Buffer      │  ← Normal terminal, scrolls up
│  Agent: Previous response...        │
│  You: Earlier message...            │
│  Agent: Another response...         │
├─────────────────────────────────────┤
│  ┌─ Inline Viewport (8 lines) ────┐ │  ← Ratatui manages this
│  │ > your input here_              │ │  ← tui-textarea widget
│  │ ───────────────────────────── │ │  ← separator
│  │ [plan] ● Ready | @file ▾       │ │  ← status bar
│  │                                 │ │
│  │ ┌─ fuzzy menu ─────────────┐   │ │  ← popup overlay
│  │ │ > /sea                   │   │ │
│  │ │   /search                │   │ │
│  │ │   /session               │   │ │
│  │ └──────────────────────────┘   │ │
│  └─────────────────────────────────┘ │
└─────────────────────────────────────┘
```

## Core Components

### 1. Terminal Setup (Inline Mode)

```rust
use ratatui::{Terminal, TerminalOptions, Viewport};
use crossterm::terminal::enable_raw_mode;

fn setup_inline_terminal(height: u16) -> Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    // NO EnterAlternateScreen - stay in normal buffer

    let backend = CrosstermBackend::new(stdout());
    let options = TerminalOptions {
        viewport: Viewport::Inline(height),  // e.g., 8 lines
    };

    Terminal::with_options(backend, options)
}
```

### 2. Message Flow with `insert_before()`

When agent responds, push content into scrollback:

```rust
// Agent message received
terminal.insert_before(message_lines, |buf| {
    // Render agent message into buffer
    Paragraph::new(format!("Agent: {}", message))
        .style(Style::default().fg(Color::Blue))
        .render(buf.area, buf);
})?;

// Viewport stays at bottom, message scrolls up
```

### 3. Input Widget (tui-textarea)

**Recommendation:** Use `tui-textarea` for robust multiline editing.

```rust
use tui_textarea::{TextArea, Input};

struct ChatInput {
    textarea: TextArea<'static>,
    completion_state: Option<CompletionState>,
}

impl ChatInput {
    fn new() -> Self {
        let mut textarea = TextArea::default();
        // Configure for chat input
        textarea.set_cursor_line_style(Style::default());
        textarea.set_block(Block::default().borders(Borders::NONE));
        Self {
            textarea,
            completion_state: None,
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> Option<ChatAction> {
        match key.code {
            // Send on Enter (no modifiers)
            KeyCode::Enter if key.modifiers.is_empty() => {
                let msg = self.textarea.lines().join("\n");
                self.textarea = TextArea::default();
                Some(ChatAction::Send(msg))
            }
            // Newline on Ctrl+Enter
            KeyCode::Enter if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.textarea.insert_newline();
                None
            }
            // Trigger completion
            KeyCode::Char('/') if self.at_word_start() => {
                self.textarea.input(Input::from(key));
                self.show_completion(CompletionType::Command);
                None
            }
            KeyCode::Char('@') if self.at_word_start() => {
                self.textarea.input(Input::from(key));
                self.show_completion(CompletionType::FileOrAgent);
                None
            }
            // Default handling
            _ => {
                self.textarea.input(Input::from(key));
                None
            }
        }
    }
}
```

### 4. Fuzzy Completion System

**Already available:** `nucleo-matcher` in workspace dependencies.

```rust
use nucleo_matcher::{
    pattern::{CaseMatching, Normalization, Pattern},
    Config, Matcher, Utf32Str,
};

struct CompletionState {
    completion_type: CompletionType,
    query: String,
    all_items: Vec<CompletionItem>,
    filtered_items: Vec<CompletionItem>,
    selected_index: usize,
    multi_select: bool,
    selections: Vec<usize>,  // For checkbox mode
}

impl CompletionState {
    fn refilter(&mut self) {
        let mut matcher = Matcher::new(Config::DEFAULT);
        let pattern = Pattern::parse(
            &self.query,
            CaseMatching::Ignore,
            Normalization::Smart,
        );

        let mut matches: Vec<_> = self.all_items
            .iter()
            .enumerate()
            .filter_map(|(idx, item)| {
                let mut buf = Vec::new();
                let haystack = Utf32Str::new(&item.text, &mut buf);
                pattern.score(haystack, &mut matcher)
                    .map(|score| (idx, score))
            })
            .collect();

        matches.sort_by(|a, b| b.1.cmp(&a.1));
        self.filtered_items = matches
            .into_iter()
            .map(|(idx, _)| self.all_items[idx].clone())
            .collect();

        self.selected_index = 0;
    }
}

#[derive(Clone)]
struct CompletionItem {
    text: String,
    description: Option<String>,
    item_type: CompletionType,
}

enum CompletionType {
    Command,      // /command
    File,         // @file
    Agent,        // @agent
}
```

### 5. Completion Popup Rendering

```rust
fn render_completion_popup(
    frame: &mut Frame,
    area: Rect,
    state: &CompletionState,
) {
    // Calculate popup position (above input, anchored to trigger position)
    let popup_height = (state.filtered_items.len() as u16).min(8) + 2;
    let popup_area = Rect {
        x: state.trigger_column,
        y: area.y.saturating_sub(popup_height),
        width: 40.min(area.width - state.trigger_column),
        height: popup_height,
    };

    // Clear background
    frame.render_widget(Clear, popup_area);

    // Build list items with optional checkboxes
    let items: Vec<ListItem> = state.filtered_items
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let checkbox = if state.multi_select {
                if state.selections.contains(&idx) { "[✓] " } else { "[ ] " }
            } else {
                ""
            };

            let highlight = if idx == state.selected_index {
                ">> "
            } else {
                "   "
            };

            let content = format!(
                "{}{}{}",
                highlight,
                checkbox,
                item.text,
            );

            let style = if idx == state.selected_index {
                Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            ListItem::new(content).style(style)
        })
        .collect();

    let list = List::new(items)
        .block(Block::default()
            .title(format!(" {} ", state.query))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)));

    frame.render_widget(list, popup_area);
}
```

### 6. Keybinding Architecture

**Modal input handling:**

```rust
enum InputMode {
    Normal,      // Regular text input
    Completion,  // Fuzzy menu active
    MultiSelect, // Checkbox selection
}

impl App {
    fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        // Global bindings (always active)
        if key.code == KeyCode::Char('c')
            && key.modifiers.contains(KeyModifiers::CONTROL) {
            return self.handle_interrupt();
        }

        match self.input_mode {
            InputMode::Normal => self.handle_normal_key(key),
            InputMode::Completion => self.handle_completion_key(key),
            InputMode::MultiSelect => self.handle_multiselect_key(key),
        }
    }

    fn handle_completion_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            // Navigation
            KeyCode::Up | KeyCode::Char('k') => self.completion_prev(),
            KeyCode::Down | KeyCode::Char('j') => self.completion_next(),
            KeyCode::PageUp => self.completion_page_up(),
            KeyCode::PageDown => self.completion_page_down(),

            // Selection
            KeyCode::Enter | KeyCode::Tab => self.completion_confirm(),
            KeyCode::Char(' ') if self.completion.multi_select => {
                self.completion_toggle()
            }

            // Cancel
            KeyCode::Esc => self.completion_cancel(),

            // Filter - pass to query
            KeyCode::Char(c) => {
                self.completion.query.push(c);
                self.completion.refilter();
            }
            KeyCode::Backspace => {
                self.completion.query.pop();
                if self.completion.query.is_empty() {
                    self.completion_cancel();
                } else {
                    self.completion.refilter();
                }
            }

            _ => {}
        }
        Ok(())
    }
}
```

### 7. Event Loop (Reuse Existing Pattern)

From existing `tui/mod.rs` - adapt for inline mode:

```rust
pub async fn run_chat_tui(/* channels */) -> Result<()> {
    let mut terminal = setup_inline_terminal(8)?;
    let mut app = ChatApp::new(/* ... */);

    loop {
        // Render if dirty
        if app.render_state.is_dirty() {
            terminal.draw(|frame| {
                render_chat_viewport(&mut app, frame);
            })?;
            app.render_state.clear();
        }

        // Poll events (10ms timeout)
        if event::poll(Duration::from_millis(10))? {
            if let Event::Key(key) = event::read()? {
                app.handle_key(key)?;
            }
        }

        // Process channel events
        while let Ok(chunk) = app.stream_rx.try_recv() {
            app.handle_stream_chunk(chunk)?;
        }

        if app.should_exit {
            break;
        }
    }

    // Cleanup
    disable_raw_mode()?;
    Ok(())
}
```

## Layout Structure

### Viewport Layout (8 lines)

```rust
fn render_chat_viewport(app: &mut ChatApp, frame: &mut Frame) {
    let input_height = app.input.calculate_height(frame.area().width);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(input_height),  // Input (1-5 lines)
            Constraint::Length(1),             // Separator
            Constraint::Length(1),             // Status bar
            Constraint::Min(0),                // Spacer (completion popup area)
        ])
        .split(frame.area());

    // Render widgets
    render_input(&app.input, frame, chunks[0]);
    render_separator(frame, chunks[1]);
    render_status_bar(app, frame, chunks[2]);

    // Render completion popup (overlays)
    if let Some(completion) = &app.input.completion_state {
        render_completion_popup(frame, frame.area(), completion);
    }
}
```

### Status Bar Design

```
[plan] ● Ready | /help | Ctrl+C to cancel
```

```rust
fn render_status_bar(app: &ChatApp, frame: &mut Frame, area: Rect) {
    let mode_style = match app.mode {
        ChatMode::Plan => Style::default().fg(Color::Cyan),
        ChatMode::Act => Style::default().fg(Color::Green),
        ChatMode::Auto => Style::default().fg(Color::Yellow),
    };

    let status_icon = if app.is_streaming { "⟳" } else { "●" };
    let status_text = if app.is_streaming {
        "Streaming..."
    } else {
        "Ready"
    };

    let spans = vec![
        Span::styled(
            format!("[{}]", app.mode.name()),
            mode_style.add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(status_icon, mode_style),
        Span::raw(" "),
        Span::raw(status_text),
        Span::raw(" | "),
        Span::styled("/help", Style::default().fg(Color::DarkGray)),
    ];

    let status = Paragraph::new(Line::from(spans))
        .style(Style::default().bg(Color::DarkGray));

    frame.render_widget(status, area);
}
```

## Data Flow

```
User Input
    │
    ▼
┌──────────────────┐
│  ChatApp.handle_ │
│  key()           │
└────────┬─────────┘
         │
    ┌────┴────┐
    │         │
    ▼         ▼
Normal    Completion
Mode      Mode
    │         │
    ▼         │
textarea  filter/
.input()  navigate
    │         │
    └────┬────┘
         │
         ▼
┌──────────────────┐
│  render_state.   │
│  mark_dirty()    │
└────────┬─────────┘
         │
         ▼
┌──────────────────┐
│  terminal.draw() │
└────────┬─────────┘
         │
         ▼
    ┌────┴────┐
    │         │
    ▼         ▼
Viewport  insert_
widgets   before()
          (messages)
```

## Completion Triggers

| Trigger | Type | Source | Multi-Select |
|---------|------|--------|--------------|
| `/` | Command | SlashCommandRegistry | No |
| `@` | File | `kiln.list_notes()` | Yes (for context) |
| `@@` | Agent | AgentRegistry | No |
| Tab | Context | Recent files + history | Yes |

## Dependencies

**Already in workspace:**
- `ratatui = "0.29"` - TUI framework
- `crossterm = "0.29"` - Terminal control
- `nucleo-matcher = "0.3.1"` - Fuzzy matching
- `tokio` - Async runtime

**Add:**
- `tui-textarea = "0.6"` - Text input widget

## File Structure

```
crates/crucible-cli/src/
├── chat/
│   ├── mod.rs           # Chat module exports
│   ├── session.rs       # REPLACE with TUI-based session
│   ├── handlers.rs      # Keep command handlers
│   ├── slash_registry.rs # Keep registry
│   └── ...
├── chat_tui/            # NEW module
│   ├── mod.rs           # Entry point, event loop
│   ├── app.rs           # ChatApp state
│   ├── input.rs         # ChatInput with textarea
│   ├── completion.rs    # CompletionState, fuzzy logic
│   ├── render.rs        # Viewport rendering
│   └── widgets/
│       ├── mod.rs
│       ├── input.rs
│       ├── status.rs
│       └── completion_popup.rs
└── tui/                 # Existing (reuse patterns)
    ├── mod.rs
    ├── app.rs
    └── ...
```

## Migration Path

### Phase 1: Infrastructure
1. Add `tui-textarea` dependency
2. Create `chat_tui/` module structure
3. Implement basic inline viewport setup
4. Basic input → send → display cycle

### Phase 2: Completion System
1. Implement `CompletionState` with nucleo
2. Add `/` trigger for commands
3. Render completion popup
4. Keyboard navigation

### Phase 3: Full Features
1. `@` file/agent completion
2. Multi-select with checkboxes
3. Status bar with mode/progress
4. `/clear` command (context reset)

### Phase 4: Polish
1. History navigation (up/down when empty)
2. Syntax highlighting for input
3. Streaming indicator
4. Error display

## Reusable from Existing TUI

| Component | Location | Reuse Level |
|-----------|----------|-------------|
| Event loop pattern | `tui/mod.rs:78-116` | High |
| Dirty flags | `tui/app.rs:60-83` | 100% |
| Ring buffer | `tui/log_buffer.rs` | Adapt for messages |
| Throttling | `tui/app.rs:259-272` | 100% |
| Widget pattern | `tui/widgets/*.rs` | Pattern only |
| Channel events | `tui/events.rs` | Extend |

## Success Criteria

- [ ] Chat works with normal terminal scrollback
- [ ] Inline viewport stays at bottom (8 lines)
- [ ] Agent responses scroll up via `insert_before()`
- [ ] `/command` completion with fuzzy filter
- [ ] `@file` completion with multi-select
- [ ] Status bar shows mode + streaming state
- [ ] Ctrl+C cancels current operation
- [ ] `/clear` resets context (new session)
