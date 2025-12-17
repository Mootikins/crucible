# Internal Agent System

## Why

Currently, Crucible's chat functionality requires external ACP agents (claude-code, gemini-cli, etc.). Users who want to:
- Use local LLMs (Ollama, llama.cpp)
- Use direct API access (OpenAI, Anthropic) without ACP overhead
- Customize agent behavior via agent cards

...have no path forward. The chat framework (`AgentHandle` trait, `ChatSession`) is already decoupled from ACP, but there's no internal implementation.

This change adds a complete internal agent system that:
1. Implements `AgentHandle` using direct LLM API calls
2. Uses agent cards for system prompts and configuration
3. Manages conversation context with pluggable strategies
4. Provides Ollama and OpenAI-compatible provider backends

## What Changes

**LLM Provider Implementations:**
- Extend existing `LlmProvider` trait with `context_window()` method
- Add `OllamaProvider` implementation in crucible-llm
- Add `OpenAiCompatibleProvider` for OpenAI/Azure/LiteLLM/vLLM

**Context Management:**
- New `ContextStrategy` trait for managing token budgets
- `SlidingWindowStrategy` as default (keeps system prompt + recent messages)
- Token estimation via heuristic (4 chars/token), actual tracking from API responses
- `/compact` command for manual compaction

**CrucibleAgentHandle:**
- Implements `AgentHandle` trait using `LlmProvider` + `ContextStrategy`
- Injects agent card system prompt automatically
- Tracks token usage from API responses

**Configuration & Integration:**
- API key resolution: env var → file → plaintext (with warning)
- Backend selection at startup: `--internal` (default) or `--acp <agent>`
- New slash commands: `/compact`, `/context`, `/agent`

## Impact

### Affected Specs
- **internal-agent-system** (new) - Core internal agent functionality
- **agent-system** (extends) - Agent cards now usable with internal backend

### Affected Code

**crucible-core:**
- `src/traits/llm.rs` - Add `context_window()` to `LlmProvider`
- `src/traits/context.rs` - NEW - `ContextStrategy` trait

**crucible-llm:**
- `src/text_generation/` - NEW - Provider implementations
- `src/context/` - NEW - `SlidingWindowStrategy`

**crucible-cli:**
- `src/agents/` - NEW - `CrucibleAgentHandle`, `AgentFactory`
- `src/chat/commands.rs` - Add `/compact`, `/context`, `/agent`
- `src/chat/session.rs` - Backend selection logic

**crucible-config:**
- Add `[llm]` configuration section

### User-Facing Impact
- Users can chat with local LLMs without ACP setup
- Agent cards work with internal backend
- Context management is automatic with manual override

## Future Work (not in this change)
- System keyring for API keys (`cru config set-key`)
- `/subagent` command for child agents
- `SummarizationStrategy` for context compaction
- Kebab-case config migration

---

## Amendment: Context Stack Operations

*Added via add-session-daemon proposal*

### Context as Stack

Context is modeled as a stack/deque for granular control. Each entry is a message (human, agent, tool call, tool result).

```
┌─────────────────────────────────┐
│ Tool result: error              │ ← top (newest)
├─────────────────────────────────┤
│ Tool call: edit file X          │
├─────────────────────────────────┤
│ Agent: "I'll edit file X..."    │
├─────────────────────────────────┤
│ Human: "Fix the auth bug"       │
├─────────────────────────────────┤
│ System prompt                   │ ← bottom (never popped)
└─────────────────────────────────┘
```

### Stack Operations

| Operation | Description |
|-----------|-------------|
| `pop(n)` | Remove top N entries |
| `checkpoint(name)` | Mark current position as named restore point |
| `rollback(name)` | Pop until named checkpoint |
| `replace_top(summary)` | Pop top entry, push LLM-generated summary |
| `reset()` | Pop all except system prompt |
| `summarize()` | LLM-generate summary of current context |

### Failure Recovery Patterns

| Failure Type | Action | Rationale |
|--------------|--------|-----------|
| Tool error | `pop(1)` + inject error msg | Bad execution, not bad thinking |
| Wrong approach | `rollback(checkpoint)` | Keep problem understanding, discard bad path |
| Confusion spiral | `reset()` + summary | Polluted context, fresh start |
| Fundamental misunderstanding | `reset()` + human clarification | Need new information |

### Slash Commands

```
/context                    Show context stack summary
/context pop [n]            Remove last N entries (default 1)
/context checkpoint <name>  Create named checkpoint
/context rollback <name>    Rollback to checkpoint
/context reset              Clear all except system prompt
/context summarize          Replace context with LLM summary
```

### Philosophy

LLMs are stateless. Long conversations accumulate:
- Outdated assumptions
- Conflicting instructions
- Error spirals ("I apologize, let me try again...")

The context stack embraces this - `reset` + concise summary often works better than continuing a polluted conversation. Checkpoint/rollback provides methodology for recovery, not just last-resort reset.

### Integration

- Context stack trait in `crucible-core/src/traits/context.rs`
- Implemented by `CrucibleAgentHandle` for internal agents
- ACP agents: context ops translate to session management (limited support)

