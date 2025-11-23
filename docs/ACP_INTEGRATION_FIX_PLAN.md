# ACP Integration Fix Plan

## Executive Summary

This document outlines the comprehensive plan to fix ACP (Agent Client Protocol) integration for Crucible, ensuring compatibility with opencode, claude-acp, gemini-acp, and codex-acp agents using Test-Driven Development (TDD) with mock agents.

## Current State

### What's Working ✅

- Protocol version fixed (using version 1)
- Agent discovery finds all known agents
- Agent process spawning succeeds
- MCP server configuration is correct
- Tools initialize successfully

### What's Broken ❌

1. **Handshake Incomplete**: Process hangs after tools initialization
2. **No Agent-Specific Handling**: All agents treated identically
3. **Insufficient Testing**: No integration tests with mock agents
4. **Basic Error Handling**: Doesn't handle agent-specific quirks

## ACP Protocol Requirements

### Initialization Handshake (3 Phases)

```
Client                          Agent
  |                               |
  |-------- initialize --------->|  (Phase 1: Version & Capability Exchange)
  |<------- InitializeResponse --|
  |                               |
  |-------- authenticate ------->|  (Phase 2: Authentication - Optional)
  |<------- AuthenticateResponse-|
  |                               |
  |-------- session/new -------->|  (Phase 3: Session Establishment)
  |<------- NewSessionResponse --|
  |                               |
  |                              Ready for chat
```

### Required Capabilities (Baseline)

**Agent methods:**
- `initialize`
- `authenticate` (if required)
- `session/new`
- `session/prompt`

**Client methods:**
- `session/request_permission`

**Agent notifications:**
- `session/cancel`

**Client notifications:**
- `session/update`

### Optional Capabilities

- `loadSession` - Session resumption
- `fs.readTextFile`, `fs.writeTextFile` - File operations
- `terminal` - Terminal access
- `session/set_mode` - Mode switching

## Agent-Specific Requirements

### OpenCode (`opencode acp`)

**Command**: `opencode acp`

**Features**:
- Full ACP protocol support
- MCP servers from OpenCode config
- Project rules from `AGENTS.md`
- Custom tools and formatters/linters

**Limitations**:
- No `/undo` or `/redo` slash commands

**Expected Behavior**:
- Standard ACP handshake
- No authentication required
- Supports all baseline capabilities

### Claude-ACP (`@zed-industries/claude-code-acp`)

**Command**: `npx @zed-industries/claude-code-acp`

**Features**:
- Wrapper around Claude Code SDK
- Vendored CLI included
- Apache licensed

**Expected Behavior**:
- Standard ACP handshake
- May require authentication (API key)
- Full capability support

### Gemini-ACP (`gemini-cli`)

**Command**: `gemini-cli`

**Status**: Requires research

**Expected Behavior**:
- To be determined from testing

### Codex-ACP (`codex`)

**Command**: `codex`

**Status**: Requires research

**Expected Behavior**:
- To be determined from testing

## Implementation Plan

### Phase 1: Mock Agent Framework

**Goal**: Create a test harness for agent protocol testing

**Tasks**:
1. Design `MockAcpAgent` trait
2. Implement configurable response behavior
3. Add delay simulation
4. Add error injection
5. Support custom handshake sequences

**Deliverables**:
- `crates/crucible-acp/src/mock_agent.rs` (enhanced)
- `crates/crucible-acp/tests/support/mock_stdio_agent.rs`

**Test Coverage**:
- Mock agent responds to initialize
- Mock agent responds to session/new
- Mock agent handles errors
- Mock agent simulates delays

### Phase 2: OpenCode Integration

**Goal**: Full working integration with OpenCode agent

**Tasks**:
1. Create failing tests for OpenCode handshake
2. Fix handshake completion (wait for NewSessionResponse)
3. Add tests for MCP tool invocation
4. Test error scenarios (agent crash, timeout, invalid response)
5. Verify chat message exchange

**Test Files**:
- `crates/crucible-acp/tests/integration/opencode_tests.rs`

