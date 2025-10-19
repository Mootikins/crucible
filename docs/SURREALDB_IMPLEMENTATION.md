# SurrealDB Implementation Guide

> **Status**: Implementation Roadmap
> **Related**: [SURREALDB_SCHEMA.md](./SURREALDB_SCHEMA.md), [POC_ARCHITECTURE.md](./POC_ARCHITECTURE.md)
> **Purpose**: Step-by-step guide to implement the SurrealDB schema in Crucible

## Overview

This document provides a practical implementation roadmap for integrating the SurrealDB schema into Crucible's daemon-based architecture.

## Files Created

1. **Schema Design**
   - `/home/moot/crucible/docs/SURREALDB_SCHEMA.md` - Architecture document with comprehensive design rationale
   - `/home/moot/crucible/crates/crucible-surrealdb/src/schema.surql` - SurrealQL schema definitions

2. **Rust Types**
   - `/home/moot/crucible/crates/crucible-surrealdb/src/schema_types.rs` - Type-safe Rust definitions matching schema

3. **Query Examples**
   - `/home/moot/crucible/crates/crucible-surrealdb/examples/queries.surql` - Comprehensive query patterns

## Implementation Phases

### Phase 1: Database Setup

#### 1.1 Update Cargo.toml

Add SurrealDB dependency to `/home/moot/crucible/crates/crucible-surrealdb/Cargo.toml`:

```toml
[dependencies]
surrealdb = "2.0"
tokio = { version = "1.0", features = ["full"] }
chrono = { version = "0.4", features = ["serde"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
anyhow = "1.0"
```

#### 1.2 Initialize Schema

Create schema initialization function in `database.rs`:

```rust
use surrealdb::Surreal;
use surrealdb::engine::local::{Db, RocksDb};

pub struct SurrealDatabase {
    db: Surreal<Db>,
    config: SurrealDbConfig,
}

impl SurrealDatabase {
    pub async fn new(db_path: &str) -> Result<Self> {
        // Connect to RocksDB-backed SurrealDB
        let db = Surreal::new::<RocksDb>(db_path).await?;

        // Use namespace and database
        db.use_ns("crucible").use_db("vault").await?;

        Ok(Self {
            db,
            config: SurrealDbConfig::default(),
        })
    }

    pub async fn initialize_schema(&self) -> Result<()> {
        // Load and execute schema.surql
        let schema_sql = include_str!("schema.surql");
        self.db.query(schema_sql).await?;

        Ok(())
    }
}
```

#### 1.3 Test Database Connection

Create integration test in `tests/`:

```rust
#[tokio::test]
async fn test_schema_initialization() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let db = SurrealDatabase::new(db_path.to_str().unwrap()).await.unwrap();
    db.initialize_schema().await.unwrap();

    // Verify schema version
    let result: Option<i32> = db.db
        .query("SELECT schema_version FROM metadata:system")
        .await
        .unwrap()
        .take(0)
        .unwrap();

    assert_eq!(result, Some(1));
}
```

### Phase 2: Parser Integration

#### 2.1 Create Markdown Parser

Parse markdown files to extract:
- Frontmatter (YAML)
- Wikilinks `[[Document Title]]`
- Tags `#tag` or `tags: [tag1, tag2]`
- Content

Recommended crates:
- `gray_matter` for frontmatter parsing
- `pulldown-cmark` for markdown AST
- `regex` for wikilink extraction

```rust
pub struct ParsedNote {
    pub path: String,
    pub title: Option<String>,
    pub content: String,
    pub frontmatter: HashMap<String, serde_json::Value>,
    pub tags: Vec<String>,
    pub wikilinks: Vec<WikilinkRef>,
}

pub struct WikilinkRef {
    pub target: String,
    pub link_text: String,
    pub position: usize,
    pub context: Option<String>,
}

pub fn parse_markdown(path: &Path, content: &str) -> Result<ParsedNote> {
    // 1. Extract frontmatter
    let (frontmatter, content_without_fm) = extract_frontmatter(content)?;

    // 2. Extract wikilinks
    let wikilinks = extract_wikilinks(content_without_fm)?;

    // 3. Extract tags (from frontmatter and content)
    let tags = extract_tags(&frontmatter, content_without_fm)?;

    // 4. Extract title (from frontmatter or first heading)
    let title = frontmatter.get("title")
        .or_else(|| extract_first_heading(content_without_fm));

    Ok(ParsedNote {
        path: path.to_string_lossy().to_string(),
        title,
        content: content_without_fm.to_string(),
        frontmatter,
        tags,
        wikilinks,
    })
}
```

#### 2.2 Convert to Schema Types

