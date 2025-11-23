# MCP Server Integration Guide

> How AI agents discover and use Crucible's knowledge management tools

## Overview

Crucible provides a built-in **MCP (Model Context Protocol) server** that exposes 12 knowledge management tools to AI agents through a standardized protocol. This enables agents to read, write, search, and manage your knowledge base automatically.

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│  AI Agent (Claude, Gemini, etc.)                            │
│  - Receives MCP server config via ACP NewSessionRequest     │
│  - Automatically spawns `cru mcp` via stdio                  │
│  - Discovers 12 tools via MCP protocol                      │
└────────────────────┬────────────────────────────────────────┘
                     │ stdio (JSON-RPC)
                     ▼
┌─────────────────────────────────────────────────────────────┐
│  Crucible MCP Server (`cru mcp`)                            │
│  - ServerHandler implementation with tool_router            │
│  - Serves via stdio transport (rmcp 0.9.0)                  │
│  - Delegates to specialized tool modules                    │
└────────────────────┬────────────────────────────────────────┘
                     │
         ┌───────────┼───────────┐
         ▼           ▼           ▼
    ┌────────┐  ┌────────┐  ┌────────┐
    │ Note   │  │ Search │  │ Kiln   │
    │ Tools  │  │ Tools  │  │ Tools  │
    │ (6)    │  │ (3)    │  │ (3)    │
    └────────┘  └────────┘  └────────┘
