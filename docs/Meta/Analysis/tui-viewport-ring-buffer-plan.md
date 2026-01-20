# TUI Viewport Ring Buffer Architecture Plan

> **Status**: Draft  
> **Created**: 2026-01-20  
> **Scope**: crucible-cli TUI rendering pipeline refactor

## Executive Summary

Refactor the TUI rendering pipeline to use a bounded viewport cache with span-based rendering. The daemon owns message history; the TUI maintains a small, bounded cache of messages currently in the viewport. A compositor borrows from this cache during render, producing styled spans that convert to ANSI output.

**Key Benefits**:
- Memory-bounded viewport (terminal height × limited messages)
- No lifetime complexity (borrows scoped to render pass)
- Clean separation: daemon = source of truth, TUI = render cache
- Efficient incremental rendering via line diffing

## Current Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│ Current Flow                                                     │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ChatApp.view() → Node tree (owned Strings)                     │
│       ↓                                                          │
│  FramePlanner.plan() → FrameSnapshot                            │
│       ├── stdout_delta (graduated content → terminal scrollback)│
│       └── viewport (String, re-rendered each frame)             │
│       ↓                                                          │
│  OutputBuffer.render_with_overlays()                            │
│       ├── collapse_blank_lines()                                │
│       ├── clamp_to_viewport()                                   │
│       ├── composite_overlays()                                  │
│       └── line-diff against previous_lines                      │
│       ↓                                                          │
│  Terminal write (crossterm)                                      │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### Current Module Structure (~10k LOC in ink/)

| Module | LOC | Purpose |
|--------|-----|---------|
| `chat_app.rs` | 2628 | Main chat application component |
| `markdown.rs` | 1284 | Markdown → Node conversion |
| `render.rs` | 771 | Node → String rendering |
| `node.rs` | 686 | Node tree types |
| `style.rs` | 449 | Style types, ANSI conversion |
| `runtime.rs` | 395 | Graduation state, filtering |
| `chat_runner.rs` | 365 | Event loop, daemon integration |
| `taffy_layout.rs` | 344 | Taffy-based layout calculations |
| `layout.rs` | 341 | Layout types and helpers |
| `output.rs` | 307 | OutputBuffer, line diffing |
| `overlay.rs` | 271 | Overlay extraction/composition |
| `ansi.rs` | 294 | ANSI utilities, visible_width |
| Others | ~1500 | Focus, events, terminal, tests |

---

## Proposed Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│ New Flow                                                         │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  Daemon (owns message history via RPC)                          │
│       ↓                                                          │
│  ViewportCache (bounded, TUI-owned)                             │
│       ├── messages: VecDeque<CachedMessage>  (max ~20 messages) │
│       ├── streaming: Option<StreamingBuffer>                    │
│       └── wrapped_lines: LruCache<(MsgId, width), Vec<Line>>   │
│       ↓                                                          │
│  Compositor<'a> (borrows from cache during render)              │
│       ├── produces Vec<Span<'a>> per line                       │
│       └── handles overlays as post-process                      │
│       ↓                                                          │
│  LineBuffer (ring buffer, terminal-sized)                       │
│       ├── lines: VecDeque<RenderedLine>  (max = term height)   │
│       └── diff against previous frame → minimal writes          │
│       ↓                                                          │
│  Terminal write (crossterm StyledContent)                       │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

---

## Phase 1: Extract Crate (crucible-ink)

**Goal**: Separate the general-purpose TUI framework from chat-specific code.

### 1.1 Create New Crate Structure

```
crates/
├── crucible-ink/                    # New: General TUI framework
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── node.rs                  # Node tree types
│       ├── style.rs                 # Style, Color, ANSI conversion
│       ├── render.rs                # Node → output rendering
│       ├── layout.rs                # Layout calculation
│       ├── ansi.rs                  # ANSI utilities
│       ├── span.rs                  # NEW: Span<'a> types
│       ├── line_buffer.rs           # NEW: Ring buffer for lines
│       ├── viewport.rs              # Viewport clamping utilities
│       ├── overlay.rs               # Overlay composition
│       ├── focus.rs                 # Focus management
│       ├── event.rs                 # Input events
│       ├── component.rs             # Component trait
│       ├── terminal.rs              # Terminal abstraction
│       └── testing/                 # Test harness
│           ├── mod.rs
│           └── harness.rs
│
├── crucible-cli/
│   └── src/tui/
│       ├── mod.rs
│       ├── chat_app.rs              # Chat-specific component
│       ├── chat_runner.rs           # Event loop + daemon
│       ├── markdown.rs              # Markdown rendering
│       └── agent_selection.rs       # Agent picker
```

