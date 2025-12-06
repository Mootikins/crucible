# Desktop UI Design

## Context

Crucible needs a native desktop interface for power users. This complements the web UI:
- **Web UI** → Self-hosted, network accessible, any device
- **Desktop UI** → Native performance, keyboard-first, local power use

Both share the same backend (`ChatAgent` trait) and parser infrastructure.

**Constraints:**
- Must use GPUI (Apache-2.0 licensed, proven in Zed)
- Must reuse existing `crucible-parser` for markdown
- Must implement `ChatAgent` consumer (same as web)
- MVP is chat-only, notes browser deferred

## Goals / Non-Goals

**Goals:**
- Streaming chat with Claude/agents via `ChatAgent` trait
- Markdown rendering for responses (headings, paragraphs, code, lists)
- Keyboard shortcuts (Cmd+Enter send, Esc cancel)
- Native look and feel

**Non-Goals (MVP):**
- Notes browser/editor (Phase 2+)
- Graph view (Phase 4)
- Unified components with web (IDNI)
- Vim/helix motions (future)
- Settings modal (config files for now)
- Conversation persistence

## Decisions

### UI Framework: GPUI + gpui-component

**Decision:** Use Zed's GPUI framework with Longbridge's gpui-component library.

**Rationale:**
- Apache-2.0 licensed (both GPUI and gpui-component)
- GPU-accelerated, high performance
- **60+ ready-made components** including:
  - Input, Button, Checkbox, Switch, Select
  - Dialog, Popover, Tooltip, Notification
  - Table, List, Tree (virtualized for large data)
  - Editor with syntax highlighting and LSP support
  - Markdown rendering built-in!
  - 20+ themes with dark mode
- Battle-tested in Zed editor
- Cross-platform (macOS, Linux, Windows)
- ~12MB release binary

**Key gpui-component features we'll use:**
- `Input` / `NumberInput` - Chat input
- `VirtualList` / `Scrollable` - Message list
- `Dialog` / `Popover` - Modals
- `Editor` - Future notes editing
- Markdown rendering - Response display
- `Notification` - Alerts/errors

**Alternatives considered:**
- Iced: Good, but no component library this complete
- egui: Immediate mode, less suited for complex layouts
- Tauri: WebView-based, not truly native
- Electron: Heavy, not Rust-native
- Building from scratch: Unnecessary now that gpui-component exists

### Crate Structure

**Decision:** Standalone `crucible-desktop` crate with own binary.

```
crates/
├── crucible-core/       # Parser, types (shared)
├── crucible-cli/        # Terminal interface
├── crucible-web/        # Svelte + Actix
└── crucible-desktop/    # GPUI app ← NEW
    ├── Cargo.toml
    └── src/
        ├── main.rs
        ├── app.rs
        ├── views/
        │   ├── mod.rs
        │   ├── chat.rs
        │   ├── message_list.rs
        │   └── input.rs
        ├── rendering/
        │   ├── mod.rs
        │   └── blocks.rs
        └── theme.rs
```

**Rationale:**
- Clean separation from CLI
- Own binary (`cru-desktop`)
- Shares libs, not UI code

### Backend Integration

**Decision:** Consume `ChatAgent` trait from `chat-interface` spec.

```rust
// In crucible-desktop
struct ChatView {
    agent: Box<dyn ChatAgent>,
    messages: Vec<Message>,
    input: String,
}
```

**Rationale:**
- Same backend as web UI
- No duplication of ACP logic
- Future agents work automatically

### Markdown Rendering

**Decision:** Use gpui-component's built-in Markdown renderer for MVP.

**gpui-component provides:**
- Markdown and HTML element rendering
- Syntax highlighting via Tree-sitter
- Code blocks with language detection
- Full CommonMark support

**MVP approach:**
1. Use gpui-component's Markdown element directly for agent responses
2. For custom rendering needs, fall back to our parser AST

