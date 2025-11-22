# ACP Integration Working Document

**Branch**: `claude/start-acp-integration-01KJ6WM9VPbX1yKC7MhriCB6`
**Started**: 2025-11-22
**Status**: Planning Phase

## References

- **Spec**: `openspec/changes/add-acp-integration/specs/acp-integration/spec.md`
- **Proposal**: `openspec/changes/add-acp-integration/proposal.md`
- **Tasks**: `openspec/changes/add-acp-integration/tasks.md`
- **ACP Docs**: https://docs.rs/agent-client-protocol/0.7.0/agent_client_protocol/index.html
- **MVP Guide**: `docs/ACP-MVP.md`
- **Status Summary**: `OPENSPEC_STATUS_SUMMARY.md` (shows 35% complete)

## Current State Assessment

### ✅ What Exists
1. **Crate dependency**: `agent-client-protocol = "0.6"` in `crates/crucible-cli/Cargo.toml`
2. **Agent discovery**: `crates/crucible-cli/src/acp/agent.rs` - discovers claude-code, gemini-cli, codex
3. **Context enrichment**: `crates/crucible-cli/src/acp/context.rs` - semantic search integration
4. **Basic client shell**: `crates/crucible-cli/src/acp/client.rs` - process spawning, placeholder methods
5. **Chat command**: `crates/crucible-cli/src/commands/chat.rs` - one-shot mode works
6. **Tool system**: 95% complete with 10 MCP-compatible tools (NoteTools, SearchTools, KilnTools)
7. **Pipeline**: 90% complete, fully functional

### ❌ What's Missing (Critical Gaps)
1. **JSON-RPC protocol**: No actual agent_client_protocol::Client trait implementation
2. **Response streaming**: No real-time streaming from agent to user
3. **File operations**: No handlers for agent read_text_file/write_text_file requests
4. **Session management**: No state persistence or multi-turn context
5. **Interactive chat loop**: Only one-shot queries work
6. **Tool integration**: ACP tool calls not bridged to Crucible tools
7. **Permission system**: No enforcement of plan (read-only) vs act (write) modes

### 📊 Completion Status
- **Overall**: 35% complete
- **Foundation**: 80% (discovery, context, basic structure)
- **Protocol**: 0% (no JSON-RPC implementation)
- **Features**: 20% (one-shot works, interactive missing)

## Architecture Decisions

### Decision Log

| # | Decision | Rationale | Date | Status |
|---|----------|-----------|------|--------|
| D1 | TBD: Use agent-client-protocol 0.6 or upgrade to 0.7? | Docs reference 0.7, but we have 0.6 | 2025-11-22 | PENDING |
| D2 | TBD: Separate crucible-acp crate vs keep in crucible-cli? | Spec mentions new crate, but scaffolding in CLI | 2025-11-22 | PENDING |
| D3 | TBD: Chat-only first (read) vs full read/write? | MVP scope definition | 2025-11-22 | PENDING |
| D4 | TBD: Which parser to use for frontmatter? | serde_yaml, toml, or both? | 2025-11-22 | PENDING |
| D5 | TBD: Testing strategy - mock agent or real claude-code? | Test coverage approach | 2025-11-22 | PENDING |
| D6 | TBD: Session persistence - where to store? | .crucible/sessions/ or kiln-relative? | 2025-11-22 | PENDING |

## Open Questions

### Critical Questions for User

1. **Version**: Should we upgrade to `agent-client-protocol = "0.7.0"` (docs reference) or stick with `0.6`?

2. **Scope**: What's the MVP scope for this work?
   - Option A: Chat-only (read-only mode, no agent file writes)
   - Option B: Full implementation (both plan and act modes)
   - Option C: Incremental (chat first, then add file ops)

3. **Crate structure**: Should we:
   - Keep ACP code in `crucible-cli/src/acp/`
   - Create new `crucible-acp` crate (per spec proposal)

4. **Testing priorities**: Which agent(s) to prioritize?
   - Claude Code (Anthropic)
   - Gemini CLI (Google)
   - Mock agent for CI/CD
   - All of the above

5. **File operations**: For agent file read/write:
   - Map directly to filesystem (simple but less abstract)
   - Map through kiln operations (more abstract, respects boundaries)
   - Hybrid approach

