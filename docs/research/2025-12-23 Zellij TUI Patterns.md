# Zellij TUI Implementation Patterns Research

**Date:** 2025-12-23
**Repository:** https://github.com/zellij-org/zellij
**Version Analyzed:** v0.44.0
**Purpose:** Extract TUI architecture patterns for Crucible chat interface

## Executive Summary

Zellij is a terminal multiplexer written in Rust that **does not use ratatui/tui-rs**. Instead, it uses a **custom rendering engine** that generates raw VTE/ANSI escape codes. The architecture is based on a **client-server model** with WASM-based plugins for extensibility.

**Key Insight:** Zellij's approach is fundamentally different from typical ratatui-based TUIs. It's closer to tmux's model, with the server maintaining terminal state and rendering directly to VTE codes.

## Tech Stack

### Core Libraries

| Component | Library | Purpose |
|-----------|---------|---------|
| **Terminal Parsing** | `vte` (0.11.0) | VTE escape sequence parsing |
| **Terminal Input** | `termwiz` (0.23.2) | Keyboard/mouse input handling |
| **ANSI Styling** | `ansi_term` (0.12.1) | Color and style codes |
| **Plugin Runtime** | `wasmi` (0.51.1) | WASM interpreter for plugins |
| **Async Runtime** | `tokio` (1.38.1) | Async task handling |
| **IPC** | `interprocess` (1.2.1) | Client-server communication |

### Key Absence

**No ratatui or tui-rs** - They render directly to ANSI/VTE codes via string formatting.

## Architecture

### High-Level Structure

```
┌─────────────────────────────────────────────────────────────┐
│                      Zellij Binary                          │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  ┌──────────────┐              ┌──────────────┐            │
│  │ zellij-client│◄────IPC─────►│zellij-server │            │
│  │              │              │              │            │
│  │ - Input      │              │ - Screen     │            │
│  │ - Output     │              │ - Tabs       │            │
│  │ - Terminal   │              │ - Panes      │            │
│  └──────────────┘              │ - Plugins    │            │
│                                 │ - Rendering  │            │
│                                 └──────────────┘            │
│                                        │                    │
│                                        │                    │
│                                 ┌──────▼──────┐             │
│                                 │   Plugins   │             │
│                                 │   (WASM)    │             │
│                                 └─────────────┘             │
└─────────────────────────────────────────────────────────────┘
```

### Crate Organization

```
zellij/
├── zellij-client/        # Client-side: input handling, terminal I/O
├── zellij-server/        # Server-side: rendering, state, panes, tabs
├── zellij-utils/         # Shared types, IPC messages, data structures
├── zellij-tile/          # Plugin API (runs in WASM)
├── zellij-tile-utils/    # Plugin utilities
└── default-plugins/      # Built-in UI plugins (status-bar, tab-bar, etc.)
    ├── status-bar/
    ├── tab-bar/
    ├── strider/
    └── ...
```

### Component Separation

**Client (`zellij-client`)**
- Handles raw terminal I/O
- Parses keyboard/mouse input via `termwiz`
- Sends input events to server via IPC
- Receives rendered output from server
- Manages terminal raw mode

**Server (`zellij-server`)**
- Owns all UI state (Screen → Tabs → Panes)
- Performs VTE parsing of terminal output
- Renders panes to ANSI escape codes
- Manages plugin lifecycle
- Handles layout calculations

**Plugins (WASM)**
- Run in sandboxed WASM environment
- Render UI by returning ANSI-styled strings
- Subscribe to events (tab changes, mode changes, etc.)
- Communicate with host via protobuf

## Key Patterns

### 1. Widget Composition

Zellij doesn't use "widgets" in the ratatui sense. Instead, it uses:

**Custom Component System (`zellij-server/src/ui/components/`)**

Components are parsed from special escape sequences:

```rust
// Component format: component_name;x/y/width/height;params...
// Example: "text;10/5/50/1;Hello World"

pub struct UiComponentParser<'a> {
    grid: &'a mut Grid,
    style: Style,
    arrow_fonts: bool,
}

impl UiComponentParser {
    pub fn parse(&mut self, bytes: Vec<u8>) -> Result<()> {
        // 1. Decode component name and coordinates
        // 2. Generate ANSI escape codes for the component
        // 3. Feed those codes back through VTE parser (Grid)
    }
}
```

**Available Components:**
- `text` - Styled text with color indices
- `table` - Grid of cells
- `ribbon` - Horizontal separator with arrows
- `nested_list` - Hierarchical list