---

## Amendment: ACP Capabilities Flow

*Added to align with Agent Client Protocol spec for modes and slash commands*

### Agent-Defined Capabilities

The ACP spec defines that agents advertise their capabilities during session setup:

**Session Modes** - Agents return `SessionModeState` in `NewSessionResponse`:
```rust
pub struct SessionModeState {
    pub current_mode_id: SessionModeId,
    pub available_modes: Vec<SessionMode>,
}

pub struct SessionMode {
    pub id: SessionModeId,      // e.g., "plan", "code", "architect"
    pub name: String,           // Human readable
    pub description: Option<String>,
}
```

**Slash Commands** - Agents send `AvailableCommandsUpdate` notification:
```rust
pub struct AvailableCommand {
    pub name: String,              // e.g., "create_plan", "research"
    pub description: String,
    pub input: Option<AvailableCommandInput>,
}
```

### Mode Registry

New `ModeRegistry` mirrors the `SlashCommandRegistry` pattern:

```rust
pub struct ModeRegistry {
    /// Reserved modes (client-defined, always available)
    reserved: Vec<ModeDescriptor>,
    /// Agent-provided modes
    agent_modes: Option<SessionModeState>,
    /// Current active mode
    current_mode_id: SessionModeId,
}

impl ModeRegistry {
    /// Get all available modes (reserved + agent)
    pub fn list_all(&self) -> Vec<&ModeDescriptor>;

    /// Set mode (validates against available modes)
    pub fn set_mode(&mut self, mode_id: &SessionModeId) -> Result<()>;

    /// Update from agent notification
    pub fn update_from_agent(&mut self, state: SessionModeState);
}
```

### Reserved Commands (Namespaced)

Client-only commands that cannot be overridden by agents use `/crucible:` namespace when conflicts occur:

| Command | Namespace | Description |
|---------|-----------|-------------|
| `/exit`, `/quit` | N/A (reserved) | Exit session |
| `/help` | N/A (reserved) | Show merged command list |
| `/search` | `/crucible:search` if conflict | Local semantic search |
| `/context` | `/crucible:context` if conflict | Context stack operations |
| `/clear` | `/crucible:clear` if conflict | Clear screen |

**Conflict Resolution:**
1. Agent registers command `/search`
2. Client detects conflict with reserved command
3. Client's `/search` becomes `/crucible:search`
4. Agent's `/search` takes the bare name
5. Help shows both: `/search` (agent) and `/crucible:search` (local)

### AgentHandle Extensions

Add capability methods to `AgentHandle` trait:

```rust
pub trait AgentHandle {
    // Existing methods...

    /// Get available modes (None if agent doesn't support modes)
    fn get_modes(&self) -> Option<&SessionModeState>;

    /// Set current mode
    async fn set_mode(&mut self, mode_id: &SessionModeId) -> ChatResult<()>;

    /// Called when agent changes mode autonomously
    async fn on_mode_update(&mut self, mode_id: SessionModeId) -> ChatResult<()> {
        Ok(()) // Default: ignore
    }

    /// Get available commands
    fn get_commands(&self) -> &[AvailableCommand];

    /// Called when agent updates available commands
    async fn on_commands_update(&mut self, commands: Vec<AvailableCommand>) -> ChatResult<()>;
}
```

### Internal Agent Default Modes

`CrucibleAgentHandle` provides default modes matching current behavior:

```rust
impl CrucibleAgentHandle {
    fn default_modes() -> SessionModeState {
        SessionModeState {
            current_mode_id: "plan".into(),
            available_modes: vec![
                SessionMode {
                    id: "plan".into(),
                    name: "Plan".to_string(),
                    description: Some("Read-only mode, no file modifications".into()),
                },
                SessionMode {
                    id: "act".into(),
                    name: "Act".to_string(),
                    description: Some("Write-enabled, requests permission".into()),
                },
                SessionMode {
                    id: "auto".into(),
                    name: "Auto".to_string(),
                    description: Some("Auto-approve all operations".into()),
                },
            ],
        }
    }
}
```

### TUI Integration

TUI queries registries instead of hardcoding:

```rust
// Status line renders from mode registry
let mode = session.mode_registry().current();
render_status_line(mode.name, mode_color(mode.id));

// Mode switching validates against registry
if let Some(mode) = session.mode_registry().find(&mode_id) {
    session.set_mode(&mode.id).await?;
} else {
    return Err(ChatError::InvalidMode(mode_id));
}

// Help shows merged command list
let commands = session.command_registry().list_all();
for cmd in commands {
    println!("/{} - {}", cmd.name, cmd.description);
}
```

### Data Flow

```
Agent Session Setup
       │
       ▼
NewSessionResponse { modes: Some(SessionModeState) }
       │
       ▼
ModeRegistry.update_from_agent(modes)
       │
       ▼
session/update { available_commands_update }
       │
       ▼
CommandRegistry.update_from_agent(commands)
       │
       ▼
TUI queries registries for display & validation
```
