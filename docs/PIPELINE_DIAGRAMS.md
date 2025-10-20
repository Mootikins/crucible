# Async Pipeline Architecture - Visual Diagrams

> Supplementary visual documentation for the async pipeline architecture

## High-Level Data Flow

```
┌─────────────────────────────────────────────────────────────────┐
│                      FILE SYSTEM (Vault)                        │
│  ~/vault/Projects/crucible.md modified                          │
└──────────────────────────┬──────────────────────────────────────┘
                           │ (inotify/FSEvents)
                           ▼
        ┌──────────────────────────────────────┐
        │   FileWatcher (notify-debouncer)     │
        │   - Debounce: 100ms                  │
        │   - Filter: *.md only                │
        │   - Event: FileEvent                 │
        └───────────────┬──────────────────────┘
                        │ mpsc::channel (bounded: 256)
                        │ FileEvent { path, kind, timestamp }
                        ▼
        ┌──────────────────────────────────────┐
        │   Parser Pool (spawn_blocking)       │
        │   Workers: num_cpus (e.g., 8)        │
        │   - Read file content                │
        │   - Extract frontmatter (YAML/TOML)  │
        │   - Parse wikilinks [[note]]         │
        │   - Extract tags #tag                │
        │   - Parse content structure          │
        └───────────────┬──────────────────────┘
                        │ broadcast::channel (1024)
                        │ ParsedDocument
                        │
            ┌───────────┴───────────┐
            │    Broadcast Fanout   │
            │  (zero-copy to sinks) │
            └───┬───────────────┬───┘
                │               │
       ┌────────▼─────┐    ┌────▼──────────┐
       │  DB Sink     │    │ Logger Sink   │
       │ (SurrealDB)  │    │ (tracing)     │
       │              │    │               │
       │ - Buffer     │    │ - Unbounded   │
       │ - Batch      │    │ - Never drops │
       │ - Retry      │    │ - Fire & forget│
       │ - Circuit    │    │               │
       │   breaker    │    │               │
       └──────┬───────┘    └───────────────┘
              │
              ▼
    ┌─────────────────┐
    │  SurrealDB      │
    │  - notes table  │
    │  - links edges  │
    │  - tags index   │
    └─────────────────┘
```

## Memory Layout

```
┌─────────────────────────────────────────────────────────────────┐
│                      PIPELINE MEMORY BUDGET                      │
│                     Total: ~2.1 MB bounded                       │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌────────────────────────────────────────────────────┐         │
│  │  File Event Queue (MPSC channel)                   │         │
│  │  Capacity: 256 events                              │         │
│  │  Size per event: ~200 bytes                        │         │
│  │  Total: 51 KB                                      │         │
│  └────────────────────────────────────────────────────┘         │
│                           ↓                                      │
│  ┌────────────────────────────────────────────────────┐         │
│  │  Parsed Document Queue (Broadcast channel)         │         │
│  │  Capacity: 1024 documents                          │         │
│  │  Size per doc: ~2 KB                               │         │
│  │  Total: 2 MB                                       │         │
│  │                                                     │         │
│  │  ParsedDocument breakdown:                         │         │
│  │    PathBuf:       24 bytes                         │         │
│  │    Frontmatter:  200 bytes (avg)                   │         │
│  │    Wikilinks:    500 bytes (10 × 50)               │         │
│  │    Tags:         200 bytes (5 × 40)                │         │
│  │    Content:      1 KB (excerpt)                    │         │
│  │    Metadata:     100 bytes                         │         │
│  └────────────────────────────────────────────────────┘         │
│                           ↓                                      │
│  ┌────────────────────────────────────────────────────┐         │
│  │  Task Overhead                                     │         │
│  │  Parser workers: num_cpus × 2 KB = 16 KB          │         │
│  │  Sink tasks: 2 × 2 KB = 4 KB                      │         │
│  │  Total: ~20 KB                                     │         │
│  └────────────────────────────────────────────────────┘         │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

## Concurrency Model

```
┌─────────────────────────────────────────────────────────────────┐
│                      TOKIO RUNTIME                              │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  Main Task                                                       │
│  ├─ FileWatcher (async)                                         │
│  │   └─ notify-debouncer (background thread)                    │
│  │                                                               │
│  ├─ Parser Pool (blocking tasks)                                │
│  │   ├─ Worker 1 ───┐                                           │
│  │   ├─ Worker 2 ───┤                                           │
│  │   ├─ Worker 3 ───┤ spawn_blocking                            │
│  │   ├─ Worker 4 ───┤ (CPU-bound work)                          │
│  │   ├─ Worker 5 ───┤                                           │
│  │   ├─ Worker 6 ───┤                                           │
│  │   ├─ Worker 7 ───┤                                           │
│  │   └─ Worker 8 ───┘                                           │
│  │                                                               │
│  ├─ DB Sink Task (async)                                        │
│  │   └─ SurrealDB connection pool                               │
│  │                                                               │
│  └─ Logger Sink Task (async)                                    │
│      └─ tracing subscriber                                      │
│                                                                  │
│  Total: 1 + num_cpus + 2 tasks                                  │
│         = 11 tasks on 8-core system                             │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

