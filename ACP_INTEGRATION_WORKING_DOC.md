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
| D1 | ✅ Upgrade to agent-client-protocol 0.7.0 | Latest version with newest features | 2025-11-22 | APPROVED |
| D2 | ✅ Create new crucible-acp crate | Clean separation, reusable component | 2025-11-22 | APPROVED |
| D3 | ✅ Incremental: Chat + context insertion first | Baseline functionality, then iterate | 2025-11-22 | APPROVED |
| D4 | ✅ Hybrid file operations model | ACP fs → project files, Tools → kiln ops | 2025-11-22 | APPROVED |
| D5 | ✅ Mock agent first, then Claude Code | Controlled testing, then real integration | 2025-11-22 | APPROVED |
| D6 | ⏳ Session persistence TBD | Will decide during session implementation | 2025-11-22 | DEFERRED |

### Key Architecture: Dual-Path File Access

**ACP Filesystem Operations** (ReadTextFileRequest/WriteTextFileRequest):
- Map to actual filesystem paths relative to CWD
- Agent reads project files, configs, code
- Standard ACP file abstraction

**Tool Calls** (via MCP):
- Access kiln operations (semantic_search, read_note, etc.)
- Query knowledge base
- Specialized knowledge management operations

**Rationale**: Separates concerns - workspace files (ACP fs) vs knowledge base (tools). Follows ACP design patterns while leveraging our tool system.

### SOLID Principles - Design Constraints

**CRITICAL**: Traits and associated types defined in crucible-core. Concrete types OK for handoff/associated types.

**Single Responsibility**:
- Each module handles one concern (session, filesystem, protocol, tools)
- Separate traits for read vs write operations

**Open/Closed**:
- Extensible via traits (e.g., new agent types, new tool adapters)
- Closed to modification of core protocol handling

**Liskov Substitution**:
- Any `impl Client` must be swappable
- Tool adapters must be interchangeable

**Interface Segregation**:
- Separate traits for different capabilities
- Don't force clients to depend on unused methods
- Example: `FilesystemReader`, `FilesystemWriter`, `ToolExecutor` as separate traits

**Dependency Inversion**:
- **crucible-core** defines traits and associated types
- **crucible-acp** implements ACP-specific logic using core traits
- **crucible-cli** provides concrete implementations
- Use dependency injection for all services

**Design Pattern**:
```rust
// crucible-core/src/traits/acp.rs
pub trait SessionManager {
    type Session;
    type Config;
    async fn create_session(&mut self, config: Self::Config) -> Result<Self::Session>;
}

pub trait ToolBridge {
    type Call;
    type Result;
    async fn execute_tool(&self, call: Self::Call) -> Result<Self::Result>;
}

// crucible-core/src/types/acp.rs (concrete types for handoff)
pub struct SessionConfig { /* fields */ }
pub struct SessionId { /* fields */ }
pub struct ToolCall { /* fields */ }
pub struct ToolResult { /* fields */ }

// crucible-acp/src/client.rs (implements traits from core)
impl SessionManager for AcpClient {
    type Session = SessionId;
    type Config = SessionConfig;
    // implementation
}

// crucible-cli (uses both core and acp)
use crucible_core::traits::SessionManager;
use crucible_acp::AcpClient;
```

## Open Questions - ✅ RESOLVED

All critical questions answered (see Decision Log above).

## Implementation Plan - DETAILED TDD APPROACH

### Phase 0: Planning & Setup ✅ COMPLETE
- [x] Read openspec and documentation
- [x] Understand current codebase state
- [x] Create working document
- [x] Get clarifying answers from user
- [x] Create detailed TDD plan (this document)
- [x] Update dependencies to ACP 0.7.0
- [x] Create crucible-acp crate structure
- [x] Run baseline test suite (verify no breakage)

### Phase 1: Core Infrastructure (Mock Agent Foundation) ✅ COMPLETE
**Goal**: Create new crate, implement mock agent for testing, upgrade ACP

#### 1.1: Crate Setup & Dependency Upgrade ✅
**TDD Cycle 1**: Upgrade ACP to 0.7.0 ✅
- ✅ RED: Update Cargo.toml to require 0.7.0, expect compilation failures
- ✅ GREEN: Fix breaking changes, ensure builds
- ✅ REFACTOR: Update imports and types as needed
- ✅ CHECKPOINT: `cargo test --workspace` passes

**TDD Cycle 2**: Create crucible-acp crate ✅
- ✅ RED: Add crate to workspace, write failing integration test
- ✅ GREEN: Create lib.rs, basic structure, test passes
- ✅ REFACTOR: Add module structure (client, session, filesystem, protocol)
- ✅ CHECKPOINT: `cargo build --workspace` succeeds