**File:** `zellij-server/src/ui/components/mod.rs`

### 2. State Management

**Hierarchical State Model:**

```rust
Screen              // Top level, manages all tabs
  ├── Tab           // Contains panes, layout logic
  │   ├── TiledPanes      // Grid-based pane management
  │   │   └── Pane        // Terminal or Plugin
  │   └── FloatingPanes   // Overlays
  └── ModeInfo      // Input mode, keybinds
```

**State is owned by server, never client.**

**Screen Structure (`zellij-server/src/screen.rs`):**

```rust
pub struct Screen {
    bus: Bus<ScreenInstruction>,
    tabs: BTreeMap<usize, Tab>,
    active_tab_indices: BTreeMap<ClientId, usize>,
    mode_info: BTreeMap<ClientId, ModeInfo>,
    size: Size,
    // ... more state
}
```

**Tab manages pane geometry:**

```rust
pub struct Tab {
    tiled_panes: TiledPanes,        // Grid layout
    floating_panes: FloatingPanes,  // Overlays
    sixel_image_store: SixelImageStore,
    viewport: Viewport,
    // ... more state
}
```

**Pane trait for polymorphism:**

```rust
pub trait Pane {
    fn render(&mut self, client_id: Option<ClientId>) -> Option<String>;
    fn scroll_up(&mut self, count: usize, client_id: ClientId);
    fn scroll_down(&mut self, count: usize, client_id: ClientId);
    fn handle_pty_bytes(&mut self, bytes: VteBytes);
    // ... many more methods
}
```

**File:** `zellij-server/src/panes/mod.rs`

### 3. Event Handling

**Actor Model with Message Passing:**

Each subsystem runs in its own thread and communicates via channels.

```rust
pub enum ScreenInstruction {
    Render,
    NewTab(...),
    CloseTab(...),
    Resize(Size),
    // ... ~100+ instruction types
}

// Main loop
impl Screen {
    pub fn handle_instruction(&mut self, instruction: ScreenInstruction) {
        match instruction {
            ScreenInstruction::Render => self.render(),
            ScreenInstruction::Resize(size) => self.resize_to_screen(size),
            // ...
        }
    }
}
```

**Input Flow:**

```
User Input
   │
   ▼
termwiz::InputEvent
   │
   ▼
InputHandler::handle_input()
   │
   ▼
Map to Action (via keybinds)
   │
   ▼
ClientToServerMsg (IPC)
   │
   ▼
ScreenInstruction
   │
   ▼
Screen::handle_instruction()
```

**File:** `zellij-client/src/input_handler.rs`

### 4. Rendering System

**Direct VTE Code Generation:**

No intermediate representation like ratatui's `Frame`. Instead, they build ANSI strings directly.

**Output Pipeline (`zellij-server/src/output/mod.rs`):**

```rust
// 1. Collect character chunks from panes
struct CharacterChunk {
    terminal_characters: Vec<TerminalCharacter>,
    x: usize, // column offset
    y: usize, // row offset
    // ... selection, styles
}

// 2. Serialize to VTE codes
fn serialize_chunks(
    character_chunks: Vec<CharacterChunk>,
    // ...
) -> Result<String> {
    let mut vte_output = String::new();

    for chunk in character_chunks {
        // Write cursor position: ESC[y;xH
        vte_goto_instruction(chunk.x, chunk.y, &mut vte_output)?;

        // Write styled characters
        for character in chunk.terminal_characters {
            write_changed_styles(&mut character_styles, ...)?;
            vte_output.push(character.character);
        }
    }

    Ok(vte_output)
}
```

**Style Tracking (Optimization):**

They track current style state to avoid redundant ANSI codes:

```rust
fn write_changed_styles(
    character_styles: &mut CharacterStyles,
    current_character_styles: CharacterStyles,
    // ...
) -> Result<()> {
    // Only emit ANSI codes if style changed
    if let Some(new_styles) =
        character_styles.update_and_return_diff(&current_character_styles, ...)
    {
        write!(vte_output, "{}", new_styles)?;
    }
    Ok(())
}
```

**File:** `zellij-server/src/output/mod.rs`

### 5. Scrolling Implementation

**Grid-Based Scrollback:**

Each terminal pane maintains three buffers:

```rust
pub struct Grid {
    lines_above: VecDeque<Row>,    // Scrollback buffer
    viewport: Vec<Row>,             // Visible area
    lines_below: Vec<Row>,          // Lines after cursor (rare)
    // ...
}
```