```rust
impl From<ParsedNote> for Note {
    fn from(parsed: ParsedNote) -> Self {
        Note::new(parsed.path, parsed.content)
            .with_title(parsed.title.unwrap_or_default())
            .with_tags(parsed.tags)
            .with_metadata(parsed.frontmatter)
    }
}
```

### Phase 3: File Watcher Integration

#### 3.1 Watcher Setup

Use `notify-debouncer` as specified in POC_ARCHITECTURE.md:

```rust
use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};
use std::time::Duration;

pub async fn watch_vault(vault_path: &Path, db: Arc<SurrealDatabase>) -> Result<()> {
    let (tx, rx) = std::sync::mpsc::channel();

    let mut debouncer = new_debouncer(Duration::from_millis(500), tx)?;
    debouncer.watcher().watch(vault_path, RecursiveMode::Recursive)?;

    while let Ok(events) = rx.recv() {
        for event in events {
            match event.kind {
                DebouncedEventKind::Any => {
                    for path in event.paths {
                        if path.extension() == Some(OsStr::new("md")) {
                            handle_file_change(&path, &db).await?;
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

async fn handle_file_change(path: &Path, db: &SurrealDatabase) -> Result<()> {
    tracing::info!("Processing file change: {}", path.display());

    // 1. Read file
    let content = std::fs::read_to_string(path)?;

    // 2. Parse markdown
    let parsed = parse_markdown(path, &content)?;

    // 3. Convert to Note
    let note: Note = parsed.into();

    // 4. Upsert to database
    db.upsert_note(&note).await?;

    // 5. Extract and store wikilinks
    db.update_wikilinks(&note, &parsed.wikilinks).await?;

    tracing::info!("Indexed: {}", path.display());

    Ok(())
}
```

#### 3.2 Database Upsert Operations

```rust
impl SurrealDatabase {
    pub async fn upsert_note(&self, note: &Note) -> Result<()> {
        // Use path as record ID for idempotent upserts
        let record_id = format!("notes:{}", note.path);

        self.db.query(r#"
            UPDATE $record_id CONTENT $note
        "#)
        .bind(("record_id", record_id))
        .bind(("note", note))
        .await?;

        Ok(())
    }

    pub async fn update_wikilinks(&self, note: &Note, links: &[WikilinkRef]) -> Result<()> {
        let note_id = format!("notes:{}", note.path);

        // 1. Delete existing wikilinks from this note
        self.db.query("DELETE FROM wikilink WHERE in = $note_id")
            .bind(("note_id", &note_id))
            .await?;

        // 2. Create new wikilinks
        for link in links {
            let target_id = format!("notes:{}", link.target);

            self.db.query(r#"
                RELATE $from->wikilink->$to SET
                    link_text = $link_text,
                    position = $position,
                    context = $context
            "#)
            .bind(("from", &note_id))
            .bind(("to", target_id))
            .bind(("link_text", &link.link_text))
            .bind(("position", link.position as i32))
            .bind(("context", &link.context))
            .await?;
        }

        Ok(())
    }
}
```

### Phase 4: Query Implementation

#### 4.1 Type-Safe Query Builders

```rust
impl SurrealDatabase {
    pub async fn search_by_tags(&self, tags: &[String]) -> Result<Vec<Note>> {
        let result = self.db.query(r#"
            SELECT * FROM notes
            WHERE tags CONTAINSALL $tags
            ORDER BY modified_at DESC
        "#)
        .bind(("tags", tags))
        .await?;

        let notes: Vec<Note> = result.take(0)?;
        Ok(notes)
    }

    pub async fn get_backlinks(&self, note_path: &str) -> Result<Vec<Note>> {
        let note_id = format!("notes:{}", note_path);

        let result = self.db.query(r#"
            SELECT <-wikilink<-notes.* FROM $note_id
        "#)
        .bind(("note_id", note_id))
        .await?;

        let notes: Vec<Note> = result.take(0)?;
        Ok(notes)
    }

    pub async fn full_text_search(&self, query: &str, limit: u32) -> Result<Vec<SearchResult>> {
        let result = self.db.query(r#"
            SELECT
                *,
                search::score(1) AS score,
                search::highlight('<mark>', '</mark>', 1) AS snippet
            FROM notes
            WHERE content_text @1@ $query
            ORDER BY score DESC
            LIMIT $limit
        "#)
        .bind(("query", query))
        .bind(("limit", limit))
        .await?;

        let results: Vec<SearchResult> = result.take(0)?;
        Ok(results)
    }

    pub async fn semantic_search(
        &self,
        embedding: &[f32],
        limit: u32,
        min_similarity: Option<f32>,
    ) -> Result<Vec<SearchResult>> {
        let mut query = r#"
            SELECT
                *,
                vector::distance::cosine(embedding, $embedding) AS score
            FROM notes
            WHERE embedding IS NOT NONE
        "#.to_string();

        if let Some(threshold) = min_similarity {
            query.push_str(&format!(
                " AND vector::distance::cosine(embedding, $embedding) < {}",
                threshold
            ));
        }

        query.push_str(" ORDER BY score ASC LIMIT $limit");

        let result = self.db.query(&query)
            .bind(("embedding", embedding))
            .bind(("limit", limit))
            .await?;

        let results: Vec<SearchResult> = result.take(0)?;
        Ok(results)
    }
}
```

