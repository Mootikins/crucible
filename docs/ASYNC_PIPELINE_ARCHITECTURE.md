# Async Watcher → Parser → Multi-Output Pipeline Architecture

> **Status**: Design Document
> **Created**: 2025-10-19
> **Author**: System Architecture Design

## Executive Summary

This document defines the async Rust architecture for Crucible's file-watching pipeline that watches kiln files, parses markdown content, and distributes parsed data to multiple outputs (SurrealDB + Logger). The design prioritizes **zero-allocation hot paths**, **backpressure handling**, and **async-first concurrency patterns**.

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                     FILE SYSTEM (Kiln)                         │
└───────────────────────┬─────────────────────────────────────────┘
                        │
                        ▼
        ┌───────────────────────────────┐
        │   FileWatcher (notify-based)  │
        │   - Debounced events          │
        │   - Filter .md files          │
        └───────────┬───────────────────┘
                    │ FileEvent
                    │
                    ▼
        ┌───────────────────────────────┐
        │  Parser Pool (bounded)        │
        │  - Parse frontmatter          │
        │  - Extract wikilinks          │
        │  - Extract tags               │
        │  - Parse content blocks       │
        └───────────┬───────────────────┘
                    │ ParsedDocument
                    │
                    ▼
        ┌───────────────────────────────┐
        │   Broadcast/Fanout            │
        │   (tokio::sync::broadcast)    │
        └─────┬──────────────┬──────────┘
              │              │
    ┌─────────▼───┐      ┌───▼──────────┐
    │ SurrealDB   │      │   Tracing    │
    │   Sink      │      │    Sink      │
    │ (buffered)  │      │  (unbounded) │
    └─────────────┘      └──────────────┘
```

## Core Design Decisions

### 1. Concurrency Model: Channel-Based Pipeline

**Decision**: Use **bounded MPSC channels** with **broadcast for fanout**.

**Rationale**:
- **MPSC channels** provide clear ownership boundaries and backpressure semantics
- **Broadcast channel** enables zero-copy distribution to multiple sinks
- **Bounded channels** prevent memory exhaustion under load
- **Actor-based alternatives** (e.g., Actix) add complexity without compelling benefits for this use case

**Implementation**:
```rust
// Watcher → Parser: Bounded MPSC (backpressure on watcher)
let (file_tx, file_rx) = tokio::sync::mpsc::channel::<FileEvent>(256);

// Parser → Sinks: Broadcast (multi-consumer, lagging receivers drop)
let (parsed_tx, _) = tokio::sync::broadcast::channel::<ParsedDocument>(1024);
```

**Memory Analysis**:
- FileEvent queue: ~256 events × ~200 bytes = **51 KB max**
- ParsedDocument queue: ~1024 docs × ~2 KB = **2 MB max**
- **Total pipeline memory**: ~2.1 MB under full saturation

### 2. Backpressure Strategy: Layered with Graceful Degradation

**Problem**: Slow SurrealDB writes could stall the entire pipeline.

**Solution**: **Staged backpressure** with **circuit breaker pattern**.

```
FileWatcher → [Bounded MPSC] → Parser Pool → [Broadcast] → Sinks
     ↑                ↑                            ↑
     │                │                            │
  Blocks when     Bounded by      Lagging receivers drop
  channel full    CPU cores       (logger never blocks)
```

**Backpressure Tiers**:

1. **Parser → Sinks** (Broadcast channel):
   - **Logger sink**: Unbounded, never drops (tracing is fire-and-forget)
   - **DB sink**: Buffered writes with timeout
   - **Lagging behavior**: Broadcast drops oldest for slow consumers

2. **Watcher → Parser** (MPSC channel):
   - **Bounded at 256 events**
   - **Behavior**: Watcher blocks when full (natural rate limiting)
   - **Rationale**: Filesystem events are inherently bursty; blocking prevents OOM

3. **Circuit Breaker** (DB sink):
   - **Thresholds**:
     - Open circuit after 10 consecutive write failures
     - Half-open after 30s cooldown
     - Close after 3 successful writes
   - **Behavior**: Drop events when open, log errors, prevent cascading failure

**Memory-Safe Guarantee**: Pipeline memory bounded at **~2.1 MB** regardless of input rate.

### 3. Error Propagation: Fail-Fast with Isolation

**Design Principle**: **Errors in one sink must not affect others**.

**Strategy**:

```rust
pub enum PipelineError {
    /// Recoverable error - log and continue
    Transient {
        component: String,
        error: anyhow::Error,
        event: Option<FileEvent>,
    },