## Backpressure Handling

```
┌─────────────────────────────────────────────────────────────────┐
│                    BACKPRESSURE LAYERS                          │
└─────────────────────────────────────────────────────────────────┘

Scenario: Slow DB Writes

1. Normal Operation
   ┌─────────┐   fast   ┌────────┐   fast   ┌──────┐
   │ Watcher │ ──────→  │ Parser │ ──────→  │  DB  │
   └─────────┘          └────────┘          └──────┘

2. DB Slows Down
   ┌─────────┐   fast   ┌────────┐   slow   ┌──────┐
   │ Watcher │ ──────→  │ Parser │ ──/──→   │  DB  │
   └─────────┘          └────────┘          └──────┘
                             ↓
                    Broadcast queue fills
                    (1024 slots)

3. Broadcast Full → Lagging Receiver
   ┌─────────┐   fast   ┌────────┐          ┌──────┐
   │ Watcher │ ──────→  │ Parser │ ─ ✗ ─→   │  DB  │
   └─────────┘          └────────┘          └──────┘
                             │
                     Oldest events dropped
                     (DB sink lags, logger OK)

4. Parser Queue Fills (worst case)
   ┌─────────┐   slow   ┌────────┐          ┌──────┐
   │ Watcher │ ──/──→   │ Parser │ ─ ✗ ─→   │  DB  │
   └─────────┘          └────────┘          └──────┘
       ↑
   Watcher blocks
   (natural rate limiting)

5. Circuit Breaker Opens
   ┌─────────┐   fast   ┌────────┐          ┌──────┐
   │ Watcher │ ──────→  │ Parser │          │  DB  │ (failing)
   └─────────┘          └────────┘          └──────┘
                             │
                        Circuit OPEN
                        Drop events to DB
                        (logger still works)

Result: Pipeline cannot OOM, degrades gracefully
```

## Circuit Breaker State Machine

```
┌─────────────────────────────────────────────────────────────────┐
│              CIRCUIT BREAKER STATE TRANSITIONS                  │
└─────────────────────────────────────────────────────────────────┘

                    ┌────────────┐
                    │   CLOSED   │
                    │  (normal)  │
                    └──────┬─────┘
                           │
                  failure_threshold reached
                  (e.g., 5 consecutive failures)
                           │
                           ▼
                    ┌────────────┐
              ┌─────│    OPEN    │
              │     │ (failing)  │
              │     └──────┬─────┘
              │            │
              │     reset_timeout elapsed
              │     (e.g., 30 seconds)
              │            │
              │            ▼
              │     ┌────────────┐
              │     │ HALF-OPEN  │
              │     │  (testing) │
              │     └──┬───────┬─┘
              │        │       │
              │        │   success_threshold reached
              │        │   (e.g., 3 successes)
              │        │       │
              │        │       └──────────┐
              │        │                  │
              │    any failure            ▼
              │        │            ┌────────────┐
              └────────┘            │   CLOSED   │
                                    │  (normal)  │
                                    └────────────┘

Configuration:
- failure_threshold: 5 (open after 5 failures)
- reset_timeout: 30s (test recovery after 30s)
- success_threshold: 3 (close after 3 successes)
- half_open_timeout: 10s (reopen if stuck in half-open)
```

## Error Flow

