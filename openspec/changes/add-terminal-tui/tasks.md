# Tasks: Terminal TUI

## Phase 1: Core TUI Infrastructure ✅ COMPLETE

### 1.1 Dependencies and Module Setup ✅
- [x] Add `ratatui = "0.29"` and `crossterm` to crucible-cli
- [x] Create `crates/crucible-cli/src/tui/mod.rs` module structure
- [x] Export TUI components from module

### 1.2 TuiState Implementation ✅
- [x] Define `TuiState` struct with input buffer, streaming state
- [x] Implement `DisplayMessage` type for rendered messages
- [x] Implement `StreamingBuffer` for accumulating TextDelta
- [x] Add cursor position tracking

### 1.3 Input Handling ✅
- [x] Create `input.rs` with crossterm event polling
- [x] Handle Enter key → send message
- [x] Handle Ctrl+J → insert newline
- [x] Handle Ctrl+C → cancel/exit logic (double-tap to exit)
- [x] Handle Up/Down/PgUp/PgDn → scroll (terminal scrollback)
- [x] Handle Shift+Tab → cycle mode

### 1.4 Event Polling ✅
- [x] Implement ring buffer polling loop
- [x] Handle `TextDelta` → accumulate in streaming buffer
- [x] Handle `AgentResponded` → finalize message
- [x] Handle `ToolCalled` / `ToolCompleted` → update pending tools
- [x] Track `last_seen_seq` for incremental updates

### 1.5 Basic Rendering ✅
- [x] Create `render.rs` with main render function
- [x] Widget-at-bottom approach (messages go to terminal scrollback)
- [x] Implement input area with cursor
- [x] Implement status bar (mode, pending tools)
- [x] Handle terminal resize events

### 1.6 Session Integration ✅
- [x] Wire TUI loop into `ChatSession::run()`
- [x] Create `AgentEventBridge` for ChatChunk → SessionEvent conversion
- [x] Connect to `Session` ring buffer
- [x] Implement graceful shutdown on exit

---

## Phase 2: Rich Features

### 2.1 Session Helper Methods ✅
- [x] Add `Session::recent_messages(limit)` helper
- [x] Add `Session::pending_tools()` helper
- [x] Add `Session::is_streaming()` helper
- [x] Add `Session::cancel()` for interrupting operations

### 2.2 Markdown Rendering ✅ COMPLETE
- [x] Add termimad + syntect dependencies
- [x] Create `tui/markdown.rs` with MarkdownRenderer
- [x] Auto-detect terminal theme (dark/light)
- [x] Render markdown structure (bold, italic, lists, blockquotes)
- [x] Syntax highlight code blocks
- [x] Integrate into TuiRunner::print_assistant_response()
- [x] Use in one-shot mode for consistency

### 2.3 Tool Call Visualization
- [x] Display tool name when called
- [x] Show tool completion status
- [ ] Add spinner animation for pending tools
- [ ] Show tool arguments (collapsed by default)
- [ ] Show tool results/errors inline

### 2.4 Slash Commands
- [ ] Integrate existing `SlashCommandRegistry`
- [ ] Add `/cancel` command for interrupting
- [ ] Add `/context` command for context management
- [ ] Add `/compact` command for history compaction
- [ ] Command autocomplete with Tab

---

## Phase 3: Polish

### 3.1 Visual Improvements
- [x] Basic color scheme (mode colors)
- [ ] User message vs assistant message styling
- [ ] Error message highlighting
- [ ] Loading indicators and animations

### 3.2 Keyboard Refinements
- [ ] Command history (Up arrow in empty input)
- [ ] Input editing (Ctrl+A, Ctrl+E, Ctrl+K)
- [ ] Word navigation (Ctrl+Left/Right)

### 3.3 Testing
- [x] Bridge tests (AgentEventBridge)
- [x] Unit tests for TuiState transitions
- [x] Unit tests for MarkdownRenderer (4 tests: creation, plain text, theme detection, code highlighting)
- [ ] Integration test with mock Session

---

## Verification

- [x] `cru chat` launches TUI interface
- [x] Messages display with streaming
- [x] Mode switching works (Plan/Act/Auto)
- [x] Markdown renders correctly (termimad + syntect)
- [ ] Slash commands execute correctly
- [x] Ctrl+C cancels and double Ctrl+C exits
- [x] Tool calls show progress
- [x] Terminal scroll works for history
