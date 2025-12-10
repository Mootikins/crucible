# Tab/Enter Prompt Integration Research

## Overview

Research into adding Tab/Enter prompt functionality similar to cursor-agent's shell integration, with support for workflow triggers like `note: <content>`.

## Current State

### Cursor-Agent Implementation

The cursor-agent shell integration provides:

1. **Tab Toggle**: Press Tab at the beginning of a line to toggle "agent mode"
2. **Enter Handler**: In agent mode, Enter sends the buffer to cursor-agent instead of executing as a shell command
3. **Mode Indicator**: Visual feedback showing agent mode on/off
4. **Streaming**: Direct TTY streaming for real-time responses

Key implementation details from `/tmp/cursor-integration.sh`:

```zsh
# Mode: 0 = normal zsh, 1 = send buffer as prompt to cursor-agent
zsh_agent_mode=0

toggle-agent-mode() {
  (( zsh_agent_mode ^= 1 ))
  zle -M "Agent mode: $([ $zsh_agent_mode -eq 1 ] && echo on || echo off)"
}

# Tab wrapper: at the beginning of the current line, toggle; otherwise perform normal completion
tab-toggle-or-complete() {
  if [[ -z $LBUFFER || ${LBUFFER[-1]} == $'\n' ]]; then
    toggle-agent-mode
  else
    zle expand-or-complete
  fi
}

# Combined Enter dispatcher
please-fix-or-accept-line() {
  if (( zsh_agent_mode )); then
    local prompt_text="$BUFFER"
    # Stream to cursor-agent
    cursor-agent --resume $CURSOR_AGENT_CHAT_ID "$prompt_text" <"$TTY" >"$TTY" 2>&1
    # Redraw prompt
    zle reset-prompt
  else
    zle .orig-accept-line
  fi
}
```

### Crucible CLI Current Architecture

**Location**: `crates/crucible-cli/src/chat/session.rs`

**Current Input Handling**:
- Uses `reedline` for line editing
- Slash commands (`/search`, `/plan`, `/act`, etc.) via `SlashCommandRegistry`
- Shift+Tab for silent mode cycling
- Ctrl+J for multiline input
- Regular input goes to `handle_message()` → agent

**Key Components**:
1. **SlashCommandRegistry**: Registry pattern for commands (`/exit`, `/search`, etc.)
2. **ChatSession**: Main orchestrator with `run()` loop
3. **ContextEnricher**: Adds relevant notes to prompts
4. **Display**: Handles output formatting

## Proposed Implementation

### Phase 1: Basic Tab/Enter Prompt

Add Tab/Enter prompt functionality to Crucible CLI:

1. **Mode Toggle**: Tab at start of line toggles "prompt mode"
2. **Enter Handler**: In prompt mode, send to agent instead of executing
3. **Visual Indicator**: Show mode in prompt (e.g., `[prompt]` prefix)

**Changes Needed**:

1. **Extend `ChatSession`** (`crates/crucible-cli/src/chat/session.rs`):
   - Add `prompt_mode: bool` state
   - Add Tab keybinding handler
   - Modify Enter handler to check mode

2. **Update Keybindings**:
   ```rust
   // Add Tab binding for prompt mode toggle
   keybindings.add_binding(
       KeyModifiers::NONE,
       KeyCode::Tab,
       ReedlineEvent::ExecuteHostCommand("\x00toggle-prompt".to_string()),
   );
   ```

3. **Modify Input Handler**:
   ```rust
   // In run() loop, check for prompt mode toggle
   if input == "\x00toggle-prompt" {
       self.prompt_mode = !self.prompt_mode;
       // Show indicator
       continue;
   }
   
   // In handle_message or before it:
   if self.prompt_mode {
       // Send directly to agent, skip shell execution
       self.send_to_agent(&input, agent).await?;
       self.prompt_mode = false; // Reset after use
       continue;
   }
   ```

### Phase 2: Workflow Triggers

Add prefix-based workflow triggers like `note:`, `task:`, `fix:`, etc.

**Design**:

1. **Trigger Registry**: Similar to `SlashCommandRegistry`, but for prefixes
2. **Pattern Matching**: Check input for trigger prefixes before sending to agent
3. **Workflow Execution**: Route to specific handlers/workflows

**Example Triggers**:

- `note: <content>` → Create a note with the content
- `task: <description>` → Create a task/workflow
- `fix: <issue>` → Run fix workflow
- `search: <query>` → Quick search (could alias `/search`)
- `agent: <prompt>` → Explicit agent prompt (same as Tab mode)

**Implementation**:

```rust
// New trigger registry
pub struct PromptTriggerRegistry {
    triggers: HashMap<String, Box<dyn PromptTriggerHandler>>,
}

pub trait PromptTriggerHandler {
    async fn handle(&self, content: &str, ctx: &mut ChatContext) -> Result<()>;
}

// In ChatSession:
fn check_triggers(&self, input: &str) -> Option<&dyn PromptTriggerHandler> {
    for (prefix, handler) in &self.trigger_registry.triggers {
        if input.starts_with(prefix) {
            return Some(handler.as_ref());
        }
    }
    None
}

// In run() loop:
if let Some(handler) = self.check_triggers(&input) {
    let content = input.strip_prefix(prefix).unwrap_or(input).trim();
    handler.handle(content, &mut ctx).await?;
    continue;
}
```

### Phase 3: Advanced Features