### 1.2 Public API Surface

```rust
// crucible-ink/src/lib.rs
pub mod node;       // Node, text, col, row, styled, etc.
pub mod style;      // Style, Color
pub mod span;       // Span<'a>, StyledSpan<'a>
pub mod render;     // render_to_spans(), render_to_string()
pub mod layout;     // Size, Direction, Padding, Gap
pub mod component;  // Component trait, ComponentHarness
pub mod terminal;   // Terminal abstraction
pub mod focus;      // FocusContext
pub mod event;      // Event, KeyEvent
pub mod testing;    // Test utilities

// Re-exports for convenience
pub use node::*;
pub use style::{Style, Color};
pub use component::Component;
```

### 1.3 Migration Steps

1. Create `crates/crucible-ink/Cargo.toml`
2. Move non-chat modules with `pub use` re-exports
3. Update `crucible-cli` to depend on `crucible-ink`
4. Ensure all tests pass
5. Commit: `refactor(tui): extract crucible-ink crate`

### 1.4 Tests

```rust
// crucible-ink/src/lib.rs
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn smoke_test_node_creation() {
        let node = col([
            text("Hello"),
            styled("World", Style::new().bold()),
        ]);
        // ... assert structure
    }
}
```

**Test files to migrate**:
- `tests/node_tests.rs`
- `tests/render_tests.rs`
- `tests/layout_tests.rs`
- `tests/focus_tests.rs`
- `tests/error_boundary_tests.rs`

---

## Phase 2: Span-Based Rendering

**Goal**: Introduce `Span<'a>` as intermediate representation between Nodes and ANSI strings.

### 2.1 New Types

```rust
// crucible-ink/src/span.rs

/// A borrowed text span with styling.
/// Lifetime 'a ties to the source content (ViewportCache).
#[derive(Debug, Clone, Copy)]
pub struct Span<'a> {
    pub text: &'a str,
    pub style: Style,
}

impl<'a> Span<'a> {
    pub fn new(text: &'a str, style: Style) -> Self {
        Self { text, style }
    }
    
    pub fn plain(text: &'a str) -> Self {
        Self { text, style: Style::default() }
    }
    
    /// Visible width (excluding ANSI codes, handling Unicode)
    pub fn width(&self) -> usize {
        unicode_width::UnicodeWidthStr::width(self.text)
    }
    
    /// Convert to ANSI string
    pub fn to_ansi(&self) -> String {
        if self.style == Style::default() {
            self.text.to_string()
        } else {
            format!("{}{}\x1b[0m", self.style.to_ansi_codes(), self.text)
        }
    }
}

/// A line composed of spans
#[derive(Debug, Clone)]
pub struct SpanLine<'a> {
    pub spans: Vec<Span<'a>>,
}

impl<'a> SpanLine<'a> {
    pub fn new(spans: Vec<Span<'a>>) -> Self {
        Self { spans }
    }
    
    pub fn width(&self) -> usize {
        self.spans.iter().map(|s| s.width()).sum()
    }
    
    pub fn to_ansi(&self) -> String {
        self.spans.iter().map(|s| s.to_ansi()).collect()
    }
}
```

### 2.2 Render Pipeline Changes

```rust
// New render output type
pub enum RenderOutput<'a> {
    Spans(Vec<SpanLine<'a>>),
    String(String),  // Fallback for complex nodes
}

// Add span-based rendering alongside string rendering
pub fn render_to_spans<'a>(
    node: &'a Node,
    width: usize,
    content_source: &'a dyn ContentSource,
) -> Vec<SpanLine<'a>> {
    // ...
}

/// Trait for content that can be borrowed during render
pub trait ContentSource {
    fn get_content(&self, id: &str) -> Option<&str>;
}
```

### 2.3 Tests

```rust
#[test]
fn span_width_calculation() {
    let span = Span::new("hello", Style::new());
    assert_eq!(span.width(), 5);
    
    let wide = Span::new("你好", Style::new());
    assert_eq!(wide.width(), 4);  // CJK double-width
}

#[test]
fn span_to_ansi_plain() {
    let span = Span::plain("hello");
    assert_eq!(span.to_ansi(), "hello");
}

#[test]
fn span_to_ansi_styled() {
    let span = Span::new("hello", Style::new().bold());
    assert!(span.to_ansi().contains("\x1b[1m"));
    assert!(span.to_ansi().contains("hello"));
}

#[test]
fn span_line_composition() {
    let line = SpanLine::new(vec![
        Span::new("Hello ", Style::new()),
        Span::new("World", Style::new().bold()),
    ]);
    assert_eq!(line.width(), 11);
}
```

