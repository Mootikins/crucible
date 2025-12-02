# Chat Experience Improvements

## Why

The chat interface works well functionally but can be polished to provide a smoother, more modern experience. Users would benefit from:

1. **Cleaner Visuals**: Less clutter, better focus on content
2. **Richer Interactions**: File references, inline search, better shortcuts
3. **Improved Feedback**: Real-time progress, clearer errors, session stats
4. **Modern UX**: Features users expect from AI chat interfaces

These improvements build on the chat framework to enhance the user experience without changing core functionality.

## What Changes

**Visual Polish:**
- Remove decorative boxes for cleaner output
- Streamline message formatting
- Better markdown rendering in terminal
- Reduced visual noise in metadata display

**Interactive Enhancements:**
- `@file` reference system with fuzzy selection
- `/search` command for inline knowledge base search
- Command history and recall
- Better keyboard shortcuts (Shift+Tab for mode cycling)
- Double Ctrl+C to exit safely

**User Feedback:**
- Real-time progress indicators for long operations
- Session statistics (messages, tokens, context used)
- Clearer error messages with recovery suggestions
- Visible tool calls during agent execution

**Note**: This proposal depends on `chat-interface` being implemented first, as it enhances the framework rather than reimplementing it.

## Impact

### Affected Specs
- **chat-interface** (dependency) - Must be implemented first
- **chat-improvements** (new) - Define UX enhancements to framework

### Affected Code
**Enhancements to Chat Framework:**
- `crates/crucible-cli/src/chat/display.rs` - Enhanced formatting and rendering
- `crates/crucible-cli/src/chat/commands.rs` - Add file reference and search commands
- `crates/crucible-cli/src/chat/session.rs` - Add session statistics tracking

**New Components:**
- `crates/crucible-cli/src/chat/file_picker.rs` - NEW - Fuzzy file selection
- `crates/crucible-cli/src/chat/history.rs` - NEW - Command history management

**Dependencies:**
- `fuzzy-matcher = "0.3"` - For `@file` fuzzy selection
- `crossterm = "0.27"` - For enhanced keyboard handling

### User-Facing Impact
- **Cleaner Experience**: More readable, less cluttered output
- **More Productive**: File references and inline search speed up workflows
- **Better Understanding**: Session stats and visible tool calls provide transparency
- **Modern Feel**: Matches expectations from other AI chat tools

### Timeline
- **Dependency**: Requires `chat-interface` to be implemented first
- **Week 1**: Visual polish and file references
- **Week 2**: Interactive features and statistics
- **Estimated effort**: 1-2 weeks after framework is ready
