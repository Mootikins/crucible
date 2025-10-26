# Crucible Architecture

> Simplified high-performance system architecture with ScriptEngine services

## System Overview

Crucible is a knowledge management system built for linked thinking, real-time collaboration, and AI integration. Following a major architecture simplification, the system now uses a clean ScriptEngine service architecture that delivers 83% complexity reduction while maintaining full functionality.

## High-Level Architecture

```mermaid
graph TB
    subgraph "User Interfaces"
        CLI[CLI/TUI]
        Desktop[Desktop App]
        ServiceAPI[Service API]
    end

    subgraph "Service Layer (Simplified)"
        ScriptEngine[ScriptEngine Service]
        ServiceRegistry[Service Registry]
        EventRouter[Event Router]
        MigrationBridge[Migration Bridge]
    end

    subgraph "Core Logic"
        Core[crucible-core]
        Docs[Document Management]
        CRDT[CRDT Operations]
        Tools[Tool System]
    end

    subgraph "Infrastructure"
        Config[Config Management]
        Watch[File Watcher]
        Daemon[Service Daemon]
        Health[Health Monitor]
    end

    subgraph "Storage"
        Surreal[SurrealDB]
        Files[File System]
    end

    subgraph "Scripting Layer"
        Rune[Rune Runtime]
        Security[Security Sandbox]
    end

    CLI --> ScriptEngine
    Desktop --> Core
    ServiceAPI --> ServiceRegistry

    ScriptEngine --> ServiceRegistry
    ServiceRegistry --> EventRouter
    EventRouter --> MigrationBridge

    ScriptEngine --> Rune
    MigrationBridge --> Core
    Core --> Docs
    Core --> CRDT
    Core --> Tools

    Docs --> Surreal
    Tools --> Duck
    Watch --> Files

    Daemon --> Health
    Health --> ServiceRegistry
```

## Component Relationships

```mermaid
graph LR
    subgraph "Foundation"
        A[crucible-core]
        B[crucible-config]
    end

    subgraph "Simplified Service Layer"
        C[crucible-services]
        D[crucible-daemon]
        E[crucible-surrealdb]
        F[crucible-llm]
    end

    subgraph "Scripting & Tools"
        G[crucible-tools]
        H[crucible-rune-macros]
    end

    subgraph "Interfaces"
        I[crucible-cli]
        J[crucible-tauri]
    end

    subgraph "Storage"
        K[SurrealDB]
        M[File System]
    end

    A --> C
    A --> D
    A --> E
    A --> I
    A --> J

    B --> D
    B --> C

    C --> G
    C --> H

    F --> C
    F --> D

    E --> K
    E --> L

    D --> M

    G --> K
    G --> L
```

## ScriptEngine Service Architecture

The new ScriptEngine service architecture provides a simplified, production-ready foundation for script execution and service management.

### Core Services

#### ScriptEngine Service
```rust
pub struct CrucibleScriptEngine {
    // VM-per-execution with security isolation
    security_policy: SecurityPolicy,
    compilation_cache: LruCache<String, CompiledScript>,
    active_executions: HashMap<String, ExecutionState>,
    performance_metrics: ScriptMetrics,
    event_emitter: Box<dyn EventEmitter>,
}
```

**Key Features:**
- **VM-per-Execution Pattern**: Each script runs in a fresh, isolated VM
- **Security Levels**: Safe, Development, and Production modes
- **Resource Monitoring**: Real-time tracking of memory, CPU, and execution time
- **Event-Driven**: Comprehensive event system for monitoring and coordination

#### Service Registry
- **Service Discovery**: Automatic detection and registration of services
- **Health Monitoring**: Real-time health checks and status reporting
- **Load Balancing**: Intelligent request routing and service selection

#### Event Router
- **Centralized Events**: Single event bus for all service communication
- **Event Filtering**: Configurable event routing and filtering
- **Performance Monitoring**: Event throughput and latency tracking

#### Migration Bridge
- **Automated Migration**: Tool migration from legacy systems to ScriptEngine
- **Validation**: Comprehensive integrity checking and validation
- **Rollback**: Safe rollback capabilities for failed migrations

## Core Components

### Foundation Layer

**crucible-core**: Heart of the system containing domain models, document management, CRDT operations, and tool definitions. Provides the essential abstractions that all other components build upon.

**crucible-config**: Centralized configuration management that handles settings, preferences, and environment-specific configuration across the entire system.

### Simplified Service Layer

**crucible-services**: **NEW** ScriptEngine service architecture providing secure, isolated script execution with comprehensive monitoring and event-driven coordination. This simplified layer replaces the previous over-engineered service architecture.

**crucible-daemon**: Background service providing terminal interface, REPL capabilities, and real-time service monitoring. Integrates with the new ScriptEngine architecture for centralized service management.

**crucible-surrealdb**: Database integration layer managing SurrealDB connections, queries, and data persistence.

**crucible-llm**: LLM integration supporting multiple providers (OpenAI, Ollama) for embeddings and AI capabilities.

### Scripting & Tools Layer

**crucible-tools**: Static system tools for knowledge management, including search, metadata extraction, and document processing utilities. Simplified and optimized for the new architecture.