---

## Phase 3: ViewportCache

**Goal**: Bounded message cache that the compositor borrows from.

### 3.1 Types

```rust
// crucible-cli/src/tui/viewport_cache.rs

use std::collections::VecDeque;
use arcstr::ArcStr;  // or Arc<str>

/// Maximum messages to keep in viewport cache
const MAX_CACHED_MESSAGES: usize = 32;

/// A cached message with pre-wrapped lines
#[derive(Debug)]
pub struct CachedMessage {
    pub id: String,
    pub role: Role,
    pub content: ArcStr,  // Immutable, cheap to clone
    /// Wrapped lines cache, keyed by width
    wrapped: Option<(usize, Vec<String>)>,
}

impl CachedMessage {
    pub fn new(id: String, role: Role, content: impl Into<ArcStr>) -> Self {
        Self {
            id,
            role,
            content: content.into(),
            wrapped: None,
        }
    }
    
    /// Get wrapped lines for given width, computing if needed
    pub fn wrapped_lines(&mut self, width: usize) -> &[String] {
        if self.wrapped.as_ref().map(|(w, _)| *w) != Some(width) {
            let lines = wrap_content(&self.content, width);
            self.wrapped = Some((width, lines));
        }
        &self.wrapped.as_ref().unwrap().1
    }
}

/// Bounded cache of messages for viewport rendering
pub struct ViewportCache {
    messages: VecDeque<CachedMessage>,
    streaming: Option<StreamingBuffer>,
    anchor: Option<ViewportAnchor>,
}

/// Anchor point for resize stability
pub struct ViewportAnchor {
    pub message_id: String,
    pub line_offset: usize,
}

impl ViewportCache {
    pub fn new() -> Self {
        Self {
            messages: VecDeque::with_capacity(MAX_CACHED_MESSAGES),
            streaming: None,
            anchor: None,
        }
    }
    
    /// Add a message, evicting oldest if at capacity
    pub fn push_message(&mut self, msg: CachedMessage) {
        if self.messages.len() >= MAX_CACHED_MESSAGES {
            self.messages.pop_front();
        }
        self.messages.push_back(msg);
    }
    
    /// Get content by message ID (for compositor to borrow)
    pub fn get_content(&self, id: &str) -> Option<&str> {
        self.messages
            .iter()
            .find(|m| m.id == id)
            .map(|m| m.content.as_str())
    }
    
    /// Start streaming content
    pub fn start_streaming(&mut self) {
        self.streaming = Some(StreamingBuffer::new());
    }
    
    /// Append to streaming buffer
    pub fn append_streaming(&mut self, delta: &str) {
        if let Some(ref mut buf) = self.streaming {
            buf.append(delta);
        }
    }
    
    /// Complete streaming → becomes a cached message
    pub fn complete_streaming(&mut self, id: String, role: Role) {
        if let Some(buf) = self.streaming.take() {
            let content = buf.into_content();
            self.push_message(CachedMessage::new(id, role, content));
        }
    }
}

/// Buffer for in-progress streaming content
pub struct StreamingBuffer {
    content: String,
}

impl StreamingBuffer {
    pub fn new() -> Self {
        Self { content: String::new() }
    }
    
    pub fn append(&mut self, delta: &str) {
        self.content.push_str(delta);
    }
    
    pub fn content(&self) -> &str {
        &self.content
    }
    
    pub fn into_content(self) -> String {
        self.content
    }
}
```

### 3.2 Tests

```rust
#[test]
fn viewport_cache_bounds_messages() {
    let mut cache = ViewportCache::new();
    
    for i in 0..50 {
        cache.push_message(CachedMessage::new(
            format!("msg-{}", i),
            Role::User,
            format!("Content {}", i),
        ));
    }
    
    assert!(cache.messages.len() <= MAX_CACHED_MESSAGES);
    // Oldest messages evicted
    assert!(cache.get_content("msg-0").is_none());
    assert!(cache.get_content("msg-49").is_some());
}

#[test]
fn viewport_cache_streaming_flow() {
    let mut cache = ViewportCache::new();
    
    cache.start_streaming();
    cache.append_streaming("Hello ");
    cache.append_streaming("World");
    cache.complete_streaming("msg-1".to_string(), Role::Assistant);
    
    assert_eq!(cache.get_content("msg-1"), Some("Hello World"));
}

#[test]
fn cached_message_wrapping() {
    let mut msg = CachedMessage::new(
        "test".to_string(),
        Role::User,
        "This is a longer message that will need wrapping",
    );
    
    let lines_20 = msg.wrapped_lines(20);
    assert!(lines_20.len() > 1);
    
    let lines_80 = msg.wrapped_lines(80);
    assert_eq!(lines_80.len(), 1);
}
```

