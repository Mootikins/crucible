# ACP Chat Integration Plan

> Complete plan to verify and validate MCP server integration with ACP chat

## Overview

This document outlines the step-by-step plan to verify that the ACP chat command (`cru chat`) successfully integrates with the MCP server (`cru mcp`) to expose Crucible's 12 knowledge management tools to AI agents.

## Current Status

### ✅ Completed Components

1. **MCP Server Implementation** (`crates/crucible-tools/src/mcp_server.rs`)
   - ServerHandler trait with #[tool_handler] macro
   - 12 tools: NoteTools (6), SearchTools (3), KilnTools (3)
   - Single-router delegation pattern
   - Status: ✅ COMPLETE

2. **CLI MCP Command** (`crates/crucible-cli/src/commands/mcp.rs`)
   - `cru mcp` command starts MCP server via stdio
   - File-based logging to `~/.crucible/mcp.log`
   - Status: ✅ COMPLETE

3. **ACP Client MCP Configuration** (`crates/crucible-acp/src/client.rs:372-392`)
   - NewSessionRequest populates `mcp_servers` field
   - McpServer::Stdio with `cru mcp` command
   - Automatic path resolution for `cru` binary
   - Status: ✅ COMPLETE

4. **File-Based Logging** (`crates/crucible-cli/src/main.rs:30-75`)
   - Detects stdio-based commands (Mcp, Chat)
   - Routes logs to file instead of stderr
   - Prevents JSON-RPC corruption
   - Status: ✅ COMPLETE

5. **Test MCP Client** (`crates/crucible-cli/examples/test_mcp_server.rs`)
   - Spawns `cru mcp` and verifies tool discovery
   - Validates all 12 tools are discovered
   - Status: ✅ COMPLETE

6. **Integration Tests** (`crates/crucible-acp/tests/mcp_integration_test.rs`)
   - 5 tests covering protocol structure
   - Validates serialization and schema compliance
   - Status: ✅ COMPLETE (5/5 passing)

7. **Documentation**
   - MCP_INTEGRATION.md: Complete user guide
   - ACP_TESTING_PLAN.md: 6-phase testing strategy
   - Status: ✅ COMPLETE

### 🔧 Needs Testing/Validation

1. **Standalone MCP Server** - Manual verification needed
2. **ACP Chat Integration** - E2E testing with real agent needed
3. **Tool Execution** - Verify tools work through agent
4. **Error Handling** - Test failure scenarios
5. **Binary Path Resolution** - Verify `cru` is found correctly

## Testing Plan

### Phase 1: Standalone MCP Server Testing

**Objective:** Verify MCP server works independently

#### 1.1 Build Verification

```bash
# Build the project
cargo build --release

# Verify binary exists
ls -lh target/release/cru

# Add to PATH for testing
export PATH="$PWD/target/release:$PATH"
```

#### 1.2 Manual MCP Server Test

```bash
# Start MCP server with debug logging
RUST_LOG=debug cru mcp
```

Expected: Server starts and waits for stdin (will appear to hang - this is correct)

#### 1.3 Test Client Validation

```bash
# Run the test client
cargo run --release --example test_mcp_server

# Or with custom binary path
CRUCIBLE_BIN=/path/to/cru cargo run --release --example test_mcp_server
```

**Success Criteria:**
- ✅ Server spawns without errors
- ✅ Client connects via stdio
- ✅ All 12 tools discovered
- ✅ Tool names match expectations

**Expected Output:**
```
🚀 Testing Crucible MCP Server

📝 Spawning server and connecting...
    Using binary: ./target/release/cru
    Service connected successfully
✅ Connected to server: crucible-mcp-server

📋 Listing tools...
✅ Found 12 tools:

  • create_note
    Create a new note with optional frontmatter
  • read_note
    Read note content with optional line range
  ...

✅ SUCCESS: All 12 tools discovered!
```

### Phase 2: ACP Chat Command Testing

**Objective:** Verify chat command discovers agents and starts sessions

#### 2.1 Agent Discovery Test

```bash
# Test agent discovery
RUST_LOG=debug cru chat --help
```

Check logs in `~/.crucible/mcp.log` for agent discovery attempts.

