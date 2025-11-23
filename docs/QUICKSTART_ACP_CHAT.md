# Quickstart: ACP Chat with MCP Tools

> Get started with Crucible's AI agent integration in 5 minutes

## What This Does

Enables AI agents (like Claude Code) to automatically access your Crucible knowledge base through 12 built-in tools for reading, writing, and searching notes.

## Prerequisites

- Rust toolchain installed
- An ACP-compatible AI agent (e.g., claude-code)
- A Crucible kiln (note repository)

## Step 1: Build Crucible

```bash
# Clone the repository (if you haven't already)
git clone https://github.com/your-org/crucible.git
cd crucible

# Build in release mode
cargo build --release

# Add to PATH
export PATH="$PWD/target/release:$PATH"

# Verify installation
cru --version
```

## Step 2: Initialize Your Kiln

```bash
# Create a kiln directory
mkdir -p ~/my-kiln

# Configure Crucible
cru config set kiln.path ~/my-kiln

# Create some test notes
echo "# Test Note\n\nThis is a test note." > ~/my-kiln/test.md
echo "# Another Note\n\nAbout testing MCP integration." > ~/my-kiln/test2.md
```

## Step 3: Test MCP Server (Optional)

Verify the MCP server works standalone:

```bash
# Run the test client
cargo run --release --example test_mcp_server

# Expected output:
# ✅ Connected to server: crucible-mcp-server
# ✅ Found 12 tools:
#   • create_note
#   • read_note
#   ...
# ✅ SUCCESS: All 12 tools discovered!
```

## Step 4: Install an ACP Agent

Choose one of these agents:

### Option A: Claude Code (Recommended)

```bash
# Install via npm
npm install -g @anthropic-ai/claude-code

# Verify installation
claude-code --version
```

### Option B: Other ACP Agents

Check the agent's documentation for installation instructions. Ensure it supports:
- ACP version 0.7.0 or later
- MCP (Model Context Protocol)

## Step 5: Start Chat

```bash
# Start interactive chat
cru chat

# The agent will automatically:
# 1. Receive MCP server configuration
# 2. Spawn `cru mcp` as a child process
# 3. Discover all 12 Crucible tools
# 4. Make them available in the conversation
```

## Step 6: Use the Tools

Try these example queries in the chat:

### List Notes
```
You: List all notes in the kiln
Agent: [Uses list_notes tool]
Agent: I found 2 notes in your kiln:
  - test.md
  - test2.md
```

### Read a Note
```
You: Show me the contents of test.md
Agent: [Uses read_note tool]
Agent: Here's the content of test.md:
  # Test Note

  This is a test note.
```

### Create a New Note
```
You: Create a note called ideas.md with the title "Project Ideas"
Agent: [Uses create_note tool]
Agent: I've created ideas.md with the title "Project Ideas"
```

### Search Notes
```
You: Search for notes about testing
Agent: [Uses text_search tool]
Agent: I found 1 note mentioning "testing":
  - test2.md
```

### Semantic Search
```
You: Find notes similar to "project planning"
Agent: [Uses semantic_search tool]
Agent: Here are semantically similar notes: ...
```

## Available Tools (12 Total)

### Note Management (6 tools)
- `create_note` - Create a new note with frontmatter
- `read_note` - Read note content
- `read_metadata` - Get note metadata only
- `update_note` - Update note content/frontmatter
- `delete_note` - Remove a note
- `list_notes` - List notes in a directory

### Search (3 tools)
- `semantic_search` - AI-powered similarity search
- `text_search` - Fast full-text search
- `property_search` - Search by frontmatter properties

### Kiln Info (3 tools)
- `get_kiln_info` - Get kiln statistics
- `get_kiln_roots` - Get kiln directory info
- `get_kiln_stats` - Detailed kiln metrics

## Chat Modes

### Plan Mode (Default - Read-Only)

```bash
cru chat  # Starts in plan mode
```

In plan mode, the agent can:
- ✅ Read notes
- ✅ Search notes
- ✅ Get kiln info
- ❌ Cannot create or modify notes

### Act Mode (Write-Enabled)

```bash
cru chat --act  # Start in act mode
```

In act mode, the agent can:
- ✅ Read notes
- ✅ Search notes
- ✅ Create notes
- ✅ Update notes
- ✅ Delete notes

You can toggle modes during the session:
```
/plan   - Switch to read-only mode
/act    - Switch to write-enabled mode
/exit   - Exit the chat
```

## One-Shot Queries

Run a single query without interactive mode:

```bash
# Ask a question
cru chat "How many notes do I have?"

# With context enrichment disabled
cru chat --no-context "List all notes"
```

## Troubleshooting

### Agent Not Found

```
Error: No ACP-compatible agent found
```

**Solution:** Install an ACP agent (see Step 4) and ensure it's in your PATH.

### MCP Server Not Starting

```
Error: Failed to spawn MCP server
```

