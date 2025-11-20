# In-Project Agent System

## Why

The ACP integration enables external agents to interact with Crucible's knowledge base. However, complex tasks often require task decomposition, specialized expertise, and multi-step workflows. An in-project agent system allows:

1. **Task Decomposition**: Break large tasks into focused subtasks handled by specialized subagents
2. **Specialized Expertise**: Define agents optimized for specific roles (code review, testing, documentation)
3. **Multi-Step Workflows**: Chain agents together (research → plan → implement → review)
4. **Foundation for A2A**: Build toward Agent-to-Agent (A2A) communication with parallel execution and inter-agent channels

This system learns from successful patterns in Claude Code, Gemini Code Assist, and similar tools, while maintaining Crucible's plaintext-first, kiln-centric philosophy.

## What Changes

**New Capabilities:**
- Agent definition via markdown files with frontmatter (`.crucible/agents/*.md`)
- Agent registry for discovering and validating agent definitions
- Agent spawning tool for primary agents to create subagents
- Session management with markdown storage and wikilink-based parent/child tracking
- Permission inheritance system (subagents cannot exceed parent permissions)
- Execution queue for sequential agent processing (designed for future concurrency)
- Progress observability showing subagent actions to users
- **Reflection system** for self-improvement (optional, Reflexion pattern)
- **Human approval gates** for sensitive operations (HITL pattern)
- **Distributed tracing** with trace IDs and parent chain tracking

**Architecture:**
- Maximum agent depth: 2 levels (User → Primary Agent → Subagents)
- Execution model: Separate LLM calls per agent (stateless subagents)
- Context isolation: Subagents receive only task description + kiln read access
- Result format: Markdown files stored in session folders
- Queue-based execution: Sequential for MVP, provider-specific limits for future

**Storage Pattern:**
- Session folders: `.crucible/sessions/YYYY-MM-DD-description/`
- Parent-child relationships via wikilinks in session markdown
- Similar to Gemini's session storage approach
- Enables session replay, debugging, and learning

## Impact

### Affected Specs
- **agent-system** (new capability) - Define agent orchestration, spawning, permissions
- **tool-system** (reference) - Agents will use existing MCP tools (eventually per-MCP-server agents)
- **acp-integration** (reference) - ACP tests context injection, agent system is next evolution
- **session-management** (new sub-capability) - Session storage, wikilink relationships

### Affected Code
**New Components:**
- `crates/crucible-agents/` - NEW crate for agent system
  - `src/registry.rs` - Agent discovery and validation
  - `src/definition.rs` - Agent definition parsing (frontmatter + markdown)
  - `src/spawner.rs` - Subagent spawning logic
  - `src/session.rs` - Session folder management, wikilink tracking
  - `src/queue.rs` - Execution queue (sequential for now)
  - `src/permissions.rs` - Permission inheritance and validation
  - `src/observer.rs` - Progress reporting to users
  - `src/reflection.rs` - Self-evaluation and retry logic (NEW)
  - `src/approval.rs` - Human-in-the-loop approval gates (NEW)
  - `src/tracing.rs` - Trace ID and parent chain tracking (NEW)

**Integration Points:**
- `crates/crucible-cli/src/commands/chat.rs` - Primary agent can spawn subagents
- `crates/crucible-tools/` - Agents use existing kiln tools (read_note, semantic_search, etc.)
- `.crucible/agents/` - User-defined agents in project kiln
- `~/.config/crucible/agents/` - System-wide agent templates

**Dependencies Added:**
- Reuse existing dependencies (serde, tokio, anyhow)
- Consider `async-channel` for future queue enhancements

### User-Facing Impact
- **Agent Discovery**: Users can list available agents via `cru agents list`
- **Custom Agents**: Users define project-specific agents in `.crucible/agents/`
- **Session History**: Sessions saved to `.crucible/sessions/` for review and learning
- **Progress Visibility**: Users see abbreviated subagent actions during execution
- **Debugging**: Wikilink-based session structure enables easy navigation and debugging
- **Quality Improvement**: Optional reflection enables agents to self-critique and improve outputs
- **Safety Controls**: Approval gates prevent destructive actions without user permission
- **Trace Visibility**: `cru sessions trace` command visualizes execution flow for debugging

### Timeline
- **Week 1**: Agent definition format, registry, basic spawning, permissions
- **Week 2**: Session management, queue, reflection system, tracing
- **Week 3**: Approval gates, CLI integration, observability
- **Week 4**: Testing, documentation, default system agents
- **Estimated effort**: 3-4 weeks for working MVP with advanced features
