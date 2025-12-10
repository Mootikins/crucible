# Quick Prompt Shell Integration Design

## Context

Quick prompts provide a lightweight, scriptable interface for common Crucible operations **directly from the user's shell prompt**. This is a **shell integration** feature similar to cursor-agent's integration, where users source a script in their `.zshrc` or `.bashrc` to enable quick prompts from anywhere in their shell.

**Stakeholders**: Shell users (zsh/bash), power users, automation scripts
**Constraints**: Must work as shell integration script, communicate with Crucible CLI/API

## Architecture

### Shell Integration Flow

```
User's Shell (.zshrc / .bashrc)
├── Sources: eval "$(crucible shell-integration zsh)"
├── Defines: prompt_mode variable
├── Sets up: ZLE widgets (zsh) or readline bindings (bash)
└── Tab/Enter handlers → call `crucible quick-prompt` subcommand
    ↓
Crucible CLI
├── `crucible quick-prompt "note: content"` command
├── Trigger registry matches prefix
├── Routes to handler (note creation, search, agent, etc.)
└── Returns result to shell
```

### Component Structure

```
Shell Integration Script (generated)
├── prompt_mode toggle function
├── Tab keybinding handler
├── Enter keybinding handler
└── Calls: crucible quick-prompt "$input"

Crucible CLI
├── quick-prompt subcommand
├── trigger_registry: PromptTriggerRegistry
├── trigger handlers
└── Executes action and returns result
```

### Integration Points

1. **Shell Integration**: Script generation for zsh/bash
2. **ZLE/Readline**: Keybinding handlers in user's shell
3. **CLI Subcommand**: `crucible quick-prompt` handles execution
4. **Trigger Registry**: Prefix matching and handler routing
5. **Storage/Agent Systems**: Note creation, search, agent communication

## Goals / Non-Goals

**Goals:**
- Fast, lightweight shell integration for common Crucible operations
- Tab/Enter toggle for prompt mode (intuitive UX)
- Prefix-based trigger system (`note:`, `agent:`, `search:`)
- Works in user's regular shell (zsh/bash)
- Extensible trigger system for future additions
- Visual prompt indicator for mode state

**Non-Goals (MVP):**
- Full chat session in shell (use `crucible chat` for that)
- Complex multi-line input editing
- Trigger autocomplete (future enhancement)
- Custom keybindings (Tab/Enter are fixed for MVP)
- Fish shell support (zsh/bash only for MVP)

## Decisions

### Shell Integration Approach: Generated Scripts

**Decision:** Generate shell integration scripts via `crucible shell-integration zsh|bash` command that users source in their `.zshrc`/`.bashrc`.

**Rationale:**
- Standard pattern (used by cursor-agent, git, etc.)
- Allows versioning and updates via Crucible CLI
- Users can inspect/modify scripts if needed
- Works across different shell configurations

**Alternatives considered:**
- Binary shell functions: Harder to update, less transparent
- Separate installation script: Extra step, less discoverable
- Plugin system: Overkill for MVP

### Prompt Mode Toggle: Tab at Start of Line

**Decision:** Tab key at the beginning of a line toggles prompt mode on/off.

**Rationale:**
- Intuitive and discoverable (Tab is commonly used for mode switching)
- Doesn't interfere with normal tab completion (only at start of line)
- Clear visual feedback via prompt indicator
- Resets after each use (prevents confusion)

**Alternatives considered:**
- Custom keybinding (Ctrl+P, etc.): Less discoverable, conflicts possible
- Command prefix (`cru:`): Requires typing, less seamless
- Always-on mode: Too intrusive, interferes with normal shell usage

### Trigger System: Prefix-Based Matching

**Decision:** Use prefix matching (`note:`, `agent:`, `search:`) to route to handlers.

**Rationale:**
- Natural language feel (`note: content` reads well)
- Easy to extend with new triggers
- Clear separation between trigger and content
- Falls back to agent handler for unknown prefixes

**Alternatives considered:**
- Command-style (`cru note content`): Less natural, requires parsing
- Regex patterns: Overly complex for MVP
- Config-only triggers: Less discoverable, harder to document

### Communication: CLI Subcommand

**Decision:** Shell integration calls `crucible quick-prompt "$input"` subcommand.

**Rationale:**
- Uses existing CLI infrastructure
- No need for separate API server
- Works offline (no network dependency)
- Easy to test and debug

**Alternatives considered:**
- HTTP API: Requires server running, adds complexity
- Named pipe/Unix socket: More complex, less portable
- Direct library calls: Requires shell to load Rust library (not feasible)

### ZLE Widgets vs Readline Bindings

**Decision:** Use zsh ZLE widgets for zsh, bash readline bindings for bash.

**Rationale:**
- Native integration for each shell
- Better UX (proper keybinding support)
- Standard approach for shell integrations
- Allows prompt customization

**Implementation notes:**
- Zsh: `zle -N` to define widgets, bind to keys
- Bash: `bind` command in `.bashrc` for readline bindings
- Both need to detect cursor position (start of line)

## Risks / Trade-offs

**Risk:** Shell integration conflicts with user's existing keybindings.
- **Mitigation:** Only bind Tab at start of line, check cursor position carefully. Document conflicts.

**Risk:** Generated scripts become stale if Crucible CLI is updated.
- **Mitigation:** Scripts are simple and stable. Users can regenerate with `crucible shell-integration zsh`.

**Risk:** Prompt mode state gets out of sync (user switches terminals).
- **Mitigation:** Mode resets after each use. State is per-shell-session, not global.

**Trade-off:** Tab at start of line prevents normal tab completion at line start.
- **Accepted:** Users can type a space first, or use Tab twice quickly. This is acceptable for the convenience gained.

**Trade-off:** Requires Crucible CLI to be in PATH.
- **Accepted:** Standard requirement for CLI tools. Document in installation instructions.

## Migration Plan

N/A - New feature, no existing functionality to migrate.

Users will need to:
1. Run `crucible shell-integration zsh` (or `bash`)
2. Add `eval "$(crucible shell-integration zsh)"` to `.zshrc` (or `.bashrc`)
3. Restart shell or source the file

## Open Questions

1. **Trigger configuration format**: Should triggers be configured via TOML config file, or hardcoded for MVP?
   - **Tentative answer:** Hardcoded triggers for MVP (`note:`, `agent:`, `search:`). Add config support in Phase 2.

2. **Error handling**: How should errors be displayed in shell? Stderr? Special format?
   - **Tentative answer:** Print errors to stderr with clear messages. Shell integration can format if needed.

3. **Streaming responses**: Should agent responses stream to shell, or wait for completion?
   - **Tentative answer:** Wait for completion for MVP. Streaming adds complexity and may interfere with shell prompt.

4. **Multi-line input**: Should prompt mode support multi-line input?
   - **Tentative answer:** Single-line only for MVP. Multi-line can be added later if needed.
