# TDD Plan for ACP Chat Integration

## Current Status

**Problem**: Chat command hangs when run with real agents, but all tests pass. This indicates our tests are not representative of actual behavior.

**Evidence**:
- ✅ 193 tests passing in crucible-acp
- ❌ `./target/release/cru chat --agent opencode "test"` hangs indefinitely
- 🤔 No test exercises the actual streaming response path we just implemented

## Test Pyramid Analysis

### What We're Testing (Current)
1. Mock agent framework (11 tests) - ✅ Works
2. OpenCode integration with mock (7 tests) - ✅ Works
3. ChatSession unit tests - ⚠️ Don't test streaming
4. Client handshake tests - ⚠️ Don't test prompt/response

### What We're NOT Testing (Gaps)
1. ❌ **Streaming response handling** - Critical path we just implemented
2. ❌ **session/update notification parsing** - Core to ACP protocol
3. ❌ **AgentMessageChunk accumulation** - How we get content
4. ❌ **Real stdio communication loop** - Where the hang occurs
5. ❌ **ChatSession → AcpClient integration** - The actual flow CLI uses

## Proposed TDD Approach

### Phase 1: Unit Tests for Streaming (RED → GREEN → REFACTOR)

#### Test 1: Parse session/update notification
```rust
#[test]
fn test_parse_session_update_notification() {
    let json = r#"{
        "jsonrpc": "2.0",
        "method": "session/update",
        "params": {
            "type": "agentMessageChunk",
            "content": {
                "type": "text",
                "text": "Hello"
            }
        }
    }"#;

    // Should parse without id field (it's a notification)
    // Should extract "Hello" from AgentMessageChunk
}
```

#### Test 2: Accumulate content from multiple chunks
```rust
#[test]
fn test_accumulate_streaming_chunks() {
    // Send multiple session/update notifications
    // Verify content accumulates correctly
    // Final response should have all chunks concatenated
}
```

#### Test 3: Handle final PromptResponse
```rust
#[test]
fn test_prompt_response_stop_reason() {
    let json = r#"{
        "jsonrpc": "2.0",
        "id": 1,
        "result": {
            "stopReason": "end_turn"
        }
    }"#;

    // Should parse as final response
    // Should trigger end of streaming loop
}
```

### Phase 2: Integration Tests with Mock Agent (RED → GREEN)

#### Test 4: End-to-end streaming chat flow
```rust
#[tokio::test]
async fn test_streaming_chat_session() {
    // 1. Spawn mock agent that sends streaming notifications
    // 2. Create ChatSession connected to mock
    // 3. Send a message
    // 4. Verify:
    //    - Multiple session/update notifications received
    //    - Content accumulated correctly
    //    - Final PromptResponse parsed
    //    - Chat returns complete response
}
```

**Mock Agent Behavior**:
```
Client sends: {"jsonrpc":"2.0","id":1,"method":"session/prompt",...}

Mock responds:
1. {"jsonrpc":"2.0","method":"session/update","params":{"type":"agentMessageChunk","content":{"type":"text","text":"The"}}}
2. {"jsonrpc":"2.0","method":"session/update","params":{"type":"agentMessageChunk","content":{"type":"text","text":" answer"}}}
3. {"jsonrpc":"2.0","method":"session/update","params":{"type":"agentMessageChunk","content":{"type":"text","text":" is 42"}}}
4. {"jsonrpc":"2.0","id":1,"result":{"stopReason":"end_turn"}}

Expected: ChatSession.send_message() returns "The answer is 42"
```

#### Test 5: CLI integration test
```rust
#[tokio::test]
async fn test_cli_chat_command() {
    // Test the actual CLI flow:
    // discover_agent → spawn → ChatSession → send_message
    // This exercises the EXACT path the user command takes
}
```

### Phase 3: Contract Tests (Validate Protocol Compliance)

#### Test 6: Validate against ACP spec examples
- Use official ACP test fixtures
- Verify we handle all stop reasons
- Test error cases (malformed JSON, missing fields)

### Phase 4: Real Agent Integration

#### Test 7: OpenCode smoke test
```rust
#[tokio::test]
#[ignore] // Run manually
async fn test_real_opencode_chat() {
    // Actual opencode binary required
    // Verify end-to-end works with real agent
}
```

## Test Implementation Order

### Week 1: Fix Immediate Issue
1. ✅ Commit current streaming implementation
2. **Write Test 4** (streaming chat session) - Should FAIL
3. Debug why it fails
4. Fix implementation
5. Verify Test 4 passes
6. Re-run CLI command - should work

### Week 2: Build Comprehensive Suite
1. Write Tests 1-3 (unit tests)
2. Write Test 5 (CLI integration)
3. Add Test 6 (contract tests)
4. Document Test 7 (manual validation)

## Success Criteria

### Minimum Viable (MVP)
- [ ] Test 4 passes (streaming chat works)
- [ ] CLI chat command works with opencode
- [ ] No hanging or timeouts

### Complete
- [ ] All unit tests (1-3) pass
- [ ] All integration tests (4-5) pass
- [ ] Contract tests (6) validate spec compliance
- [ ] Manual test (7) succeeds with real agent
- [ ] Documentation updated with working examples

## Known Issues to Address

1. **Hanging in streaming loop** - Likely never receives final response with matching id
2. **No timeout handling** - If agent misbehaves, we wait forever
3. **Error handling** - What if notification parsing fails?
4. **SessionId type mismatch** - Had to convert between types
5. **Duplicate client types** - `CrucibleAcpClient` in two crates

## Next Steps

**Immediate (Today)**:
1. Write Test 4 (streaming chat session)
2. Run it - should FAIL
3. Add debug logging to streaming implementation
4. Fix the hang issue
5. Verify test passes
6. Try CLI command again

**This Week**:
1. Complete unit tests (1-3)
2. Add CLI integration test (5)
3. Fix any issues found
4. Document working flow

**Next Week**:
1. Add contract tests (6)
2. Refactor duplicate types
3. Clean up architecture
4. Update documentation

## Questions to Answer

1. **Why does the streaming loop hang?**
   - Not receiving final response?
   - ID mismatch?
   - Agent not sending proper format?

2. **How do we verify the agent sends proper notifications?**
   - Add packet-level logging
   - Compare against ACP spec examples
   - Use strace/dtrace to see actual JSON

3. **What's the correct error handling strategy?**
   - Timeout after N seconds?
   - Return partial content on error?
   - Retry logic?

## Resources

- ACP Spec: https://agentclientprotocol.com/protocol/prompt-turn
- agent-client-protocol crate: v0.7.0
- Our streaming implementation: `crates/crucible-acp/src/client.rs:569-645`
