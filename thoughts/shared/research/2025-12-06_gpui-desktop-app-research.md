# GPUI Desktop App Research

**Date:** 2025-12-06
**Purpose:** Investigate Zed's GPUI library for alternative desktop app development

## Executive Summary

GPUI is a **production-ready, GPU-accelerated UI framework** for Rust that powers the Zed editor. It provides a hybrid immediate/retained mode rendering model with a Tailwind-like styling API.

### Key Finding: Licensing is Favorable

| Crate | License | Usability |
|-------|---------|-----------|
| **gpui** | **Apache-2.0** | ✅ Freely usable |
| **ui** (component library) | GPL-3.0 | ⚠️ Copyleft - must open source |
| Zed application | AGPL-3.0 | ❌ Cannot use directly |

**Bottom line:** We can use GPUI core freely. The `ui` component library requires GPL compliance (open-sourcing derivative works), but we could build our own components on top of GPUI.

---

## GPUI Architecture

### Core Design

GPUI uses a **three-tier abstraction**:

1. **Entities** - Smart pointer-based state management (like Rc with reactivity)
2. **Views** - High-level declarative UI via `Render` trait
3. **Elements** - Low-level imperative building blocks

### Rendering Pipeline

```
Application::run()
    ↓
View.render() → Element Tree
    ↓
Taffy Layout Engine (CSS Flexbox/Grid)
    ↓
GPU Renderer (Metal/Blade/DirectX)
    ↓
Window Display
```

### Platform Support

| Platform | Renderer | Status |
|----------|----------|--------|
| macOS | Metal | ✅ Production |
| Linux X11 | Blade GPU | ✅ Production |
| Linux Wayland | Blade GPU | ✅ Production |
| Windows | DirectX | ✅ Production |

---

## Core Elements (Available in Apache-2.0 GPUI)

These are the building blocks we get for free:

| Element | Purpose |
|---------|---------|
| `Div` | Universal flexbox/grid container with styling |
| `Text` | Text rendering with styling |
| `Canvas` | Custom painting via callback |
| `Img` | Image rendering |
| `Svg` | Vector graphics |
| `List` | Efficient list rendering |
| `UniformList` | Optimized uniform-height lists |
| `Anchored` | Positioned overlays/tooltips |
| `Animation` | Element animation wrapper |

---

## Styling API (Tailwind-like)

GPUI uses a fluent, Tailwind-inspired styling API:

```rust
div()
    .flex()
    .flex_col()
    .gap_4()
    .p_4()
    .bg(rgb(0x1e1e1e))
    .rounded_md()
    .border_1()
    .border_color(rgb(0x333333))
    .child("Hello World")
```

### Available Style Methods

- **Layout:** `flex()`, `grid()`, `flex_col()`, `flex_row()`, `gap_*()`, `justify_*()`, `items_*()`
- **Sizing:** `w_full()`, `h_*()`, `size_*()`, `min_w_*()`, `max_h_*()`
- **Spacing:** `p_*()`, `m_*()`, `px_*()`, `py_*()`, `pt_*()`, `ml_*()`
- **Colors:** `bg()`, `text_color()`, `border_color()`
- **Borders:** `border_*()`, `rounded_*()`, `shadow_*()`
- **Text:** `text_xl()`, `text_center()`, `truncate()`, `line_clamp()`

---

## Event Handling

### Mouse Events

```rust
div()
    .on_mouse_down(MouseButton::Left, |ev, cx| { ... })
    .on_click(cx.listener(|this, ev, cx| { ... }))
    .on_scroll_wheel(|ev, cx| { ... })
    .hover(|style| style.bg(rgb(0x333333)))
```

### Keyboard Events

```rust
// Define actions
actions!(my_app, [Save, Undo, Redo]);

// Handle actions
div()
    .key_context("editor")
    .on_action(cx.listener(|this, _: &Save, cx| { ... }))
```

### Drag & Drop

```rust
div()
    .on_drag(data, |info, position, phase, cx| { ... })
    .on_drop(cx.listener(|this, dropped_item, cx| { ... }))
```