```
┌─────────────────────────────────────────────────────────────────┐
│                     ERROR HANDLING FLOW                          │
└─────────────────────────────────────────────────────────────────┘

┌──────────────┐
│  FileEvent   │
└──────┬───────┘
       │
       ▼
┌──────────────────┐
│  Parser Worker   │
│                  │
│  ┌────────────┐  │     Parse Error (malformed MD)
│  │ Parse File │  ├────────────────────────────┐
│  └────────────┘  │                            │
│       │          │                            ▼
│       │ OK       │                  ┌─────────────────┐
│       ▼          │                  │  Log Error      │
│  ┌────────────┐  │                  │  Skip Document  │
│  │  Frontmtr  │  │                  │  Continue       │
│  └────────────┘  │                  └─────────────────┘
│       │          │
│       │ OK       │
│       ▼          │
│  ┌────────────┐  │
│  │  Wikilinks │  │
│  └────────────┘  │
│       │          │
│       ▼          │
│  ParsedDocument  │
└──────┬───────────┘
       │
       ▼
┌──────────────────┐
│   Broadcast      │
└──────┬───────────┘
       │
   ┌───┴────┐
   │        │
   ▼        ▼
┌─────┐  ┌──────────┐
│ Log │  │ DB Sink  │
│Sink │  │          │
└──┬──┘  │  ┌──────────────┐
   │     │  │ Write to DB  │
   │     │  └──────┬───────┘
   │     │         │
   │ OK  │         │ Error (timeout/connection)
   │     │         │
   │     │         ▼
   │     │  ┌──────────────────┐
   │     │  │  Retry (max 3)   │
   │     │  └──────┬───────────┘
   │     │         │
   │     │         │ Still failing
   │     │         │
   │     │         ▼
   │     │  ┌──────────────────┐
   │     │  │ Circuit Breaker  │
   │     │  │ record_failure() │
   │     │  └──────┬───────────┘
   │     │         │
   │     │         │ threshold reached
   │     │         │
   │     │         ▼
   │     │  ┌──────────────────┐
   │     │  │ Circuit OPENS    │
   │     │  │ Drop future writes│
   │     │  │ Log errors       │
   │     │  └──────────────────┘
   │     │
   ▼     ▼
Always succeeds (logger never fails)
DB failures isolated
```

## Type Relationships

```
┌─────────────────────────────────────────────────────────────────┐
│                      TYPE HIERARCHY                              │
└─────────────────────────────────────────────────────────────────┘

FileEvent (crucible-watch)
  ├─ id: Uuid
  ├─ kind: FileEventKind
  ├─ path: PathBuf
  ├─ timestamp: DateTime<Utc>
  └─ metadata: Option<EventMetadata>

ParsedDocument (crucible-core/parser)
  ├─ path: PathBuf
  ├─ frontmatter: Option<Frontmatter>
  │   ├─ raw: String
  │   ├─ format: FrontmatterFormat
  │   └─ properties: OnceLock<HashMap>  ← Lazy parsed
  ├─ wikilinks: Vec<Wikilink>
  │   ├─ target: String
  │   ├─ alias: Option<String>
  │   ├─ offset: usize
  │   ├─ is_embed: bool
  │   ├─ block_ref: Option<String>
  │   └─ heading_ref: Option<String>
  ├─ tags: Vec<Tag>
  │   ├─ name: String
  │   ├─ path: Vec<String>  ← Nested components
  │   └─ offset: usize
  ├─ content: DocumentContent
  │   ├─ plain_text: String  ← 1000 char excerpt
  │   ├─ headings: Vec<Heading>
  │   ├─ code_blocks: Vec<CodeBlock>
  │   ├─ word_count: usize
  │   └─ char_count: usize
  ├─ parsed_at: DateTime<Utc>
  ├─ content_hash: String
  └─ file_size: u64

MarkdownParser (trait)
  ├─ parse_file(&self, path) → Result<ParsedDocument>
  ├─ parse_content(&self, content, path) → Result<ParsedDocument>
  ├─ capabilities(&self) → ParserCapabilities
  └─ can_parse(&self, path) → bool

OutputSink (trait)
  ├─ write(&self, doc: ParsedDocument) → Result<()>
  ├─ flush(&self) → Result<()>
  ├─ health_check(&self) → SinkHealth
  ├─ shutdown(&self) → Result<()>
  ├─ name(&self) → &'static str
  └─ config(&self) → SinkConfig

CircuitBreaker
  ├─ state: CircuitState
  ├─ count: u32
  ├─ last_state_change: Option<Instant>
  ├─ config: CircuitBreakerConfig
  ├─ can_execute(&mut self) → bool
  ├─ record_success(&mut self)
  └─ record_failure(&mut self)
```

## Performance Profile

