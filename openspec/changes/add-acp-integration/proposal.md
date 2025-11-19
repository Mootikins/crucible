# Add Agent Context Protocol Integration

## Why

The ACP-MVP requires seamless integration with the Agent Client Protocol to enable external AI agents (Claude Code, Gemini CLI, etc.) to access Crucible knowledge through standardized interfaces. While the ACP research and design work has been completed (see docs/ACP-MVP.md and docs/ACP-RESEARCH-REPORT.md), there's no formal specification for how ACP integrates with Crucible's architecture and systems.

Current gaps include:

1. **No ACP Integration Layer**: Missing the bridge between ACP protocol messages and Crucible's tool/query systems
2. **Filesystem Abstraction Missing**: ACP expects filesystem interfaces but Crucible needs kiln-agnostic access patterns
3. **Session Management Undefined**: No specification for managing agent sessions and context persistence
4. **Permission Integration**: ACP permission model needs integration with Crucible's kiln access controls

The ACP integration specification will define how external agents connect, authenticate, and interact with Crucible knowledge through the Agent Client Protocol while maintaining security, performance, and the philosophy of "agents should respect your mental model."

## What Changes

**NEW CAPABILITY:**

**ACP Client Integration:**
- Implement ACP client using official `agent-client-protocol` crate
- Define filesystem abstraction mapping (ACP calls → kiln operations)
- Create session management and context persistence
- Implement multi-agent support (Claude Code, Gemini, etc.)

**Kiln Access Abstraction:**
- Map ACP filesystem operations to kiln note access patterns
- Implement note name/wikilink resolution for agent requests
- Create permission boundary enforcement for ACP sessions
- Provide agent-friendly error handling and recovery

**Context Enrichment Pipeline:**
- Integrate query system with ACP prompt enrichment
- Define automatic context injection workflows
- Create context optimization strategies for token efficiency
- Implement agent feedback and learning mechanisms

**Security and Permissions:**
- Integrate ACP permission model with kiln access controls
- Define session scoping and isolation
- Implement approval workflows for sensitive operations
- Create audit logging for agent interactions

**Performance and Optimization:**
- Optimize ACP message handling for low-latency interactions
- Implement connection pooling and resource management
- Create caching strategies for frequent agent operations
- Add monitoring and analytics for ACP usage

## Impact

### Affected Specs
- **acp-integration** (NEW) - Define Agent Client Protocol integration patterns
- **tool-system** (reference) - Tools will be primary interface for ACP agents
- **query-system** (reference) - Context enrichment will power ACP prompt injection
- **cli** (reference) - CLI will host ACP client for chat interface

### Affected Code
**New Components:**
- `crates/crucible-acp/` - NEW - ACP integration crate
- `crates/crucible-acp/src/client.rs` - ACP client implementation
- `crates/crucible-acp/src/session.rs` - Session management and context
- `crates/crucible-acp/src/filesystem.rs` - Filesystem abstraction layer
- `crates/crucible-cli/src/acp/` - NEW - CLI ACP integration and chat interface

**Integration Points:**
- `crates/crucible-tools/src/kiln_tools/` - Tool system integration for ACP
- `crates/crucible-query/src/` - Query system for context enrichment
- `crates/crucible-cli/src/core_facade.rs` - Enhanced with ACP capabilities
- `crates/crucible-core/src/traits/` - Permission and access control integration

**Dependencies Added:**
- `agent-client-protocol = "0.6"` - Official ACP Rust implementation
- `tokio` enhancements for concurrent agent handling
- Connection and session management libraries

### Implementation Strategy

**Phase 1: Core ACP Integration (Week 1)**
- Implement basic ACP client using agent-client-protocol crate
- Create filesystem abstraction mapping ACP calls to kiln operations
- Implement session management and tool registration
- Basic agent connection and communication

**Phase 2: Context Enrichment (Week 1-2)**
- Integrate query system for automatic context injection
- Implement permission model integration
- Add multi-agent support and session isolation
- Error handling and recovery mechanisms

**Phase 3: Advanced Features (Week 2)**
- Performance optimization and caching
- Agent feedback and learning systems
- Monitoring and analytics
- Comprehensive testing and validation

### User-Facing Impact
- **Natural Chat Interface**: Users can interact with their knowledge using natural language through familiar AI agents
- **Context-Aware Agents**: Agents receive relevant knowledge automatically, leading to more insightful responses
- **Multi-Agent Support**: Users can choose their preferred AI agent (Claude Code, Gemini, etc.) with consistent behavior
- **Safe Agent Interaction**: Permission model ensures agents respect user boundaries and privacy

### Timeline
- **Week 1**: Core ACP integration and basic agent communication
- **Week 2**: Context enrichment and advanced features
- **Estimated effort**: 2 weeks for complete implementation

### Dependencies
- Tool system specification (agent tool access)
- Query system specification (context enrichment)
- Existing semantic search and storage implementations
- Agent Client Protocol crate and documentation
- CLI rework for chat interface integration