    /// Fatal error - shut down pipeline
    Fatal {
        component: String,
        error: anyhow::Error,
    },
}
```

**Error Handling by Stage**:

| Stage | Error Type | Action | Propagation |
|-------|-----------|--------|-------------|
| Watcher | IO error | Log + retry | No propagation |
| Parser | Malformed markdown | Log + emit ParseError event | Continue |
| DB Sink | Write timeout | Buffer + retry (max 3) | Independent from logger |
| Logger Sink | Never fails | N/A | N/A |

**Isolation Mechanism**: Each sink runs in **independent tokio task** with its own broadcast receiver.

### 4. Parsing Library: pulldown-cmark with Custom Extensions

**Comparison**:

| Library | Pros | Cons | Verdict |
|---------|------|------|---------|
| **pulldown-cmark** | Fast (zero-copy), incremental, battle-tested | CommonMark strict (needs extensions) | **SELECTED** |
| comrak | GFM support, full-featured | Allocates more, slower | Rejected |
| Custom parser | Full control | High maintenance, slower | Rejected |

**Selection**: **pulldown-cmark** + custom event processor

**Rationale**:
- **Zero-copy iteration** over markdown tokens
- **Incremental parsing** (memory-efficient for large files)
- **Extensible** via custom event processing for wikilinks
- **Performance**: Processes ~50 MB/s on typical kiln files

**Extension Strategy** (Wikilinks/Tags):
```rust
// Custom event processor wraps pulldown_cmark::Parser
struct CrucibleMarkdownParser<'a> {
    inner: pulldown_cmark::Parser<'a, 'a>,
    wikilink_regex: Regex, // Compiled once, reused
    tag_regex: Regex,
}

impl<'a> CrucibleMarkdownParser<'a> {
    fn parse(&mut self, content: &'a str) -> ParsedDocument {
        // Single-pass iteration, extract:
        // - Frontmatter (manual YAML parse before iterator)
        // - Wikilinks (regex on Link events)
        // - Tags (regex on Text events)
        // - Headings (Event::Start(Tag::Heading))
    }
}
```

### 5. Async Task Spawning Strategy: Bounded Worker Pool

**Design**: **Fixed-size parser pool** with **per-sink task**.

```rust
// Parser pool: CPU-bound, limit to num_cpus
let parser_pool_size = num_cpus::get();
for _ in 0..parser_pool_size {
    tokio::task::spawn_blocking(move || {
        // CPU-intensive parsing in blocking task
        // Prevents starving async runtime
    });
}

// Sinks: 1 task per sink (IO-bound, can use async runtime)
tokio::spawn(db_sink.run(broadcast_rx.resubscribe()));
tokio::spawn(logger_sink.run(broadcast_rx.resubscribe()));
```

**Worker Pool Sizing**:
- **Parser workers**: `num_cpus` (CPU-bound work)
- **DB sink**: 1 task (IO-bound, uses async connection pool internally)
- **Logger sink**: 1 task (fire-and-forget tracing)

**Total task count**: `num_cpus + 3` (watcher + 2 sinks)

**Memory per task**: ~2 KB stack = **~20 KB total** on 8-core system

## Data Structures

### FileEvent (Existing - Reuse)

Already defined in `/home/moot/crucible/crates/crucible-watch/src/events.rs`:

```rust
pub struct FileEvent {
    pub id: Uuid,
    pub kind: FileEventKind,
    pub path: PathBuf,
    pub timestamp: DateTime<Utc>,
    pub is_dir: bool,
    pub metadata: Option<EventMetadata>,
}
```

### ParsedDocument (New)

```rust
/// Zero-copy parsed markdown document
pub struct ParsedDocument {
    /// Original file path
    pub path: PathBuf,