**Scrolling Logic:**

```rust
impl Grid {
    pub fn scroll_up_one_line(&mut self) {
        // Transfer row from viewport to lines_below
        if let Some(last_line) = self.viewport.pop() {
            self.lines_below.insert(0, last_line);
        }

        // Pull row from lines_above to viewport
        if let Some(next_line) = self.lines_above.pop_back() {
            self.viewport.insert(0, next_line);
        }
    }

    pub fn scroll_down_one_line(&mut self) {
        // Reverse of scroll_up
    }
}
```

**Viewport Transfer Functions:**

```rust
fn transfer_rows_from_lines_above_to_viewport(
    lines_above: &mut VecDeque<Row>,
    viewport: &mut Vec<Row>,
    count: usize,
    max_viewport_width: usize,
) -> usize {
    // Handle line wrapping during transfer
    // Canonical rows vs wrapped rows
}
```

**Canonical Rows:** Lines that end with a newline (as opposed to wrapped lines).

**File:** `zellij-server/src/panes/grid.rs`

### 6. Plugin System Architecture

**WASM-Based Sandboxing:**

Plugins compile to WebAssembly and run in `wasmi` interpreter.

**Plugin API (`zellij-tile`):**

```rust
use zellij_tile::prelude::*;

#[derive(Default)]
struct State {
    tabs: Vec<TabInfo>,
    mode_info: ModeInfo,
    // ...
}

register_plugin!(State);

impl ZellijPlugin for State {
    fn load(&mut self, configuration: BTreeMap<String, String>) {
        // Initialize plugin
        subscribe(&[EventType::TabUpdate, EventType::ModeUpdate]);
    }

    fn update(&mut self, event: Event) -> bool {
        // Handle event, return true if should re-render
        match event {
            Event::TabUpdate(tabs) => {
                self.tabs = tabs;
                true
            }
            _ => false
        }
    }

    fn render(&mut self, rows: usize, cols: usize) {
        // Build ANSI string and print to stdout
        println!("{}", render_ui(&self.tabs, rows, cols));
    }
}
```

**Host Functions (WASM imports):**

Plugins call host via FFI:

```rust
// In plugin (WASM side):
pub fn subscribe(event_types: &[EventType]) {
    let plugin_command = PluginCommand::Subscribe(event_types);
    object_to_stdout(&protobuf_encode(plugin_command));
    unsafe { host_run_plugin_command() }; // FFI call
}
```

**Communication Flow:**

```
Plugin (WASM)
   │ stdout (protobuf)
   ▼
Plugin Worker (host)
   │ PluginInstruction
   ▼
Plugin Thread
   │ ScreenInstruction
   ▼
Screen
```

**File:** `zellij-tile/src/shim.rs`

### 7. Theming and Styling

**Palette-Based Theming:**

```rust
pub struct Palette {
    pub text_selected: StyleDeclaration,
    pub text_unselected: StyleDeclaration,
    pub ribbon_selected: StyleDeclaration,
    pub ribbon_unselected: StyleDeclaration,
    // ... more semantic colors
}

pub struct StyleDeclaration {
    pub base: PaletteColor,
    pub background: PaletteColor,
    pub emphasis_0: PaletteColor, // Highlight color
    pub emphasis_1: PaletteColor,
}

pub enum PaletteColor {
    Rgb((u8, u8, u8)),
    EightBit(u8),
}
```

**Style Application:**

```rust
// In status bar plugin
let colored_elements = ColoredElements {
    selected: SegmentStyle {
        prefix_separator: style!(bg, fg),
        char_shortcut: style!(emphasis, bg).bold(),
        // ...
    },
    unselected: SegmentStyle { /* ... */ },
};

// Usage
format!("{}{}{}",
    colored_elements.selected.char_shortcut,
    "Tab",
    RESET
)
```

**File:** `default-plugins/status-bar/src/main.rs`

### 8. Error Handling

**Context-Rich Error Propagation:**

```rust
use zellij_utils::errors::prelude::*;

fn resize_to_screen(&mut self, size: Size) -> Result<()> {
    let err_context = || format!("failed to resize to screen size: {size:#?}");

    for tab in self.tabs.values_mut() {
        tab.resize_whole_tab(size)
            .with_context(err_context)?;
    }

    Ok(())
}
```

**Non-Fatal Error Handling:**

```rust
// Extension trait for logging errors without crashing
active_tab_mut!($screen, $client_id, |tab: &mut Tab| {
    tab.close_pane(pane_id).non_fatal();
});
```