1. **Multi-line Support**: Allow multiline prompts (Ctrl+J already supported)
2. **History Integration**: Save prompt mode inputs to history
3. **Context Awareness**: Auto-enrich prompts with relevant notes
4. **Streaming**: Real-time streaming responses (already supported via agent)
5. **Custom Triggers**: User-defined trigger prefixes via config

## Integration Points

### 1. Reedline Integration

**Current**: Uses `reedline` for line editing
**Needed**: 
- Tab keybinding (at start of line detection)
- Custom prompt indicator
- Mode state management

**Code Location**: `crates/crucible-cli/src/chat/session.rs:155-167`

### 2. Agent Communication

**Current**: `agent.send_message(&message).await` in `handle_message()`
**Needed**: 
- Direct agent communication for prompt mode
- Skip context enrichment if desired
- Maintain streaming support

**Code Location**: `crates/crucible-cli/src/chat/session.rs:274-352`

### 3. Slash Command System

**Current**: `SlashCommandRegistry` handles `/commands`
**Needed**: 
- Similar registry for prefix triggers
- Could extend existing system or create parallel one

**Code Location**: `crates/crucible-cli/src/chat/slash_registry.rs`

### 4. Workflow System

**Current**: Workflows are planned but not yet implemented
**Needed**: 
- Workflow definitions
- Workflow execution engine
- Trigger → workflow mapping

**Future**: `crates/crucible-core/src/workflow/` (not yet created)

## Implementation Plan

### Step 1: Basic Tab/Enter Prompt (MVP)

1. Add `prompt_mode: bool` to `ChatSession`
2. Add Tab keybinding handler
3. Modify Enter handler to check mode
4. Add visual indicator to prompt
5. Test basic toggle and agent communication

**Files to Modify**:
- `crates/crucible-cli/src/chat/session.rs`

**Estimated Complexity**: Low-Medium

### Step 2: Trigger Registry

1. Create `PromptTriggerRegistry` similar to `SlashCommandRegistry`
2. Add `PromptTriggerHandler` trait
3. Implement basic triggers (`note:`, `agent:`)
4. Integrate into `ChatSession.run()`

**Files to Create**:
- `crates/crucible-cli/src/chat/trigger_registry.rs`

**Files to Modify**:
- `crates/crucible-cli/src/chat/session.rs`
- `crates/crucible-cli/src/chat/mod.rs`

**Estimated Complexity**: Medium

### Step 3: Workflow Integration

1. Design workflow trigger → handler mapping
2. Implement `note:` trigger (create note)
3. Implement `task:` trigger (create task)
4. Add config for custom triggers

**Files to Create**:
- `crates/crucible-cli/src/chat/triggers/` (directory)
- `crates/crucible-cli/src/chat/triggers/note.rs`
- `crates/crucible-cli/src/chat/triggers/task.rs`

**Estimated Complexity**: Medium-High

## Design Decisions

### 1. Tab vs Shift+Tab

**Current**: Shift+Tab cycles modes silently
**Proposal**: Tab at start of line toggles prompt mode
**Rationale**: More discoverable, matches cursor-agent UX

### 2. Mode Persistence

**Option A**: Reset after each use (like cursor-agent)
**Option B**: Persist until toggled off
**Recommendation**: Option A (reset after use) - clearer UX

### 3. Trigger vs Slash Commands

**Current**: Slash commands (`/search`, `/plan`)
**Proposal**: Prefix triggers (`note:`, `task:`)
**Rationale**: 
- More natural for quick actions
- Less typing than `/note`
- Can coexist with slash commands

### 4. Context Enrichment

**Question**: Should prompt mode use context enrichment?
**Options**:
- Always enrich (current behavior)
- Skip enrichment for speed
- Configurable per trigger

**Recommendation**: Configurable, default to enrich

## Examples

### Basic Usage

```
User: [Tab]  # Toggle prompt mode
Prompt: [prompt] 🤔 > 
User: write a function to parse markdown
Agent: [streaming response...]
Prompt: Plan 🤔 >  # Mode reset
```

### Trigger Usage

```
User: note: Meeting notes from standup
System: [Creates note, shows confirmation]
User: task: Review PR #123
System: [Creates task, shows confirmation]
User: agent: explain how CRDTs work
Agent: [streaming response...]
```

### Advanced Usage

```
User: [Tab]
Prompt: [prompt] 🤔 > 
User: note: 
  # Project Ideas
  - Build a CLI tool
  - Add Tab/Enter prompts
  - Implement workflows
[Ctrl+J for multiline, Enter to submit]
System: [Creates note with multiline content]
```

## Open Questions

1. **Workflow System**: How should triggers map to workflows? Direct handler vs workflow engine?
2. **Error Handling**: What happens if a trigger fails? Show error or fall back to agent?
3. **Customization**: How configurable should triggers be? Config file? Runtime registration?
4. **Performance**: Should trigger checking happen before or after context enrichment?
5. **History**: Should prompt mode inputs be saved differently than regular commands?

## Next Steps

1. Review this research with team
2. Create OpenSpec proposal if approved
3. Implement Phase 1 (basic Tab/Enter)
4. Test and iterate
5. Implement Phase 2 (triggers)
6. Design workflow system integration

## References

- Cursor-agent shell integration: `/tmp/cursor-integration.sh`
- Crucible CLI session: `crates/crucible-cli/src/chat/session.rs`
- Slash command registry: `crates/crucible-cli/src/chat/slash_registry.rs`
- Systems architecture: `openspec/SYSTEMS.md`