#### 1.2: Mock Agent for Testing ✅
**TDD Cycle 3**: Mock agent process ✅
- ✅ RED: Write test expecting mock agent to respond to initialize
- ✅ GREEN: Implement MockAgent struct with stdio communication
- ✅ REFACTOR: Extract message handling utilities
- ✅ CHECKPOINT: Mock agent responds to basic messages

**TDD Cycle 4**: Mock agent message protocol ✅
- ✅ RED: Test expects mock agent to handle session requests
- ✅ GREEN: Implement JSON-RPC message parsing and responses
- ✅ REFACTOR: Create message builder utilities
- ✅ CHECKPOINT: Mock agent handles all basic message types

#### 1.3: Client Trait Implementation ✅
**TDD Cycle 5**: Client trait skeleton ✅
- ✅ RED: Write test expecting Client trait implementation
- ✅ GREEN: Implement CrucibleAcpClient with SessionManager trait
- ✅ REFACTOR: Organize trait methods by category
- ✅ CHECKPOINT: Trait compiles, mock tests pass

**TDD Cycle 6**: Session initialization ✅
- ✅ RED: Test expects successful session creation
- ✅ GREEN: Implement session state tracking (create, load, end)
- ✅ REFACTOR: Extract session state management
- ✅ CHECKPOINT: Can create and initialize sessions (19/19 tests passing)

### Phase 2: Filesystem & Tool Integration 🚧 IN PROGRESS
**Goal**: Implement file operations and tool call bridging

#### 2.1: Filesystem Abstraction ✅
**TDD Cycle 7**: Read file operations ✅
- ✅ RED: Test expects ReadTextFileRequest to return file contents
- ✅ GREEN: Implement read_text_file handler with path resolution
- ✅ REFACTOR: Add error handling and validation
- ✅ CHECKPOINT: Can read files from CWD (24/24 tests passing)

**TDD Cycle 8**: Write file operations (plan mode restriction) ✅
- ✅ RED: Test expects write denied in plan mode
- ✅ GREEN: Implement write_text_file with mode checking
- ✅ REFACTOR: Extract permission enforcement
- ✅ CHECKPOINT: Plan mode blocks writes, act mode allows (29/29 tests passing)

#### 2.2: Tool System Bridge ⏳ NEXT
**TDD Cycle 9**: Tool discovery and registration
- [ ] RED: Test expects tool catalog to include Crucible tools
- [ ] GREEN: Implement tool registration from crucible-tools
- [ ] REFACTOR: Create tool descriptor conversion utilities
- [ ] CHECKPOINT: Agent sees all available Crucible tools

**TDD Cycle 10**: Tool execution
- [ ] RED: Test expects tool call to execute and return result
- [ ] GREEN: Implement tool call routing to crucible-tools
- [ ] REFACTOR: Add result formatting and error handling
- [ ] CHECKPOINT: Tools execute successfully from ACP

### Phase 3: Context Enrichment & Streaming
**Goal**: Integrate semantic search and implement response streaming

#### 3.1: Context Injection
**TDD Cycle 11**: Context enrichment integration
- RED: Test expects enriched prompt to include search results
- GREEN: Integrate ContextEnricher with session prompts
- REFACTOR: Make context size configurable
- CHECKPOINT: Prompts include relevant knowledge base context

**TDD Cycle 12**: Context caching
- RED: Test expects repeated queries to reuse context
- GREEN: Implement context cache with TTL
- REFACTOR: Add cache invalidation logic
- CHECKPOINT: Context queries are cached efficiently

#### 3.2: Response Streaming
**TDD Cycle 13**: Stream handler
- RED: Test expects streaming responses to UI
- GREEN: Implement session_update handler for streaming
- REFACTOR: Extract formatting utilities
- CHECKPOINT: Agent responses stream to terminal

**TDD Cycle 14**: Multi-turn conversation
- RED: Test expects conversation history to persist
- GREEN: Implement session history management
- REFACTOR: Add history pruning for token limits
- CHECKPOINT: Multi-turn conversations work

### Phase 4: Interactive Chat Interface
**Goal**: Replace placeholder with working interactive loop

#### 4.1: REPL Integration
**TDD Cycle 15**: Interactive input loop
- RED: Test expects continuous input/output cycle
- GREEN: Implement chat loop with rustyline/reedline
- REFACTOR: Extract input handling utilities
- CHECKPOINT: Interactive chat accepts continuous input

