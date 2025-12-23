---
tags: [meta, ux, tui, user-stories]
---

# TUI User Stories

User-facing requirements for the Crucible chat TUI. These define **what** users should experience, guiding [[docs/plans/2025-12-23-tui-redesign|TUI Redesign]] implementation.

## Streaming

### US-1: Tokens Appear Immediately

**As a user**, when I send a message, I want to see the assistant's response appear token-by-token as it's generated, so I know the system is working and can read along.

**Acceptance:**
- First token appears within 500ms of send (network permitting)
- Tokens render smoothly without flicker
- Token count updates in status bar

### US-2: Input Stays Responsive

**As a user**, while the assistant is responding, I want the input box to remain responsive, so I can start typing my next message or cancel the current response.

**Acceptance:**
- Input clears immediately on send (before response starts)
- Can type while streaming
- Ctrl+C cancels current generation
- Status shows "Thinking..." → "Generating (N tokens)"

### US-3: Streaming Indicator

**As a user**, I want a clear visual indicator when the assistant is still generating, so I know to wait before the response is complete.

**Acceptance:**
- Spinner or pulsing indicator during generation
- Partial content shows cursor/block at end
- Clear transition when complete

## Splash Screen

### US-4: Agent Selection

**As a user**, when I start the chat with no conversation, I want to see a dashboard where I can select which agent to use, so I can pick the right model for my task.

**Acceptance:**
- Shows configured agents with descriptions
- Arrow keys to navigate, Enter to select
- Shows current working directory
- Recent sessions visible for quick resume

### US-5: Welcome Experience

**As a user**, I want the initial screen to feel polished and informative, so the tool feels professional.

**Acceptance:**
- Crucible logo/banner
- Current directory displayed
- Help hint visible (e.g., "? for help")
- Clean, uncluttered layout

## Tool Calls

### US-6: Tool Progress

**As a user**, when the assistant uses tools, I want to see which tool is running and its progress, so I understand what's happening.

**Acceptance:**
- Tool name displayed with spinner while running
- Checkmark when complete, X on error
- Output preview (last few lines) while running
- Collapsible full output on complete

### US-7: Tool Approval (Future)

**As a user**, for dangerous operations, I want to be asked for confirmation before the tool executes, so I can prevent unintended changes.

**Acceptance:**
- Modal dialog shows tool name and arguments
- [Y]es / [N]o / [A]lways shortcuts
- Clear indication of what will happen

## Scrolling

### US-8: Bottom-Anchored Conversation

**As a user**, I want new messages to appear at the bottom near my input, pushing older messages up, so the conversation feels natural like a chat app.

**Acceptance:**
- First message appears near input, not at top of screen
- New messages push content up
- Auto-scroll follows latest content

### US-9: Smooth Scroll History

**As a user**, I want to scroll through conversation history smoothly using mouse wheel or keyboard, so I can review earlier messages.

**Acceptance:**
- Mouse wheel scrolls conversation
- Page Up/Down for large jumps
- Arrow keys for fine control (when not in input)
- Scroll position indicator (optional)

### US-10: Scroll Lock on Review

**As a user**, when I scroll up to review history, I want the view to stay where I scrolled, not jump to bottom on new content.

**Acceptance:**
- Scrolling up disables auto-scroll
- New content indicator when not at bottom
- Easy way to jump back to bottom (End key, click indicator)

## Dialogs & Modals

### US-11: Confirmation Dialogs

**As a user**, for destructive actions, I want a clear confirmation dialog, so I don't accidentally do something irreversible.

**Acceptance:**
- Centered modal overlay
- Clear action description
- Keyboard shortcuts (y/n)
- Escape to cancel

### US-12: Selection Dialogs

**As a user**, I want list selection dialogs (agents, sessions, etc.) to be easy to navigate.

**Acceptance:**
- Arrow keys to navigate
- Type to filter
- Enter to select
- Escape to cancel

## Content Rendering

### US-13: Markdown Styling

**As a user**, I want assistant responses with markdown (bold, code, lists) to render with appropriate styling.

**Acceptance:**
- **Bold** and *italic* visible
- `inline code` highlighted
- Code blocks with syntax highlighting
- Lists properly indented

### US-14: Code Block Highlighting

**As a user**, I want code blocks to have syntax highlighting based on language, so they're easy to read.

**Acceptance:**
- Language tag (```rust) triggers appropriate highlighting
- Line numbers (optional)
- Copy hint (future: click to copy)

## Session Management

### US-15: Exit Preserves Context

**As a user**, when I exit the chat, I want my conversation preserved, so I can resume later.

**Acceptance:**
- Session auto-saved
- Resume option on next launch
- Clear session boundaries in history

---

## See Also

- [[Plugin User Stories]] - Extension system requirements
- [[Roadmap]] - Feature prioritization
- [[docs/plans/2025-12-23-tui-redesign]] - Implementation plan
