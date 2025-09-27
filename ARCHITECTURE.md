# Crucible Architecture

## Overview

Crucible is a knowledge management system built with a modern, modular architecture that supports real-time collaboration, semantic search, and extensibility through plugins. The system is designed around CRDTs (Conflict-free Replicated Data Types) for seamless multi-user collaboration.

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
**Focus**: AI-enhanced features
- Vector embeddings
- Semantic search
- Auto-tagging
- Smart suggestions

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

## Future Considerations

### Scalability
- Distributed CRDT synchronization
- Multi-database support
- Cloud storage integration
- Mobile applications

### Intelligence
- Advanced AI models
- Custom embedding models
- Automated content organization
- Predictive text and suggestions

---

*This architecture is designed for extensibility, performance, and user experience while maintaining the simplicity and power of a local-first knowledge management system.*