**Custom Error Types:**

```rust
#[derive(thiserror::Error, Debug)]
pub enum ScreenContext {
    #[error("Failed to resize screen")]
    ResizeScreen,
    #[error("Failed to render")]
    Render,
    // ...
}
```

### 9. Testing UI Components

**Snapshot Testing with `insta`:**

```rust
#[test]
fn render_status_bar() {
    let mut state = State::default();
    state.tabs = mock_tabs();

    let output = render_status_bar(&state, 80, 2);

    insta::assert_snapshot!(output);
}
```

**Mock Terminal State:**

They create mock `Grid` instances for testing rendering logic.

**File:** `zellij-server/src/output/unit/output_tests.rs`

## Code Examples

### Example 1: Simple Text Rendering

```rust
// zellij-server/src/ui/components/text.rs

pub fn text(
    content: Text,
    style: &Style,
    coordinates: Option<Coordinates>
) -> Vec<u8> {
    let base_style = CharacterStyles::from(style.colors.text_unselected)
        .bold(Some(AnsiCode::On));

    let (text, _) = stringify_text(
        &content,
        None,
        &coordinates,
        &style.colors,
        base_style,
    );

    match coordinates {
        Some(coords) => {
            // ESC[y;xH + style + text
            format!("{}{}{}", coords, base_style, text)
                .as_bytes()
                .to_vec()
        },
        None => {
            format!("{}{}", base_style, text)
                .as_bytes()
                .to_vec()
        }
    }
}
```

### Example 2: Mouse Event Handling

```rust
// zellij-client/src/input_handler.rs

use termwiz::input::{InputEvent, MouseEvent as TermwizMouseEvent};

impl InputHandler {
    fn handle_input(&mut self) {
        loop {
            let event = self.os_input.read_from_stdin();

            match event {
                InputEvent::Mouse(mouse_event) => {
                    let our_event = from_termwiz(
                        &mut self.mouse_old_event,
                        mouse_event
                    );

                    self.send_client_instructions.send(
                        ClientInstruction::Action(Action::MouseEvent(our_event))
                    );
                },
                InputEvent::Key(key_event) => {
                    let action = self.config.keybinds
                        .get_action_for_key(self.mode, &key_event);

                    if let Some(action) = action {
                        self.send_client_instructions.send(
                            ClientInstruction::Action(action)
                        );
                    }
                },
                // ...
            }
        }
    }
}
```

### Example 3: Pane Rendering

```rust
// zellij-server/src/panes/terminal_pane.rs

impl Pane for TerminalPane {
    fn render(&mut self, client_id: Option<ClientId>) -> Option<String> {
        // 1. Get character chunks from grid
        let character_chunks = self.grid.read_viewport_as_chunks();

        // 2. Serialize to VTE codes
        let output = serialize_chunks(
            character_chunks,
            None, // sixel_chunks
            Some(&mut self.link_handler),
            None, // sixel_image_store
            self.styled_underlines,
            Some(self.grid.size()),
        ).ok()?;

        // 3. Add cursor position if needed
        if self.is_focused {
            let cursor_output = self.grid.render_cursor(client_id);
            return Some(format!("{}{}", output, cursor_output));
        }

        Some(output)
    }
}
```

### Example 4: Layout Calculation

```rust
// zellij-server/src/panes/tiled_panes/tiled_pane_grid.rs

impl TiledPaneGrid {
    pub fn layout(&mut self, direction: Direction, space: PaneGeom) {
        let pane_count = self.panes.len();

        match direction {
            Direction::Horizontal => {
                let width_per_pane = space.cols / pane_count;
                let mut x = space.x;

                for pane in self.panes.values_mut() {
                    pane.set_geom(PaneGeom {
                        x,
                        y: space.y,
                        cols: width_per_pane,
                        rows: space.rows,
                    });
                    x += width_per_pane;
                }
            },
            Direction::Vertical => {
                // Similar but with rows
            }
        }
    }
}
```

### Example 5: Plugin UI Component

