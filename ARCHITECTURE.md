# Crucible Architecture

## Overview

Crucible is a knowledge management system built with a modern, modular architecture that supports real-time collaboration, semantic search, and extensibility through plugins. The system is designed around CRDTs (Conflict-free Replicated Data Types) for seamless multi-user collaboration and promotes **linked thinking** - the seamless connection and evolution of ideas across time and context. The architecture is inspired by the concept of a knowledge forge, where information is processed and refined through multiple levels of AI-powered analysis.

## 🧠 Linked Thinking Architecture

Crucible's architecture is designed to support **linked thinking** through:
- **Contextual Awareness**: Every component maintains awareness of related concepts and dependencies
- **Evolutionary Development**: Ideas and code evolve through iterative refinement and connection
- **Cross-Reference Intelligence**: The system understands and maintains relationships between ideas
- **Temporal Context**: Changes are tracked and contextualized across time

## Tech Stack

### Core Technologies
- **Rust**: Core business logic, CRDT operations, and performance-critical components
- **Tauri**: Desktop application framework providing secure native integration
- **Svelte 5**: Modern reactive frontend framework with excellent performance
- **Yrs**: CRDT library for conflict-free collaborative editing
- **PGlite**: Embedded PostgreSQL with vector extensions for semantic search

### Data Layer
- **Yrs CRDTs**: Document structure and real-time synchronization
- **PGlite + pgvector**: Vector embeddings and semantic search
- **IndexedDB**: Local persistence and offline support
- **WebRTC**: Peer-to-peer synchronization (future)

## Architecture Layers

### 1. Core Layer (`crates/crucible-core`)
**Purpose**: Business logic and data structures
**Components**:
- Document CRDT operations
- Node hierarchy management
- Property system
- Canvas spatial data

**Key Files**:
- `document.rs` - DocumentNode structure and operations
- `crdt.rs` - CRDT utilities and conflict resolution
- `canvas.rs` - Spatial positioning and canvas operations
- `properties.rs` - Metadata and property management

### 2. Backend Layer (`crates/crucible-tauri`)
**Purpose**: Desktop application backend and system integration
**Components**:
- Tauri commands and IPC
- File system operations
- Native OS integration
- State management

**Key Files**:
- `commands.rs` - Tauri command handlers
- `events.rs` - Event system and notifications
- `main.rs` - Application entry point

### 3. Frontend Layer (`packages/desktop`)
**Purpose**: User interface and user experience
**Components**:
- Svelte components and stores
- Document editing interface
- Canvas visualization
- Search and navigation

**Key Files**:
- `stores/document.ts` - Document state management
- `components/Editor.svelte` - Main editing interface
- `components/Breadcrumbs.svelte` - Navigation component
- `search/embeddings.ts` - Semantic search integration

### 4. Database Layer (`packages/desktop/src/lib/db`)
**Purpose**: Data persistence and retrieval
**Components**:
- PGlite database setup
- Vector embeddings storage
- Schema management
- Query optimization

**Key Files**:
- `index.ts` - Database initialization
- `embeddings.ts` - Vector operations
- `migrations/` - Schema evolution

### 5. Plugin System (`crates/crucible-plugins`)
**Purpose**: Extensibility and customization
**Components**:
- Rune runtime for plugin execution
- Plugin API definitions
- Security sandboxing
- Hot reload support

**Key Files**:
- `runtime.rs` - Plugin execution environment
- `lib.rs` - Plugin API definitions

### 6. MCP Integration (`crates/crucible-mcp`)
**Purpose**: AI agent integration and tooling
**Components**:
- MCP server implementation
- Tool definitions
- Agent communication protocols
- Resource exposure

**Key Files**:
- `tools.rs` - MCP tool implementations
- `lib.rs` - MCP server setup

### 7. Agent Code Generation (`specs/code-generation/`)
**Purpose**: AI agent specifications for automated code generation
**Components**:
- Agent specifications and capabilities
- Code generation templates and patterns
- Workflow specifications (GitHub Actions-style)
- Context management and state synchronization
- Quality validation and testing patterns

**Key Files**:
- `agent-specifications.md` - Detailed agent capabilities and patterns
- `workflow-specifications.md` - Workflow definitions for code generation

### 8. A2A Protocol Integration (`specs/sprint-4/`, `specs/a2a-protocol/A2A-SPEC.md`)
**Purpose**: Agent-to-agent communication and configuration management
**Components**:
- A2A protocol implementation and communication framework
- Agent discovery, management, and configuration systems
- Security, authentication, and permission management
- Streaming and asynchronous operations
- Cross-platform agent deployment and symlink-based architecture
- Semantic versioning and compatibility matrices
- Standardized directory structures with `~/.agents/` shared source

**Key Files**:
- `A2A-SPEC.md` - Comprehensive A2A configuration specification (now in specs/a2a-protocol/)
- Agent configuration schemas and registry management
- Cross-platform deployment and symlink management

### 9. Visual Programming Interface (`docs/features/visual-node-editor.md`, `specs/visual-node-editor/`)
**Purpose**: Node-based workflow builder for agent orchestration
**Components**:
- Visual node editor framework built on existing A2A protocol infrastructure
- Node registry and execution engine with Rune runtime integration
- A2A protocol integration for multi-agent workflow orchestration
- Canvas-based workflow visualization (reusing existing Canvas infrastructure)
- WebGL-accelerated rendering with performance optimizations
- Drag-and-drop node management and connection systems
- Template library and debugging tools
- Integration with Crucible's document and plugin systems
- Security sandboxing and resource management for node execution

**Key Files**:
- `technical-specs.md` - Technical implementation specifications
- `visual-node-editor.md` - Feature documentation and UX design
- Node execution engine and A2A coordinator integration