#### 4.2 REPL Integration

Implement REPL commands as specified in POC_ARCHITECTURE.md:

```rust
pub enum ReplCommand {
    SurrealQL(String),
    SearchTags(Vec<String>),
    SearchText(String),
    SearchSemantic(String),
    GetBacklinks(String),
    Stats,
    Help,
    Quit,
}

pub async fn execute_command(cmd: ReplCommand, db: &SurrealDatabase) -> Result<String> {
    match cmd {
        ReplCommand::SurrealQL(query) => {
            let result = db.db.query(&query).await?;
            Ok(format_result(result))
        }

        ReplCommand::SearchTags(tags) => {
            let notes = db.search_by_tags(&tags).await?;
            Ok(format_notes(notes))
        }

        ReplCommand::SearchText(query) => {
            let results = db.full_text_search(&query, 20).await?;
            Ok(format_search_results(results))
        }

        ReplCommand::GetBacklinks(path) => {
            let notes = db.get_backlinks(&path).await?;
            Ok(format_notes(notes))
        }

        ReplCommand::Stats => {
            let stats = db.get_vault_stats().await?;
            Ok(format_stats(stats))
        }

        // ... other commands
    }
}
```

### Phase 5: Embedding Integration

#### 5.1 Embedding Generation

Use a local embedding model (e.g., all-MiniLM-L6-v2):

```rust
use rust_bert::pipelines::sentence_embeddings::SentenceEmbeddingsBuilder;

pub struct EmbeddingService {
    model: SentenceEmbeddingsModel,
}

impl EmbeddingService {
    pub fn new() -> Result<Self> {
        let model = SentenceEmbeddingsBuilder::local("all-MiniLM-L6-v2")
            .create_model()?;

        Ok(Self { model })
    }

    pub fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let embeddings = self.model.encode(&[text])?;
        Ok(embeddings[0].clone())
    }
}
```

#### 5.2 Batch Embedding Updates

```rust
pub async fn update_embeddings(
    db: &SurrealDatabase,
    embedding_service: &EmbeddingService,
) -> Result<()> {
    // Get notes without embeddings or with stale embeddings
    let notes = db.db.query(r#"
        SELECT path, content, modified_at, embedding_updated_at
        FROM notes
        WHERE embedding IS NONE
           OR modified_at > embedding_updated_at
        LIMIT 100
    "#).await?;

    let notes: Vec<Note> = notes.take(0)?;

    for note in notes {
        tracing::info!("Generating embedding for: {}", note.path);

        let embedding = embedding_service.embed(&note.content)?;

        db.db.query(r#"
            UPDATE $record_id SET
                embedding = $embedding,
                embedding_model = "all-MiniLM-L6-v2",
                embedding_updated_at = time::now()
        "#)
        .bind(("record_id", format!("notes:{}", note.path)))
        .bind(("embedding", embedding))
        .await?;
    }

    Ok(())
}
```

### Phase 6: Testing Strategy

#### 6.1 Unit Tests

