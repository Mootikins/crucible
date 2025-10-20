# Crucible Architecture

> High-level system architecture and component overview

## System Overview

Crucible is a knowledge management system built for linked thinking, real-time collaboration, and AI integration. The architecture follows a layered approach with clear separation between user interfaces, services, core logic, and storage.

## High-Level Architecture

```mermaid
graph TB
    subgraph "User Interfaces"
        CLI[CLI/TUI]
        Desktop[Desktop App]
        MCP[MCP Server]
    end

    subgraph "Service Layer"
        McpSvc[MCP Service]
        LLM[LLM Integration]
        Sync[Sync Engine]
    end

    subgraph "Core Logic"
        Core[crucible-core]
        Docs[Document Management]
        CRDT[CRDT Operations]
        Agents[Agent System]
    end

    subgraph "Infrastructure"
        Config[Config Management]
        Watch[File Watcher]
        Plugins[Plugin System]
    end

    subgraph "Storage"
        Surreal[SurrealDB]
        Duck[DuckDB]
        Files[File System]
    end

    CLI --> Core
    Desktop --> Core
    MCP --> McpSvc

    McpSvc --> Core
    LLM --> Core
    Sync --> Core

    Core --> Docs
    Core --> CRDT
    Core --> Agents

    Docs --> Surreal
    CRDT --> Surreal
    Agents --> Duck

    Watch --> Core
    Config --> Core
    Plugins --> Core

    Watch --> Files
```

## Component Relationships

```mermaid
graph LR
    subgraph "Foundation"
        A[crucible-core]
        B[crucible-config]
    end

    subgraph "Services"
        C[crucible-mcp]
        D[crucible-daemon]
        E[crucible-surrealdb]
        F[crucible-llm]
    end

    subgraph "Interfaces"
        G[crucible-cli]
        H[crucible-tauri]
    end

    subgraph "Storage"
        I[SurrealDB]
        J[DuckDB]
        K[File System]
    end

    A --> C
    A --> D
    A --> E
    A --> G
    A --> H

    B --> D
    B --> C

    C --> G
    C --> H

    F --> C
    F --> D

    E --> I
    E --> J

    D --> K
```

## Core Components

### Foundation Layer

**crucible-core**: Heart of the system containing domain models, document management, CRDT operations, and agent definitions. Provides the essential abstractions that all other components build upon.

**crucible-config**: Centralized configuration management that handles settings, preferences, and environment-specific configuration across the entire system.

### Service Layer

**crucible-mcp**: MCP protocol server enabling AI agent integration with semantic search, document indexing, and tool-based interactions.

**crucible-daemon**: Background service providing terminal interface, REPL capabilities, and real-time file monitoring.

**crucible-surrealdb**: Database integration layer managing SurrealDB connections, queries, and data persistence.

**crucible-llm**: LLM integration supporting multiple providers (OpenAI, Ollama) for embeddings and AI capabilities.

### Interface Layer

**crucible-cli**: Command-line interface with interactive REPL, fuzzy search, and chat capabilities for terminal users.

**crucible-tauri**: Desktop application backend providing native integration, system notifications, and desktop-specific features.

**Frontend Packages**:
- **Desktop**: Tauri + Svelte 5 application for rich desktop experience
- **Shared**: Common frontend components and utilities
- **Obsidian Plugin**: Integration with Obsidian for extending existing workflows

### Supporting Systems

**crucible-watch**: File system monitoring service that detects changes and triggers document processing pipelines.

**crucible-plugins**: Plugin system using Rune scripting for dynamic extensibility and custom tool execution.

**crucible-sync**: Real-time synchronization engine managing CRDT operations and collaborative editing.

## Data Flow Patterns

### Document Processing

```mermaid
sequenceDiagram
    participant FS as File System
    participant W as Watcher
    participant P as Parser
    participant D as Database
    participant S as Search

    FS->>W: File Change
    W->>P: Parse Request
    P->>D: Store Document
    D->>S: Update Index
    S->>W: Index Ready
```

### AI Agent Interaction

```mermaid
sequenceDiagram
    participant AI as AI Agent
    participant MCP as MCP Server
    participant DB as Database
    participant LLM as LLM Service

    AI->>MCP: Tool Request
    MCP->>DB: Query Documents
    DB->>MCP: Query Results
    MCP->>LLM: Generate Embeddings
    LLM->>MCP: Embedding Results
    MCP->>AI: Formatted Response
```

### Real-time Collaboration

```mermaid
sequenceDiagram
    participant U1 as User 1
    participant CRDT as CRDT Engine
    participant DB as Database
    participant U2 as User 2

    U1->>CRDT: Edit Operation
    CRDT->>DB: Persist Change
    CRDT->>U2: Broadcast Update
    U2->>CRDT: Acknowledge
```

## Key Architectural Decisions

**Layered Architecture**: Clear separation between interfaces, services, core logic, and storage enables independent development and maintenance.

**Multi-Model Database**: SurrealDB provides graph, document, and relational capabilities while DuckDB handles analytics and vector operations.

**Async-First Design**: Built on Tokio for high concurrency and non-blocking operations throughout the system.

**Trait-Based Extensibility**: Rust traits enable pluggable components and easy testing while maintaining performance.

**MCP Integration**: Native support for AI agents through the Model Context Protocol enables intelligent workflows and semantic search.

**Plugin System**: Rune scripting allows dynamic extensibility without compromising core system security or performance.