---

## What We'd Need to Build

Since the `ui` component library is GPL-licensed, we'd need to build these ourselves (or use GPL):

### Essential Components to Create

1. **Buttons** - Standard, icon, toggle variants
2. **Text Input** - Single line, multiline, with validation
3. **Labels** - Various sizes and emphasis levels
4. **Modals/Dialogs** - Overlay containers with focus trapping
5. **Lists/Tables** - Virtualized for performance
6. **Menus** - Context menus, dropdowns
7. **Tabs** - Tab navigation
8. **Scrollbars** - Custom styled scrollbars
9. **Tooltips** - Hover hints

### From Zed's GPL `ui` crate (for reference, not copying):

- Button, IconButton, ToggleButton
- Checkbox, Switch, Radio
- Label, Headline, HighlightedLabel
- Modal, Popover, ContextMenu
- List, ListItem, DataTable
- Tab, TabBar
- Avatar, Badge, Indicator
- Tooltip, Callout, Banner
- ProgressBar, Disclosure

---

## Comparison with Alternatives

| Framework | Rendering | Styling | Maturity | License |
|-----------|-----------|---------|----------|---------|
| **GPUI** | GPU (Metal/Blade) | Tailwind-like | Pre-1.0, production-used | Apache-2.0 |
| **Iced** | wgpu | Custom | Stable | MIT |
| **egui** | GPU immediate | Immediate mode | Stable | MIT/Apache-2.0 |
| **Tauri** | WebView | CSS/HTML | Stable | MIT/Apache-2.0 |
| **Slint** | GPU | QML-like DSL | Stable | GPL/Commercial |

### GPUI Advantages

- **Battle-tested** in Zed (millions of users)
- **Excellent keyboard support** (editor-focused)
- **High performance** for complex UIs
- **Native feel** across platforms
- **Rust-native** with strong typing

### GPUI Challenges

- **Pre-1.0 API** - Breaking changes expected
- **Limited docs** - Must read Zed source code
- **No official component library** (for Apache-2.0)
- **Smaller community** than alternatives

---

## Recommendation

### Option A: Use GPUI + Build Components (Apache-2.0)

**Effort:** High
**License:** Permissive

Build our own component library on top of GPUI core. Use Zed's `ui` crate as **reference** for patterns but implement from scratch.

### Option B: Use GPUI + GPL Components

**Effort:** Low
**License:** Copyleft (must open-source)

Use Zed's full `ui` component library under GPL-3.0. Requires open-sourcing the application.

### Option C: Alternative Framework

**Effort:** Medium
**License:** Permissive

Consider Iced, egui, or Tauri if GPL is unacceptable and building components from scratch is too much work.

---

---

## Chat App Feasibility Assessment

For a basic chat UI with plaintext → eventual markdown, here's the breakdown:

### Core Components Needed

| Component | GPUI Support | Effort |
|-----------|--------------|--------|
| Message list (scrollable) | `UniformList` / `List` built-in | Low |
| Text input (multiline) | Primitives exist, need assembly | Medium |
| Basic buttons | `Div` + styling + click handler | Low |
| Scrollbar | Built-in with `List` | Low |
| Markdown rendering | `StyledText` + `pulldown_cmark` | Low-Medium |

### Text Input Assessment

GPUI provides all the primitives but not a ready-made component:

- **`EntityInputHandler` trait** - Bridges platform IME with your text state
- **`ShapedLine`** - Laid-out text with cursor position helpers (`x_for_index()`, `index_for_x()`)
- **`TextSystem`** - Shapes and lays out text

**Effort estimates:**
- Single-line input: **1-2 days** (example exists in `gpui/examples/input.rs` - 746 lines)
- Basic multiline: **3-5 days** (add line array, y-coordinate mapping)
- Production quality: **2-4 weeks** (word wrap, undo/redo, smooth scrolling)

The Zed editor is 25k lines but that's for a full code editor with LSP, syntax highlighting, multi-buffer, collaboration, etc. A chat input is much simpler.