```

## Available Tools

### Note Tools (6 tools)

#### `create_note`
Create a new note with YAML frontmatter.

**Parameters:**
- `path` (string, required): Relative path from kiln root (e.g., "projects/ml/notes.md")
- `content` (string, required): Note content (can include frontmatter)
- `frontmatter` (object, optional): YAML frontmatter as key-value pairs

**Example:**
```json
{
  "path": "ideas/new-project.md",
  "content": "# New Project\n\nThis is my new project idea.",
  "frontmatter": {
    "tags": ["project", "idea"],
    "status": "draft"
  }
}
```

#### `read_note`
Read note content with optional line range selection.

**Parameters:**
- `path` (string, required): Relative path from kiln root
- `start_line` (number, optional): First line to read (1-indexed)
- `end_line` (number, optional): Last line to read (inclusive)
- `limit` (number, optional): Maximum number of lines to return

**Example:**
```json
{
  "path": "daily/2024-01-15.md",
  "start_line": 10,
  "limit": 20
}
```

#### `read_metadata`
Get note metadata without loading full content (efficient for large notes).

**Parameters:**
- `path` (string, required): Relative path from kiln root

**Returns:**
- Frontmatter properties
- File size and modification time
- No content loaded

#### `update_note`
Update note content and/or frontmatter.

**Parameters:**
- `path` (string, required): Relative path from kiln root
- `content` (string, optional): New note content
- `frontmatter` (object, optional): Frontmatter updates (merged with existing)

**Example:**
```json
{
  "path": "projects/ml/notes.md",
  "frontmatter": {
    "status": "in-progress",
    "updated": "2024-01-15"
  }
}
```

#### `delete_note`
Remove a note from the kiln.

**Parameters:**
- `path` (string, required): Relative path from kiln root

#### `list_notes`
List notes in a directory.

**Parameters:**
- `folder` (string, optional): Directory path (defaults to root)
- `recursive` (boolean, optional): Include subdirectories (default: false)

### Search Tools (3 tools)

#### `semantic_search`
Find semantically similar notes using embeddings (block-level granularity).

**Parameters:**
- `query` (string, required): Search query
- `limit` (number, optional): Maximum results (default: 10)
- `threshold` (number, optional): Minimum similarity score (0.0-1.0)

**Returns:**
- Note paths and matched blocks
- Similarity scores
- Block-level context

#### `text_search`
Fast full-text search across all notes.

**Parameters:**
- `query` (string, required): Search query
- `limit` (number, optional): Maximum results (default: 10)
- `case_sensitive` (boolean, optional): Case-sensitive matching (default: false)
- `folder` (string, optional): Restrict to folder

#### `property_search`
Search by frontmatter properties and tags.

**Parameters:**
- `properties` (object, required): Key-value pairs to match
- `match_all` (boolean, optional): Require all properties (AND) vs any (OR)
- `folder` (string, optional): Restrict to folder

**Example:**
```json
{
  "properties": {
    "tags": "ml",
    "status": "active"
  },
  "match_all": true
}
```

### Kiln Tools (3 tools)

#### `get_kiln_info`
Get kiln path and comprehensive statistics.

**Returns:**
- Kiln root path
- Total notes count
- Storage size
- Last updated timestamp

#### `get_kiln_roots`
Get kiln root directory information.

**Returns:**
- Root paths
- Directory structure

#### `get_kiln_stats`
Get detailed kiln statistics.

**Returns:**
- Notes by directory
- Tag distribution
- Embedding status
- Storage metrics

## How It Works

### 1. Agent Connection

When you run `cru chat`, the CLI:
1. Discovers available ACP-compatible agents
2. Creates a `CrucibleClient` configured with agent path
3. Spawns the agent process

### 2. Protocol Handshake

The client performs the ACP handshake:
1. **Initialize**: Agent and client exchange capabilities
2. **NewSession**: Client sends session config **including MCP server info**:
   ```json
   {
     "cwd": "/path/to/workspace",
     "mcp_servers": [
       {
         "name": "crucible",
         "command": "/path/to/cru",
         "args": ["mcp"],
         "env": []
       }
     ]
   }
   ```

### 3. MCP Server Spawn

The agent:
1. Receives the `mcp_servers` configuration
2. Spawns `cru mcp` as a child process
3. Communicates via stdio (stdin/stdout)
4. Sends MCP protocol messages (JSON-RPC 2.0)

### 4. Tool Discovery

The agent:
1. Sends `tools/list` request to the MCP server
2. Receives list of 12 available tools with schemas
3. Caches tool definitions for the session

### 5. Tool Execution

When the agent wants to use a tool:
1. Formats request with tool name and parameters
2. Sends `tools/call` request via stdio
3. Receives structured response
4. Integrates results into conversation

## Testing the Integration

### Manual Testing

1. **Start the MCP server directly:**
   ```bash
   cru mcp
   ```
   The server will wait for JSON-RPC messages on stdin.

2. **Test with mock input:**
   ```bash
   echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' | cru mcp
   ```

3. **Use with a compatible agent:**
   ```bash
   cru chat
   # Ask the agent: "List all notes in the projects folder"
   ```

### Expected Behavior

✅ **Success indicators:**
- MCP server starts without errors
- Agent discovers all 12 tools
- Tools execute and return structured results
- Agent can read, search, and modify notes

❌ **Common issues:**
- **Path resolution**: Ensure `cru` binary is in PATH or use absolute path
- **Stdio blocking**: MCP server blocks waiting for input (normal behavior)
- **Permission errors**: Check file system permissions for kiln directory

## Configuration

### Environment Variables

The MCP server respects standard Crucible configuration:

```bash
export KILN_PATH=/path/to/your/kiln
export RUST_LOG=debug  # Enable debug logging
cru mcp
```

### Agent Configuration

Agents receive the MCP server configuration automatically via the ACP handshake. No manual configuration needed!

## Troubleshooting

### Server Not Starting

**Issue**: `cru mcp` command not found

**Solution**: Ensure Crucible CLI is built and in PATH:
```bash
cargo build --release
export PATH="$PWD/target/release:$PATH"
cru mcp
```

### Agent Not Discovering Tools

**Issue**: Agent doesn't see Crucible tools

**Checklist:**
1. ✅ Agent supports MCP protocol
2. ✅ Agent receives `mcp_servers` in NewSessionRequest
3. ✅ `cru mcp` command is accessible and executable
4. ✅ MCP server starts without errors

**Debug:**
```bash
# Check what the agent receives
RUST_LOG=debug cru chat
```

### Tool Execution Failures

**Issue**: Tools return errors when called

**Common causes:**
- Invalid file paths (not relative to kiln root)
- Missing kiln configuration
- Permission errors

**Debug:**
```bash
# Enable debug logging
RUST_LOG=debug cru mcp
```

## Implementation Details

### Code Organization

```
crates/
├── crucible-tools/
│   ├── src/
│   │   ├── mcp_server.rs       # MCP server with ServerHandler
│   │   ├── notes.rs            # NoteTools implementation
│   │   ├── search.rs           # SearchTools implementation
│   │   └── kiln.rs             # KilnTools implementation
│   └── tests/
│       └── integration_tests.rs # Tool integration tests
├── crucible-cli/
│   └── src/
│       └── commands/
│           └── mcp.rs          # `cru mcp` CLI command
└── crucible-acp/
    ├── src/
    │   └── client.rs           # ACP client with mcp_servers config
    └── tests/
        └── mcp_integration_test.rs # MCP protocol tests
```

### Dependencies

- `rmcp = "0.9.0"` - Rust MCP SDK with stdio transport
- `agent-client-protocol = "0.7.0"` - ACP 0.7.0 with MCP support
- `tokio` - Async runtime for stdio handling

### Protocol Compliance

- **MCP Version**: Compatible with MCP specification
- **Transport**: stdio (required for all agents)
- **Message Format**: JSON-RPC 2.0
- **Tool Schema**: JSON Schema for parameter validation

## Future Enhancements

- [ ] HTTP/SSE transport support for web agents
- [ ] Tool usage analytics and logging
- [ ] Rate limiting and quota management
- [ ] Custom tool registration via plugins
- [ ] Multi-kiln support with workspace switching

## References

- [Model Context Protocol Specification](https://modelcontextprotocol.io/)
- [Agent Context Protocol 0.7.0](https://agentclientprotocol.com/)
- [Crucible Architecture Guide](./ARCHITECTURE.md)
