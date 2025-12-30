---
tags:
  - research
  - search
  - tooling
---

# Tantivy CLI

[[Help/Concepts/Semantic Search|Semantic Search]] covers Crucible's vector-based search. This note documents **Tantivy**, a full-text search alternative for keyword/BM25 ranking.

## What It Is

Tantivy is a Rust full-text search library (like Lucene). The CLI provides quick indexing and search without code integration.

- **Apache 2.0 / MIT licensed**
- ~2x faster than Lucene
- Persistent index in a directory

## Installation

```bash
cargo install tantivy-cli
```

## Usage

### Create Index

```bash
# Interactive schema creation
tantivy new -i ./search_index
```

Schema example (prompted):
- Field name: `title`, type: `text`, stored: `true`
- Field name: `body`, type: `text`, stored: `true`
- Field name: `path`, type: `text`, stored: `true`

### Index Documents

Documents as JSON lines:

```bash
cat << 'EOF' | tantivy index -i ./search_index
{"title": "Rust Basics", "body": "Introduction to Rust programming", "path": "notes/rust.md"}
{"title": "Search Engines", "body": "How search engines work with inverted indexes", "path": "notes/search.md"}
EOF
```

### Search

```bash
tantivy search -i ./search_index -q "rust programming"
```

### HTTP Server

```bash
tantivy serve -i ./search_index -p 3000
# GET http://localhost:3000/api/?q=rust&nhits=10
```

## When to Use

| Need | Use Tantivy | Use ripgrep |
|------|-------------|-------------|
| Ranked results (BM25) | Yes | No |
| Fuzzy/typo tolerance | Yes | No |
| Persistent index | Yes | No (scans each time) |
| Simple grep-style match | Overkill | Yes |
| Available everywhere | Needs install | Usually present |

## Relevance to Crucible

Tantivy could complement [[Help/Concepts/Semantic Search|vector search]]:

- **Vectors**: Semantic similarity ("notes about productivity")
- **Tantivy**: Exact/keyword match ("notes containing 'GTD'")
- **Hybrid**: Combine scores via RRF fusion

For now, ripgrep handles keyword search. Tantivy is an option if ranking or persistent indexing becomes necessary.

## Links

- [tantivy-cli GitHub](https://github.com/quickwit-oss/tantivy-cli)
- [Tantivy docs.rs](https://docs.rs/tantivy/latest/tantivy/)
- [Tantivy ARCHITECTURE.md](https://github.com/quickwit-oss/tantivy/blob/main/ARCHITECTURE.md)
