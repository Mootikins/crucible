# ACP Agent Testing Plan

> Comprehensive plan for testing Crucible's MCP server integration with real ACP-compatible agents

## Testing Objectives

1. ✅ **Protocol Compliance**: Verify ACP 0.7.0 handshake works correctly
2. ✅ **MCP Discovery**: Confirm agents receive and parse `mcp_servers` configuration
3. ✅ **Tool Availability**: Ensure all 12 tools are discoverable by agents
4. ✅ **Tool Execution**: Validate each tool executes correctly and returns expected results
5. ✅ **Error Handling**: Test graceful degradation and error messages
6. ✅ **Performance**: Measure tool execution latency and resource usage

## Test Matrix

### Supported Agents

| Agent | Version | Status | Notes |
|-------|---------|--------|-------|
| Claude Code | Latest | 🟡 Pending | Primary target agent |
| Gemini CLI | Latest | 🟡 Pending | Secondary target |
| Custom Agent | - | 🟡 Pending | MockAgent for controlled testing |

### Test Environments

| Environment | Purpose | Setup |
|-------------|---------|-------|
| Unit Tests | Individual tool verification | `cargo test --package crucible-tools` |
| Integration Tests | MCP protocol compliance | `cargo test --package crucible-acp` |
| Manual Testing | Real agent interaction | `cru chat` with live agent |
| CI/CD | Automated regression | GitHub Actions (future) |

## Test Phases

### Phase 1: Protocol Verification ✅ COMPLETE

**Goal**: Verify MCP server configuration is properly sent during ACP handshake

**Tests**:
- ✅ `test_mcp_server_configuration_in_handshake` - Structure validation
- ✅ `test_connect_with_handshake_includes_mcp_servers` - Path resolution
- ✅ `test_mcp_server_with_env_variables` - Environment variable support
- ✅ `test_multiple_mcp_servers` - Multi-server protocol support
- ✅ `test_mcp_server_schema_compliance` - JSON schema validation

**Status**: All 5 tests passing

**Evidence**:
```bash
$ cargo test --package crucible-acp --test mcp_integration_test
running 5 tests
test test_mcp_server_configuration_in_handshake ... ok
test test_connect_with_handshake_includes_mcp_servers ... ok
test test_mcp_server_with_env_variables ... ok
test test_multiple_mcp_servers ... ok
test test_mcp_server_schema_compliance ... ok
```

### Phase 2: Tool Discovery Testing 🟡 IN PROGRESS

**Goal**: Verify agents can discover all 12 tools via MCP protocol

#### Test 2.1: MCP Server Startup

**Procedure**:
1. Start MCP server manually:
   ```bash
   cru mcp
   ```
2. Verify server is listening on stdin
3. Send initialize request:
   ```json
   {
     "jsonrpc": "2.0",
     "id": 1,
     "method": "initialize",
     "params": {
       "protocolVersion": "2024-11-05",
       "capabilities": {},
       "clientInfo": {
         "name": "test-client",
         "version": "1.0.0"
       }
     }
   }
   ```
4. Expect server info response with 12 tools capability