#### 2.2 Chat Session Start (Mock Mode)

```bash
# Try to start chat (will fail if no agent available)
RUST_LOG=debug cru chat

# Check logs for handshake details
cat ~/.crucible/mcp.log | grep -A10 "mcp_servers"
```

**Success Criteria:**
- ✅ Agent discovery runs without panics
- ✅ Graceful error if no agent available
- ✅ Logs show correct error messages

### Phase 3: ACP Handshake Verification

**Objective:** Verify `mcp_servers` field is populated in NewSessionRequest

#### 3.1 Protocol Capture Test

Create a test that captures the handshake:

```rust
// Test file: crates/crucible-acp/tests/handshake_verification.rs
#[tokio::test]
async fn test_handshake_includes_mcp_servers() {
    use crucible_acp::client::{CrucibleAcpClient, ClientConfig};

    // This test would spawn the client and capture the NewSessionRequest
    // Verify mcp_servers field is populated correctly
}
```

**Success Criteria:**
- ✅ NewSessionRequest contains `mcp_servers` array
- ✅ Array contains one McpServer::Stdio entry
- ✅ Entry has correct command path and args
- ✅ JSON serialization is valid

#### 3.2 Path Resolution Verification

```bash
# Test that the binary path resolution works
cargo test --package crucible-acp test_connect_with_handshake_includes_mcp_servers
```

**Success Criteria:**
- ✅ Test passes
- ✅ Path is absolute
- ✅ Path points to correct `cru` binary

### Phase 4: End-to-End Agent Testing

**Objective:** Test with a real ACP-compatible agent

#### 4.1 Prerequisites

1. **Install an ACP-compatible agent** (e.g., claude-code, gemini-cli)
   ```bash
   # Example: Install claude-code
   # Follow installation instructions from agent documentation
   ```

2. **Verify agent supports ACP 0.7.0**
   ```bash
   # Check agent version
   <agent-command> --version
   ```

3. **Verify agent supports MCP protocol**
   - Check agent documentation for MCP support
   - Verify it can spawn MCP servers via stdio

#### 4.2 Interactive Chat Test

```bash
# Start chat with agent
RUST_LOG=debug cru chat

# In the chat, test tool usage:
# Ask: "List all notes in the kiln"
# Ask: "Create a new note called test.md with content 'Hello World'"
# Ask: "Search for notes about testing"
```

**Success Criteria:**
- ✅ Agent spawns successfully
- ✅ MCP server is spawned by agent
- ✅ Agent discovers all 12 tools
- ✅ Agent can execute tools and get results
- ✅ Tool results are displayed in chat

#### 4.3 Verify MCP Server Logs

```bash
# Check MCP server logs
cat ~/.crucible/mcp.log

# Look for:
# - Server initialization
# - Tool registration
# - Tool execution requests
# - Tool responses
```

### Phase 5: Issue Identification and Fixes

Based on testing results, address any issues found:

#### Common Issues and Solutions

**Issue 1: Binary Not Found**
```
Error: Failed to spawn MCP server: No such file or directory
```

**Solution:**
- Verify `cru` is in PATH
- Update path resolution in `client.rs` to use absolute path
- Consider using `which cru` to find binary

**Fix:**
```rust
// In crates/crucible-acp/src/client.rs
let command = std::env::current_exe()
    .ok()
    .and_then(|exe| exe.parent().map(|p| p.join("cru")))
    .or_else(|| which::which("cru").ok())  // Fallback to PATH search
    .unwrap_or_else(|| PathBuf::from("cru"));
```

**Issue 2: stdio Interference**
```
Error: Invalid JSON-RPC message
```

**Solution:**
- Verify file-based logging is working
- Check no println!/eprintln! in MCP server code
- Verify all logs go to file

**Issue 3: Agent Doesn't Support MCP**
```
Error: Agent does not understand mcp_servers field
```

**Solution:**
- Verify agent version supports ACP 0.7.0
- Check agent documentation for MCP support
- Try with a different agent

**Issue 4: Tools Not Discovered**
```
Agent response: I don't have access to any Crucible tools
```

**Solution:**
- Check MCP server logs for tool registration
- Verify `tools/list` request succeeded
- Check agent logs for MCP errors

