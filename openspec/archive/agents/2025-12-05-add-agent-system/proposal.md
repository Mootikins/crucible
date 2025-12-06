# Agent Cards and Workflow Definitions

## Why

Users need a way to define reusable agent workflows and task patterns in their knowledge base. Agent cards provide:

1. **Reusable Workflows**: Define common task patterns once, reuse across projects
2. **Agent Metadata**: Describe what agents do, what they're good at, when to use them
3. **Task Routing**: Match user queries to appropriate agent definitions
4. **Extensibility**: Foundation for future agent orchestration and delegation

This system enables users to build a library of specialized agents tailored to their domain, similar to Claude Code's slash commands but stored as markdown in the kiln.

## What Changes

**New Agent Card Format:**
- Agent definitions as markdown files with YAML frontmatter
- Stored in `.crucible/agents/` (project-specific) or `~/.config/crucible/agents/` (system-wide)
- Frontmatter includes: name, description, keywords, model preference, optional ACP delegation
- Markdown body contains agent instructions and context

**Agent Registry:**
- Automatic discovery of agent cards on CLI startup
- Project agents override system agents (by name)
- Validation of agent card structure and required fields
- Listing and inspection via `cru agents list` and `cru agents show`

**Agent Matching:**
- Keyword-based matching of queries to agent cards
- Ranked results with similarity scores
- Enables routing queries to appropriate specialized agents

**Optional ACP Delegation:**
- Agent cards can specify `acp_server` field to delegate execution
- Example: `acp_server: claude-code` routes execution to external ACP agent
- Enables hybrid approach: agent cards define *what*, ACP defines *how*

## Impact

### Affected Specs
- **agent-system** (new) - Define agent card format, registry, and matching
- **acp-integration** (reference) - Agent cards can delegate to ACP servers
- **cli** (reference) - CLI commands for managing agent cards

### Affected Code
**Current Implementation:**
- `crates/crucible-core/src/agent/` - Already exists, needs specification
  - `types.rs` - `AgentDefinition` struct
  - `loader.rs` - `AgentLoader` for parsing markdown
  - `matcher.rs` - `CapabilityMatcher` for query matching
  - `mod.rs` - `AgentRegistry` for management

**New Components:**
- `crates/crucible-core/src/agent/delegation.rs` - NEW - Optional ACP delegation logic
- Agent card examples in `.crucible/agents/examples/`

**CLI Integration:**
- `cru agents list` - Show all registered agent cards
- `cru agents show <name>` - Display specific agent card details
- `cru agents validate` - Validate all agent card definitions

**Dependencies:**
- No new dependencies (uses existing serde, frontmatter parsing)

### User-Facing Impact
- **Customizable Agents**: Users define project-specific agent workflows
- **Reusable Patterns**: Common tasks codified as agent cards
- **Discovery**: Easy to find and understand available agents
- **Clear Documentation**: Agent cards self-document their purpose and usage
- **Foundation for Future**: Enables agent orchestration, spawning, sessions (future work)

### Timeline
- **Week 1**: Specify agent card format, update registry implementation
- **Week 2**: Add delegation logic, CLI commands, examples
- **Estimated effort**: 1-2 weeks to formalize existing code