    /// Parsed frontmatter (lazy-parsed on access)
    pub frontmatter: Option<Frontmatter>,

    /// Extracted wikilinks [[note]]
    pub wikilinks: Vec<Wikilink>,

    /// Extracted tags #tag
    pub tags: Vec<Tag>,

    /// Document content blocks
    pub content: DocumentContent,

    /// Parse timestamp
    pub parsed_at: DateTime<Utc>,

    /// Original event that triggered parse
    pub source_event: FileEvent,
}

/// Frontmatter with YAML/TOML support
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Frontmatter {
    /// Raw frontmatter string
    pub raw: String,

    /// Parsed key-value pairs (lazy)
    #[serde(skip)]
    pub properties: OnceCell<HashMap<String, serde_json::Value>>,

    /// Frontmatter format
    pub format: FrontmatterFormat,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FrontmatterFormat {
    Yaml,   // ---\nkey: value\n---
    Toml,   // +++\nkey = "value"\n+++
    None,
}

/// Wikilink [[target|alias]]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Wikilink {
    /// Target note name
    pub target: String,

    /// Optional alias
    pub alias: Option<String>,

    /// Character offset in source
    pub offset: usize,

    /// Whether it's an embed ![[note]]
    pub is_embed: bool,
}

/// Tag #tag or #nested/tag
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Tag {
    /// Tag name (without #)
    pub name: String,

    /// Nested path components
    pub path: Vec<String>,

    /// Character offset in source
    pub offset: usize,
}

/// Parsed content structure
#[derive(Debug, Clone)]
pub struct DocumentContent {
    /// Plain text (no markdown syntax)
    pub plain_text: String,

    /// Heading structure
    pub headings: Vec<Heading>,

    /// Code blocks (for potential indexing)
    pub code_blocks: Vec<CodeBlock>,
}

#[derive(Debug, Clone)]
pub struct Heading {
    pub level: u8,        // 1-6
    pub text: String,
    pub offset: usize,
}

#[derive(Debug, Clone)]
pub struct CodeBlock {
    pub language: Option<String>,
    pub content: String,
    pub offset: usize,
}
```

**Memory Layout Analysis**:

```
ParsedDocument size estimate:
- PathBuf:           24 bytes (SmallVec optimization)
- Frontmatter:       ~200 bytes (avg YAML frontmatter)
- Wikilinks:         ~50 bytes × 10 avg = 500 bytes
- Tags:              ~40 bytes × 5 avg = 200 bytes
- Content:           ~1 KB (plain text excerpt)
- Metadata:          ~100 bytes
─────────────────────────────────────────────
Total:               ~2 KB per document
```

**Optimization Notes**:
- `OnceCell<HashMap>` defers YAML parsing until accessed
- `plain_text` stores excerpt only (first 1000 chars for search preview)
- Full content remains on disk; this is **metadata index only**

## Trait Definitions

### FileWatcher (Existing - Extend)

Already defined in `/home/moot/crucible/crates/crucible-watch/src/traits.rs`. No changes needed.

### MarkdownParser (New)

```rust
/// Trait for parsing markdown documents
#[async_trait]
pub trait MarkdownParser: Send + Sync {
    /// Parse a markdown file from path
    async fn parse_file(&self, path: &Path) -> Result<ParsedDocument, ParserError>;

    /// Parse markdown content from string
    fn parse_content(&self, content: &str, source_path: PathBuf)
        -> Result<ParsedDocument, ParserError>;

    /// Get parser capabilities
    fn capabilities(&self) -> ParserCapabilities;
}