---

## Phase 4: Compositor

**Goal**: Compositor that borrows from ViewportCache during render, producing spans.

### 4.1 Types

```rust
// crucible-ink/src/compositor.rs

use crate::span::{Span, SpanLine};
use crate::style::Style;

/// Compositor that borrows content and produces styled spans.
/// 
/// Lifetime 'a is tied to the render pass - compositor must not
/// outlive the function that creates it.
pub struct Compositor<'a> {
    content_source: &'a dyn ContentSource,
    width: usize,
    lines: Vec<SpanLine<'a>>,
}

impl<'a> Compositor<'a> {
    pub fn new(source: &'a dyn ContentSource, width: usize) -> Self {
        Self {
            content_source: source,
            width,
            lines: Vec::new(),
        }
    }
    
    /// Add a plain text line
    pub fn push_text(&mut self, text: &'a str) {
        self.lines.push(SpanLine::new(vec![Span::plain(text)]));
    }
    
    /// Add a styled line
    pub fn push_styled(&mut self, text: &'a str, style: Style) {
        self.lines.push(SpanLine::new(vec![Span::new(text, style)]));
    }
    
    /// Add a line composed of multiple spans
    pub fn push_spans(&mut self, spans: Vec<Span<'a>>) {
        self.lines.push(SpanLine::new(spans));
    }
    
    /// Render a message by ID (borrows content from source)
    pub fn render_message(&mut self, id: &str, style: Style) -> bool {
        if let Some(content) = self.content_source.get_content(id) {
            for line in content.lines() {
                self.push_styled(line, style);
            }
            true
        } else {
            false
        }
    }
    
    /// Finalize and return lines
    pub fn finish(self) -> Vec<SpanLine<'a>> {
        self.lines
    }
}

/// Trait for content sources the compositor can borrow from
pub trait ContentSource {
    fn get_content(&self, id: &str) -> Option<&str>;
}
```

### 4.2 Integration Pattern

```rust
// In chat_app.rs or chat_runner.rs

impl ChatApp {
    pub fn render_frame(&mut self, terminal: &mut Terminal) -> io::Result<()> {
        // 1. Refresh cache from daemon if needed
        self.refresh_viewport_cache();
        
        // 2. Create compositor that borrows from cache
        //    Compositor MUST NOT escape this function
        let compositor = Compositor::new(&self.viewport_cache, terminal.width());
        
        // 3. Build spans
        self.compose_view(&compositor);
        let span_lines = compositor.finish();
        
        // 4. Convert to ANSI and render
        let ansi_lines: Vec<String> = span_lines
            .iter()
            .map(|line| line.to_ansi())
            .collect();
        
        terminal.render_lines(&ansi_lines)
    }
}
```

### 4.3 Tests

```rust
#[test]
fn compositor_borrows_from_source() {
    struct TestSource {
        content: String,
    }
    
    impl ContentSource for TestSource {
        fn get_content(&self, _id: &str) -> Option<&str> {
            Some(&self.content)
        }
    }
    
    let source = TestSource {
        content: "Hello World".to_string(),
    };
    
    let mut comp = Compositor::new(&source, 80);
    assert!(comp.render_message("any", Style::new()));
    
    let lines = comp.finish();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].spans[0].text, "Hello World");
}

#[test]
fn compositor_lifetime_scoped_to_render() {
    // This test verifies the pattern compiles correctly
    fn render_pass(source: &impl ContentSource) -> Vec<String> {
        let comp = Compositor::new(source, 80);
        // ... build spans ...
        let lines = comp.finish();
        // Convert to owned before returning
        lines.iter().map(|l| l.to_ansi()).collect()
    }
    
    struct TestSource;
    impl ContentSource for TestSource {
        fn get_content(&self, _: &str) -> Option<&str> {
            Some("test")
        }
    }
    
    let result = render_pass(&TestSource);
    assert!(!result.is_empty());
}
```

---

## Phase 5: LineBuffer (Ring Buffer)

