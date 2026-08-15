---
title: "cru process"
description: Process markdown files for indexing and search
tags:
  - reference
  - cli
---

# cru process

Process markdown files in your kiln to enable search and AI features.

## Synopsis

```
cru process [OPTIONS] [PATH]
```

## Description

The `process` command parses all markdown files in your kiln and stores structured data in the local database. This enables semantic search, knowledge graph queries, and AI agent integration.

The optional positional `PATH` targets a specific file or directory; when
omitted, the entire kiln is processed. Processing itself runs inside the
daemon — the CLI sends the request and reports the summary.

**What gets processed:** `.md` and `.markdown` notes, `.canvas` documents, and
`.txt` files. Notes and canvases contribute links to the graph; plain text is
indexed for full-text search only. Everything else in the kiln — images, PDFs,
attachments — is left alone.

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

Preview without making database changes.

```bash
cru process --dry-run
```

For a single file this prints the file that would be processed. For a full
kiln it does not enumerate files — it prints a one-line notice that the daemon
would discover and process every indexable file (the `--json` summary reports
`discovered: 0` in this case).

### `--parallel <N>`

Accepted for compatibility but currently a **no-op**: processing runs in the
daemon, which manages its own parallelism.

### `--json`

Emit a single JSON summary (`target`, `mode`, `dry_run`, `discovered`,
`processed`, `skipped`, `errors`) instead of human-readable text. Conflicts
with `--watch`.

```bash
cru process --json
```

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

1. **Discovery** - Find every indexable file in the kiln (`.md`, `.markdown`, `.canvas`, `.txt`)
2. **Filtering** - Skip ignored directories (`.crucible`, `.git`, etc.)
3. **Hashing** - Check for content changes
4. **Parsing** - Extract structure from markdown
5. **Enrichment** - Generate embeddings
6. **Storage** - Write to database

## Example Output

```
ℹ Initializing storage...
✓ Storage initialized (daemon mode)
Processing kiln via daemon...
Pipeline processing complete!
  Discovered: 38 indexable files
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
- Use `--json` when scripting to get a machine-readable summary

## See Also

- `:h stats` - View kiln statistics
- `:h search` - Search processed content
- `:h config.embedding` - Embedding configuration
- [[Guides/Getting Started]] - Initial setup guide