## Data Flow

### Document Operations
1. **User Input** → Svelte Component
2. **State Update** → Document Store (Yrs)
3. **CRDT Sync** → Core Layer
4. **Persistence** → PGlite Database
5. **Vectorization** → Embeddings Service
6. **UI Update** → Reactive Svelte Components

### Search Operations
1. **Query Input** → Search Component
2. **Vector Generation** → Transformers.js
3. **Similarity Search** → PGlite + pgvector
4. **Result Ranking** → Search Service
5. **UI Display** → Search Results Component

### Collaboration Flow
1. **Local Changes** → Yrs Document
2. **Update Generation** → CRDT Operations
3. **Sync Protocol** → WebRTC (future)
4. **Conflict Resolution** → Yrs CRDTs
5. **UI Synchronization** → Reactive Updates

## Sprint-Based Development

### Sprint 1: Foundation (Weeks 1-4)
**Focus**: Core editor with basic CRDT functionality
- Rust core document structure
- Svelte frontend integration
- Basic Tauri commands
- Simple persistence

### Sprint 2: Persistence & UI (Weeks 5-8)
**Focus**: Storage layer and polished interface
- PGlite integration
- Document save/load
- UI polish and animations
- Breadcrumb navigation

### Sprint 3: Canvas & Properties (Weeks 9-12)
**Focus**: Spatial view and metadata
- Canvas renderer with WebGL
- Property system
- Node positioning
- Custom property types

### Sprint 4: Intelligence Layer (Weeks 13-16)
**Focus**: AI-enhanced features and agent integration
- Vector embeddings and semantic search with RAG capabilities
- Auto-tagging and smart suggestions with ML pipelines
- A2A protocol integration for agent communication and configuration management
- Agent code generation capabilities with comprehensive templates and workflows
- Context engineering for linked thinking with pattern recognition
- Visual programming interface for multi-agent workflow orchestration
- Cross-platform agent deployment with standardized configurations
- Quality validation and testing frameworks for generated code

## Data Specifications

### Document Schema
- **Format**: JSON Schema + Zod validation
- **Structure**: Hierarchical nodes with CRDT properties
- **Validation**: Type-safe operations with runtime checks

### Embeddings Schema
- **Format**: 384-dimensional vectors
- **Storage**: PGlite with pgvector extension
- **Search**: Cosine similarity with configurable thresholds

### Canvas Schema
- **Format**: Spatial positioning with Automerge
- **Properties**: Position, size, connections, metadata
- **Collaboration**: Conflict-free spatial updates

## Security Considerations

### Data Protection
- Local-first architecture with optional sync
- End-to-end encryption for sensitive data
- Plugin sandboxing for security

### Access Control
- Document-level permissions
- Plugin execution restrictions
- API rate limiting

## Performance Characteristics

### Scalability
- **Documents**: 100k+ nodes per document
- **Search**: <100ms semantic search
- **Collaboration**: 5+ concurrent users
- **Memory**: Efficient CRDT operations

### Optimization Strategies
- Virtual scrolling for large documents
- Lazy loading of document sections
- Efficient vector indexing
- Background embedding generation

## Extension Points

### Plugin API
- Document manipulation hooks
- Custom property types
- Search result filters
- UI component extensions

### MCP Tools
- Document search and retrieval
- Content generation
- Analysis and insights
- Workflow automation

### Agent Code Generation
- Component generation from natural language with detailed templates
- Pattern recognition and application with comprehensive validation
- Context-aware code modification with learning capabilities
- Test and documentation generation with quality metrics
- Quality validation and optimization with automated feedback
- GitHub Actions-style workflow execution with error handling
- Context management and state synchronization across agents

### A2A Protocol
- Agent discovery and registration with configuration management
- Inter-agent communication with standardized message formats
- Capability sharing and coordination with compatibility matrices
- Security and authentication with granular permission controls
- Streaming and asynchronous operations with cross-platform support
- Semantic versioning and dependency management
- Symlink-based architecture for shared resource management
- Resource limits and sandboxing for security

### Context Engineering
- Project context management with pattern learning
- Code quality assessment with automated feedback
- Performance metrics collection and optimization
- Integration testing with compatibility validation
- User preference learning and adaptation
- Gap analysis and specification generation

## Future Considerations

### Scalability
- Distributed CRDT synchronization
- Multi-database support
- Cloud storage integration
- Mobile applications

### Intelligence
- Advanced AI models and agent coordination
- Custom embedding models and semantic understanding
- Automated content organization through linked thinking
- Predictive text and suggestions with contextual awareness
- Multi-agent collaboration and code generation

## Key Resources

- **[Gap Analysis](./specs/GAP_ANALYSIS_COMPREHENSIVE.md)**: Comprehensive analysis of implementation gaps and context engineering needs
- **[Agent Specifications](./specs/code-generation/agent-specifications.md)**: Detailed specifications for AI agent code generation with templates and validation
- **[Workflow Specifications](./specs/code-generation/workflow-specifications.md)**: GitHub Actions-style workflows for agent operations with error handling
- **[A2A Protocol Specification](./specs/a2a-protocol/A2A-SPEC.md)**: Comprehensive agent-to-agent configuration and communication framework
- **[Visual Node Editor](./specs/visual-node-editor/technical-specs.md)**: Technical specifications for visual programming interface
- **[Agent Configuration](./AGENTS.md)**: AI agent documentation and linked thinking principles

---

*This architecture is designed for extensibility, performance, and user experience while maintaining the simplicity and power of a local-first knowledge management system that promotes linked thinking through AI agent integration and contextual awareness.*