**Goal**: Terminal-sized ring buffer for efficient line-based rendering.

### 5.1 Types

```rust
// crucible-ink/src/line_buffer.rs

use std::collections::VecDeque;

/// A rendered line ready for terminal output
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedLine {
    /// ANSI-encoded content
    pub content: String,
    /// Visible width (cached)
    pub width: usize,
}

impl RenderedLine {
    pub fn new(content: String) -> Self {
        let width = visible_width(&content);
        Self { content, width }
    }
    
    pub fn blank() -> Self {
        Self {
            content: String::new(),
            width: 0,
        }
    }
}

/// Ring buffer of terminal lines for efficient rendering
pub struct LineBuffer {
    lines: VecDeque<RenderedLine>,
    capacity: usize,  // terminal height - 1 (reserve for status)
    width: usize,     // terminal width
}

impl LineBuffer {
    pub fn new(width: usize, height: usize) -> Self {
        let capacity = height.saturating_sub(1);
        Self {
            lines: VecDeque::with_capacity(capacity),
            capacity,
            width,
        }
    }
    
    /// Resize buffer (e.g., terminal resize)
    pub fn resize(&mut self, width: usize, height: usize) {
        self.width = width;
        self.capacity = height.saturating_sub(1);
        
        // Truncate if needed (keep bottom lines)
        while self.lines.len() > self.capacity {
            self.lines.pop_front();
        }
    }
    
    /// Clear and set new content
    pub fn set_lines(&mut self, lines: impl IntoIterator<Item = RenderedLine>) {
        self.lines.clear();
        for line in lines {
            if self.lines.len() < self.capacity {
                self.lines.push_back(line);
            }
        }
    }
    
    /// Compute diff against another buffer, return minimal operations
    pub fn diff(&self, other: &LineBuffer) -> LineDiff {
        let mut ops = Vec::new();
        
        let max_lines = self.lines.len().max(other.lines.len());
        for i in 0..max_lines {
            let old = self.lines.get(i);
            let new = other.lines.get(i);
            
            match (old, new) {
                (Some(o), Some(n)) if o != n => {
                    ops.push(DiffOp::Replace(i, n.clone()));
                }
                (None, Some(n)) => {
                    ops.push(DiffOp::Insert(i, n.clone()));
                }
                (Some(_), None) => {
                    ops.push(DiffOp::Clear(i));
                }
                _ => {}  // No change
            }
        }
        
        LineDiff { ops }
    }
    
    /// Get line at index
    pub fn get(&self, index: usize) -> Option<&RenderedLine> {
        self.lines.get(index)
    }
    
    /// Number of lines
    pub fn len(&self) -> usize {
        self.lines.len()
    }
    
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
    
    /// Iterator over lines
    pub fn iter(&self) -> impl Iterator<Item = &RenderedLine> {
        self.lines.iter()
    }
}

/// Diff operations for minimal terminal updates
#[derive(Debug)]
pub enum DiffOp {
    Replace(usize, RenderedLine),
    Insert(usize, RenderedLine),
    Clear(usize),
}

#[derive(Debug)]
pub struct LineDiff {
    pub ops: Vec<DiffOp>,
}

impl LineDiff {
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }
}
```

### 5.2 Tests

```rust
#[test]
fn line_buffer_respects_capacity() {
    let mut buf = LineBuffer::new(80, 10);  // 9 lines capacity
    
    let lines: Vec<_> = (0..20)
        .map(|i| RenderedLine::new(format!("Line {}", i)))
        .collect();
    
    buf.set_lines(lines);
    
    assert_eq!(buf.len(), 9);  // Capped at capacity
}

#[test]
fn line_buffer_diff_detects_changes() {
    let mut old = LineBuffer::new(80, 10);
    old.set_lines(vec![
        RenderedLine::new("Line 0".to_string()),
        RenderedLine::new("Line 1".to_string()),
    ]);
    
    let mut new = LineBuffer::new(80, 10);
    new.set_lines(vec![
        RenderedLine::new("Line 0".to_string()),  // Same
        RenderedLine::new("Changed".to_string()), // Different
    ]);
    
    let diff = old.diff(&new);
    
    assert_eq!(diff.ops.len(), 1);
    assert!(matches!(diff.ops[0], DiffOp::Replace(1, _)));
}

#[test]
fn line_buffer_resize_truncates() {
    let mut buf = LineBuffer::new(80, 20);
    buf.set_lines((0..15).map(|i| RenderedLine::new(format!("L{}", i))));
    
    assert_eq!(buf.len(), 15);
    
    buf.resize(80, 10);  // Shrink to 9 capacity
    
    assert_eq!(buf.len(), 9);
    // Should keep bottom lines
    assert!(buf.get(0).unwrap().content.contains("L6"));
}
```