**TDD Cycle 16**: Mode toggle commands
- RED: Test expects /plan and /act commands to work
- GREEN: Implement mode switching with client updates
- REFACTOR: Add status indicators and confirmations
- CHECKPOINT: Can toggle between plan and act modes

#### 4.2: Enhanced UX
**TDD Cycle 17**: Progress indicators
- RED: Test expects loading indicators during operations
- GREEN: Implement spinners for context search and agent calls
- REFACTOR: Clean up terminal output formatting
- CHECKPOINT: User sees clear feedback during operations

**TDD Cycle 18**: Error recovery
- RED: Test expects graceful handling of agent crashes
- GREEN: Implement reconnection logic and error messages
- REFACTOR: Add retry logic with backoff
- CHECKPOINT: Handles errors without crashing

### Phase 5: Real Agent Integration
**Goal**: Test with actual Claude Code agent

#### 5.1: Claude Code Integration
**TDD Cycle 19**: Agent spawning
- RED: Test expects claude-code to spawn and initialize
- GREEN: Update agent discovery to detect claude-code
- REFACTOR: Handle agent-specific quirks
- CHECKPOINT: Claude Code spawns successfully

**TDD Cycle 20**: End-to-end workflow
- RED: Test expects complete conversation flow
- GREEN: Fix any issues discovered with real agent
- REFACTOR: Optimize performance and UX
- CHECKPOINT: Full chat workflow works with Claude Code

### Phase 6: Polish & Documentation
**Goal**: Production readiness

#### 6.1: Testing & Validation
- Comprehensive unit test coverage (>80%)
- Integration tests for all major flows
- Error case coverage
- Performance testing
- CHECKPOINT: `cargo test --workspace` passes, coverage report

#### 6.2: Documentation
- Update crate README
- API documentation
- Usage examples
- Troubleshooting guide
- CHECKPOINT: Documentation complete and accurate

#### 6.3: Migration
- Move agent discovery from CLI to crucible-acp
- Update CLI to use new crate
- Remove placeholder code
- CHECKPOINT: CLI cleanly integrated with crucible-acp

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

## Summary: 20 TDD Cycles Across 6 Phases

| Phase | Cycles | Focus | Status | Time |
|-------|--------|-------|--------|------|
| Phase 0 | Setup | Planning & infrastructure | ✅ COMPLETE | 2-3 hours |
| Phase 1 | 1-6 | Mock agent & client trait | ✅ COMPLETE | 4-6 hours |
| Phase 2 | 7-10 | Filesystem & tools | 🚧 50% (7-8 done) | 3-4 hours |
| Phase 3 | 11-14 | Context & streaming | ⏳ PENDING | 3-4 hours |
| Phase 4 | 15-18 | Interactive chat | ⏳ PENDING | 3-4 hours |
| Phase 5 | 19-20 | Real agent testing | ⏳ PENDING | 2-3 hours |
| Phase 6 | N/A | Polish & docs | ⏳ PENDING | 2-3 hours |
| **TOTAL** | **20** | **Complete implementation** | **40% COMPLETE** | **~20-27 hours** |

## Test Results Summary

**Current Status:**
- `crucible-acp`: **29/29 tests passing** ✅
- `crucible-core`: **554/554 tests passing** ✅
- `crucible-workspace`: **900+ tests passing** ✅
- Pre-existing failures: 5 fastembed tests (documented)

**Test Breakdown by Cycle:**
- Cycles 3-4 (MockAgent): 4 tests
- Cycle 5 (Client trait): 2 tests
- Cycle 6 (Sessions): 3 tests
- Cycle 7 (Read files): 5 tests
- Cycle 8 (Write files): 5 tests
- **Total new tests**: 19 tests added

## Next Steps

1. ✅ Complete Phase 0 - Setup
2. ✅ Execute TDD Cycles 1-6 (Phase 1)
3. ✅ Execute TDD Cycles 7-8 (Phase 2 partial)
4. ⏳ Execute TDD Cycles 9-10 (Phase 2 complete)
5. ⏳ Run workspace checkpoint after Phase 2
6. ⏳ Execute TDD Cycles 11-14 (Phase 3)
7. ⏳ Execute TDD Cycles 15-18 (Phase 4)
8. ⏳ Execute TDD Cycles 19-20 (Phase 5)
9. ⏳ Complete Phase 6 (Polish & Documentation)

---

**Last Updated**: 2025-11-22
**Status**: Ready to begin implementation
**Next Action**: Phase 0 - Setup (upgrade ACP, create crate)