**Test Cases**:
```rust
#[tokio::test]
async fn test_opencode_handshake_success() {
    // RED: Create mock OpenCode agent
    // Verify:
    // 1. Initialize request sent with protocol version 1
    // 2. NewSession request sent with MCP servers
    // 3. Session ID returned
    // 4. Client ready for chat
}

#[tokio::test]
async fn test_opencode_mcp_tool_invocation() {
    // RED: Mock OpenCode with MCP tools
    // Verify:
    // 1. Tools list received from agent
    // 2. Can call MCP tool via agent
    // 3. Tool result returned
}

#[tokio::test]
async fn test_opencode_chat_exchange() {
    // RED: Mock OpenCode agent
    // Verify:
    // 1. Send chat message
    // 2. Receive response
    // 3. History tracked
}

#[tokio::test]
async fn test_opencode_error_handling() {
    // RED: Mock agent that returns errors
    // Verify:
    // 1. Protocol errors handled gracefully
    // 2. Connection errors detected
    // 3. Retry logic works
}
```

### Phase 3: Claude-ACP Integration

**Goal**: Support Claude-ACP with authentication

**Tasks**:
1. Create failing tests for Claude handshake
2. Add authentication support (API key)
3. Handle vendored CLI differences
4. Test with mock Claude agent
5. Verify full protocol compliance

**Test Files**:
- `crates/crucible-acp/tests/integration/claude_acp_tests.rs`

**Test Cases**:
```rust
#[tokio::test]
async fn test_claude_acp_handshake_with_auth() {
    // RED: Mock Claude requiring auth
    // Verify authentication phase works
}

#[tokio::test]
async fn test_claude_acp_capabilities() {
    // RED: Mock Claude with specific capabilities
    // Verify capability negotiation
}
```

### Phase 4: Gemini-ACP Integration

**Goal**: Support Gemini agent

**Tasks**:
1. Research Gemini-ACP protocol specifics
2. Create failing tests for Gemini handshake
3. Implement agent-specific quirks
4. Test with mock Gemini agent
5. Document Gemini-specific behavior

**Test Files**:
- `crates/crucible-acp/tests/integration/gemini_acp_tests.rs`

### Phase 5: Codex-ACP Integration

**Goal**: Support Codex agent

**Tasks**:
1. Research Codex-ACP protocol specifics
2. Create failing tests for Codex handshake
3. Implement agent-specific quirks
4. Test with mock Codex agent
5. Document Codex-specific behavior

**Test Files**:
- `crates/crucible-acp/tests/integration/codex_acp_tests.rs`

### Phase 6: Integration Testing

**Goal**: End-to-end testing with real agents (when available)

**Tasks**:
1. Create optional integration tests (requires agent installation)
2. Test with real OpenCode agent
3. Test with real Claude-ACP agent
4. Test with real Gemini agent (if available)
5. Test with real Codex agent (if available)

**Test Files**:
- `crates/crucible-acp/tests/integration/real_agent_tests.rs` (optional, `#[ignore]` by default)

### Phase 7: Documentation

**Goal**: Comprehensive documentation for users and developers

**Deliverables**:
1. User guide: How to use ACP chat with different agents
2. Developer guide: How to add new agent support
3. Architecture doc: ACP client design and protocol handling
4. Testing guide: How to run tests and add new test cases

**Files**:
- `docs/user/ACP_CHAT_GUIDE.md`
- `docs/dev/ACP_ARCHITECTURE.md`
- `docs/dev/ACP_TESTING_GUIDE.md`

## Test Strategy

### Mock Agent Design

```rust
pub struct MockAcpAgent {
    config: MockAgentConfig,
    responses: HashMap<String, Value>,
    state: AgentState,
}

pub struct MockAgentConfig {
    protocol_version: u16,
    requires_auth: bool,
    capabilities: Vec<String>,
    simulate_delay_ms: Option<u64>,
    error_injection: Option<ErrorConfig>,
}

impl MockAcpAgent {
    pub fn opencode_compatible() -> Self { /* ... */ }
    pub fn claude_compatible() -> Self { /* ... */ }
    pub fn gemini_compatible() -> Self { /* ... */ }
    pub fn codex_compatible() -> Self { /* ... */ }
}
```