---

## Phase 6: Wire It Together

**Goal**: Integrate all components into the chat application.

### 6.1 Updated ChatApp Structure

```rust
// crucible-cli/src/tui/chat_app.rs

pub struct InkChatApp {
    // Existing fields...
    input: InputBuffer,
    mode: ChatMode,
    status: String,
    // ...
    
    // NEW: Replace items with viewport cache
    viewport_cache: ViewportCache,
    
    // NEW: Line buffer for rendering
    line_buffer: LineBuffer,
    prev_line_buffer: LineBuffer,
    
    // Keep graduation for stdout
    graduation: GraduationState,
}

impl InkChatApp {
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            // ...
            viewport_cache: ViewportCache::new(),
            line_buffer: LineBuffer::new(width as usize, height as usize),
            prev_line_buffer: LineBuffer::new(width as usize, height as usize),
            graduation: GraduationState::new(),
        }
    }
    
    /// Main render entry point
    pub fn render(&mut self, ctx: &ViewContext<'_>) -> RenderResult {
        // Swap buffers
        std::mem::swap(&mut self.line_buffer, &mut self.prev_line_buffer);
        self.line_buffer.set_lines(std::iter::empty());  // Clear current
        
        // Compose into line buffer
        self.compose(ctx);
        
        // Compute diff
        let diff = self.prev_line_buffer.diff(&self.line_buffer);
        
        RenderResult {
            diff,
            cursor: self.compute_cursor_position(),
        }
    }
    
    fn compose(&mut self, ctx: &ViewContext<'_>) {
        // Create compositor that borrows from viewport_cache
        let mut comp = Compositor::new(&self.viewport_cache, self.line_buffer.width());
        
        // Render messages
        for msg in self.viewport_cache.visible_messages() {
            self.compose_message(&mut comp, msg);
        }
        
        // Render streaming if active
        if let Some(streaming) = self.viewport_cache.streaming() {
            self.compose_streaming(&mut comp, streaming);
        }
        
        // Render input box
        self.compose_input(&mut comp, ctx);
        
        // Render status bar
        self.compose_status(&mut comp);
        
        // Convert spans to rendered lines
        let span_lines = comp.finish();
        let rendered: Vec<_> = span_lines
            .into_iter()
            .map(|sl| RenderedLine::new(sl.to_ansi()))
            .collect();
        
        self.line_buffer.set_lines(rendered);
    }
}
```

### 6.2 Graduation Integration

```rust
impl InkChatApp {
    /// Check if messages should graduate to terminal scrollback
    fn maybe_graduate(&mut self) -> Option<String> {
        // Graduate oldest messages when viewport is "full"
        let should_graduate = self.viewport_cache.messages.len() > GRADUATION_THRESHOLD;
        
        if should_graduate {
            if let Some(msg) = self.viewport_cache.messages.pop_front() {
                // Format for stdout
                let formatted = self.format_for_graduation(&msg);
                self.graduation.commit_graduation(&[GraduatedContent {
                    key: msg.id.clone(),
                    content: formatted.clone(),
                    newline: true,
                }]);
                return Some(formatted);
            }
        }
        
        None
    }
}
```

### 6.3 Tests

```rust
#[test]
fn chat_app_render_uses_viewport_cache() {
    let mut app = InkChatApp::new(80, 24);
    
    // Add message via viewport cache
    app.viewport_cache.push_message(CachedMessage::new(
        "msg-1".to_string(),
        Role::User,
        "Hello World",
    ));
    
    let ctx = ViewContext::new(&FocusContext::new());
    let result = app.render(&ctx);
    
    // Should have rendered content
    assert!(!app.line_buffer.is_empty());
}

#[test]
fn chat_app_streaming_appears_in_viewport() {
    let mut app = InkChatApp::new(80, 24);
    
    app.viewport_cache.start_streaming();
    app.viewport_cache.append_streaming("Streaming...");
    
    let ctx = ViewContext::new(&FocusContext::new());
    app.render(&ctx);
    
    // Check line buffer contains streaming content
    let has_streaming = app.line_buffer.iter()
        .any(|l| l.content.contains("Streaming"));
    assert!(has_streaming);
}

#[test]
fn chat_app_graduation_moves_to_stdout() {
    let mut app = InkChatApp::new(80, 24);
    
    // Fill beyond graduation threshold
    for i in 0..50 {
        app.viewport_cache.push_message(CachedMessage::new(
            format!("msg-{}", i),
            Role::User,
            format!("Message {}", i),
        ));
    }
    
    // Should have some graduated
    let stdout = app.maybe_graduate();
    assert!(stdout.is_some());
}
```