**Expected Result**:
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "protocolVersion": "2024-11-05",
    "capabilities": {
      "tools": {
        "listChanged": null
      }
    },
    "serverInfo": {
      "name": "crucible-mcp-server",
      "version": "0.1.0",
      "title": "Crucible MCP Server"
    },
    "instructions": "Crucible MCP server exposing 12 tools for knowledge management..."
  }
}
```

**Success Criteria**:
- ✅ Server responds without errors
- ✅ `capabilities.tools` is present
- ✅ `serverInfo.name` is "crucible-mcp-server"

#### Test 2.2: Tool Listing

**Procedure**:
1. After initialization, send tools/list request:
   ```json
   {
     "jsonrpc": "2.0",
     "id": 2,
     "method": "tools/list",
     "params": {}
   }
   ```
2. Parse response and count tools

**Expected Result**:
```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "result": {
    "tools": [
      {
        "name": "create_note",
        "description": "Create a new note with YAML frontmatter",
        "inputSchema": { ... }
      },
      // ... 11 more tools
    ]
  }
}
```

**Success Criteria**:
- ✅ Exactly 12 tools returned
- ✅ Each tool has `name`, `description`, `inputSchema`
- ✅ Tool names match expected: `create_note`, `read_note`, etc.

#### Test 2.3: Tool Schema Validation

**Procedure**:
For each of the 12 tools, validate:
1. `inputSchema` is valid JSON Schema
2. Required parameters are marked
3. Parameter types are correct
4. Descriptions are present

**Tools to validate**:
- [ ] create_note
- [ ] read_note
- [ ] read_metadata
- [ ] update_note
- [ ] delete_note
- [ ] list_notes
- [ ] semantic_search
- [ ] text_search
- [ ] property_search
- [ ] get_kiln_info
- [ ] get_kiln_roots
- [ ] get_kiln_stats

### Phase 3: Tool Execution Testing 🟡 PENDING

**Goal**: Verify each tool executes correctly with valid inputs

#### Test 3.1: Note Tool Execution

**Test Cases**:

##### 3.1.1: create_note
```json
{
  "jsonrpc": "2.0",
  "id": 3,
  "method": "tools/call",
  "params": {
    "name": "create_note",
    "arguments": {
      "path": "test/sample.md",
      "content": "# Test Note\n\nThis is a test.",
      "frontmatter": {
        "tags": ["test"],
        "created": "2024-01-15"
      }
    }
  }
}
```

**Expected**:
- ✅ Note created at `{kiln}/test/sample.md`
- ✅ Frontmatter correctly serialized
- ✅ Response includes success status

##### 3.1.2: read_note
```json
{
  "jsonrpc": "2.0",
  "id": 4,
  "method": "tools/call",
  "params": {
    "name": "read_note",
    "arguments": {
      "path": "test/sample.md"
    }
  }
}
```

**Expected**:
- ✅ Returns note content
- ✅ Includes frontmatter
- ✅ Correct file path

##### 3.1.3: read_metadata
```json
{
  "jsonrpc": "2.0",
  "id": 5,
  "method": "tools/call",
  "params": {
    "name": "read_metadata",
    "arguments": {
      "path": "test/sample.md"
    }
  }
}
```

**Expected**:
- ✅ Returns frontmatter only
- ✅ No content loaded
- ✅ Includes file stats (size, modified time)

##### 3.1.4: update_note
```json
{
  "jsonrpc": "2.0",
  "id": 6,
  "method": "tools/call",
  "params": {
    "name": "update_note",
    "arguments": {
      "path": "test/sample.md",
      "frontmatter": {
        "status": "updated"
      }
    }
  }
}
```

**Expected**:
- ✅ Frontmatter updated
- ✅ Content preserved
- ✅ Original tags preserved

##### 3.1.5: list_notes
```json
{
  "jsonrpc": "2.0",
  "id": 7,
  "method": "tools/call",
  "params": {
    "name": "list_notes",
    "arguments": {
      "folder": "test",
      "recursive": false
    }
  }
}
```

**Expected**:
- ✅ Returns note paths in test folder
- ✅ Excludes subdirectories (recursive=false)
- ✅ Includes sample.md

##### 3.1.6: delete_note
```json
{
  "jsonrpc": "2.0",
  "id": 8,
  "method": "tools/call",
  "params": {
    "name": "delete_note",
    "arguments": {
      "path": "test/sample.md"
    }
  }
}
```

**Expected**:
- ✅ File removed from disk
- ✅ Success response
- ✅ Subsequent read_note returns error

#### Test 3.2: Search Tool Execution

##### 3.2.1: text_search
```json
{
  "jsonrpc": "2.0",
  "id": 9,
  "method": "tools/call",
  "params": {
    "name": "text_search",
    "arguments": {
      "query": "test",
      "limit": 10
    }
  }
}
```

**Expected**:
- ✅ Returns matching notes
- ✅ Results sorted by relevance
- ✅ Respects limit parameter

##### 3.2.2: semantic_search
```json
{
  "jsonrpc": "2.0",
  "id": 10,
  "method": "tools/call",
  "params": {
    "name": "semantic_search",
    "arguments": {
      "query": "machine learning concepts",
      "limit": 5
    }
  }
}
```

**Expected**:
- ✅ Returns semantically similar blocks
- ✅ Includes similarity scores
- ✅ Block-level granularity

##### 3.2.3: property_search
```json
{
  "jsonrpc": "2.0",
  "id": 11,
  "method": "tools/call",
  "params": {
    "name": "property_search",
    "arguments": {
      "properties": {
        "tags": "test"
      },
      "match_all": false
    }
  }
}
```

**Expected**:
- ✅ Returns notes with matching tags
- ✅ Respects match_all logic
- ✅ Handles array properties (tags)

#### Test 3.3: Kiln Tool Execution

##### 3.3.1: get_kiln_info
```json
{
  "jsonrpc": "2.0",
  "id": 12,
  "method": "tools/call",
  "params": {
    "name": "get_kiln_info",
    "arguments": {}
  }
}
```

**Expected**:
- ✅ Returns kiln path
- ✅ Includes note count
- ✅ Shows storage size

##### 3.3.2: get_kiln_roots
```json
{
  "jsonrpc": "2.0",
  "id": 13,
  "method": "tools/call",
  "params": {
    "name": "get_kiln_roots",
    "arguments": {}
  }
}
```

**Expected**:
- ✅ Returns root paths
- ✅ Shows directory structure

##### 3.3.3: get_kiln_stats
```json
{
  "jsonrpc": "2.0",
  "id": 14,
  "method": "tools/call",
  "params": {
    "name": "get_kiln_stats",
    "arguments": {}
  }
}
```

**Expected**:
- ✅ Returns detailed statistics
- ✅ Notes by directory
- ✅ Tag distribution

### Phase 4: End-to-End Agent Testing 🟡 PENDING

**Goal**: Test with real ACP-compatible agents in production-like scenarios

#### Test 4.1: Claude Code Integration

**Setup**:
1. Install Claude Code (if available)
2. Configure as ACP agent
3. Start Crucible chat:
   ```bash
   cru chat
   ```

**Test Scenarios**:

##### Scenario A: Note Creation
**Prompt**: "Create a new note called 'daily/2024-01-15.md' with a heading 'Daily Log' and tags 'journal' and 'daily'"

**Expected Agent Behavior**:
1. Uses `create_note` tool
2. Formats markdown correctly
3. Sets frontmatter with tags
4. Confirms creation

**Success Criteria**:
- ✅ Note created at correct path
- ✅ Content matches request
- ✅ Tags are in frontmatter array

##### Scenario B: Search and Summarize
**Prompt**: "Find all notes about machine learning and summarize the key concepts"

**Expected Agent Behavior**:
1. Uses `semantic_search` or `text_search`
2. Reads matching notes with `read_note`
3. Synthesizes summary
4. Cites source notes

**Success Criteria**:
- ✅ Relevant notes found
- ✅ Summary is coherent
- ✅ Source notes cited

##### Scenario C: Property Query
**Prompt**: "List all notes tagged with 'project' that have status 'active'"

**Expected Agent Behavior**:
1. Uses `property_search` tool
2. Filters by multiple properties
3. Returns formatted list

**Success Criteria**:
- ✅ Correct notes returned
- ✅ All match criteria
- ✅ Well-formatted response

##### Scenario D: Update and Verify
**Prompt**: "Update the note 'daily/2024-01-15.md' to add a 'reviewed' tag, then show me the updated metadata"

**Expected Agent Behavior**:
1. Uses `update_note` to add tag
2. Uses `read_metadata` to verify
3. Shows updated frontmatter

**Success Criteria**:
- ✅ Tag added successfully
- ✅ Other metadata preserved
- ✅ Verification shows new tag

##### Scenario E: Kiln Overview
**Prompt**: "Give me an overview of my knowledge base - how many notes, what categories, etc."

**Expected Agent Behavior**:
1. Uses `get_kiln_stats` tool
2. Interprets statistics
3. Presents user-friendly summary

**Success Criteria**:
- ✅ Accurate statistics
- ✅ Helpful categorization
- ✅ Clear presentation

#### Test 4.2: Error Handling

**Test Cases**:

##### E.1: Invalid Path
**Prompt**: "Read the note at '../../../etc/passwd'"

**Expected**:
- ✅ Tool returns error
- ✅ Agent handles gracefully
- ✅ Security boundary maintained

##### E.2: Non-existent Note
**Prompt**: "Read the note 'does-not-exist.md'"

**Expected**:
- ✅ Tool returns not found error
- ✅ Agent explains to user
- ✅ Suggests alternatives

##### E.3: Invalid Parameters
**Prompt**: "Create a note without a path"

**Expected**:
- ✅ Tool validation fails
- ✅ Clear error message
- ✅ Agent asks for correction

### Phase 5: Performance Testing 🟡 PENDING

**Goal**: Measure tool execution performance and resource usage

#### Metrics to Collect

| Metric | Target | Measurement |
|--------|--------|-------------|
| Server startup time | < 1s | Time from spawn to ready |
| Tool discovery latency | < 100ms | `tools/list` response time |
| Simple read latency | < 50ms | `read_note` for small file |
| Search latency | < 500ms | `text_search` across 1000 notes |
| Semantic search latency | < 2s | `semantic_search` with embeddings |
| Memory usage (idle) | < 50MB | Server process RSS |
| Memory usage (active) | < 200MB | During search operations |

#### Load Testing

**Test**: Concurrent tool calls
- 10 simultaneous `read_note` calls
- 5 simultaneous `semantic_search` calls
- 20 rapid-fire `list_notes` calls

**Expected**:
- ✅ No crashes or hangs
- ✅ Reasonable response times
- ✅ No resource leaks

### Phase 6: Regression Testing 🟡 PENDING

**Goal**: Prevent regressions in future changes

#### Automated Test Suite

**Unit Tests** (43 tests currently passing):
```bash
cargo test --package crucible-tools --lib
```

**Integration Tests** (5 MCP tests currently passing):
```bash
cargo test --package crucible-acp --test mcp_integration_test
```

**Future CI/CD**:
- Run on every PR
- Test against multiple agent versions
- Performance regression detection

## Test Execution Checklist

### Pre-Testing
- [ ] Build Crucible in release mode
- [ ] Set up test kiln with sample notes
- [ ] Configure environment variables
- [ ] Install target agents

### During Testing
- [ ] Document all test results
- [ ] Capture error messages
- [ ] Record performance metrics
- [ ] Take screenshots/logs

### Post-Testing
- [ ] Update test status in this document
- [ ] File issues for failures
- [ ] Update documentation
- [ ] Share results with team

## Known Issues & Limitations

### Current Limitations
- ⚠️ MCP server only supports stdio transport (HTTP/SSE not yet implemented)
- ⚠️ No rate limiting on tool calls
- ⚠️ Limited error context in some tool responses
- ⚠️ No tool usage analytics

### Planned Improvements
- [ ] Add HTTP/SSE transport support
- [ ] Implement rate limiting
- [ ] Enhanced error messages with suggestions
- [ ] Tool usage logging and analytics
- [ ] Performance optimizations for large kilns

## Success Metrics

### Definition of Done

Phase 2 Complete:
- ✅ All 12 tools discoverable
- ✅ Tool schemas validate
- ✅ Server responds to protocol messages

Phase 3 Complete:
- ✅ All 12 tools execute successfully
- ✅ Error handling works correctly
- ✅ Results match expected format

Phase 4 Complete:
- ✅ Works with at least 1 real agent (Claude Code or Gemini CLI)
- ✅ End-to-end scenarios pass
- ✅ User-facing errors are clear

Phase 5 Complete:
- ✅ Performance targets met
- ✅ Resource usage acceptable
- ✅ No memory leaks

## Next Steps

1. **Immediate** (Phase 2):
   - [ ] Test MCP server startup manually
   - [ ] Verify tool discovery via JSON-RPC
   - [ ] Document tool schemas

2. **Short-term** (Phase 3):
   - [ ] Create test script for tool execution
   - [ ] Test each of 12 tools individually
   - [ ] Validate error cases

3. **Medium-term** (Phase 4):
   - [ ] Set up Claude Code integration
   - [ ] Run end-to-end scenarios
   - [ ] Document agent behavior

4. **Long-term** (Phase 5-6):
   - [ ] Performance benchmarking
   - [ ] CI/CD automation
   - [ ] Regression test suite

## References

- [MCP Integration Guide](./MCP_INTEGRATION.md)
- [Model Context Protocol Spec](https://modelcontextprotocol.io/)
- [Agent Context Protocol 0.7.0](https://agentclientprotocol.com/)
- [Test Results Log](./test_results/) (future)