**Custom rendering (if needed later):**
```rust
// rendering/blocks.rs
pub fn render_content(content: &NoteContent, cx: &mut Context) -> impl IntoElement {
    v_flex()
        .gap_2()
        .children(content.blocks.iter().map(|block| {
            match block.block_type {
                ASTBlockType::Heading => render_heading(block),
                ASTBlockType::Paragraph => render_paragraph(block),
                ASTBlockType::Code => render_code_block(block),
                ASTBlockType::List => render_list(block),
                _ => render_fallback(block),
            }
        }))
}
```

**Rationale:**
- gpui-component already solves markdown rendering
- Our parser still useful for wikilinks, callouts, LaTeX (extensions gpui-component may not have)
- Can mix: use their renderer for standard markdown, custom for Obsidian extensions

### Text Input

**Decision:** Use gpui-component's `Input` for simple cases, `Editor` for multiline.

**gpui-component provides:**
- `Input` - Single-line text input with validation
- `Editor` - Full text editor with syntax highlighting, LSP support

**MVP scope:**
- Use `Input` or `Editor` component from gpui-component
- Multiline text entry
- Standard keyboard shortcuts (copy/paste/select all)

**Deferred:**
- Undo/redo (may be built into Editor)
- Vim motions

### Keyboard Shortcuts

**Decision:** Standard shortcuts for MVP.

```rust
actions!(desktop, [SendMessage, Cancel, NewLine]);

cx.bind_keys([
    KeyBinding::new("cmd-enter", SendMessage, Some("ChatInput")),
    KeyBinding::new("escape", Cancel, Some("ChatInput")),
    KeyBinding::new("shift-enter", NewLine, Some("ChatInput")),
]);
```

### Theming

**Decision:** Single dark theme for MVP, theming system later.

```rust
// theme.rs
pub struct Theme {
    pub bg_primary: Hsla,
    pub bg_secondary: Hsla,
    pub text_primary: Hsla,
    pub text_muted: Hsla,
    pub accent: Hsla,
    pub border: Hsla,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            bg_primary: rgb(0x1e1e1e),
            bg_secondary: rgb(0x252525),
            text_primary: rgb(0xe0e0e0),
            text_muted: rgb(0x808080),
            accent: rgb(0x569cd6),
            border: rgb(0x333333),
        }
    }
}
```

## Risks / Trade-offs

**Risk:** GPUI is pre-1.0, API may change.
- **Mitigation:** Pin version, update carefully. Active development is good sign.

**Risk:** Text input complexity (cursor, selection, IME).
- **Mitigation:** Start with example code, keep scope minimal.

**Risk:** No ready-made components (buttons, modals, etc.).
- **Mitigation:** Build minimal set as needed. Use Zed's patterns as reference.

**Trade-off:** Separate UI codebases for web and desktop.
- **Accepted:** Shared backend logic. UI duplication is manageable (~500-1000 lines per component). Unified components deferred (IDNI).

## Open Questions

1. **Installation:** How to distribute? Cargo install? Package managers?
   - **Tentative:** `cargo install` for now, platform packages later.

2. **Config location:** Where does desktop app read config?
   - **Tentative:** Same as CLI (`~/.config/crucible/`).

3. **Chat history:** Persist conversations?
   - **Tentative:** Not for MVP. Add file-based persistence later.

## Future Notes

### Vim/Helix Motions (Deferred)

Build a keybind DSL parser instead of copying GPL vim code:

```toml
[motions]
"h" = "cursor_left"
"j" = "cursor_down"
"w" = "word_forward"
"gg" = "document_start"

[operators]
"d" = "delete"
"c" = "change"
```

Benefits: Not GPL-encumbered, user-configurable, supports both vim and helix styles.

### Unified Components (IDNI)

Potential future approach: Rune component definitions that generate:
- Svelte components (for web)
- GPUI components (for desktop)
- CLI output (tables, sixel images)

Compile-time generation, not runtime abstraction. Revisit when plugin ecosystem matters.

### Notes Browser (Phase 2)

After chat MVP:
1. File tree sidebar
2. Markdown preview pane
3. Read-only initially
4. Editor in Phase 3
5. Graph view in Phase 4