**Solution:**
1. Verify `cru` binary exists: `which cru`
2. Check it's in PATH: `export PATH="$PWD/target/release:$PATH"`
3. Check logs: `cat ~/.crucible/mcp.log`

### Tools Not Discovered

```
Agent: I don't have access to any Crucible tools
```

**Solution:**
1. Verify agent supports ACP 0.7.0+ with MCP
2. Check MCP server logs: `cat ~/.crucible/mcp.log`
3. Restart the chat session

### Invalid Kiln Path

```
Error: Kiln path not found
```

**Solution:**
```bash
# Set kiln path
cru config set kiln.path ~/my-kiln

# Verify
cru config get kiln.path
```

## Debug Mode

Enable detailed logging:

```bash
# Set log level
export RUST_LOG=debug

# Start chat
cru chat

# Check logs
cat ~/.crucible/mcp.log
```

## Advanced Usage

### Custom Context Size

```bash
# Include more context in queries (default: 5)
cru chat --context-size 10
```

### Disable Context Enrichment

```bash
# Faster queries, no automatic context
cru chat --no-context
```

### Specific Agent

```bash
# Use a specific agent if multiple are installed
cru chat --agent claude-code
```

## What's Happening Behind the Scenes

1. **Handshake:** When you run `cru chat`, the CLI:
   - Discovers available ACP agents
   - Spawns the agent process
   - Sends a handshake with MCP server configuration

2. **MCP Server Spawn:** The agent:
   - Receives the MCP server config
   - Spawns `cru mcp` as a child process
   - Connects via stdio (standard input/output)

3. **Tool Discovery:** The agent:
   - Sends a `tools/list` request to the MCP server
   - Receives definitions for all 12 tools
   - Makes them available in the conversation

4. **Tool Execution:** During chat:
   - Agent decides to use a tool
   - Sends `tools/call` request with parameters
   - MCP server executes the tool
   - Returns structured results
   - Agent integrates results into response

## Next Steps

### Learn More
- Read `docs/MCP_INTEGRATION.md` for architecture details
- See `docs/ACP_TESTING_PLAN.md` for testing strategies
- Check `AGENTS.md` for AI agent development

### Customize
- Configure embedding models for semantic search
- Set up custom note templates
- Create automation workflows

### Extend
- Add custom tools to the MCP server
- Integrate with other knowledge bases
- Build agent plugins

## Getting Help

- **Documentation:** `docs/` directory
- **Examples:** `crates/crucible-cli/examples/`
- **Tests:** `crates/crucible-acp/tests/`
- **Issues:** File on GitHub

## Performance Tips

1. **Semantic Search:** Requires embeddings - first search may be slow
2. **Large Kilns:** Consider pagination for list operations
3. **Context Size:** Reduce if queries are slow
4. **File Logging:** Check `~/.crucible/mcp.log` size periodically

## Security Notes

- **Plan Mode:** Default for safety - agent can't modify notes
- **Act Mode:** Only enable when you trust the agent's actions
- **Local Only:** MCP server runs locally, no network exposure
- **No API Keys:** MCP server doesn't need API keys

## FAQ

**Q: Do I need an internet connection?**
A: Only for the AI agent itself. The MCP server runs locally.

**Q: Can multiple agents use the same kiln?**
A: Yes, but not simultaneously. Each chat session spawns its own MCP server.

**Q: Are my notes sent to the cloud?**
A: Only what you explicitly discuss with the agent. The MCP server is local.

**Q: Can I use this in scripts?**
A: Yes! Use one-shot mode: `cru chat "query here"`

**Q: How do I update the tools?**
A: Rebuild Crucible: `cargo build --release`

## Example Session

```bash
$ cru chat

🤖 Crucible Chat
=================
Mode: plan (read-only)

Commands:
  /plan - Switch to plan mode (read-only)
  /act - Switch to act mode (write-enabled)
  /exit - Exit chat

plan 📖 > List all my notes

╭─ Agent Response ──────────────────────────╮
│ I found 2 notes in your kiln:              │
│                                            │
│ 1. test.md - "Test Note"                  │
│ 2. test2.md - "Another Note"              │
│                                            │
│ Would you like me to show the content of  │
│ any of these notes?                       │
╰────────────────────────────────────────────╯

plan 📖 > /act

→ Mode switched to: act (write-enabled)

act ✏️ > Create a note called journal.md with today's date

╭─ Agent Response ──────────────────────────╮
│ I've created journal.md with today's date │
│ (2024-01-15). Would you like me to add    │
│ any content to it?                        │
╰────────────────────────────────────────────╯

act ✏️ > /exit

👋 Goodbye!
```

## Success!

You're now ready to use AI agents with your Crucible knowledge base. The agents can help you:
- Organize notes
- Find information quickly
- Create structured content
- Maintain your knowledge graph

Happy note-taking! 📝✨