/// Parser capabilities and configuration
#[derive(Debug, Clone)]
pub struct ParserCapabilities {
    /// Supports frontmatter parsing
    pub frontmatter: bool,

    /// Supports wikilink extraction
    pub wikilinks: bool,

    /// Supports tag extraction
    pub tags: bool,

    /// Supports full content parsing
    pub content_parsing: bool,

    /// Maximum file size (bytes)
    pub max_file_size: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum ParserError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Frontmatter parse error: {0}")]
    FrontmatterError(String),

    #[error("File too large: {size} bytes (max {max})")]
    FileTooLarge { size: usize, max: usize },

    #[error("Invalid UTF-8 encoding")]
    EncodingError,

    #[error("Parsing failed: {0}")]
    ParseFailed(String),
}
```

### OutputSink (New)

```rust
/// Trait for pipeline output destinations
#[async_trait]
pub trait OutputSink: Send + Sync {
    /// Process a parsed document
    async fn write(&self, doc: ParsedDocument) -> Result<(), SinkError>;

    /// Flush buffered writes
    async fn flush(&self) -> Result<(), SinkError>;

    /// Get sink name (for logging/metrics)
    fn name(&self) -> &'static str;

    /// Get sink health status
    async fn health_check(&self) -> SinkHealth;

    /// Graceful shutdown
    async fn shutdown(&self) -> Result<(), SinkError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SinkHealth {
    Healthy,
    Degraded { reason: String },
    Unhealthy { reason: String },
}

#[derive(Debug, thiserror::Error)]
pub enum SinkError {
    #[error("Write failed: {0}")]
    WriteFailed(String),

    #[error("Connection error: {0}")]
    ConnectionError(String),

    #[error("Timeout after {0:?}")]
    Timeout(std::time::Duration),

    #[error("Sink closed")]
    Closed,
}

/// Concrete implementations

/// SurrealDB sink with buffering and retry logic
pub struct SurrealDBSink {
    db: Arc<surrealdb::Surreal<surrealdb::engine::local::Db>>,
    buffer: Arc<Mutex<Vec<ParsedDocument>>>,
    buffer_size: usize,
    flush_interval: Duration,
    circuit_breaker: Arc<Mutex<CircuitBreaker>>,
}

/// Tracing/logging sink (never blocks)
pub struct TracingSink {
    level: tracing::Level,
}

#[async_trait]
impl OutputSink for TracingSink {
    async fn write(&self, doc: ParsedDocument) -> Result<(), SinkError> {
        // Fire-and-forget logging
        tracing::event!(
            self.level,
            path = %doc.path.display(),
            wikilinks = doc.wikilinks.len(),
            tags = doc.tags.len(),
            "Document parsed"
        );
        Ok(())
    }

    async fn flush(&self) -> Result<(), SinkError> {
        // No-op for tracing
        Ok(())
    }

    fn name(&self) -> &'static str {
        "tracing"
    }

    async fn health_check(&self) -> SinkHealth {
        SinkHealth::Healthy // Always healthy
    }

    async fn shutdown(&self) -> Result<(), SinkError> {
        Ok(())
    }
}
```

### Circuit Breaker (New)

```rust
/// Circuit breaker for fault isolation
pub struct CircuitBreaker {
    state: CircuitState,
    failure_count: u32,
    last_failure: Option<Instant>,
    config: CircuitBreakerConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CircuitState {
    Closed,      // Normal operation
    Open,        // Failing, reject requests
    HalfOpen,    // Testing if recovered
}

pub struct CircuitBreakerConfig {
    pub failure_threshold: u32,      // Open after N failures
    pub reset_timeout: Duration,      // Time before half-open
    pub success_threshold: u32,       // Close after N successes
}

impl CircuitBreaker {
    pub fn record_success(&mut self) {
        match self.state {
            CircuitState::Closed => {
                self.failure_count = 0;
            }
            CircuitState::HalfOpen => {
                // Count successes, close if threshold met
                if self.failure_count >= self.config.success_threshold {
                    self.state = CircuitState::Closed;
                    self.failure_count = 0;
                }
            }
            CircuitState::Open => {} // Shouldn't happen
        }
    }