```
┌─────────────────────────────────────────────────────────────────┐
│                  PERFORMANCE CHARACTERISTICS                     │
└─────────────────────────────────────────────────────────────────┘

Typical 50 KB Markdown Note
────────────────────────────────────────────────────────────────────

Stage                Time        CPU      Memory     Bottleneck?
─────────────────────────────────────────────────────────────────
File Watch           <1ms        0%       -          No
Debounce             100ms       0%       -          No (config)
Read File            ~1ms        0%       50 KB      No
Parse (pulldown)     ~1ms        100%     -          No
Extract Wikilinks    <0.1ms      100%     -          No
Extract Tags         <0.1ms      100%     -          No
Create ParsedDoc     <0.1ms      0%       2 KB       No
Broadcast Send       <0.01ms     0%       -          No
DB Write (batched)   ~50ms       0%       -          YES ←
─────────────────────────────────────────────────────────────────
Total (no DB)        ~102ms
Total (with DB)      ~152ms
─────────────────────────────────────────────────────────────────

8-Core System Throughput
────────────────────────────────────────────────────────────────────
Parser Pool:          8 workers × 1000 notes/sec/worker = 8000 n/s
DB Sink (batched):    100 writes/sec (SurrealDB local)
────────────────────────────────────────────────────────────────────
Pipeline Throughput:  ~100 notes/sec (DB-bound)

Latency Distribution (P50/P95/P99)
────────────────────────────────────────────────────────────────────
File → Parser:        10ms / 30ms / 50ms
Parse → DB:           50ms / 150ms / 200ms
Parse → Logger:       1ms / 3ms / 5ms
End-to-End:           60ms / 180ms / 250ms
```

## Real-World Example

```
┌─────────────────────────────────────────────────────────────────┐
│              EXAMPLE: Editing a Note                             │
└─────────────────────────────────────────────────────────────────┘

1. User edits file in nvim
   File: ~/vault/Projects/crucible.md
   ───────────────────────────────────────
   ---
   title: Crucible Architecture
   tags: [project, rust, ai]
   created: 2025-10-19
   ---

   # Overview

   Crucible is a [[knowledge-graph]] system.
   See also: [[async-patterns]] #architecture
   ───────────────────────────────────────

2. FileWatcher detects change (inotify)
   Event: Modified
   Path: /home/user/vault/Projects/crucible.md
   Timestamp: 2025-10-19T14:32:15Z

3. Debouncer waits 100ms (user still typing?)
   ... no more changes ...
   → Forward event to parser

4. Parser worker parses file
   ParsedDocument {
     path: "/home/user/vault/Projects/crucible.md",
     frontmatter: Some(Frontmatter {
       raw: "title: Crucible...",
       format: Yaml,
       properties: {
         "title": "Crucible Architecture",
         "tags": ["project", "rust", "ai"],
         "created": "2025-10-19"
       }
     }),
     wikilinks: [
       Wikilink { target: "knowledge-graph", offset: 85, ... },
       Wikilink { target: "async-patterns", offset: 145, ... }
     ],
     tags: [
       Tag { name: "architecture", path: ["architecture"], offset: 165 }
     ],
     content: DocumentContent {
       plain_text: "Overview\nCrucible is a knowledge-graph system...",
       headings: [
         Heading { level: 1, text: "Overview", offset: 60 }
       ],
       word_count: 12,
       char_count: 78
     },
     parsed_at: 2025-10-19T14:32:15.123Z,
     content_hash: "a3f9c2d8...",
     file_size: 243
   }

5. Broadcast to sinks

   Logger Sink:
   ────────────
   2025-10-19 14:32:15 INFO Document parsed
     path=/home/user/vault/Projects/crucible.md
     wikilinks=2 tags=1

   DB Sink:
   ────────────
   → Buffer document
   → After 100 docs or 5s, batch write:
     INSERT INTO notes (path, title, tags, links, ...) VALUES ...
     CREATE EDGE link FROM @src TO @target ...

6. User sees in TUI logs:
   ────────────────────────────────────────
   14:32:15 INFO  File changed: Projects/crucible.md
   14:32:15 DEBUG Re-parsing...
   14:32:15 DEBUG Extracted 2 links, 1 tag
   14:32:15 DEBUG Re-indexed (89ms)
   ────────────────────────────────────────

Total time: ~150ms from save to indexed
```

---

**Related Documents**:
- [Architecture Design](/home/moot/crucible/docs/ASYNC_PIPELINE_ARCHITECTURE.md)