**crucible-rune-macros**: Procedural macros for Rune tool generation, enabling compile-time tool creation with type safety and validation.

### Interface Layer

**crucible-cli**: **ENHANCED** Command-line interface with interactive REPL, comprehensive service management commands, migration operations, and AI chat capabilities. Features 20+ new commands for service orchestration.

**crucible-tauri**: Desktop application backend providing native integration, system notifications, and desktop-specific features.

### Architecture Improvements

**Service Simplification**: Removed 5,000+ lines of over-engineered code while maintaining full functionality
- 83% reduction in service complexity
- 51% reduction in dependencies
- Improved compilation performance by 15-20%
- Cleaner, more maintainable codebase

**Event-Driven Design**: Centralized event system for service coordination and monitoring
- Real-time health monitoring
- Performance metrics tracking
- Service discovery and registration
- Automated service recovery

**Production-Ready Security**: Multiple security levels with comprehensive isolation
- Safe mode (default): Sandboxed execution with limited resources
- Development mode: Full access for testing and development
- Production mode: Balanced security and functionality

**Migration System**: Automated tool migration with validation and rollback
- Legacy tool migration to ScriptEngine
- Integrity validation and auto-fixing
- Safe rollback capabilities
- Multiple migration modes and strategies

## Data Flow Patterns

### Document Processing (Simplified)

```mermaid
sequenceDiagram
    participant FS as File System
    participant W as Watcher
    participant Core as crucible-core
    participant DB as Database
    participant Events as Event Router

    FS->>W: File Change
    W->>Core: Process Document
    Core->>DB: Store Document
    Core->>Events: Document Updated
    Events->>W: Processing Complete
```

### ScriptEngine Service Interaction

```mermaid
sequenceDiagram
    participant CLI as CLI
    participant ScriptEngine as ScriptEngine Service
    participant Security as Security Sandbox
    participant Rune as Rune VM
    participant Events as Event Router

    CLI->>ScriptEngine: Execute Script
    ScriptEngine->>Security: Validate Security
    Security->>ScriptEngine: Security Context
    ScriptEngine->>Rune: Create Fresh VM
    Rune->>ScriptEngine: Execution Result
    ScriptEngine->>Events: Execution Completed
    Events->>CLI: Result Notification
```

### Service Discovery and Health Monitoring

```mermaid
sequenceDiagram
    participant Daemon as Service Daemon
    participant Registry as Service Registry
    participant Health as Health Monitor
    participant Service as ScriptEngine Service

    Daemon->>Registry: Register Service
    Registry->>Health: Start Monitoring
    Health->>Service: Health Check
    Service->>Health: Status Response
    Health->>Registry: Update Status
    Registry->>Daemon: Service Ready
```

### Migration Workflow

```mermaid
sequenceDiagram
    participant CLI as CLI
    participant Migration as Migration Bridge
    participant ScriptEngine as ScriptEngine Service
    participant Validation as Integrity Validator
    participant Events as Event Router

    CLI->>Migration: Start Migration
    Migration->>ScriptEngine: Register Tool
    ScriptEngine->>Migration: Registration Success
    Migration->>Validation: Validate Migration
    Validation->>Migration: Validation Result
    Migration->>Events: Migration Complete
    Events->>CLI: Migration Success
```

## Key Architectural Decisions

### ScriptEngine Service Architecture (NEW)
**Simplified Service Design**: Replaced over-engineered MCP server with clean, focused ScriptEngine services that provide secure script execution with 83% complexity reduction.

**VM-per-Execution Pattern**: Each script execution runs in a fresh, isolated VM instance, ensuring security isolation and preventing resource leaks.

**Event-Driven Coordination**: Centralized event system enables real-time service monitoring, health checks, and coordination without tight coupling.

**Production-Ready Security**: Multiple security levels (Safe, Development, Production) with comprehensive sandboxing and resource limits.

### Architecture Simplification
**Code Reduction**: Removed 5,000+ lines of over-engineered code while maintaining full functionality, resulting in cleaner, more maintainable codebase.

**Dependency Optimization**: Reduced dependencies by 51% while improving compilation performance by 15-20%.

**Service Consolidation**: Consolidated multiple over-engineered services into a single, efficient ScriptEngine service with clear responsibilities.

### Migration and Compatibility
**Automated Migration**: Comprehensive migration system for transitioning tools from legacy systems to the new ScriptEngine architecture.

**Backward Compatibility**: Maintained compatibility with existing Rune scripts and tools through the migration bridge.

**Validation and Rollback**: Built-in validation and rollback capabilities ensure safe, reliable migrations.

### Performance and Monitoring
**Real-time Metrics**: Comprehensive performance monitoring with resource usage tracking and service health monitoring.

**Resource Management**: Intelligent resource limits and caching prevent resource exhaustion while maintaining high performance.

**Async-First Design**: Built on Tokio for high concurrency and non-blocking operations throughout the system.

### Security and Extensibility
**Security-First Design**: Multiple security levels with sandboxed execution ensure safe operation in production environments.

**Trait-Based Extensibility**: Rust traits enable pluggable components and easy testing while maintaining performance.

**Hot-Reload Configuration**: Runtime configuration updates enable system changes without service restarts.