### Markdown Rendering Assessment

**This is actually easy.** GPUI has excellent rich text support, AND we already have a parser.

#### Existing Parser (crucible-parser)

We already have a `markdown-it` based parser in `crates/crucible-parser/` that extracts:

| Element | Extracted? | Type |
|---------|-----------|------|
| Headings | ✅ | `Heading { level, text, offset }` |
| Paragraphs | ✅ | `Paragraph { content, offset }` |
| Code blocks | ✅ | `CodeBlock { language, content, offset }` |
| Wikilinks | ✅ | `Wikilink { target, alias, is_embed, ... }` |
| Tags | ✅ | `Tag { name, offset }` |
| Callouts | ✅ | `Callout { callout_type, title, content }` |
| LaTeX | ✅ | `LatexExpression { expression, is_block }` |
| Tables | ✅ | `Table { headers, rows, columns }` |
| Lists | ✅ | `ListBlock { list_type, items }` |
| Horizontal rules | ✅ | `HorizontalRule { style, offset }` |

**The `AstConverter` already walks the markdown-it AST and produces structured `NoteContent`.**

#### Rendering Strategy

We'd create a **`MarkdownRenderer` trait** that both web and GPUI can implement:

```rust
trait MarkdownRenderer {
    type Output;

    fn render_heading(&mut self, heading: &Heading) -> Self::Output;
    fn render_paragraph(&mut self, para: &Paragraph) -> Self::Output;
    fn render_code_block(&mut self, code: &CodeBlock) -> Self::Output;
    fn render_table(&mut self, table: &Table) -> Self::Output;
    fn render_list(&mut self, list: &ListBlock) -> Self::Output;
    fn render_callout(&mut self, callout: &Callout) -> Self::Output;
    fn render_latex(&mut self, latex: &LatexExpression) -> Self::Output;
    // ...
}
```

**For GPUI**, the implementation would map to:
- `Heading` → `div().text_xl().font_weight(BOLD).child(text)`
- `Paragraph` → `StyledText::new(text).with_default_highlights(inline_styles)`
- `CodeBlock` → `div().bg(code_bg).font_family(mono).child(highlighted_code)`
- `Table` → Custom element using GPUI's layout primitives
- `Callout` → `div().bg(callout_color).border().child(icon).child(content)`

#### GPUI Rich Text Primitive

```rust
// GPUI's StyledText with highlight ranges
StyledText::new("Hello **world**")
    .with_default_highlights(&base_style, vec![
        (6..15, HighlightStyle { font_weight: Some(BOLD), ..default() }),
    ])
```

#### Effort (Revised - Using Existing Parser)

- **Parser work:** Already done! ✅
- **GPUI renderer trait impl:** ~300-500 lines, 2-3 days
- **Inline formatting (bold/italic in paragraphs):** Need to add inline span extraction to parser
- **Images/embeds:** Need platform image loading integration
- **Syntax highlighting:** Reference Zed's `rich_text` crate

### Realistic Timeline for MVP Chat App

| Milestone | Estimate |
|-----------|----------|
| Hello World window | 1 hour |
| Basic layout (header, messages, input) | 1 day |
| Message list with scrolling | 1-2 days |
| Text input (basic) | 2-3 days |
| Send/receive messages (API integration) | 1-2 days |
| **MVP Total** | **~1-2 weeks** |
| Markdown rendering | +2-3 days |
| Polish (keyboard shortcuts, focus) | +2-3 days |

### Code Size Estimates

```
Minimal chat app:     1,000 - 2,000 lines
With markdown:        1,500 - 3,000 lines
With custom input:    2,500 - 4,000 lines
```

For comparison:
- GPUI input example: 746 lines
- Zed's rich_text crate: 394 lines
- Zed's full editor: 25,411 lines (overkill for chat)

---

## Verdict

**GPUI is well-suited for a chat app.** The hard parts (text layout, GPU rendering, platform integration) are solved. What we'd build:

1. **Use directly:** List, Div, styling system, event handling
2. **Assemble from primitives:** Text input (~500-1000 lines)
3. **Trivial to add:** Markdown rendering via StyledText

The main unknown is developer experience with a pre-1.0 API. A 1-day prototype spike would validate this.

---

## Next Steps

1. ~~Decide on licensing approach~~ → GPL for `ui` crate, Apache-2.0 for GPUI core is fine
2. **Prototype spike** - Build a minimal "hello world" chat layout in 1 day
3. **Validate text input** - Test the input.rs example, adapt for multiline
4. **Evaluate alternatives** - Only if GPUI spike reveals blocking issues

---

## Low-Hanging Fruit for MVP

### 1. Text Wrapping (Built-in!)

GPUI has wrapping built into the text system:

```rust
// Default behavior - wraps at container width
div()
    .w(px(600.))  // Fixed width container
    .child("Long text wraps automatically...")

// Explicit control
div()
    .whitespace_normal()  // Enable wrapping (default)
    .whitespace_nowrap()  // Disable wrapping
    .line_clamp(3)        // Limit to N lines
    .truncate()           // Ellipsis on overflow
```

The `LineWrapper` (`text_system/line_wrapper.rs`) handles:
- Word boundary breaks
- CJK text (breaks at any character)
- Hanging indents

**Adjustable width:** Just change the container's `w()` value - wrapping recalculates automatically.

### 2. Modal Dialog (Simple Pattern)

Minimal modal requires:
1. Struct with state
2. Implement `ModalView`, `EventEmitter<DismissEvent>`, `Focusable`, `Render`
3. Call `workspace.toggle_modal()` to display

```rust
pub struct MyModal {
    focus_handle: FocusHandle,
}

impl EventEmitter<DismissEvent> for MyModal {}
impl ModalView for MyModal {}

impl Focusable for MyModal {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for MyModal {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .key_context("MyModal")
            .on_action(cx.listener(|_, _: &menu::Cancel, cx| {
                cx.emit(DismissEvent);
            }))
            .elevation_2(cx)
            .p_4()
            .child("Hello from modal!")
    }
}

// Trigger with keybinding:
workspace.register_action(|workspace, _: &OpenMyModal, window, cx| {
    workspace.toggle_modal(window, cx, |w, cx| MyModal::new(cx));
});
```

**Simplest example:** `git_ui/askpass_modal.rs` (~100 lines)

### 3. Keybindings

```rust
// Define action
actions!(my_app, [OpenSettings, SendMessage, Cancel]);

// Bind keys
cx.bind_keys([
    KeyBinding::new("cmd-,", OpenSettings, None),
    KeyBinding::new("cmd-enter", SendMessage, Some("ChatInput")),
    KeyBinding::new("escape", Cancel, Some("Modal")),
]);

// Handle in element
div()
    .key_context("ChatInput")
    .on_action(cx.listener(|this, _: &SendMessage, cx| {
        this.send_message(cx);
    }))
```

---

## Zed Component Licensing Summary

| Crate | License | Reusable? |
|-------|---------|-----------|
| `gpui` | Apache-2.0 | ✅ Freely |
| `ui` (components) | GPL-3.0 | ⚠️ Copyleft |
| `vim` | GPL-3.0 | ⚠️ Copyleft |
| `editor` | GPL-3.0 | ⚠️ Copyleft |
| `workspace` | GPL-3.0 | ⚠️ Copyleft |

**Vim motions:** GPL-3.0, so can only be used in GPL projects. However, vim motion *algorithms* can be studied and reimplemented independently. The crate is ~500KB of comprehensive vim emulation (motions, text objects, ex commands, visual mode, etc.).

---

## Future: Vim/Helix Motion System

**TODO:** Investigate building a flexible keybind system that could support both vim and helix-style motions.

### Idea: Keybind DSL Parser

Instead of hardcoding vim motions, create a parser for a keybind config format:

