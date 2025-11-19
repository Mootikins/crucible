# Add Tool System for Agent Knowledge Access

## Why

The ACP-MVP requires a comprehensive tool system to enable agents to access and manipulate knowledge within the kiln, following the philosophy of "agents should respect your mental model" and "collaboration should feel natural." Currently, Crucible has tool execution abstractions (`ToolExecutor` trait) but lacks a standardized set of kiln-specific tools and the permission model needed for safe agent interaction.

The ACP-MVP's core value proposition is context enrichment where agents need tools to:
1. Access kiln content without filesystem dependencies
2. Search and retrieve relevant knowledge for context enrichment
3. Perform knowledge operations within defined boundaries
4. Respect user privacy and control patterns

Without a formal tool system specification, agent integration will be inconsistent and potentially unsafe.

## What Changes

**NEW CAPABILITY:**

**Core Tool Categories:**
- **Knowledge Access Tools**: Read, search, and discover kiln content using note names and wikilinks
- **Knowledge Manipulation Tools**: Create, update, and delete notes with permission controls
- **Metadata Tools**: Query and manage tags, links, and note properties
- **Administrative Tools**: Index management, system status, and tool discovery

**Kiln-Agnostic Design:**
- Tools reference notes by name/wikilink, not filesystem paths
- Supports multiple storage backends (files, database, future remote services)
- Follows Obsidian-style reference patterns for familiarity

**Permission Model:**
- Full read access within current working directory by default
- Explicit permission prompts for write operations (with auto-approve option)
- User approval for access outside current directory scope
- Future: Per-kiln permissions for personal vs work separation

**Integration Points:**
- Builds on existing `ToolExecutor` trait from `crucible-core`
- Defines static tool discovery for MVP simplicity
- Native tool implementation (no MCP dependency)
- Structured JSON results for flexible formatting

## Impact

### Affected Specs
- **tool-system** (NEW) - Define comprehensive tool system for agent knowledge access
- **acp-integration** (future) - Tools will be primary interface for ACP agents

### Affected Code
**New Components:**
- `crates/crucible-tools/src/kiln_tools/` - NEW - Core kiln access tools
- `crates/crucible-tools/src/permission.rs` - NEW - Permission management system
- `crates/crucible-cli/src/acp/tools/` - NEW - ACP tool integration layer

**Existing Integration:**
- `crates/crucible-core/src/traits/tools.rs` - Enhanced with kiln-specific tool definitions
- `crates/crucible-tools/src/search_tools.rs` - Adapted for kiln-agnostic access
- `crates/crucible-surrealdb/src/kiln_integration.rs` - Backend tool implementations

**ACP Integration:**
- ACP client tools bridge (agent calls → native tool execution)
- Tool registration and discovery for agent startups
- Permission flow integration with ACP session management

### Implementation Strategy

**Phase 1: Core Knowledge Access (Week 1)**
- Implement note read/list tools using note names and wikilinks
- Add semantic search tool for context enrichment
- Create basic permission model with user approval flows
- Static tool discovery for agent startup

**Phase 2: Knowledge Manipulation (Week 1-2)**
- Add note creation and editing tools with permission controls
- Implement tag and metadata management tools
- Add batch operations for efficient note processing
- Integration testing with ACP client

**Phase 3: Advanced Features (Week 2)**
- Administrative tools for index and system management
- Enhanced permission features (directory approval, auto-approve)
- Performance optimization and caching
- Comprehensive testing and validation

### User-Facing Impact
- **Enhanced Agent Capabilities**: Agents can naturally access and manipulate knowledge using familiar note references
- **Safe Agent Interaction**: Permission model ensures users maintain control over their knowledge
- **Storage Agnostic**: Tools work regardless of whether notes are stored as files or in database
- **Improved Context Enrichment**: Better tools lead to more relevant context for agent responses

### Timeline
- **Week 1**: Core tools and permissions
- **Week 2**: Advanced features and ACP integration
- **Estimated effort**: 2 weeks for complete implementation

### Dependencies
- Existing `ToolExecutor` trait infrastructure
- Semantic search implementation (already complete)
- SurrealDB kiln integration (already complete)
- ACP client integration (parallel development)