---

## Phase 7: Resize Handling

**Goal**: Stable viewport during terminal resize with anchor-based positioning.

### 7.1 Anchor System

```rust
impl ViewportCache {
    /// Set anchor to current viewport position
    pub fn set_anchor(&mut self, message_id: String, line_offset: usize) {
        self.anchor = Some(ViewportAnchor {
            message_id,
            line_offset,
        });
    }
    
    /// Get anchor position
    pub fn anchor(&self) -> Option<&ViewportAnchor> {
        self.anchor.as_ref()
    }
    
    /// Clear anchor (e.g., after user scrolls)
    pub fn clear_anchor(&mut self) {
        self.anchor = None;
    }
}

impl InkChatApp {
    pub fn handle_resize(&mut self, width: u16, height: u16) {
        // Save anchor before resize
        let anchor = self.compute_current_anchor();
        
        // Resize buffers
        self.line_buffer.resize(width as usize, height as usize);
        self.prev_line_buffer.resize(width as usize, height as usize);
        
        // Invalidate wrapped line caches (width changed)
        self.viewport_cache.invalidate_wrapping();
        
        // Restore anchor
        if let Some(anchor) = anchor {
            self.viewport_cache.set_anchor(anchor.message_id, anchor.line_offset);
        }
        
        // Force full redraw
        self.force_redraw = true;
    }
    
    fn compute_current_anchor(&self) -> Option<ViewportAnchor> {
        // Find topmost visible message and its line offset
        // ...implementation...
        None
    }
}
```

### 7.2 Tests

```rust
#[test]
fn resize_preserves_anchor() {
    let mut app = InkChatApp::new(80, 24);
    
    // Add content
    app.viewport_cache.push_message(CachedMessage::new(
        "msg-1".to_string(),
        Role::User,
        "A long message that will wrap at 80 columns but differently at 40",
    ));
    
    // Set initial anchor
    app.viewport_cache.set_anchor("msg-1".to_string(), 0);
    
    // Resize
    app.handle_resize(40, 24);
    
    // Anchor should still exist
    assert!(app.viewport_cache.anchor().is_some());
    assert_eq!(app.viewport_cache.anchor().unwrap().message_id, "msg-1");
}

#[test]
fn resize_invalidates_wrapping() {
    let mut cache = ViewportCache::new();
    let mut msg = CachedMessage::new(
        "test".to_string(),
        Role::User,
        "Content that wraps",
    );
    
    // Warm cache at width 80
    let _ = msg.wrapped_lines(80);
    
    cache.push_message(msg);
    cache.invalidate_wrapping();
    
    // Next access should recompute
    // (test via checking wrapped cache is None internally)
}
```

---

## Testing Strategy

### Unit Tests (per module)

| Module | Test Focus |
|--------|------------|
| `span.rs` | Width calculation, ANSI conversion, Unicode handling |
| `line_buffer.rs` | Capacity, diffing, resize behavior |
| `viewport_cache.rs` | Bounding, streaming, content retrieval |
| `compositor.rs` | Borrowing, span building, lifetime safety |

### Integration Tests

```rust
// tests/tui_integration.rs

#[test]
fn full_chat_flow_with_viewport_cache() {
    let mut app = InkChatApp::new(80, 24);
    let mut terminal = MockTerminal::new(80, 24);
    
    // Simulate user input
    app.handle_input("Hello, assistant!");
    app.handle_submit();
    
    // Simulate streaming response
    app.handle_streaming_start();
    app.handle_streaming_delta("Hi there!");
    app.handle_streaming_complete();
    
    // Render
    let result = app.render(&ViewContext::default());
    terminal.apply_diff(&result.diff);
    
    // Verify
    assert!(terminal.contains("Hello, assistant!"));
    assert!(terminal.contains("Hi there!"));
}

#[test]
fn resize_during_streaming() {
    let mut app = InkChatApp::new(80, 24);
    
    app.handle_streaming_start();
    app.handle_streaming_delta("Partial content");
    
    // Resize mid-stream
    app.handle_resize(40, 24);
    
    app.handle_streaming_delta(" more content");
    app.handle_streaming_complete();
    
    // Should not panic, content preserved
    let ctx = ViewContext::default();
    let result = app.render(&ctx);
    assert!(!result.diff.is_empty() || app.line_buffer.len() > 0);
}
```