```toml
# Example keybind config (vim-style)
[motions]
"h" = "cursor_left"
"j" = "cursor_down"
"k" = "cursor_up"
"l" = "cursor_right"
"w" = "word_forward"
"b" = "word_backward"
"gg" = "document_start"
"G" = "document_end"

[operators]
"d" = "delete"
"c" = "change"
"y" = "yank"

[text_objects]
"iw" = "inner_word"
"aw" = "around_word"
"i\"" = "inner_double_quote"
```

### Benefits
- **Not GPL-encumbered** - Our own implementation
- **Flexible** - Users can customize or use helix/kakoune style
- **Logging** - Mild logging/errors for unknown bindings
- **Extensible** - Add new motions without code changes

### Helix Compatibility
Helix uses selection-first model (`select -> action` vs vim's `action -> motion`).
A sufficiently flexible keybind system could express both paradigms.

### Implementation Approach
1. Define core "primitives" (cursor movements, selections, text objects)
2. Parser reads config → builds keybind tree
3. Key sequences matched against tree
4. Unknown sequences logged with helpful error
5. User can override/extend defaults

**Priority:** Future (after MVP chat works)

---

## gpui-component Library (Game Changer!)

**Discovery:** [Longbridge gpui-component](https://github.com/longbridge/gpui-component) provides **60+ ready-made components** for GPUI.

**License:** Apache-2.0 (same as GPUI core)

### Available Components

| Category | Components |
|----------|------------|
| **Input** | Button, Checkbox, Input, NumberInput, OtpInput, Radio, Select, Switch, Toggle |
| **Display** | Avatar, Badge, Icon, Image, Label, Tag, Kbd, Spinner, Skeleton, Progress |
| **Layout** | Accordion, Collapsible, GroupBox, Sidebar, Sheet, Resizable, Scrollable, VirtualList |
| **Data** | Table, List, Tree, DescriptionList, Menu |
| **Dialogs** | Dialog, Popover, Tooltip, Notification, Dropdown |
| **Forms** | Form, Clipboard |
| **Visualization** | Chart, Plot |
| **Specialized** | Calendar, DatePicker, ColorPicker, **Editor**, WebView, Settings, TitleBar |

### Key Features for Our Use Case

- **Markdown rendering built-in** with syntax highlighting (Tree-sitter)
- **Editor component** with LSP support (200K+ lines)
- **VirtualList** for large message lists
- **Dialog/Popover** for modals
- **20+ themes** with dark mode
- **~12MB** release binary

### Impact on MVP

This dramatically reduces our implementation work:

| Before | After |
|--------|-------|
| Build text input from scratch | Use `Input`/`Editor` component |
| Build markdown renderer | Use built-in Markdown element |
| Build modal system | Use `Dialog` component |
| Build scrolling list | Use `VirtualList` |
| Build theme system | Use built-in 20+ themes |

**Revised estimate:** MVP could be **days, not weeks**.

---

## Other GPUI Projects (awesome-gpui)

### Apps Built with GPUI
- **Zed** - Code editor
- **Loungy** - App launcher (Spotlight/Alfred alternative)
- **pgui** - PostgreSQL GUI
- **hummingbird** - Music player
- **vleer** - Music streaming
- **gpui-todos** - Task management

### Libraries
- **gpui-component** - 60+ components (Apache-2.0)
- **gpui-d3rs** / **gpui-px** - Plotting
- **gpui-router** - Routing
- **gpui-storybook** - Component preview
- **plotters-gpui** - Plotting backend

### Tools
- **create-gpui-app** - Project scaffolding
- **gpui-book** - Learning guide

---

## Resources

- [GPUI Docs](https://docs.rs/gpui/latest/gpui/)
- [GPUI Website](https://www.gpui.rs/)
- [gpui-component](https://github.com/longbridge/gpui-component) - Component library
- [gpui-component Docs](https://longbridge.github.io/gpui-component/)
- [awesome-gpui](https://github.com/zed-industries/awesome-gpui) - Curated list
- [Zed Source](https://github.com/zed-industries/zed)
- [Zed Discord](https://zed.dev/community-links)
