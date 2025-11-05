# Crucible Data Processing Pipeline - Architectural Diagrams

This document contains comprehensive architectural diagrams for Crucible's data processing pipeline using Mermaid syntax. These diagrams serve as technical documentation for developers working on the system.

## Table of Contents
1. [High-Level System Architecture](#1-high-level-system-architecture)
2. [Data Processing Pipeline Flow](#2-data-processing-pipeline-flow)
3. [Database Schema Diagram](#3-database-schema-diagram)
4. [Component Interaction Diagram](#4-component-interaction-diagram)
5. [Performance and Scaling Diagram](#5-performance-and-scaling-diagram)

---

## 1. High-Level System Architecture

### Overall System Components and Relationships

```mermaid
graph TB
    subgraph "Presentation Layer"
        CLI[CLI/TUI Application]
        Desktop[Desktop App - Tauri]
        WebUI[Web Interface - Svelte]
        Agents[Background Agents]
    end

    subgraph "Core Orchestration Layer"
        Core[crucible-core]
        CRDT[CRDT Document Engine]
        Config[Configuration Manager]
        ToolAPI[Tool & Agent Façade]
        StorageAPI[Storage Façade]
        TaskRouter[Task Router System]
    end

    subgraph "Data Processing Pipeline"
        FileWatcher[File Watcher Service]
        Scanner[Kiln Scanner]
        Parser[Document Parser]
        Queue[Transaction Queue]
        Consumer[Database Consumer]
        EmbeddingPool[Embedding Thread Pool]
    end

    subgraph "Storage Layer"
        SurrealDB[SurrealDB Database]
        RocksDB[RocksDB Storage Engine]
        VectorStore[Vector Embedding Store]
        FileSystem[File System]
    end

    subgraph "External Services"
        LLMProvider[LLM Embedding Provider]
        OllamaService[Ollama Service]
        OpenAIService[OpenAI API]
    end

    %% Data Flow Connections
    CLI --> Core
    Desktop --> Core
    WebUI --> Core
    Agents --> Core

    Core --> CRDT
    Core --> Config
    Core --> ToolAPI
    Core --> StorageAPI
    Core --> TaskRouter

    %% File Processing Flow
    FileWatcher --> Scanner
    Scanner --> Parser
    Parser --> Queue
    Queue --> Consumer
    Consumer --> StorageAPI

    %% Embedding Pipeline
    Consumer --> EmbeddingPool
    EmbeddingPool --> LLMProvider
    LLMProvider --> OllamaService
    LLMProvider --> OpenAIService
    EmbeddingPool --> VectorStore

    %% Storage Integration
    StorageAPI --> SurrealDB
    SurrealDB --> RocksDB
    VectorStore --> SurrealDB
    Scanner --> FileSystem

    %% Bidirectional Communication
    StorageAPI -.-> Core
    ToolAPI -.-> Core

    classDef presentation fill:#e1f5fe
    classDef core fill:#f3e5f5
    classDef pipeline fill:#e8f5e8
    classDef storage fill:#fff3e0
    classDef external fill:#fce4ec

    class CLI,Desktop,WebUI,Agents presentation
    class Core,CRDT,Config,ToolAPI,StorageAPI,TaskRouter core
    class FileWatcher,Scanner,Parser,Queue,Consumer,EmbeddingPool pipeline
    class SurrealDB,RocksDB,VectorStore,FileSystem storage
    class LLMProvider,OllamaService,OpenAIService external
```

### Component Architecture with Module Boundaries

```mermaid
graph LR
    subgraph "crates/crucible-cli"
        CLI_Main[main.rs]
        REPL[REPL System]
        Commands[Command Handlers]
        TUI[TUI Components]
    end

    subgraph "crates/crucible-core"
        Core_Facade[Core Façade]
        DocumentEngine[Document Engine]
        TaskRouter[Task Router]
        AgentSystem[Agent System]
    end

    subgraph "crates/crucible-surrealdb"
        DB_Client[SurrealClient]
        TransactionQueue[TransactionQueue]
        EmbeddingPipeline[Embedding Pipeline]
        KilnProcessor[Kiln Processor]
        SchemaManager[Schema Manager]
    end

    subgraph "crates/crucible-llm"
        EmbeddingProvider[Embedding Provider]
        LLM_Client[LLM Client]
        ModelRegistry[Model Registry]
    end

    subgraph "Database Layer"
        SurrealDB_Server[SurrealDB Server]
        NotesTable[notes Table]
        EmbeddingsTable[embeddings Table]
        GraphRelations[Graph Relations]
    end

    CLI_Main --> REPL
    REPL --> Commands
    Commands --> Core_Facade

    Core_Facade --> DocumentEngine
    Core_Facade --> TaskRouter
    Core_Facade --> DB_Client

    DB_Client --> TransactionQueue
    DB_Client --> EmbeddingPipeline
    DB_Client --> KilnProcessor

    EmbeddingPipeline --> EmbeddingProvider
    EmbeddingProvider --> LLM_Client

    DB_Client --> SurrealDB_Server
    SurrealDB_Server --> NotesTable
    SurrealDB_Server --> EmbeddingsTable
    SurrealDB_Server --> GraphRelations

    classDef cli fill:#e3f2fd
    classDef core fill:#f1f8e9
    classDef surreal fill:#fce4ec
    classDef llm fill:#f3e5f5
    classDef db fill:#fff8e1

    class CLI_Main,REPL,Commands,TUI cli
    class Core_Facade,DocumentEngine,TaskRouter,AgentSystem core
    class DB_Client,TransactionQueue,EmbeddingPipeline,KilnProcessor,SchemaManager surreal
    class EmbeddingProvider,LLM_Client,ModelRegistry llm
    class SurrealDB_Server,NotesTable,EmbeddingsTable,GraphRelations db
```

---

## 2. Data Processing Pipeline Flow

### Complete File Processing Pipeline

```mermaid
flowchart TD
    Start([CLI Start]) --> CheckFlags{Check CLI Flags}
    CheckFlags -->|No --no-process| FileScan[Scan Kiln Directory]
    CheckFlags -->|--no-process| SkipProcess[Skip File Processing]

    FileScan --> DiscoverFiles[Discover Markdown Files]
    DiscoverFiles --> CheckChanges{Check File Changes}
    CheckChanges -->|Unchanged Files| SkipUnchanged[Skip Unchanged]
    CheckChanges -->|New/Modified Files| ProcessFile[Process Files]

    ProcessFile --> ParseDocument[Parse Document to ParsedDocument]
    ParseDocument --> CreateTransaction[Create DatabaseTransaction]
    CreateTransaction --> Enqueue[Enqueue in TransactionQueue]

    Enqueue --> ConsumerStart[Database Consumer Thread]
    ConsumerStart --> ProcessTransaction[Process Transaction]

    ProcessTransaction --> StoreDocument[Store Document in notes Table]
    StoreDocument --> CreateRelations[Create Graph Relations]

    CreateRelations --> CreateWikiLinks[Create wikilink Relations]
    CreateWikiLinks --> CreateTags[Create tag Associations]
    CreateTags --> CreateEmbeds[Create embed Relations]

    CreateEmbeds --> GenerateEmbeddings{Generate Embeddings?}
    GenerateEmbeddings -->|Yes| EmbeddingPipeline[Process Embeddings]
    GenerateEmbeddings -->|No| UpdateTimestamp[Update processed_at]

    EmbeddingPipeline --> ChunkDocument[Chunk Document Content]
    ChunkDocument --> GenerateVectors[Generate Vector Embeddings]
    GenerateVectors --> StoreEmbeddings[Store in embeddings Table]
    StoreEmbeddings --> CreateGraphEdges[Create has_embedding Relations]
    CreateGraphEdges --> UpdateTimestamp

    UpdateTimestamp --> ProcessComplete[Processing Complete]
    SkipUnchanged --> ProcessComplete
    SkipProcess --> ProcessComplete
    ProcessComplete --> Ready([CLI Ready for Commands])

    %% Error handling paths
    ProcessFile -.-> ParseError[Parse Error]
    ParseDocument -.-> ParseError
    ProcessTransaction -.-> DBError[Database Error]
    StoreDocument -.-> DBError
    GenerateEmbeddings -.-> EmbedError[Embedding Error]

    ParseError --> LogError[Log Error & Continue]
    DBError --> LogError
    EmbedError --> LogError
    LogError --> ProcessComplete

    classDef startEnd fill:#c8e6c9
    classDef process fill:#e1f5fe
    classDef decision fill:#fff3e0
    classDef error fill:#ffcdd2
    classDef storage fill:#f1f8e9

    class Start,Ready,ProcessComplete startEnd
    class FileScan,DiscoverFiles,ProcessFile,ParseDocument,CreateTransaction,Enqueue,ConsumerStart,ProcessTransaction,StoreDocument,CreateRelations,CreateWikiLinks,CreateTags,CreateEmbeds,GenerateVectors,StoreEmbeddings,CreateGraphEdges,UpdateTimestamp,ChunkDocument process
    class CheckFlags,CheckChanges,GenerateEmbeddings decision
    class ParseError,DBError,EmbedError,LogError error
    class SkipProcess,SkipUnchanged,EmbeddingPipeline storage
```

### Transaction Queue Architecture

```mermaid
sequenceDiagram
    participant File as File Scanner
    participant Queue as TransactionQueue
    participant Consumer as DatabaseConsumer
    participant DB as SurrealDB
    participant Embed as EmbeddingPool
    participant LLM as LLM Provider

    Note over File,LLM: Simplified CRUD Transaction Processing

    File->>Queue: enqueue(DatabaseTransaction::Create)
    Note over Queue: Transaction with priority 1 (high)

    Queue->>Consumer: (Transaction, ResultSender)
    Note over Consumer: Single-threaded processing

    Consumer->>DB: Check if document exists
    DB-->>Consumer: Document status

    alt Document doesn't exist
        Consumer->>DB: CREATE notes:record_id
        Consumer->>DB: Create wikilink relations
        Consumer->>DB: Create tag relations
    else Document exists (Update)
        Consumer->>DB: UPDATE notes:record_id
        Consumer->>DB: Update relations as needed
    end

    Consumer->>Embed: Process document embeddings
    Embed->>LLM: Generate embeddings for chunks
    LLM-->>Embed: Vector embeddings
    Embed->>DB: Store embeddings with graph relations

    Consumer->>DB: Update processed_at timestamp
    DB-->>Consumer: Transaction complete

    Consumer-->>Queue: TransactionResult::Success
    Queue-->>File: Processing complete

    Note over File,LLM: Eliminates RocksDB lock contention
    Note over File,LLM: Parallel file processing, serialized DB writes
```

### Incremental Processing Flow

```mermaid
flowchart TD
    Start([Startup]) --> ScanDirectory[Scan Kiln Directory]
    ScanDirectory --> GetFileList[Get All Markdown Files]
    GetFileList --> QueryDB[Query Database for Existing Files]

    QueryDB --> CompareHashes{Compare Content Hashes}
    CompareHashes -->|Hash Mismatch| ProcessChanged[Mark as Changed]
    CompareHashes -->|File Not in DB| ProcessNew[Mark as New]
    CompareHashes -->|Hash Matches| SkipUnchanged[Skip Processing]

    ProcessChanged --> DeleteOldEmbeddings[Delete Old Embeddings]
    ProcessNew --> CreateTransaction[Create Transaction]
    DeleteOldEmbeddings --> CreateTransaction

    CreateTransaction --> BatchTransactions[Batch Related Transactions]
    BatchTransactions --> ProcessBatch[Process Batch in Consumer]
    ProcessBatch --> UpdateHash[Update Content Hash]
    UpdateHash --> CompleteProcessing[Processing Complete]

    SkipUnchanged --> CheckTimestamp{Check Processing Timestamp}
    CheckTimestamp -->|Recently Processed| CompleteProcessing
    CheckTimestamp -->|Old Timestamp| ForceReprocess[Force Reprocess]
    ForceReprocess --> CreateTransaction

    CompleteProcessing --> Ready([Ready for User Commands])

    %% Performance optimizations
    BatchTransactions --> ParallelBatches{Multiple Batches?}
    ParallelBatches -->|Yes| ProcessParallel[Process Batches in Parallel]
    ParallelBatches -->|No| ProcessBatch
    ProcessParallel --> UpdateHash

    classDef startEnd fill:#c8e6c9
    classDef process fill:#e1f5fe
    classDef decision fill:#fff3e0
    classDef optimization fill:#e8f5e8

    class Start,Ready,CompleteProcessing startEnd
    class ScanDirectory,GetFileList,QueryDB,ProcessChanged,ProcessNew,DeleteOldEmbeddings,CreateTransaction,BatchTransactions,ProcessBatch,UpdateHash,ForceReprocess,ProcessParallel process
    class CompareHashes,CheckTimestamp,ParallelBatches decision
    class SkipUnchanged optimization
```

---

## 3. Database Schema Diagram

### SurrealDB Tables and Relationships

```mermaid
erDiagram
    notes {
        string id PK
        string path UK
        string title
        string content
        json metadata
        array tags
        string content_hash
        integer file_size
        string folder
        datetime created_at
        datetime modified_at
        datetime processed_at
    }

    embeddings {
        string id PK
        array vector
        string embedding_model
        integer chunk_size
        integer chunk_position
        string chunk_hash
        integer vector_dimensions
        datetime created_at
    }

    tags {
        string id PK
        string name
        datetime created_at
    }

    %% Graph Relations (SurrealDB relation tables)
    has_embedding {
        string id PK
        notes_id FK
        embeddings_id FK
        datetime created_at
    }

    wikilink {
        string id PK
        from_note_id FK
        to_note_id FK
        string link_text
        integer position
        datetime created_at
    }

    embeds {
        string id PK
        from_note_id FK
        to_note_id FK
        string embed_type
        string reference_target
        string display_alias
        integer position
        datetime created_at
    }

    tagged_with {
        string id PK
        notes_id FK
        tag_id FK
        datetime added_at
    }

    %% Relationships
    notes ||--o{ has_embedding : "has"
    notes ||--o{ wikilink : "links_to"
    notes ||--o{ embeds : "embeds"
    notes ||--o{ tagged_with : "tagged_with"
    embeddings ||--o{ has_embedding : "embedded_in"
    tags ||--o{ tagged_with : "applies_to"

    %% Wikilink self-references
    wikilink }o--|| notes : "from"
    wikilink }o--|| notes : "to"

    %% Embeds self-references
    embeds }o--|| notes : "from"
    embeds }o--|| notes : "to"

    %% Constraints and Indexes
    %% notes: UNIQUE(path), INDEX(content_hash), INDEX(processed_at)
    %% embeddings: INDEX(chunk_hash), INDEX(embedding_model), INDEX(created_at)
    %% tags: UNIQUE(name)
```

### Data Flow Through Database Schema

```mermaid
flowchart LR
    subgraph "Input Layer"
        Markdown[Markdown File]
        ParsedDoc[ParsedDocument]
        EmbeddingVec[Vector Embedding]
    end

    subgraph "Database Tables"
        NotesTable[notes Table]
        EmbeddingsTable[embeddings Table]
        TagsTable[tags Table]
    end

    subgraph "Graph Relations"
        HasEmbedding[has_embedding Relation]
        Wikilink[wikilink Relation]
        Embeds[embeds Relation]
        TaggedWith[tagged_with Relation]
    end

    subgraph "Query Patterns"
        DocQuery[Document Retrieval]
        SemanticSearch[Semantic Search]
        GraphTraversal[Graph Traversal]
    end

    %% Document storage flow
    Markdown --> ParsedDoc
    ParsedDoc --> NotesTable

    %% Embedding flow
    ParsedDoc --> EmbeddingVec
    EmbeddingVec --> EmbeddingsTable
    NotesTable --> HasEmbedding
    EmbeddingsTable --> HasEmbedding

    %% Relation creation
    NotesTable --> Wikilink
    NotesTable --> Embeds
    NotesTable --> TaggedWith
    TagsTable --> TaggedWith

    %% Query flows
    NotesTable --> DocQuery
    EmbeddingsTable --> SemanticSearch
    HasEmbedding --> GraphTraversal
    Wikilink --> GraphTraversal
    Embeds --> GraphTraversal
    TaggedWith --> GraphTraversal

    %% Bidirectional relationships
    HasEmbedding -.-> NotesTable
    HasEmbedding -.-> EmbeddingsTable
    Wikilink -.-> NotesTable
    Embeds -.-> NotesTable
    TaggedWith -.-> NotesTable
    TaggedWith -.-> TagsTable

    classDef input fill:#e3f2fd
    classDef tables fill:#f1f8e9
    classDef relations fill:#fff3e0
    classDef queries fill:#f3e5f5

    class Markdown,ParsedDoc,EmbeddingVec input
    class NotesTable,EmbeddingsTable,TagsTable tables
    class HasEmbedding,Wikilink,Embeds,TaggedWith relations
    class DocQuery,SemanticSearch,GraphTraversal queries
```

### Vector Embedding Architecture

```mermaid
flowchart TD
    subgraph "Document Processing"
        Doc[ParsedDocument]
        Content[Plain Text Content]
        Chunker[Text Chunker]
    end

    subgraph "Embedding Generation"
        EmbedPool[Embedding Thread Pool]
        Model[Embedding Model]
        Provider[LLM Provider]
    end

    subgraph "Vector Storage"
        EmbedRecord[embeddings Record]
        VectorField[vector Array]
        MetadataFields[chunk metadata]
        GraphEdge[has_embedding Relation]
    end

    subgraph "Search & Retrieval"
        QueryVector[Query Embedding]
        SimilaritySearch[Cosine Similarity Search]
        GraphTraversal[Document Graph Traversal]
        RankedResults[Ranked Document Results]
    end

    %% Processing flow
    Doc --> Content
    Content --> Chunker
    Chunker --> EmbedPool

    EmbedPool --> Model
    Model --> Provider
    Provider --> EmbedRecord

    EmbedRecord --> VectorField
    EmbedRecord --> MetadataFields
    EmbedRecord --> GraphEdge

    %% Search flow
    QueryVector --> SimilaritySearch
    SimilaritySearch --> GraphTraversal
    GraphTraversal --> RankedResults

    VectorField --> SimilaritySearch
    GraphEdge --> GraphTraversal

    %% Configuration
    Config[EmbeddingConfig] --> EmbedPool
    Config --> Model

    classDef process fill:#e1f5fe
    classDef generation fill:#e8f5e8
    classDef storage fill:#fff3e0
    classDef search fill:#f3e5f5
    classDef config fill:#fce4ec

    class Doc,Content,Chunker process
    class EmbedPool,Model,Provider generation
    class EmbedRecord,VectorField,MetadataFields,GraphEdge storage
    class QueryVector,SimilaritySearch,GraphTraversal,RankedResults search
    class Config config
```

---

## 4. Component Interaction Diagram

### Module Interactions and Dependencies

```mermaid
graph TB
    subgraph "CLI Module (/crates/crucible-cli)"
        CLI_Main[main.rs]
        Commands[commands/]
        REPL[repl/]
        TUI[tui/]
    end

    subgraph "Core Module (/crates/crucible-core)"
        CoreLib[lib.rs]
        Document[document.rs]
        Database[database.rs]
        Parser[parser/]
        CRDT[crdt.rs]
    end

    subgraph "SurrealDB Module (/crates/crucible-surrealdb)"
        SurrealLib[lib.rs]
        KilnIntegration[kiln_integration.rs]
        TransactionQueue[transaction_queue.rs]
        TransactionConsumer[transaction_consumer.rs]
        KilnProcessor[kiln_processor.rs]
        SimpleIntegration[simple_integration.rs]
        EmbeddingConfig[embedding_config.rs]
        DatabaseClient[database.rs]
        SurrealClient[surreal_client.rs]
    end

    subgraph "LLM Module (/crates/crucible-llm)"
        LLMLib[lib.rs]
        Embeddings[embeddings/]
        TextGeneration[text_generation.rs]
    end

    subgraph "File System"
        KilnDir[Kiln Directory]
        MarkdownFiles[*.md Files]
        DatabaseFile[Database Files]
    end

    %% CLI to Core interactions
    CLI_Main --> CoreLib
    Commands --> CoreLib
    REPL --> CoreLib
    TUI --> CoreLib

    %% Core to SurrealDB interactions
    CoreLib --> SurrealLib
    Document --> KilnIntegration
    Database --> DatabaseClient
    Parser --> KilnProcessor

    %% SurrealDB internal interactions
    SurrealLib --> TransactionQueue
    SurrealLib --> TransactionConsumer
    SurrealLib --> KilnProcessor
    SurrealLib --> SimpleIntegration
    SurrealLib --> EmbeddingConfig

    TransactionQueue --> TransactionConsumer
    TransactionConsumer --> DatabaseClient
    TransactionConsumer --> EmbeddingConfig
    KilnProcessor --> SimpleIntegration
    SimpleIntegration --> TransactionQueue
    SimpleIntegration --> DatabaseClient

    DatabaseClient --> SurrealClient
    KilnIntegration --> SurrealClient
    KilnProcessor --> SurrealClient

    %% SurrealDB to LLM interactions
    EmbeddingConfig --> LLMLib
    EmbeddingConfig --> Embeddings
    Embeddings --> SurrealClient

    %% File system interactions
    KilnProcessor --> KilnDir
    KilnDir --> MarkdownFiles
    SurrealClient --> DatabaseFile

    %% Bidirectional data flows
    TransactionConsumer -.-> DatabaseClient
    KilnProcessor -.-> TransactionQueue
    CoreLib -.-> Document

    classDef cli fill:#e3f2fd
    classDef core fill:#f1f8e9
    classDef surreal fill:#fce4ec
    classDef llm fill:#f3e5f5
    classDef fs fill:#fff8e1

    class CLI_Main,Commands,REPL,TUI cli
    class CoreLib,Document,Database,Parser,CRDT core
    class SurrealLib,KilnIntegration,TransactionQueue,TransactionConsumer,KilnProcessor,SimpleIntegration,EmbeddingConfig,DatabaseClient,SurrealClient surreal
    class LLMLib,Embeddings,TextGeneration llm
    class KilnDir,MarkdownFiles,DatabaseFile fs
```

### Runtime Interaction Patterns

```mermaid
sequenceDiagram
    participant User
    participant CLI
    participant Core
    participant Scanner as KilnScanner
    participant Queue as TransactionQueue
    participant Consumer as DBConsumer
    participant DB as SurrealDB
    participant Embed as EmbeddingPool
    participant LLM as LLMProvider

    Note over User,LLM: Startup Processing Flow

    User->>CLI: crucible
    CLI->>CLI: Check --no-process flag
    CLI->>Core: initialize_core()
    Core->>DB: connect_and_migrate()
    DB-->>Core: connection ready

    CLI->>Scanner: scan_kiln_directory()
    Scanner->>Scanner: discover markdown files
    Scanner->>DB: bulk_query_document_hashes()
    DB-->>Scanner: existing file hashes
    Scanner->>Scanner: compare_hashes (incremental)

    loop For each changed file
        Scanner->>CLI: process_file_with_queue()
        CLI->>Queue: enqueue_document()
        Note over Queue: DatabaseTransaction::Create/Update
    end

    Note over User,LLM: Transaction Processing (Parallel)

    par Parallel Transaction Processing
        Queue->>Consumer: transaction_batch
        Consumer->>DB: begin_transaction()
        Consumer->>DB: store_parsed_document()
        Consumer->>DB: create_relationships()
        Consumer->>Embed: process_document_embeddings()
        Embed->>LLM: generate_embeddings()
        LLM-->>Embed: vector_embeddings
        Embed->>DB: store_embeddings()
        DB->>DB: commit_transaction()
        Consumer-->>Queue: TransactionResult::Success
    and User Commands
        User->>CLI: query command
        CLI->>Core: execute_query()
        Core->>DB: semantic_search()
        DB-->>Core: search_results
        Core-->>CLI: formatted_results
        CLI-->>User: results_display
    end

    Note over User,LLM: Graceful Error Handling

    alt Embedding Service Unavailable
        Embed->>Embed: circuit_breaker_open()
        Embed-->>Consumer: EmbeddingError
        Consumer->>DB: continue_without_embeddings()
        Consumer-->>Queue: TransactionResult::Success
    else Database Lock Contention
        Queue->>Queue: wait_for_capacity()
        Queue->>Consumer: retry_transaction()
        Consumer-->>Queue: TransactionResult::Success
    end
```

### Error Handling and Recovery Patterns

```mermaid
flowchart TD
    Start([Operation Start]) --> TryOperation[Attempt Operation]
    TryOperation --> Success{Operation Success?}
    Success -->|Yes| LogSuccess[Log Success]
    Success -->|No| CheckRetry{Can Retry?}

    CheckRetry -->|Yes| IncrementRetry[Increment Retry Count]
    CheckRetry -->|No| LogFailure[Log Final Failure]

    IncrementRetry --> CheckMaxRetry{Max Retries Reached?}
    CheckMaxRetry -->|Yes| LogFailure
    CheckMaxRetry -->|No| CalculateDelay[Calculate Backoff Delay]

    CalculateDelay --> ExponentialBackoff[Exponential Backoff]
    ExponentialBackoff --> WaitDelay[Wait Delay]
    WaitDelay --> TryOperation

    LogSuccess --> Continue[Continue Processing]
    LogFailure --> HandleFailure[Handle Failure]

    HandleFailure --> CircuitBreaker{Circuit Breaker Active?}
    CircuitBreaker -->|Yes| SkipOperation[Skip Operation]
    CircuitBreaker -->|No| GracefulDegradation[Graceful Degradation]

    SkipOperation --> Continue
    GracefulDegradation --> Continue

    %% Specific error handling paths
    TryOperation --> ParseError[Parse Error?]
    TryOperation --> DBError[Database Error?]
    TryOperation --> EmbedError[Embedding Error?]
    TryOperation --> NetworkError[Network Error?]

    ParseError --> LogParseError[Log Parse Error]
    DBError --> CheckDBLock{Database Lock?}
    EmbedError --> CheckEmbedService{Embedding Service Down?}
    NetworkError --> CheckRetry

    LogParseError --> Continue
    CheckDBLock -->|Yes| WaitLock[Wait and Retry]
    CheckDBLock -->|No| CheckRetry
    CheckEmbedService -->|Yes| SkipEmbeddings[Skip Embeddings]
    CheckEmbedService -->|No| CheckRetry
    SkipEmbeddings --> Continue
    WaitLock --> TryOperation

    classDef startEnd fill:#c8e6c9
    classDef process fill:#e1f5fe
    classDef decision fill:#fff3e0
    classDef error fill:#ffcdd2
    classDef recovery fill:#e8f5e8

    class Start,Continue startEnd
    class TryOperation,IncrementRetry,CalculateDelay,ExponentialBackoff,WaitDelay,LogSuccess,LogFailure,HandleFailure,GracefulDegradation,SkipOperation,WaitLock,SkipEmbeddings process
    class Success,CheckRetry,CheckMaxRetry,CircuitBreaker decision
    class ParseError,DBError,EmbedError,NetworkError,LogParseError,CheckDBLock,CheckEmbedService error
    class Continue recovery
```

---

## 5. Performance and Scaling Diagram

### Bottleneck Points and Optimization Opportunities

```mermaid
flowchart TD
    Start([User Command]) --> FileIO[File I/O Operations]
    FileIO --> ParseBottleneck{Parsing Bottleneck}
    ParseBottleneck -->|Large Files| ParseOpt[Chunked Parsing]
    ParseBottleneck -->|Many Files| ParallelParse[Parallel Parsing]

    ParseOpt --> QueueBottleneck{Queue Bottleneck}
    ParallelParse --> QueueBottleneck

    QueueBottleneck -->|High Throughput| BatchQueue[Batch Processing]
    QueueBottleneck -->|Lock Contention| SingleThread[Single Thread Consumer]

    BatchQueue --> DBBottleneck{Database Bottleneck}
    SingleThread --> DBBottleneck

    DBBottleneck -->|RocksDB Locks| TransactionOpt[Transaction Optimization]
    DBBottleneck -->|Large Embeddings| VectorOpt[Vector Storage Optimization]
    DBBottleneck -->|Complex Queries| IndexOpt[Query Indexing]

    TransactionOpt --> EmbedBottleneck{Embedding Bottleneck}
    VectorOpt --> EmbedBottleneck
    IndexOpt --> EmbedBottleneck

    EmbedBottleneck -->|LLM API Limits| LocalEmbed[Local Models]
    EmbedBottleneck -->|Memory Usage| EmbedPool[Embedding Thread Pool]
    EmbedBottleneck -->|Network Latency| BatchEmbed[Batch Embedding]

    LocalEmbed --> SearchBottleneck{Search Performance}
    EmbedPool --> SearchBottleneck
    BatchEmbed --> SearchBottleneck

    SearchBottleneck -->|Large Vector Set| VectorIndex[Vector Indexing]
    SearchBottleneck -->|Complex Queries| QueryCache[Query Caching]
    SearchBottleneck -->|Real-time Needs| Precompute[Precomputed Results]

    VectorIndex --> Complete([Optimized Performance])
    QueryCache --> Complete
    Precompute --> Complete

    %% Performance monitoring points
    FileIO -.-> Monitor1[Monitor: File I/O Latency]
    ParseBottleneck -.-> Monitor2[Monitor: Parse Throughput]
    QueueBottleneck -.-> Monitor3[Monitor: Queue Depth]
    DBBottleneck -.-> Monitor4[Monitor: DB Transaction Time]
    EmbedBottleneck -.-> Monitor5[Monitor: Embedding Latency]
    SearchBottleneck -.-> Monitor6[Monitor: Query Response Time]

    classDef bottleneck fill:#ffcdd2
    classDef optimization fill:#c8e6c9
    classDef monitoring fill:#e1f5fe
    classDef flow fill:#f5f5f5

    class ParseBottleneck,QueueBottleneck,DBBottleneck,EmbedBottleneck,SearchBottleneck bottleneck
    class ParseOpt,ParallelParse,BatchQueue,SingleThread,TransactionOpt,VectorOpt,IndexOpt,LocalEmbed,EmbedPool,BatchEmbed,VectorIndex,QueryCache,Precompute optimization
    class Monitor1,Monitor2,Monitor3,Monitor4,Monitor5,Monitor6 monitoring
    class Start,Complete flow
```

### Resource Utilization and Scaling Patterns

```mermaid
graph TB
    subgraph "CPU Utilization"
        CPU_1[File Scanning]
        CPU_2[Document Parsing]
        CPU_3[Transaction Processing]
        CPU_4[Embedding Generation]
        CPU_5[Query Processing]
    end

    subgraph "Memory Utilization"
        Mem_1[File Buffers]
        Mem_2[Document Cache]
        Mem_3[Queue Memory]
        Mem_4[Vector Cache]
        Mem_5[Database Cache]
    end

    subgraph "I/O Patterns"
        IO_1[File System Read]
        IO_2[Database Writes]
        IO_3[Database Reads]
        IO_4[Network Requests]
        IO_5[Index Updates]
    end

    subgraph "Concurrency Model"
        Thread_1[Main Thread]
        Thread_2[Scanner Thread]
        Thread_3[Consumer Thread]
        Thread_4[Embedding Pool]
        Thread_5[Background Tasks]
    end

    subgraph "Scaling Considerations"
        Scale_1[Vertical Scaling]
        Scale_2[Horizontal Scaling]
        Scale_3[Cache Scaling]
        Scale_4[Database Scaling]
        Scale_5[Network Scaling]
    end

    %% Resource relationships
    CPU_1 --> IO_1
    CPU_2 --> Mem_2
    CPU_3 --> Mem_3
    CPU_4 --> Mem_4
    CPU_5 --> Mem_5

    IO_1 --> Mem_1
    IO_2 --> Mem_5
    IO_3 --> Mem_5
    IO_4 --> Mem_4

    Thread_1 --> CPU_1
    Thread_2 --> CPU_2
    Thread_3 --> CPU_3
    Thread_4 --> CPU_4
    Thread_5 --> CPU_5

    %% Scaling relationships
    Scale_1 --> CPU_1
    Scale_1 --> CPU_2
    Scale_1 --> CPU_3
    Scale_1 --> CPU_4
    Scale_1 --> CPU_5

    Scale_2 --> Thread_4
    Scale_2 --> IO_4
    Scale_2 --> Scale_5

    Scale_3 --> Mem_2
    Scale_3 --> Mem_4
    Scale_3 --> Mem_5

    Scale_4 --> IO_2
    Scale_4 --> IO_3
    Scale_4 --> IO_5

    classDef cpu fill:#ffebee
    classDef memory fill:#e8f5e8
    classDef io fill:#e3f2fd
    classDef thread fill:#fff3e0
    classDef scaling fill:#f3e5f5

    class CPU_1,CPU_2,CPU_3,CPU_4,CPU_5 cpu
    class Mem_1,Mem_2,Mem_3,Mem_4,Mem_5 memory
    class IO_1,IO_2,IO_3,IO_4,IO_5 io
    class Thread_1,Thread_2,Thread_3,Thread_4,Thread_5 thread
    class Scale_1,Scale_2,Scale_3,Scale_4,Scale_5 scaling
```

### Performance Metrics and Monitoring

```mermaid
graph LR
    subgraph "Application Metrics"
        Latency[Response Latency]
        Throughput[Processing Throughput]
        ErrorRate[Error Rate]
        QueueDepth[Queue Depth]
        CacheHitRate[Cache Hit Rate]
    end

    subgraph "System Metrics"
        CPUUsage[CPU Usage]
        MemoryUsage[Memory Usage]
        DiskIO[Disk I/O]
        NetworkIO[Network I/O]
        DBConnections[Database Connections]
    end

    subgraph "Business Metrics"
        FilesProcessed[Files Processed]
        DocumentsIndexed[Documents Indexed]
        EmbeddingsGenerated[Embeddings Generated]
        QueriesExecuted[Queries Executed]
        UserSessions[User Sessions]
    end

    subgraph "Monitoring Stack"
        Prometheus[Prometheus]
        Grafana[Grafana Dashboards]
        Alerts[Alerting Rules]
        Logs[Structured Logs]
        Tracing[Distributed Tracing]
    end

    subgraph "Performance Targets"
        TargetLatency[< 100ms CLI Commands]
        TargetThroughput[> 1000 files/min]
        TargetErrorRate[< 1% Error Rate]
        TargetMemory[< 2GB RSS]
        TargetStartup[< 5s Startup Time]
    end

    %% Metric collection
    Latency --> Prometheus
    Throughput --> Prometheus
    ErrorRate --> Prometheus
    QueueDepth --> Prometheus
    CacheHitRate --> Prometheus

    CPUUsage --> Prometheus
    MemoryUsage --> Prometheus
    DiskIO --> Prometheus
    NetworkIO --> Prometheus
    DBConnections --> Prometheus

    FilesProcessed --> Prometheus
    DocumentsIndexed --> Prometheus
    EmbeddingsGenerated --> Prometheus
    QueriesExecuted --> Prometheus
    UserSessions --> Prometheus

    %% Monitoring visualization
    Prometheus --> Grafana
    Prometheus --> Alerts
    Prometheus --> Logs
    Prometheus --> Tracing

    %% Performance targeting
    TargetLatency -.-> Latency
    TargetThroughput -.-> Throughput
    TargetErrorRate -.-> ErrorRate
    TargetMemory -.-> MemoryUsage
    TargetStartup -.-> Latency

    classDef metrics fill:#e1f5fe
    classDef system fill:#f1f8e9
    classDef business fill:#fff3e0
    classDef monitoring fill:#f3e5f5
    classDef targets fill:#e8f5e8

    class Latency,Throughput,ErrorRate,QueueDepth,CacheHitRate metrics
    class CPUUsage,MemoryUsage,DiskIO,NetworkIO,DBConnections system
    class FilesProcessed,DocumentsIndexed,EmbeddingsGenerated,QueriesExecuted,UserSessions business
    class Prometheus,Grafana,Alerts,Logs,Tracing monitoring
    class TargetLatency,TargetThroughput,TargetErrorRate,TargetMemory,TargetStartup targets
```

## Key Architectural Insights

### Single-Binary Architecture Benefits
1. **No External Dependencies**: All processing happens in-process, eliminating daemon management overhead
2. **Incremental Processing**: Only changed files are reprocessed, ensuring fast subsequent startups
3. **Graceful Degradation**: System continues to function even if individual components fail
4. **Queue-Based Design**: Eliminates RocksDB lock contention through serialized database writes

### Performance Characteristics
- **File Processing**: Parallel file scanning with incremental change detection
- **Database Operations**: Single-threaded consumer eliminates lock contention
- **Embedding Pipeline**: Configurable thread pool with circuit breaker patterns
- **Memory Efficiency**: Streaming document processing with configurable batch sizes

### Scaling Considerations
- **Vertical Scaling**: CPU-bound operations benefit from more cores
- **Memory Scaling**: Vector embeddings require careful memory management
- **I/O Optimization**: Bulk database operations reduce query overhead
- **Network Scaling**: Local embedding models reduce external dependencies

These diagrams provide a comprehensive view of Crucible's data processing pipeline and can serve as reference documentation for developers working on system optimization, feature development, and architectural improvements.