# Improve CLI Chat Interface

## Why

The current CLI chat interface has solid functionality but suffers from visual clutter and workflow friction that reduce the user experience. Key issues include excessive visual noise (boxes around messages), limited interactive capabilities (no file references), and mode switching friction that make the chat feel less responsive than modern AI agent interfaces.

## What Changes

**Visual Cleanup:**
- Remove decorative boxes around startup messages and agent responses for cleaner output
- Simplify message formatting with minimal visual separators
- Add subtle indentation or styling improvements instead of heavy borders
- Streamline agent metadata display to reduce visual noise

**Enhanced Interactive Features:**
- Add `/search` command for manual semantic search within chat sessions
- Implement `@` file reference system with fuzzy selection (like other agent CLIs)
- Show tool calls and file edits transparently during agent execution
- Add auto-approve mode with visual indicators for approved operations
- Support Shift+Tab hotkey for seamless mode switching (plan ↔ normal ↔ auto-approve)

**Improved User Experience:**
- Add real-time feedback during long-running operations
- Implement better error display and recovery suggestions
- Add session statistics and context usage information
- Support command history search and recall
- Add keyboard shortcuts for common operations
- Double Ctrl+C to exit (first press shows warning, second within 2s exits)
- Markdown terminal rendering with ANSI escape codes for formatted output

## Impact

### Affected Specs
- **cli** (modify) - Enhance existing chat interface requirements

### Affected Code
**Major Changes:**
- `crates/crucible-cli/src/commands/chat.rs` - Streamline output formatting and add new commands
- `crates/crucible-cli/src/chat/` - NEW - File reference system and mode management
- `crates/crucible-cli/src/ui/` - NEW - Clean display components
- `crates/crucible-cli/src/config/chat.rs` - NEW - Chat-specific preferences

**Dependencies Added:**
- `fuzzy-matcher = "0.3"` - For @ file reference selection
- `crossterm = "0.27"` - For enhanced keyboard handling

### User-Facing Impact
- **Cleaner interface**: Reduced visual noise improves readability and focus
- **Better interactivity**: File references and search make conversations more productive
- **Faster workflows**: Auto-approve and hotkeys reduce mode switching friction
- **More transparency**: Visible tool calls and edits provide better understanding
- **Modern experience**: Brings Crucible chat to parity with other AI agent CLIs

### Timeline
- **Week 1**: Visual cleanup and core interactive features
- **Week 2**: Advanced features and polish
- **Estimated effort**: 1-2 weeks for complete feature set