Test individual components:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_wikilinks() {
        let content = "This links to [[Other Document]] and [[Another]].";
        let links = extract_wikilinks(content).unwrap();

        assert_eq!(links.len(), 2);
        assert_eq!(links[0].target, "Other Document");
        assert_eq!(links[1].target, "Another");
    }

    #[tokio::test]
    async fn test_tag_search() {
        let db = setup_test_db().await;

        // Insert test notes
        db.upsert_note(&Note::new("test1.md", "Content")
            .with_tags(vec!["#rust".to_string()])).await.unwrap();

        // Search by tag
        let results = db.search_by_tags(&["#rust"]).await.unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].path, "test1.md");
    }
}
```

#### 6.2 Integration Tests

Test end-to-end workflows:

```rust
#[tokio::test]
async fn test_watcher_integration() {
    let temp_vault = setup_temp_vault();
    let db = setup_test_db().await;

    // Start watcher in background
    tokio::spawn(watch_vault(temp_vault.path(), Arc::new(db.clone())));

    // Create a note
    std::fs::write(
        temp_vault.path().join("test.md"),
        "---\ntags: [rust, test]\n---\n# Test\n\nLinks to [[other]]."
    ).unwrap();

    // Wait for indexing
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Verify indexed
    let note = db.get_note("test.md").await.unwrap();
    assert!(note.is_some());

    let note = note.unwrap();
    assert_eq!(note.tags.len(), 2);

    // Verify wikilinks
    let links = db.get_wikilinks("test.md").await.unwrap();
    assert_eq!(links.len(), 1);
}
```

### Phase 7: Performance Optimization

#### 7.1 Batching

Batch multiple operations:

```rust
pub async fn batch_upsert_notes(&self, notes: &[Note]) -> Result<()> {
    let mut query = String::from("BEGIN TRANSACTION;");

    for note in notes {
        query.push_str(&format!(
            "UPDATE notes:{} CONTENT {};",
            note.path,
            serde_json::to_string(note)?
        ));
    }

    query.push_str("COMMIT TRANSACTION;");

    self.db.query(&query).await?;
    Ok(())
}
```

#### 7.2 Connection Pooling

For multi-client scenarios:

```rust
pub struct SurrealPool {
    pool: Vec<Surreal<Db>>,
    current: AtomicUsize,
}

impl SurrealPool {
    pub async fn new(db_path: &str, size: usize) -> Result<Self> {
        let mut pool = Vec::with_capacity(size);

        for _ in 0..size {
            let db = Surreal::new::<RocksDb>(db_path).await?;
            db.use_ns("crucible").use_db("vault").await?;
            pool.push(db);
        }

        Ok(Self {
            pool,
            current: AtomicUsize::new(0),
        })
    }

    pub fn get(&self) -> &Surreal<Db> {
        let idx = self.current.fetch_add(1, Ordering::SeqCst) % self.pool.len();
        &self.pool[idx]
    }
}
```

## Deployment Checklist

### Pre-Production

- [ ] Schema initialization tested
- [ ] Parser handles edge cases (malformed YAML, unicode, etc.)
- [ ] Watcher debouncing prevents duplicate indexing
- [ ] All query types functional (tags, full-text, semantic, graph)
- [ ] Embedding generation pipeline tested
- [ ] REPL accepts and executes all command types
- [ ] Error handling comprehensive
- [ ] Logging integrated with TUI

### Production

- [ ] Database path configurable via `crucible-config`
- [ ] Schema migration strategy in place
- [ ] Backup/restore procedures documented
- [ ] Performance benchmarks recorded
- [ ] Memory usage profiled
- [ ] Concurrent access tested
- [ ] Recovery from crashes tested

## Monitoring

### Key Metrics

1. **Indexing Performance**
   - Files processed per second
   - Average parse time
   - Database write latency

2. **Query Performance**
   - Full-text search latency (p50, p95, p99)
   - Semantic search latency
   - Graph traversal time (by depth)

3. **Resource Usage**
   - Database size on disk
   - Memory consumption (RSS)
   - CPU utilization

4. **Data Quality**
   - Broken wikilinks count
   - Notes missing embeddings
   - Stale embeddings count

### Logging

Use structured logging with `tracing`:

```rust
tracing::info!(
    file_path = %path.display(),
    tags = ?parsed.tags,
    wikilinks = parsed.wikilinks.len(),
    "Indexed note"
);
```

## Troubleshooting

### Common Issues

**Issue**: Schema initialization fails
- **Solution**: Check SurrealDB version (requires 2.0+), verify file permissions

**Issue**: Wikilinks not resolving
- **Solution**: Verify target notes exist, check path normalization

**Issue**: Slow semantic search
- **Solution**: Verify MTREE index created, reduce embedding dimensions, add filters

**Issue**: Database file growing too large
- **Solution**: Implement archival strategy, compress old embeddings

## Next Steps

1. **Implement Parser** (Phase 2)
2. **Integrate Watcher** (Phase 3)
3. **Build REPL** (Phase 4)
4. **Add Embeddings** (Phase 5)
5. **Optimize Performance** (Phase 7)

## References

- [SurrealDB Documentation](https://surrealdb.com/docs)
- [Schema Design](./SURREALDB_SCHEMA.md)
- [PoC Architecture](./POC_ARCHITECTURE.md)
- [Example Queries](../crates/crucible-surrealdb/examples/queries.surql)

---

**Last Updated**: 2025-10-19
**Status**: Implementation Guide
