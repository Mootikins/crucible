---
description: Your first steps with Crucible - installation, setup, and basic commands
tags:
  - guide
  - beginner
order: 1
created: 2025-01-10
modified: 2025-01-15
---

# Getting Started with Crucible

Welcome to Crucible! This guide will help you install, configure, and run your first commands.

## What is Crucible?

Crucible is a plaintext-first knowledge management system that combines markdown files with powerful semantic search, graph traversal, and AI agent integration. Your notes stay in markdown files that work with any text editor, while Crucible builds a rich knowledge graph from your wikilinks, tags, and frontmatter.

**Key Features:**
- Markdown files are your source of truth
- Semantic search at block (paragraph/heading) level
- Wikilink-based knowledge graph
- AI agent integration via Model Context Protocol (MCP)
- Incremental processing with hash-based change detection

## Prerequisites

Before installing Crucible, make sure you have:

- **Rust toolchain** (1.75 or newer)
  - Install via [rustup](https://rustup.rs/): `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- **Cargo** (comes with Rust)
- **Git** (for cloning the repository)

## Installation

### 1. Clone the Repository

```bash
git clone https://github.com/mootikins/crucible.git
cd crucible
```

### 2. Build Crucible

```bash
cargo build --release
```

The binary will be at `target/release/cru`.

### 3. Add to PATH (Optional)

```bash
# Add to your shell profile
export PATH="$PATH:/path/to/crucible/target/release"

# Or create a symlink
sudo ln -s /path/to/crucible/target/release/cru /usr/local/bin/cru
```

## Configuration

### Set Your Kiln Path

Crucible stores notes in a "kiln" - your markdown directory.

**Option 1: Environment Variable**
```bash
export CRUCIBLE_KILN_PATH="/path/to/your/notes"
```

**Option 2: Configuration File**

Create `~/.config/crucible/config.toml`:

```toml
kiln_path = "/home/user/Documents/my-kiln"

[embedding]
provider = "fastembed"

[cli]
show_progress = true
```

**Option 3: CLI Flag**
```bash
cru --kiln /path/to/notes stats
```

## Your First Commands

### 1. Check Kiln Statistics

```bash
cru stats
```

You should see:
```
Kiln Statistics

Total files: 42
Markdown files: 38
Total size: 156 KB
Kiln path: /home/user/Documents/my-kiln

Kiln scan completed successfully.
```

**Implementation:** `crates/crucible-cli/src/commands/stats.rs`

### 2. Process Your Notes

```bash
cru process
```

This parses all markdown files, extracts metadata, wikilinks, tags, and blocks, generates embeddings, and stores everything in the local database.

**Common flags:**
- `--force` - Reprocess all files regardless of changes
- `--watch` - Keep watching for changes
- `--dry-run` - Preview without making changes

**Implementation:** `crates/crucible-cli/src/commands/process.rs`

### 3. Start Chatting

```bash
cru chat
```

On first run, a setup wizard guides you through kiln path, provider, and model configuration. After setup, you enter an interactive chat session with your knowledge base.

**Chat modes** (cycle with `BackTab`):
- **Normal** (default): Full access, agent can read and modify files
- **Plan**: Read-only, agent can search and read but not modify
- **Auto**: Auto-approve tool calls without prompting

**Slash commands:** `/plan`, `/auto`, `/normal`, `/search`, `/help`
**REPL commands:** `:model`, `:set`, `:export`, `:clear`, `:help`

**Implementation:** `crates/crucible-cli/src/commands/chat.rs`

## Understanding the Database

Crucible stores processed data in a local SurrealDB database:

**Location:** `<kiln_path>/.crucible/kiln.db/`

This database contains:
- Parsed note metadata (frontmatter, tags)
- Extracted blocks (headings, paragraphs, lists)
- Wikilink relationships (knowledge graph)
- Block-level embeddings for semantic search
- Content hashes for change detection

**Important:** The database is derived data. Your markdown files are the source of truth. You can safely delete `.crucible/` and rebuild with `cru process --force`.

## Next Steps

- [[Your First Kiln]] - Create a new knowledge base from scratch
- [[Basic Commands]] - Learn all the essential CLI commands
- [[Help/Wikilinks]] - Understand Crucible's linking syntax
- [[Help/Frontmatter]] - Learn about YAML metadata
- [[Organization Styles/Index]] - Explore different ways to organize your notes

## Troubleshooting

### "Error: kiln path does not exist"

Check that `CRUCIBLE_KILN_PATH` is set correctly or configure it in your config file.

### Processing is slow

Reduce parallel workers: `cru process --parallel 1`

### Chat doesn't respond

Test with internal agent: `cru chat --internal --provider ollama`

## See Also

- `:h frontmatter` - YAML metadata format
- `:h wikilinks` - Link syntax and resolution
- `:h config` - Full configuration reference
