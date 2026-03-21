---
title: "Getting Started with Crucible"
description: "Your first steps with Crucible - installation, setup, and basic commands"
sidebar:
  order: 1
---

Welcome to Crucible! This guide will help you install, configure, and run your first commands.

## What is Crucible?

Crucible is a knowledge-grounded agent runtime — agents that draw from a knowledge graph make better decisions. Your notes, conversations, and wikilinks form a living knowledge graph that grows over time. Agents draw from this graph automatically via [Precognition](../help/concepts/precognition/), and everything beyond the knowledge core is extensible via Lua scripting and plugins.

**Key Features:**
- **Knowledge-grounded agents** — Precognition auto-injects relevant context before each LLM turn
- **Sessions are notes** — every conversation persists as searchable, linkable markdown
- **Wikilink-based knowledge graph** with block-level semantic search
- **Neovim-like architecture** — Lua/Fennel plugins, TUI-first, headless daemon with RPC
- **Plaintext first** — markdown files are your source of truth, no lock-in

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

Crucible stores notes in a "kiln", your markdown directory. The easiest way to get started is with `cru init`, which walks you through creating a config file interactively.

```bash
cru init
```

This creates `~/.config/crucible/config.toml` with your kiln path and provider settings.

You can also set it up manually:

**Option 1: Configuration File**

Create `~/.config/crucible/config.toml`:

```toml
kiln_path = "/home/user/Documents/my-kiln"

[llm]
default = "local"

[llm.providers.local]
type = "ollama"
default_model = "llama3.2"
endpoint = "http://localhost:11434"

[enrichment.provider]
type = "fastembed"

[cli]
show_progress = true
```

**Option 2: Environment Variable**
```bash
export CRUCIBLE_KILN_PATH="/path/to/your/notes"
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

The first time you run `cru chat`, Crucible automatically starts a background daemon (`cru daemon serve`) if one isn't already running. You don't need to start it manually. The daemon handles session state, file watching, and multi-session support over a Unix socket.

**Chat modes** (cycle with `BackTab`):
- **Normal** (default): Full access, agent can read and modify files
- **Plan**: Read-only, agent can search and read but not modify
- **Auto**: Auto-approve tool calls without prompting

**Slash commands:** `/plan`, `/auto`, `/normal`, `/search`, `/help`
**REPL commands:** `:model`, `:set`, `:export`, `:clear`, `:help`

**Implementation:** `crates/crucible-cli/src/commands/chat.rs`

## Understanding the Database

Crucible stores processed data in a local SQLite database:

**Location:** `<kiln_path>/.crucible/kiln.db/`

This database contains:
- Parsed note metadata (frontmatter, tags)
- Extracted blocks (headings, paragraphs, lists)
- Wikilink relationships (knowledge graph)
- Block-level embeddings for semantic search
- Content hashes for change detection

**Important:** The database is derived data. Your markdown files are the source of truth. You can safely delete `.crucible/` and rebuild with `cru process --force`.

## Next Steps

- [Your First Kiln](./your-first-kiln/) - Create a new knowledge base from scratch
- [Basic Commands](./basic-commands/) - Learn all the essential CLI commands
- [Wikilinks](../help/wikilinks/) - Understand Crucible's linking syntax
- [Frontmatter](../help/frontmatter/) - Learn about YAML metadata
- [Index](../help/cli/index/) - Explore different ways to organize your notes

## Troubleshooting

### "Error: kiln path does not exist"

Check that `CRUCIBLE_KILN_PATH` is set correctly or configure it in your config file.

### Processing is slow

Reduce parallel workers: `cru process --parallel 1`

### Chat doesn't respond

Make sure your LLM provider is running and configured. For Ollama: `cru chat --provider ollama`. For other providers, check your `config.toml` settings.

## See Also

- `:h frontmatter` - YAML metadata format
- `:h wikilinks` - Link syntax and resolution
- `:h config` - Full configuration reference
