# Quick Prompt Shell Integration

## Why

Users need fast, lightweight ways to interact with Crucible agents and workflows directly from their shell prompt without launching a full CLI chat session. Similar to cursor-agent's shell integration, Crucible should provide a shell integration script that enables quick, scriptable interactions for common tasks like creating notes, running workflows, and querying the knowledge base.

Quick prompts bridge the gap between:
- **Full CLI chat sessions** (powerful but requires launching `crucible chat`)
- **API calls** (programmatic but requires setup)
- **CLI commands** (structured but limited flexibility)

This enables:
1. **Rapid note-taking**: `note: Meeting notes from standup` → creates note instantly from shell
2. **Workflow triggers**: `task: Review PR #123` → starts workflow from shell
3. **Quick queries**: `search: CRDT implementation` → fast semantic search from shell
4. **Scriptable automation**: Can be used in shell scripts and aliases
5. **HMI (Human-Machine Interface)**: Natural language interface accessible from any shell prompt

## What Changes

**Shell Integration Script:**
- Generate shell integration script (`crucible shell-integration zsh` / `crucible shell-integration bash`)
- Tab/Enter prompt mode toggle in user's shell (zsh ZLE / bash readline)
- Prefix-based workflow triggers (`note:`, `task:`, `search:`, `agent:`)
- Lightweight prompt handler that routes to Crucible CLI/API
- Visual mode indicator in shell prompt
- Works in user's regular shell, not just Crucible CLI

**Trigger System:**
- Extensible trigger system for prefix matching
- Built-in triggers for common actions (note creation, search, agent queries)
- User-configurable custom triggers via config file
- Pattern matching for trigger prefixes
- Routes triggers to Crucible CLI commands or API calls

**Integration Points:**
- Shell integration script sources in `.zshrc` / `.bashrc`
- Uses zsh ZLE (Zsh Line Editor) widgets or bash readline for keybindings
- Communicates with Crucible via CLI subcommands or API
- Can work standalone or alongside cursor-agent integration
- Complements existing Crucible CLI commands

**User Experience:**
- Tab at start of line toggles prompt mode in user's shell
- Visual indicator: `[crucible] $` vs normal `$` prompt
- Enter in prompt mode sends to Crucible instead of executing shell command
- Mode resets after each use (clear UX)
- Works from any directory, any shell session

## Impact

### Affected Specs
- **cli** (modify) - Add shell integration requirements to CLI specification
- **apis** (future) - Quick prompts may use API endpoints when available
- **workflows** (future) - Quick prompts will trigger workflows when workflow system is implemented

### Affected Code

**New Components:**
- `crates/crucible-cli/src/commands/shell_integration.rs` - NEW - Generate shell integration scripts
- `crates/crucible-cli/src/commands/quick_prompt.rs` - NEW - Handle quick prompt execution
- `crates/crucible-cli/src/quick_prompt/` - NEW - Quick prompt handling module
  - `trigger_registry.rs` - Trigger prefix matching and routing
  - `triggers/` - Directory for trigger handlers
    - `note.rs` - Note creation trigger handler
    - `search.rs` - Quick search trigger handler
    - `agent.rs` - Direct agent prompt handler

**Modified Components:**
- `crates/crucible-cli/src/cli.rs` - Add `shell-integration` and `quick-prompt` subcommands
- `crates/crucible-cli/src/commands/mod.rs` - Export new command modules
- `crates/crucible-cli/src/config.rs` - Add trigger configuration support

**Shell Integration Scripts:**
- Generated zsh script with ZLE widgets for Tab/Enter handling
- Generated bash script with readline bindings for Tab/Enter handling
- Scripts handle prompt mode toggle and trigger routing
- Scripts call `crucible quick-prompt` subcommand with input

**Dependencies:**
- No new external dependencies (uses existing CLI infrastructure)

### User-Facing Impact
- **Faster Workflows**: Quick actions without launching full CLI chat session
- **Natural Language**: Prefix triggers feel more natural than CLI commands
- **Discoverable**: Tab toggle is intuitive and discoverable
- **Scriptable**: Can be used in shell scripts and automation
- **Always Available**: Works from any shell prompt, any directory
- **Non-Breaking**: Complements existing CLI commands, doesn't replace them

### Timeline
- **Week 1**: Implement shell integration script generation (zsh)
- **Week 2**: Add trigger registry and basic triggers (`note:`, `agent:`)
- **Week 3**: Add bash support, advanced triggers (`search:`, `task:`), and configuration
- **Estimated effort**: 2-3 weeks

### Dependencies
- **cli** (required) - Must have CLI command infrastructure
- **workflows** (optional) - Advanced triggers will use workflow system when available