    pub fn record_failure(&mut self) {
        self.failure_count += 1;
        self.last_failure = Some(Instant::now());

        if self.failure_count >= self.config.failure_threshold {
            self.state = CircuitState::Open;
        }
    }

    pub fn can_execute(&mut self) -> bool {
        match self.state {
            CircuitState::Closed => true,
            CircuitState::HalfOpen => true,
            CircuitState::Open => {
                // Check if we should transition to half-open
                if let Some(last_fail) = self.last_failure {
                    if last_fail.elapsed() >= self.config.reset_timeout {
                        self.state = CircuitState::HalfOpen;
                        self.failure_count = 0;
                        return true;
                    }
                }
                false
            }
        }
    }
}
```

## Pipeline Implementation

### Complete Pipeline Struct

```rust
pub struct DocumentPipeline {
    /// Configuration
    config: PipelineConfig,

    /// File watcher
    watcher: Arc<WatchManager>,

    /// Parser pool workers
    parser_handles: Vec<JoinHandle<()>>,

    /// Output sinks
    sinks: Vec<Box<dyn OutputSink>>,

    /// Channels
    file_tx: mpsc::Sender<FileEvent>,
    parsed_tx: broadcast::Sender<ParsedDocument>,

    /// Shutdown signal
    shutdown: Arc<AtomicBool>,
}

pub struct PipelineConfig {
    pub kiln_path: PathBuf,
    pub file_buffer_size: usize,        // Default: 256
    pub parsed_buffer_size: usize,      // Default: 1024
    pub parser_workers: usize,          // Default: num_cpus
    pub db_buffer_size: usize,          // Default: 100
    pub db_flush_interval: Duration,    // Default: 5s
}

impl DocumentPipeline {
    pub async fn new(config: PipelineConfig) -> Result<Self> {
        let (file_tx, file_rx) = mpsc::channel(config.file_buffer_size);
        let (parsed_tx, _) = broadcast::channel(config.parsed_buffer_size);

        // Initialize watcher
        let watcher_config = WatchManagerConfig::default();
        let watcher = Arc::new(WatchManager::new(watcher_config).await?);

        // Spawn parser workers
        let parser_handles = Self::spawn_parsers(
            config.parser_workers,
            file_rx,
            parsed_tx.clone(),
        );

        Ok(Self {
            config,
            watcher,
            parser_handles,
            sinks: Vec::new(),
            file_tx,
            parsed_tx,
            shutdown: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn add_sink(&mut self, sink: Box<dyn OutputSink>) {
        self.sinks.push(sink);
    }

    pub async fn start(&mut self) -> Result<()> {
        // Start watcher with event forwarding
        let file_tx = self.file_tx.clone();
        let kiln_path = self.config.kiln_path.clone();

        // Configure watcher to send events to our channel
        let watch_config = WatchConfig::new("kiln-watch")
            .with_recursive(true)
            .with_filter(
                EventFilter::new()
                    .with_extension("md")
                    .with_extension("markdown")
            );

        self.watcher.add_watch(kiln_path, watch_config).await?;

        // Spawn sink tasks
        for sink in &self.sinks {
            let sink_clone = sink.clone(); // Requires Arc<dyn OutputSink>
            let mut rx = self.parsed_tx.subscribe();
            let shutdown = Arc::clone(&self.shutdown);

            tokio::spawn(async move {
                while !shutdown.load(Ordering::Relaxed) {
                    match rx.recv().await {
                        Ok(doc) => {
                            if let Err(e) = sink_clone.write(doc).await {
                                tracing::error!(
                                    sink = sink_clone.name(),
                                    error = %e,
                                    "Sink write failed"
                                );
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!(
                                sink = sink_clone.name(),
                                lagged = n,
                                "Sink lagging, dropped events"
                            );
                        }
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
            });
        }

        Ok(())
    }

    pub async fn shutdown(&mut self) -> Result<()> {
        self.shutdown.store(true, Ordering::Relaxed);

        // Flush all sinks
        for sink in &self.sinks {
            let _ = sink.flush().await;
            let _ = sink.shutdown().await;
        }

        // Wait for parser workers
        for handle in self.parser_handles.drain(..) {
            let _ = handle.await;
        }

        Ok(())
    }

    fn spawn_parsers(
        count: usize,
        mut file_rx: mpsc::Receiver<FileEvent>,
        parsed_tx: broadcast::Sender<ParsedDocument>,
    ) -> Vec<JoinHandle<()>> {
        (0..count)
            .map(|id| {
                let mut rx = file_rx.clone(); // Clone receiver for sharing
                let tx = parsed_tx.clone();

                tokio::task::spawn_blocking(move || {
                    let parser = CrucibleMarkdownParser::new();
                    let runtime = tokio::runtime::Handle::current();

                    while let Some(event) = runtime.block_on(rx.recv()) {
                        match parser.parse_file(&event.path) {
                            Ok(doc) => {
                                let _ = tx.send(doc);
                            }
                            Err(e) => {
                                tracing::error!(
                                    worker = id,
                                    path = %event.path.display(),
                                    error = %e,
                                    "Parse failed"
                                );
                            }
                        }
                    }
                })
            })
            .collect()
    }
}
```

## Performance Characteristics

### Throughput Estimates

**Single-threaded parsing** (pulldown-cmark):
- **50 MB/s** for typical markdown
- **~1ms per 50 KB note**

**Pipeline throughput** (8-core system):
- **Parser pool**: 8 workers × 50 notes/s = **400 notes/s**
- **DB sink**: ~100 writes/s (batched, SurrealDB local)
- **Bottleneck**: DB writes

**Expected throughput**: **~100 notes/s** (DB-bound)

### Latency Targets

| Stage | P50 | P99 | Max |
|-------|-----|-----|-----|
| File event → Parser | 10ms | 50ms | 200ms |
| Parse → DB write | 50ms | 200ms | 1s |
| Parse → Log | 1ms | 5ms | 10ms |
| **End-to-end** | **60ms** | **250ms** | **1.2s** |

### Memory Footprint

| Component | Memory |
|-----------|--------|
| File event queue | 51 KB |
| Parsed doc queue | 2 MB |
| Parser workers | 20 KB |
| DB connection pool | ~1 MB |
| Total | **~3.1 MB** |

**Scalability**: Memory bounded regardless of kiln size.

## Testing Strategy

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_parser_wikilinks() {
        let parser = CrucibleMarkdownParser::new();
        let content = "See [[Note A]] and [[Note B|Alias]]";
        let doc = parser.parse_content(content, PathBuf::from("test.md")).unwrap();

        assert_eq!(doc.wikilinks.len(), 2);
        assert_eq!(doc.wikilinks[0].target, "Note A");
        assert_eq!(doc.wikilinks[1].alias, Some("Alias".to_string()));
    }

    #[tokio::test]
    async fn test_frontmatter_yaml() {
        let parser = CrucibleMarkdownParser::new();
        let content = "---\ntitle: Test\ntags: [a, b]\n---\nContent";
        let doc = parser.parse_content(content, PathBuf::from("test.md")).unwrap();

        assert!(doc.frontmatter.is_some());
        let fm = doc.frontmatter.unwrap();
        assert_eq!(fm.format, FrontmatterFormat::Yaml);
    }

    #[tokio::test]
    async fn test_circuit_breaker() {
        let mut cb = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 3,
            reset_timeout: Duration::from_secs(1),
            success_threshold: 2,
        });

        assert!(cb.can_execute());
        cb.record_failure();
        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.state, CircuitState::Open);
        assert!(!cb.can_execute());
    }
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_pipeline_end_to_end() {
    let temp_dir = tempfile::tempdir().unwrap();
    let kiln_path = temp_dir.path().to_path_buf();

    // Create test kiln
    let note_path = kiln_path.join("test.md");
    tokio::fs::write(&note_path, "---\ntitle: Test\n---\nSee [[Other]]").await.unwrap();

    // Set up pipeline
    let config = PipelineConfig {
        kiln_path: kiln_path.clone(),
        ..Default::default()
    };

    let mut pipeline = DocumentPipeline::new(config).await.unwrap();
    let (tx, mut rx) = mpsc::channel(10);

    pipeline.add_sink(Box::new(TestSink { tx }));
    pipeline.start().await.unwrap();

    // Wait for parse
    let doc = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .unwrap()
        .unwrap();

    assert_eq!(doc.wikilinks.len(), 1);
    assert_eq!(doc.wikilinks[0].target, "Other");

    pipeline.shutdown().await.unwrap();
}

struct TestSink {
    tx: mpsc::Sender<ParsedDocument>,
}

#[async_trait]
impl OutputSink for TestSink {
    async fn write(&self, doc: ParsedDocument) -> Result<(), SinkError> {
        self.tx.send(doc).await.map_err(|_| SinkError::Closed)
    }
    // ... other trait methods
}
```

### Load Tests

```rust
// Benchmark: 1000 notes, concurrent modifications
#[tokio::test]
async fn bench_high_volume_parsing() {
    let kiln = create_test_kiln(1000).await;
    let pipeline = setup_pipeline(&kiln).await;

    let start = Instant::now();
    simulate_concurrent_edits(&kiln, 100).await;
    wait_for_pipeline_idle(&pipeline, Duration::from_secs(10)).await;

    let elapsed = start.elapsed();
    println!("Processed 1000 notes in {:?}", elapsed);
    assert!(elapsed < Duration::from_secs(15)); // < 15s for 1000 notes
}
```

## Migration Path

### Phase 1: Core Infrastructure (Week 1)
- Implement `ParsedDocument`, `Wikilink`, `Tag` types
- Implement `MarkdownParser` trait + `CrucibleMarkdownParser`
- Unit tests for parser

### Phase 2: Pipeline Skeleton (Week 2)
- Implement `OutputSink` trait
- Implement `TracingSink` (simple, no DB)
- Wire watcher → parser → logger
- Integration tests

### Phase 3: Database Integration (Week 3)
- Implement `SurrealDBSink` with buffering
- Implement circuit breaker
- Add DB to pipeline
- Load tests

### Phase 4: Optimization (Week 4)
- Profile memory usage
- Optimize hot paths
- Add metrics/observability
- Production hardening

## Open Questions

1. **Incremental parsing**: Should we parse only changed sections for large files?
   - **Recommendation**: Defer to Phase 4. Full reparse is simpler and fast enough (<10ms for typical notes).

2. **Schema evolution**: How to handle frontmatter schema changes?
   - **Recommendation**: Store raw YAML in SurrealDB, parse on read. Decouples DB from frontmatter structure.

3. **Embedding generation**: Where in pipeline?
   - **Recommendation**: Separate pipeline stage (Parser → Embedder → DB). Embeddings are slow (~500ms), would block main pipeline.

4. **File deletion handling**: Cascade deletes in DB?
   - **Recommendation**: Yes. On `FileEventKind::Deleted`, delete from DB and remove orphaned links.

## References

- [Existing WatchManager](/home/moot/crucible/crates/crucible-watch/src/manager.rs)
- [FileEvent types](/home/moot/crucible/crates/crucible-watch/src/events.rs)
- [pulldown-cmark documentation](https://docs.rs/pulldown-cmark)
- [Tokio broadcast channel](https://docs.rs/tokio/latest/tokio/sync/broadcast/index.html)
- [Circuit Breaker Pattern](https://learn.microsoft.com/en-us/azure/architecture/patterns/circuit-breaker)

---

**Next Steps**: Implement Phase 1 (Core Infrastructure) and validate parser performance with real kiln data.