### Test Pyramid

```
                    ╱╲
                   ╱  ╲
                  ╱ E2E╲          (Optional - Real Agents)
                 ╱──────╲
                ╱        ╲
               ╱Integration╲      (Mock Agents)
              ╱────────────╲
             ╱              ╲
            ╱   Unit Tests   ╲   (Client methods, parsing, etc.)
           ╱──────────────────╲
```

**Unit Tests** (Fast, many):
- Protocol version serialization
- Message parsing
- Client state management
- Error handling

**Integration Tests** (Medium speed, moderate):
- Full handshake with mock agents
- Agent-specific behavior
- Error scenarios
- MCP tool invocation

**E2E Tests** (Slow, few):
- Real agent integration
- Manual testing scenarios
- Performance benchmarking

## Success Criteria

### Phase Completion Checklist

**Phase 1 - Mock Framework**:
- [ ] `MockAcpAgent` trait defined
- [ ] Mock stdio transport working
- [ ] Configurable responses
- [ ] Error injection works
- [ ] All mock agent tests pass

**Phase 2 - OpenCode**:
- [ ] Handshake completes successfully
- [ ] Session ID returned
- [ ] MCP tools accessible
- [ ] Chat messages exchange
- [ ] Error scenarios handled
- [ ] All OpenCode tests pass

**Phase 3 - Claude-ACP**:
- [ ] Authentication working
- [ ] Handshake completes
- [ ] All capabilities negotiated
- [ ] All Claude tests pass

**Phase 4 - Gemini**:
- [ ] Gemini-specific behavior documented
- [ ] Handshake completes
- [ ] All Gemini tests pass

**Phase 5 - Codex**:
- [ ] Codex-specific behavior documented
- [ ] Handshake completes
- [ ] All Codex tests pass

**Phase 6 - Integration**:
- [ ] At least one real agent tested
- [ ] E2E workflow verified
- [ ] Performance acceptable

**Phase 7 - Documentation**:
- [ ] User guide complete
- [ ] Developer guide complete
- [ ] Architecture documented
- [ ] Testing guide complete

### Overall Success Metrics

1. **All four agents work**: opencode, claude-acp, gemini, codex
2. **Test coverage > 80%**: Comprehensive test suite
3. **No hanging**: All handshakes complete
4. **Clear error messages**: Users understand failures
5. **Documentation complete**: Users and devs can contribute

## Timeline Estimate

- **Phase 1**: 1 day (Mock framework)
- **Phase 2**: 2 days (OpenCode - most critical)
- **Phase 3**: 1 day (Claude-ACP)
- **Phase 4**: 1 day (Gemini)
- **Phase 5**: 1 day (Codex)
- **Phase 6**: 1 day (Integration testing)
- **Phase 7**: 1 day (Documentation)

**Total**: ~8 days of focused work

## Next Steps

1. **Start with Phase 1**: Create robust mock agent framework
2. **Fix OpenCode first**: It's the most commonly used agent
3. **Test incrementally**: Run tests after each change
4. **Document as you go**: Don't wait until the end

## Open Questions

1. **Gemini protocol specifics**: Need to research or test with real agent
2. **Codex protocol specifics**: Need to research or test with real agent
3. **Authentication flow**: Different agents may have different auth requirements
4. **Performance**: What's acceptable latency for handshake and chat?
5. **Retry strategy**: How many retries? What backoff?

## Resources

- [ACP Protocol Overview](https://agentclientprotocol.com/protocol/overview)
- [OpenCode ACP Documentation](https://opencode.ai/docs/acp/)
- [Claude-ACP GitHub](https://github.com/zed-industries/claude-code-acp)
- [Agent Client Protocol Spec](https://github.com/agentclientprotocol/agent-client-protocol)

---

**Last Updated**: 2025-11-23
**Status**: Planning Complete - Ready to Begin Implementation