```rust
// default-plugins/status-bar/src/first_line.rs

pub fn first_line(
    mode_info: &ModeInfo,
    max_len: usize,
    colors: ColoredElements,
) -> LinePart {
    let mut output = LinePart::default();

    // Render mode indicator
    let mode_text = format!(" {} ", mode_info.mode);
    let mode_segment = format!(
        "{}{}{}{}",
        colors.selected.prefix_separator,
        colors.selected.styled_text,
        mode_text,
        colors.selected.suffix_separator,
    );

    output.part.push_str(&mode_segment);
    output.len += unicode_width::UnicodeWidthStr::width(mode_text.as_str());

    // Render keybinds
    for (key, action) in mode_info.keybinds.iter() {
        if output.len >= max_len {
            break;
        }

        let key_indicator = format!(
            "{}{}{}",
            colors.unselected.char_left_separator,
            key,
            colors.unselected.char_right_separator,
        );

        output.part.push_str(&key_indicator);
        output.len += key.len();
    }

    output
}
```

## Lessons for Crucible

### What to Adopt

1. **Clear State Ownership**
   - Single source of truth (Screen/Tab/Pane hierarchy)
   - State lives in one place, never duplicated
   - Use message passing for updates

2. **Trait-Based Polymorphism**
   - Define `Pane` trait for different content types (Terminal, Plugin, etc.)
   - For Crucible: `ConversationView` trait with `UserMessage`, `AssistantMessage`, `ToolUse` impls

3. **Semantic Color System**
   - Don't use raw colors in components
   - Define semantic names: `text_selected`, `ribbon_unselected`, etc.
   - Makes theming trivial

4. **Scrollback Buffer Pattern**
   - `lines_above` (VecDeque), `viewport` (Vec), `lines_below` (Vec)
   - Efficient for long chat histories
   - Consider for Crucible's conversation view

5. **Component Coordinates**
   - Pass `x/y/width/height` to rendering functions
   - Allows absolute positioning without complex layout logic

### What to Avoid

1. **Don't Skip ratatui**
   - Zellij's custom rendering is complex and error-prone
   - They essentially re-implemented what ratatui provides
   - Use ratatui for our chat TUI - it's the right tool

2. **Don't Over-Engineer Plugins**
   - WASM plugin system is overkill for our use case
   - Keep it simple: Rust-based extensibility is fine

3. **Avoid Deep Message Passing**
   - Zellij has ~10 instruction enum types
   - Too much indirection for a simple chat UI
   - Direct function calls are okay for smaller apps

4. **Don't Render Everything Every Frame**
   - Zellij has sophisticated dirty tracking
   - For chat UI: just re-render on new messages or scroll
   - Ratatui handles diffing for us anyway

### Architecture Recommendations for Crucible TUI

**Proposed Structure:**

```rust
// State (single source of truth)
pub struct ChatState {
    conversation: Conversation,      // Vec<Message>
    scroll_offset: usize,            // For scrolling
    input_buffer: String,            // User input
    mode: ChatMode,                  // Normal, Insert, etc.
}

// Rendering (use ratatui)
pub fn render_chat(f: &mut Frame, state: &ChatState, area: Rect) {
    // Render conversation
    let messages_widget = render_messages(&state.conversation, state.scroll_offset);
    f.render_widget(messages_widget, message_area);

    // Render input
    let input_widget = render_input(&state.input_buffer);
    f.render_widget(input_widget, input_area);
}

// Event handling (simple enum)
pub enum ChatEvent {
    Input(KeyEvent),
    NewMessage(Message),
    Scroll(isize),
}

impl ChatState {
    pub fn handle_event(&mut self, event: ChatEvent) {
        match event {
            ChatEvent::Input(key) => self.handle_input(key),
            ChatEvent::NewMessage(msg) => {
                self.conversation.push(msg);
                self.scroll_to_bottom();
            },
            ChatEvent::Scroll(delta) => {
                self.scroll_offset = self.scroll_offset.saturating_add_signed(delta);
            },
        }
    }
}
```

**Use ratatui's built-in widgets:**
- `Paragraph` for messages
- `List` for tool results
- `Block` for borders
- `Layout` for positioning

**Keep it simple:**
- No actor model needed (single-threaded event loop is fine)
- No custom VTE rendering (let ratatui handle it)
- No plugin system (unless truly needed)

### Specific Patterns to Borrow

**1. Scrollback Buffer (Adapted for ratatui):**

```rust
pub struct ConversationView {
    messages: VecDeque<Message>,  // All messages
    viewport_start: usize,         // First visible message index
    viewport_height: usize,        // How many messages fit on screen
}

impl ConversationView {
    pub fn scroll_up(&mut self, count: usize) {
        self.viewport_start = self.viewport_start.saturating_sub(count);
    }

    pub fn scroll_down(&mut self, count: usize) {
        let max_start = self.messages.len().saturating_sub(self.viewport_height);
        self.viewport_start = (self.viewport_start + count).min(max_start);
    }

    pub fn visible_messages(&self) -> &[Message] {
        let end = (self.viewport_start + self.viewport_height).min(self.messages.len());
        &self.messages.make_contiguous()[self.viewport_start..end]
    }
}
```