### Property-Based Tests

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn viewport_cache_never_exceeds_capacity(
        messages in prop::collection::vec(any::<String>(), 0..100)
    ) {
        let mut cache = ViewportCache::new();
        
        for (i, content) in messages.iter().enumerate() {
            cache.push_message(CachedMessage::new(
                format!("msg-{}", i),
                Role::User,
                content.clone(),
            ));
        }
        
        prop_assert!(cache.messages.len() <= MAX_CACHED_MESSAGES);
    }
    
    #[test]
    fn line_buffer_diff_is_complete(
        old_lines in prop::collection::vec(".*", 0..20),
        new_lines in prop::collection::vec(".*", 0..20),
    ) {
        let mut old_buf = LineBuffer::new(80, 25);
        old_buf.set_lines(old_lines.iter().map(|s| RenderedLine::new(s.clone())));
        
        let mut new_buf = LineBuffer::new(80, 25);
        new_buf.set_lines(new_lines.iter().map(|s| RenderedLine::new(s.clone())));
        
        let diff = old_buf.diff(&new_buf);
        
        // Applying diff to old should equal new
        let mut result = old_buf.clone();
        apply_diff(&mut result, &diff);
        
        for i in 0..new_buf.len() {
            prop_assert_eq!(result.get(i), new_buf.get(i));
        }
    }
}
```

---

## Migration Checklist

### Phase 1: Extract Crate
- [ ] Create `crates/crucible-ink/Cargo.toml`
- [ ] Move generic modules (node, style, render, layout, etc.)
- [ ] Update imports in `crucible-cli`
- [ ] Ensure all tests pass
- [ ] Commit

### Phase 2: Span Types
- [ ] Add `span.rs` to crucible-ink
- [ ] Implement `Span<'a>`, `SpanLine<'a>`
- [ ] Add unit tests
- [ ] Commit

### Phase 3: ViewportCache
- [ ] Add `viewport_cache.rs` to crucible-cli
- [ ] Implement bounded message cache
- [ ] Add streaming buffer
- [ ] Add unit tests
- [ ] Commit

### Phase 4: Compositor
- [ ] Add `compositor.rs` to crucible-ink
- [ ] Implement borrowing compositor
- [ ] Add `ContentSource` trait
- [ ] Add unit tests
- [ ] Commit

### Phase 5: LineBuffer
- [ ] Add `line_buffer.rs` to crucible-ink
- [ ] Implement ring buffer with diffing
- [ ] Add unit tests
- [ ] Commit

### Phase 6: Integration
- [ ] Refactor `ChatApp` to use new types
- [ ] Wire compositor to ViewportCache
- [ ] Update rendering pipeline
- [ ] Update graduation logic
- [ ] Integration tests
- [ ] Commit

### Phase 7: Resize
- [ ] Implement anchor system
- [ ] Handle resize with anchor preservation
- [ ] Add resize tests
- [ ] Commit

### Final
- [ ] Performance testing
- [ ] Manual testing across terminal emulators
- [ ] Documentation update
- [ ] Final review

---

## Appendix: String Type Recommendation

Based on the use case (bounded viewport, cheap cloning, message content):

**Recommendation**: Use `Arc<str>` from standard library.

Rationale:
- `Arc<str>` is immutable and cheap to clone (pointer copy)
- No external dependency
- Messages are immutable once created (streaming completes → becomes `Arc<str>`)
- Standard library support means better ecosystem compatibility

Alternative considered:
- `arcstr` crate: Slightly more ergonomic but adds dependency
- `compact_str`: Better for small strings but adds complexity
- `smol_str`: Good but optimized for very small strings

---

## Appendix: Crossterm Usage

Current efficient patterns to preserve:

```rust
// Efficient styled output
use crossterm::style::{Print, StyledContent, Stylize};

// Write multiple styled segments efficiently
execute!(
    stdout,
    Print(StyledContent::new(style1, "text1")),
    Print(StyledContent::new(style2, "text2")),
)?;

// Cursor movement
execute!(
    stdout,
    cursor::MoveTo(0, line),
    terminal::Clear(ClearType::CurrentLine),
    Print(content),
)?;
```

The span-based approach converts to this at the final step.
