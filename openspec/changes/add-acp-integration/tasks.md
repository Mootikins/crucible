# ACP Integration Tasks

## Progress Summary

**Completed**: Phases 3-5 (TDD Cycles 11-20 + Baseline Tests + ChatSession Agent Connection)
- ✅ Context enrichment with semantic search (Cycle 11)
- ✅ TTL-based context caching (Cycle 12)
- ✅ Response streaming infrastructure (Cycle 13)
- ✅ Conversation history management (Cycle 14)
- ✅ Interactive chat session manager (Cycle 15)
- ✅ Multi-turn conversation state tracking (Cycle 16)
- ✅ Error handling and recovery (Cycle 17)
- ✅ Session metadata and management (Cycle 18)
- ✅ Agent lifecycle methods (Cycle 19)
- ✅ ACP protocol handshake (Cycle 20)
- ✅ Comprehensive baseline integration tests (43 tests)
- ✅ MockAgent implementation
- ✅ ChatSession connected to real ACP agent (daec1ca)

**Completed**: Phase 6 - ACP Client Implementation (Corrected Architecture)
- ✅ Implemented CrucibleClient (Client trait from agent-client-protocol)
- ✅ File operations (read_text_file, write_text_file) for general coding
- ✅ Permission handling with option selection
- ✅ Agent spawning with ClientSideConnection
- ✅ Session notification handling

**In Progress**: Phase 7 - Embedded MCP Server & CLI Integration
- ⏳ Create embedded MCP server for Crucible tools
- ⏳ Expose 10 Crucible tools via MCP (read_note, create_note, etc.)
- ⏳ Wire up MCP server in NewSessionRequest
- ⏳ Update CLI to use new CrucibleClient
- ⏳ End-to-end testing with real agent

**Test Coverage**: 148/148 tests passing (117 acp + 26 cli + 5 client) - 100%
**SOLID Compliance**: Verified
**Protocol Compliance**: ACP 0.7.0
**Current Branch**: `claude/acp-cli-integration-01JRpdf8Lzjo3GWzu2mCDiKJ`
**Previous Branch**: `claude/acp-planning-baseline-tests-01EBcv3F9FjBfUC9pNEFyrcM`

See [PROGRESS_REPORT.md](./PROGRESS_REPORT.md) for detailed implementation documentation.

---

## 1. ACP Client Foundation
- [x] 1.1 Set up agent-client-protocol dependency and integration
- [x] 1.2 Implement basic ACP client class and connection handling
- [x] 1.3 Create agent process spawning and management
- [x] 1.4 Implement protocol message handling and routing
- [x] 1.5 Add connection error handling and recovery

## 2. File Operations (ACP Client Implementation)
- [x] 2.1 Implement read_text_file for general file reading ✅
- [x] 2.2 Implement write_text_file for general file writing ✅
- [x] 2.3 Handle read-only mode enforcement ✅
- [x] 2.4 Provide file operations relative to current directory ✅
- [x] 2.5 Separate from kiln operations (handled by MCP tools) ✅

**Note**: ACP file operations are general-purpose (not kiln-specific). Crucible-specific note operations are provided via embedded MCP server (see Task 4).

## 3. Session Management and Context
- [x] 3.1 Implement ACP session creation and lifecycle management
- [x] 3.2 Create session context persistence and restoration
- [x] 3.3 Add multi-session support for concurrent agent usage
- [x] 3.4 Implement session isolation and security boundaries
- [x] 3.5 Create session cleanup and resource management

## 4. Embedded MCP Server for Crucible Tools
- [ ] 4.1 Create embedded MCP server within Crucible binary
- [ ] 4.2 Expose 10 Crucible tools via MCP (read_note, create_note, etc.)
- [ ] 4.3 Implement stdio transport for MCP server
- [ ] 4.4 Provide MCP server config in NewSessionRequest
- [ ] 4.5 Agent discovers and uses tools through MCP protocol

**Note**: Standard approach per web research - editors embed MCP servers to expose their tools to agents. Agent connects to our MCP server and discovers tools automatically.

## 5. Context Enrichment Pipeline
- [x] 5.1 Integrate query system for automatic context discovery (TDD Cycle 11)
- [x] 5.2 Implement context injection into agent prompts (TDD Cycle 11)
- [x] 5.3 Create context optimization for token efficiency (TDD Cycle 11)
- [x] 5.4 Add context relevance filtering and ranking (TDD Cycle 11)
- [x] 5.5 Implement context caching for session efficiency (TDD Cycle 12)

## 6. Permission and Security Integration
- [ ] 6.1 Map ACP permissions to kiln access controls
- [ ] 6.2 Implement session scoping and directory boundaries
- [ ] 6.3 Create approval workflows for sensitive operations
- [ ] 6.4 Add audit logging for agent interactions
- [ ] 6.5 Implement permission persistence and settings

## 7. Multi-Agent Support
- [ ] 7.1 Support for Claude Code agent integration
- [ ] 7.2 Support for Gemini CLI agent integration
- [ ] 7.3 Create agent-specific configuration and settings
- [ ] 7.4 Implement agent capability detection and adaptation
- [ ] 7.5 Add agent switching and session migration

## 8. CLI Integration and Chat Interface
- [x] 8.1 Integrate ACP client into CLI chat commands (TDD Cycle 15 - foundation)
- [x] 8.2 Create interactive chat interface with agent selection (TDD Cycle 15)
- [x] 8.3 Implement real-time streaming of agent responses (TDD Cycle 13)
- [x] 8.4 Add chat history and session persistence (TDD Cycles 14, 16, 18)
- [x] 8.5 Create chat configuration and preference management (TDD Cycle 15)

## 9. Performance and Optimization
- [ ] 9.1 Optimize ACP message handling and throughput
- [ ] 9.2 Implement connection pooling and resource management
- [ ] 9.3 Add caching strategies for frequent operations
- [ ] 9.4 Optimize context enrichment latency
- [ ] 9.5 Implement resource cleanup and memory management

## 10. Testing and Validation
- [x] 10.1 Write unit tests for ACP client components
- [x] 10.2 Create integration tests with multiple agent types (MockAgent)
- [x] 10.3 Test context enrichment quality with real queries
- [ ] 10.4 Validate permission model and security boundaries
- [x] 10.5 Perform end-to-end testing with chat workflows

## 11. Monitoring and Analytics
- [ ] 11.1 Implement ACP usage monitoring and metrics
- [ ] 11.2 Create performance dashboards for agent interactions
- [ ] 11.3 Add error tracking and alerting for ACP issues
- [ ] 11.4 Create analytics for context enrichment effectiveness
- [ ] 11.5 Implement health checks and system validation

## 12. Documentation and Examples
- [ ] 12.1 Create ACP integration documentation
- [ ] 12.2 Write agent setup and configuration guides
- [ ] 12.3 Create troubleshooting guide for common issues
- [ ] 12.4 Add examples of agent workflows and use cases
- [ ] 12.5 Document security best practices and recommendations