**2. Semantic Styling:**

```rust
pub struct ChatTheme {
    pub user_message: Style,
    pub assistant_message: Style,
    pub tool_use: Style,
    pub tool_result: Style,
    pub error: Style,
    pub timestamp: Style,
}

impl Default for ChatTheme {
    fn default() -> Self {
        ChatTheme {
            user_message: Style::default().fg(Color::Cyan),
            assistant_message: Style::default().fg(Color::White),
            tool_use: Style::default().fg(Color::Yellow),
            tool_result: Style::default().fg(Color::Green),
            error: Style::default().fg(Color::Red),
            timestamp: Style::default().fg(Color::DarkGray),
        }
    }
}
```

**3. Message Rendering with Wrapping:**

```rust
pub fn render_message(message: &Message, theme: &ChatTheme, width: usize) -> Vec<Line> {
    let style = match message.role {
        Role::User => theme.user_message,
        Role::Assistant => theme.assistant_message,
    };

    // Wrap text to width
    let wrapped_lines = wrap_text(&message.content, width - 4); // -4 for padding

    wrapped_lines.into_iter()
        .map(|line| Line::from(Span::styled(line, style)))
        .collect()
}
```

## Performance Considerations

### Zellij's Optimizations

1. **Incremental Rendering**
   - Only changed character cells are re-sent
   - Style state tracking avoids redundant ANSI codes
   - Dirty tracking per pane

2. **Bounded Scrollback**
   - Default: 10,000 lines
   - Configurable via `scroll_buffer_size`
   - Old lines dropped from `VecDeque`

3. **Render Caching**
   - Panes cache their last render
   - Only re-render if content changed or resize

4. **Batch Updates**
   - Multiple screen updates batched before render
   - Resize events cached and applied in bulk

### For Crucible

**Don't Prematurely Optimize:**
- Ratatui already does dirty checking
- Chat UIs update infrequently (not 60fps)
- Start simple, profile if slow

**If Needed:**
- Virtualize message list (only render visible messages)
- Limit scrollback to last N messages
- Use `StatefulWidget` for scroll state

## Testing Strategies

**From Zellij:**

1. **Snapshot Testing**
   - Use `insta` crate for UI output snapshots
   - Test rendering edge cases (long messages, unicode, etc.)

2. **Mock Terminal State**
   - Create helper functions for test messages
   - Test scroll logic with known message sets

3. **Integration Tests**
   - Test full event → state → render pipeline
   - Verify keybindings work correctly

**Example Test:**

```rust
#[test]
fn test_scroll_to_bottom() {
    let mut view = ConversationView::new();

    // Add 100 messages
    for i in 0..100 {
        view.add_message(Message::user(format!("Message {}", i)));
    }

    view.viewport_height = 10;
    view.scroll_to_bottom();

    assert_eq!(view.viewport_start, 90); // Last 10 messages visible
}
```

## Conclusion

**Key Takeaway:** Zellij's architecture is impressive but **over-engineered for a chat TUI**. They built a terminal multiplexer with plugin support, which requires:
- Custom VTE rendering
- Actor-based concurrency
- WASM sandboxing
- Complex IPC

**For Crucible's chat interface:**
- Use **ratatui** (don't reinvent the wheel)
- Keep state management **simple** (single `ChatState` struct)
- Borrow **scrollback buffer pattern** for long conversations
- Use **semantic theming** for flexibility
- Implement **event loop** without actor model

**Patterns Worth Adopting:**
1. Scrollback buffer (lines_above, viewport, lines_below)
2. Semantic color palette
3. Trait-based message rendering
4. Snapshot testing for UI

**Patterns to Avoid:**
1. Custom VTE rendering (use ratatui)
2. Actor model for simple UI (overkill)
3. Plugin system (unless truly needed)
4. Deep message passing hierarchies

**Final Architecture Sketch for Crucible:**

```
crucible-cli/src/tui/
├── mod.rs              # Main TUI entry point
├── state.rs            # ChatState struct
├── render.rs           # Render functions (using ratatui)
├── events.rs           # Event handling
├── conversation.rs     # ConversationView (scrollback)
└── theme.rs            # ChatTheme (semantic colors)
```

Simple, maintainable, and leveraging ratatui's strengths.