### Phase 6: Validation and Documentation

#### 6.1 Create Working Examples

Document successful test cases:

```markdown
# Working Example: Claude Code Integration

## Setup
1. Install claude-code: `npm install -g @anthropic-ai/claude-code`
2. Build Crucible: `cargo build --release`
3. Add to PATH: `export PATH="$PWD/target/release:$PATH"`

## Usage
```bash
# Start chat
cru chat

# Claude Code will automatically:
# 1. Receive MCP server config in handshake
# 2. Spawn `cru mcp` as child process
# 3. Discover 12 Crucible tools
# 4. Make tools available in conversation

# Example conversation:
You: List all my notes
Claude: [Uses list_notes tool and displays results]

You: Create a note about today's meeting
Claude: [Uses create_note tool to create the note]
```
```

#### 6.2 Update Documentation

Add to `docs/MCP_INTEGRATION.md`:
- Working agent configurations
- Troubleshooting guide with real issues
- Performance tips
- Best practices

#### 6.3 Create Quickstart Guide

Create `docs/QUICKSTART_ACP_CHAT.md`:
- 5-minute setup guide
- Prerequisites
- Installation
- First chat session
- Common commands

## Implementation Checklist

### Phase 1: Pre-Testing Setup
- [ ] Build project: `cargo build --release`
- [ ] Verify binary: `ls target/release/cru`
- [ ] Run unit tests: `cargo test`
- [ ] Run integration tests: `cargo test --test mcp_integration_test`

### Phase 2: Standalone Testing
- [ ] Start MCP server manually
- [ ] Run test client
- [ ] Verify all 12 tools discovered
- [ ] Check logs in `~/.crucible/mcp.log`

### Phase 3: Protocol Testing
- [ ] Test agent discovery
- [ ] Verify handshake structure
- [ ] Check path resolution
- [ ] Validate JSON serialization

### Phase 4: Agent Integration
- [ ] Install ACP-compatible agent
- [ ] Start chat session
- [ ] Test tool discovery
- [ ] Test tool execution
- [ ] Verify results

### Phase 5: Issue Resolution
- [ ] Document any issues found
- [ ] Implement fixes
- [ ] Re-test after fixes
- [ ] Verify all tests pass

### Phase 6: Documentation
- [ ] Create working examples
- [ ] Update troubleshooting guide
- [ ] Write quickstart guide
- [ ] Add performance tips

## Success Metrics

### Must Have (P0)
- ✅ `cru mcp` command starts successfully
- ✅ Test client discovers all 12 tools
- ✅ Integration tests pass (5/5)
- ✅ File-based logging works
- ✅ No stderr output from MCP server

### Should Have (P1)
- ⏳ `cru chat` connects to agent
- ⏳ Agent spawns MCP server
- ⏳ Agent discovers tools
- ⏳ Tools execute successfully
- ⏳ Results returned to chat

### Nice to Have (P2)
- ⏳ Multiple agents tested
- ⏳ Performance benchmarks
- ⏳ Load testing
- ⏳ Error recovery testing

## Next Steps

1. **Immediate (Today)**
   - [ ] Run Phase 1 tests (standalone MCP server)
   - [ ] Run Phase 2 tests (chat command)
   - [ ] Document results

2. **Short-term (This Week)**
   - [ ] Install ACP-compatible agent
   - [ ] Run Phase 4 tests (E2E integration)
   - [ ] Fix any critical issues

3. **Medium-term (Next Week)**
   - [ ] Complete all phases
   - [ ] Write comprehensive documentation
   - [ ] Create examples

4. **Long-term**
   - [ ] Test with multiple agents
   - [ ] Performance optimization
   - [ ] Advanced features

## Resources

- **MCP Specification:** https://modelcontextprotocol.io/
- **ACP 0.7.0 Spec:** https://agentclientprotocol.com/
- **Crucible Docs:**
  - `docs/MCP_INTEGRATION.md` - Architecture and tool reference
  - `docs/ACP_TESTING_PLAN.md` - Comprehensive testing strategy
  - `AGENTS.md` - AI agent development guide

## Contact

For issues or questions:
- File an issue on GitHub
- Check existing documentation in `docs/`
- Review test files for examples
