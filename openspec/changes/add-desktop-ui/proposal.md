# Native Desktop UI with GPUI

## Why

Users need a native desktop chat interface that complements the web UI. A GPUI-based app enables:

1. **Native performance** - GPU-accelerated rendering, instant startup
2. **Power-user experience** - Keyboard-first, eventual vim/helix motions
3. **Self-hosted ecosystem** - Desktop for local power use, web for network access
4. **Foundation for notes UI** - Chat-first MVP, notes browser/editor in later phases

## What Changes

**New crate `crucible-desktop`:**
- GPUI-based native application
- Implements `ChatAgent` trait consumer (same backend as web)
- Markdown rendering using existing `crucible-parser` AST
- Streaming token display, keyboard shortcuts

**Architecture:**
- Shares `ChatAgent` trait with `crucible-web` (no duplication)
- Shares parser/AST types from `crucible-core`
- Native GPUI components (no cross-platform abstraction layer)

**Phased rollout:**
- Phase 1 (MVP): Chat interface with markdown rendering
- Phase 2: Notes browser (file tree + read-only preview)
- Phase 3: Editor + live preview
- Phase 4: Graph view (Canvas-based)

## Impact

### Affected Specs
- **chat-interface** (consumed) - Uses `ChatAgent` trait
- **desktop-ui** (new) - GPUI application spec

### Affected Code

**New crate:**
- `crates/crucible-desktop/` - GPUI application
  - `src/main.rs` - Application entry point
  - `src/app.rs` - Root application state
  - `src/views/chat.rs` - Chat view
  - `src/views/message_list.rs` - Scrollable message list
  - `src/views/input.rs` - Multiline text input
  - `src/rendering/mod.rs` - Markdown → GPUI rendering
  - `src/rendering/blocks.rs` - Block element rendering
  - `src/theme.rs` - Styling/theming

**Dependencies (crucible-desktop):**
- `gpui = "0.2"` - UI framework (Apache-2.0)
- `crucible-core` - Parser, types
- `crucible-cli` (or extracted) - `ChatAgent` trait + adapters

### User-Facing Impact
- **New binary**: `cru-desktop` launches native chat app
- **Desktop entry**: Standard `.desktop` file for Linux, `.app` bundle for macOS
- **Same backend**: Connects to same ACP agents as CLI/web

## Non-Goals (MVP)

- Notes browsing/editing (Phase 2+)
- Graph view (Phase 4)
- Unified component system with web (IDNI - deferred)
- Vim/helix motions (future, via keybind DSL)
- Settings UI (use config files for now)
- Conversation persistence (add post-MVP)