6. **Timeline**: What's acceptable for this work?
   - Spec estimates 2 weeks for full implementation
   - MVP could be done in ~1 week
   - Should we target MVP or complete implementation?

## Implementation Plan (DRAFT - Pending Decisions)

### Phase 0: Planning ✅ (Current)
- [x] Read openspec and documentation
- [x] Understand current codebase state
- [x] Create working document
- [ ] Get clarifying answers from user
- [ ] Create detailed TDD plan

### Phase 1: Core Protocol Implementation (TBD)
*Will be detailed after decisions are made*

Key areas:
- Implement `agent_client_protocol::Client` trait
- JSON-RPC message handling
- Session initialization and negotiation
- Basic tool registration

### Phase 2: File Operations & Streaming (TBD)
*Will be detailed after decisions are made*

### Phase 3: Interactive Chat Loop (TBD)
*Will be detailed after decisions are made*

### Phase 4: Testing & QA (TBD)
*Will be detailed after decisions are made*

## Technical Notes

### ACP Protocol Key Concepts (from 0.7.0 docs)

**Core Traits**:
- `Agent`: Handles requests/responses from clients
- `Client`: Handles requests/responses from agents
- `Side`: Abstracts connection handling
- `MessageHandler`: Processes protocol messages

**Message Types**:
1. **Requests**: `ClientRequest` (client→agent), `AgentRequest` (agent→client)
2. **Responses**: `ClientResponse`, `AgentResponse`
3. **Notifications**: `ClientNotification`, `AgentNotification`

**Key Capabilities**:
- Session management: `NewSessionRequest`, `LoadSessionRequest`, `SessionModeState`
- Content exchange: `ContentBlock`, `ToolCall`, `Diff`
- Tool integration: Permission requests, status tracking, result handling
- Terminal operations: Create, manage, monitor terminal sessions
- File system: `ReadTextFileRequest`, `WriteTextFileRequest`
- Capability negotiation: `AgentCapabilities`, `ClientCapabilities`

### Existing Crucible Integration Points

**Tools** (crucible-tools):
- `NoteTools`: create_note, read_note, read_metadata, update_note, delete_note, list_notes
- `SearchTools`: semantic_search, text_search, property_search
- `KilnTools`: get_kiln_info

**Search/Context** (crucible-cli):
- `ContextEnricher::enrich()`: Semantic search → markdown formatting
- `ContextEnricher::enrich_with_reranking()`: Two-stage retrieval

**Core Facade** (crucible-cli):
- `CrucibleCoreFacade`: Unified interface to core, storage, enrichment

## Test Strategy (DRAFT)

### Test Levels

1. **Unit Tests**:
   - Message serialization/deserialization
   - Session state management
   - Tool mapping and conversion
   - Permission enforcement

2. **Integration Tests**:
   - Full protocol handshake
   - File operation round-trips
   - Context enrichment pipeline
   - Multi-turn conversations

3. **End-to-End Tests**:
   - Real agent interaction (if available)
   - Mock agent for CI/CD
   - Error recovery scenarios
   - Performance benchmarks

### QA Checkpoints

- [ ] **Checkpoint 1**: After protocol implementation - run full workspace tests
- [ ] **Checkpoint 2**: After file operations - run full workspace tests
- [ ] **Checkpoint 3**: After interactive loop - run full workspace tests
- [ ] **Checkpoint 4**: Final integration - run full workspace tests + manual QA

## Dependencies & Blockers

### Current Dependencies
- `agent-client-protocol = "0.6"` (potentially upgrade to 0.7)
- `tokio` for async runtime
- `serde_json` for message serialization
- Tool system (95% complete - ready to use)
- Context enrichment (ready to use)

### Potential Blockers
- Agent availability for testing (claude-code, gemini-cli)
- Breaking changes in agent-client-protocol 0.7.0 (if we upgrade)
- Permission model design (integration with Crucible's access control)

## Next Steps

1. ✅ Complete initial research and documentation review
2. ✅ Create this working document
3. ⏳ Get answers to critical questions from user
4. ⏳ Make architecture decisions (log in Decision Log above)
5. ⏳ Create granular TDD implementation plan
6. ⏳ Begin implementation with first red test

---

**Last Updated**: 2025-11-22
**Next Review**: After receiving user feedback on questions
