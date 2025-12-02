# Chat Interface Framework

## Why

Users need a polished, consistent chat experience regardless of which AI backend they're using (ACP external agents, future internal agents, direct LLM APIs). The current chat implementation is tightly coupled to ACP, making it impossible to reuse the UX for other backends.

A reusable chat framework enables:
1. **Consistent UX**: Same interface for all AI backends
2. **Mode Switching**: Toggle between plan (read-only) and act (write-enabled) seamlessly
3. **Command Handling**: Slash commands like `/search`, `/mode`, `/plan`, `/act`
4. **Rich Display**: Markdown rendering, tool call visualization, progress indicators
5. **Extensibility**: Easy to add new agent backends

## What Changes

**Chat Framework Abstraction:**
- Define `ChatAgent` trait for any AI backend
- `ChatSession` orchestrator manages conversation flow
- Mode management independent of backend implementation
- Command parsing and dispatch

**Reusable Components:**
- Mode switching (plan → act → auto → plan)
- Slash command handling (`/search`, `/mode`, `/plan`, `/act`, `/exit`)
- Display formatting (markdown rendering, tool calls, context)
- Keyboard shortcuts (Ctrl+J for newline, Shift+Tab for mode cycling, Ctrl+C twice to exit)

**Implementation:**
- ACP client implements `ChatAgent` trait
- Future internal agents will implement same trait
- Single `ChatSession` works with any `ChatAgent` implementation

## Impact

### Affected Specs
- **chat-interface** (new) - Define `ChatAgent` trait and `ChatSession` framework
- **acp-integration** (modify) - ACP client implements `ChatAgent` trait
- **agent-system** (future) - Internal agents will implement `ChatAgent` trait

### Affected Code
**New Components:**
- `crates/crucible-cli/src/chat/` - NEW - Chat framework module
  - `mod.rs` - Module definition and exports
  - `session.rs` - `ChatSession` orchestrator
  - `agent_trait.rs` - `ChatAgent` trait definition
  - `mode.rs` - Mode management (ChatMode enum)
  - `commands.rs` - Slash command parsing
  - `display.rs` - Formatting and rendering

**Refactoring:**
- `crates/crucible-cli/src/commands/chat.rs` - Use `ChatSession` instead of direct ACP calls
- `crates/crucible-cli/src/acp/client.rs` - Implement `ChatAgent` trait

**Dependencies:**
- `async-trait = "0.1"` - For async trait methods

### User-Facing Impact
- **No UX Changes**: User experience stays identical (this is pure refactoring)
- **Foundation for Future**: Enables internal agents, direct LLM integration
- **Consistent Interface**: All future agent backends will share same UX
- **Better Reliability**: Isolated, testable components

### Timeline
- **Week 1**: Define `ChatAgent` trait, extract framework components
- **Week 2**: Implement trait for ACP, refactor chat command
- **Week 3**: Testing, documentation
- **Estimated effort**: 2-3 weeks for complete extraction
