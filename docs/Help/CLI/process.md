---
description: Process markdown files for indexing and search
tags:
  - reference
  - cli
---

# cru process

Process markdown files in your kiln to enable search and AI features.

## Synopsis

```
cru process [OPTIONS]
```

## Description

The `process` command parses all markdown files in your kiln and stores structured data in the local database. This enables semantic search, knowledge graph queries, and AI agent integration.

**What processing does:**
- Parses markdown files for structure
- Extracts frontmatter metadata
- Identifies wikilinks and builds the graph
- Extracts tags (including nested tags)
- Splits content into searchable blocks
- Generates embeddings for semantic search
- Stores everything in the local database (SQLite by default)

## Options

### `--force`

Reprocess all files regardless of whether they've changed.

```bash
cru process --force
```

By default, processing is incremental - only files with changed content are reprocessed.

### `--watch`

Keep watching for file changes and reprocess automatically.

```bash
cru process --watch
```

Use Ctrl+C to stop watching.

### `--dry-run`

Preview what would be processed without making changes.

```bash
cru process --dry-run
```

### `--parallel <N>`

Set the number of parallel workers.

```bash
cru process --parallel 4
```

Default: CPU cores / 2

## Incremental Processing

By default, Crucible uses content hashing to detect changes:

1. Calculate hash of file content
2. Compare with stored hash
3. Only reprocess if different

This makes subsequent runs fast - only changed files are processed.

**Force full reprocessing with:**
```bash
cru process --force
```

## Processing Pipeline

Files go through these stages:

1. **Discovery** - Find all `.md` files in kiln
2. **Filtering** - Skip ignored directories (`.crucible`, `.git`, etc.)
3. **Hashing** - Check for content changes
4. **Parsing** - Extract structure from markdown
5. **Enrichment** - Generate embeddings
6. **Storage** - Write to database

## Example Output

```
Initializing storage...
✓ Storage initialized
Creating processing pipeline...
✓ Pipeline ready

Processing 38 files through pipeline (with 4 workers)...
[========================================] 38/38 Processing: My Note.md

Pipeline processing complete!
   Processed: 38 files
   Skipped (unchanged): 0 files
```

## Database Location

Processed data is stored at:
```
<kiln_path>/.crucible/crucible-sqlite.db
```

This is derived data - your markdown files remain the source of truth.

## Ignored Patterns

These directories are automatically skipped:
- `.crucible/` (database)
- `.git/`
- `.obsidian/`
- `node_modules/`

## Error Handling

Processing continues if individual files fail. Errors are logged but don't stop the pipeline.

Common issues:
- **Invalid frontmatter**: YAML parsing errors are logged
- **Encoding issues**: Non-UTF8 files are skipped
- **Permission denied**: Inaccessible files are skipped

## Watch Mode

With `--watch`, Crucible monitors your kiln for changes:

```bash
cru process --watch
```

- Uses filesystem events for efficiency
- Debounces rapid changes
- Ctrl+C to exit

## Performance Tips

For large kilns (>1000 files):
- Use incremental processing (default)
- Adjust parallelism: `--parallel 8`
- Use `--dry-run` to preview scope

## Implementation

**Source code:** `crates/crucible-cli/src/commands/process.rs`

**Related modules:**
- `crates/crucible-sqlite/` - SQLite storage layer (default)
- `crates/crucible-surrealdb/` - SurrealDB storage layer (advanced)
- `crates/crucible-parser/` - Markdown parsing
- `crates/crucible-llm/src/embeddings/` - Embedding generation

## See Also

- `:h stats` - View kiln statistics
- `:h search` - Search processed content
- `:h config.embedding` - Embedding configuration
- [[Guides/Getting Started]] - Initial setup guide
