# ACP Integration for External Agents

## Why

Users need a way to interact with their knowledge base using natural language through external AI agents. The Agent Client Protocol (ACP) provides a standard way to communicate with AI agents like claude-code, gemini-cli, and others.

This integration enables:
1. **Natural Language Interface**: Ask questions and give commands in plain English
2. **Context Enrichment**: Automatic injection of relevant knowledge base content
3. **Agent Choice**: Work with your preferred AI agent (Claude, Gemini, etc.)
4. **Tool Access**: Agents can search, read, and navigate your knowledge base

## What Changes

**ACP Client Integration:**
- Implement ACP protocol client for communicating with external agents
- Automatic agent discovery (tries claude-code, gemini-cli, codex in order)
- Streaming responses from agents back to user
- Session management for conversations

**Context Enrichment:**
- Automatic semantic search for relevant notes based on user query
- Configurable context size (default 5 notes)
- Formatted context injection into agent prompts
- Optional `--no-context` flag to disable enrichment

**Tool Integration:**
- Expose knowledge base tools to agents via MCP protocol
- Tools: `read_note`, `list_notes`, `semantic_search`, `get_stats`
- In-process MCP server (no external processes)
- Agents can explore and navigate knowledge base

**Chat Command:**
- `cru chat` - Start interactive conversation with agent
- `cru chat "query"` - One-shot query mode
- Mode switching: plan (read-only) vs act (write-enabled)
- Background file processing and watching during chat

## Impact

### Affected Specs
- **acp-integration** (new) - Define ACP client, context enrichment, tool exposure
- **agent-system** (reference) - Agent cards can delegate to ACP
- **cli** (modify) - Add chat command

### Affected Code
**New Components:**
- `crates/crucible-cli/src/acp/` - NEW - ACP client implementation
  - `client.rs` - ACP protocol client
  - `agent.rs` - Agent discovery and spawning
  - `context.rs` - Context enrichment logic

**CLI Integration:**
- `crates/crucible-cli/src/commands/chat.rs` - NEW - Chat command implementation
- Update `crates/crucible-cli/src/cli.rs` with chat subcommand

**Dependencies:**
- Uses existing MCP tools from `crucible-tools` crate
- No new external dependencies

### User-Facing Impact
- **Natural Language Access**: Talk to knowledge base conversationally
- **Smart Context**: Relevant notes automatically provided to agent
- **Agent Flexibility**: Works with multiple AI agents
- **Safe by Default**: Plan mode prevents accidental modifications
- **Background Processing**: Files stay indexed during conversations

### Timeline
- **Week 1**: ACP client, agent discovery, basic chat
- **Week 2**: Context enrichment, tool integration, mode switching
- **Estimated effort**: 1-2 weeks for working MVP
