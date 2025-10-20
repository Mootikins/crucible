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
        ServiceAPI[Service API]
    end

    subgraph "Service Layer"
        Search[Search Service]
        Index[Index Service]
        Agent[Agent Service]
        Tool[Tool Registry]
        HotReload[Hot Reload Service]
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
        Tools[Static Tools]
    end

    subgraph "Storage"
        Surreal[SurrealDB]
        Duck[DuckDB]
        Files[File System]
    end

    subgraph "Scripting Layer"
        Rune[Rune Runtime]
        Macros[Procedural Macros]
    end

    CLI --> Core
    Desktop --> Core
    ServiceAPI --> Search
    ServiceAPI --> Index
    ServiceAPI --> Agent
    ServiceAPI --> Tool

    Search --> Core
    Index --> Core
    Agent --> Core
    Tool --> Core

    HotReload --> Rune

    Core --> Docs
    Core --> CRDT
    Core --> Agents

    Docs --> Surreal
    CRDT --> Surreal
    Agents --> Duck

    Watch --> Core
    Config --> Core
    Plugins --> Core

    Tools --> Core
    Watch --> Files
```

## Component Relationships

```mermaid
graph LR
    subgraph "Foundation"
        A[crucible-core]
        B[crucible-config]
    end

    subgraph "Service Layer"
        C[crucible-services]
        D[crucible-daemon]
        E[crucible-surrealdb]
        F[crucible-llm]
    end

    subgraph "Scripting & Tools"
        G[crucible-rune]
        H[crucible-tools]
        I[crucible-rune-macros]
    end

    subgraph "Interfaces"
        J[crucible-cli]
        K[crucible-tauri]
    end

    subgraph "Storage"
        L[SurrealDB]
        M[DuckDB]
        N[File System]
    end

    A --> C
    A --> D
    A --> E
    A --> J
    A --> K

    B --> D
    B --> C

    C --> G
    C --> H
    C --> I

    F --> C
    F --> D

    G --> H
    G --> I

    E --> L
    E --> M

    D --> N

    H --> L
    H --> M
```

## Core Components

### Foundation Layer

**crucible-core**: Heart of the system containing domain models, document management, CRDT operations, and agent definitions. Provides the essential abstractions that all other components build upon.

**crucible-config**: Centralized configuration management that handles settings, preferences, and environment-specific configuration across the entire system.

### Service Layer

**crucible-services**: Service abstraction layer providing search, indexing, and AI agent integration capabilities. This layer replaces the former MCP server and provides a clean interface for system services.

**crucible-daemon**: Background service providing terminal interface, REPL capabilities, and real-time file monitoring. Integrates with the new service layer architecture.

**crucible-surrealdb**: Database integration layer managing SurrealDB connections, queries, and data persistence.

**crucible-llm**: LLM integration supporting multiple providers (OpenAI, Ollama) for embeddings and AI capabilities.

### Scripting & Tools Layer

**crucible-rune**: Rune scripting system for dynamic tool execution, providing hot-reload capabilities and extensible tool creation.

**crucible-tools**: Static system tools for knowledge management, including search, metadata extraction, and document processing utilities.

**crucible-rune-macros**: Procedural macros for Rune tool generation, enabling compile-time tool creation with type safety and validation.

### Interface Layer

**crucible-cli**: Command-line interface with interactive REPL, fuzzy search, and chat capabilities for terminal users.

**crucible-tauri**: Desktop application backend providing native integration, system notifications, and desktop-specific features.

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
    participant Service as Service Layer
    participant Tool as Tool Registry
    participant DB as Database
    participant LLM as LLM Service

    AI->>Service: Tool Request
    Service->>Tool: Find Tool
    Tool->>Service: Tool Definition
    Service->>DB: Query Documents
    DB->>Service: Query Results
    Service->>LLM: Generate Embeddings
    LLM->>Service: Embedding Results
    Service->>AI: Formatted Response
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

**Service-Oriented Architecture**: Replaced MCP server with a clean service abstraction layer that provides search, indexing, and agent integration capabilities.

**Rune Scripting System**: Dynamic tool execution with hot-reload capabilities enables extensible tool creation without system restarts.

**Procedural Macros**: Compile-time tool generation ensures type safety and validation for custom tools.

**Layered Architecture**: Clear separation between interfaces, services, core logic, and storage enables independent development and maintenance.

**Multi-Model Database**: SurrealDB provides graph, document, and relational capabilities while DuckDB handles analytics and vector operations.

**Async-First Design**: Built on Tokio for high concurrency and non-blocking operations throughout the system.

**Trait-Based Extensibility**: Rust traits enable pluggable components and easy testing while maintaining performance.

**Plugin System**: Rune scripting allows dynamic extensibility without compromising core system security or performance.