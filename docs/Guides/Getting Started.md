---
title: Getting Started
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

Crucible is a knowledge-grounded agent runtime — agents that draw from a knowledge graph make better decisions. Your notes, conversations, and wikilinks form a living knowledge graph that grows over time. Agents draw from this graph automatically via [[Help/Concepts/Precognition|Precognition]], and everything beyond the knowledge core is extensible via Lua scripting and plugins.

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

### Initialize with `cru init`

Crucible stores notes in a **kiln** — a directory of markdown files that forms your knowledge graph. Projects (code repositories) can bind to one or more kilns.

The fastest way to set up is `cru init`. It detects what kind of directory you are in and walks you through an interactive setup.

#### Initializing a kiln

Run `cru init` inside your notes directory:

```bash
cd ~/notes
cru init
```

Crucible detects whether `.crucible/kiln.toml` or `.crucible/project.toml` already exists. If neither is found, it asks what this directory is:

```
? What is this directory?
> Kiln (knowledge store for notes and sessions)
  Project (code repository with kiln bindings)
```

For a kiln, it prompts for a **name** and **data classification** (public, internal, confidential, restricted), then creates `.crucible/kiln.toml` and registers the kiln in your global config under `[kilns]`.

#### Initializing a project

Run `cru init` inside a code repository:

```bash
cd ~/myproject
cru init
```

Choose "Project" when prompted. Crucible creates `.crucible/project.toml` and asks which of your registered kilns to bind. The project is registered in your global config under `[projects.*]`.

#### Non-interactive mode

Use `-y` to skip prompts and accept defaults (defaults to kiln, uses the directory name as the kiln name):

```bash
cru init -y
```

Use `--force` to reinitialize an already-configured directory.

#### First-run setup wizard

If no global config exists yet (`~/.config/crucible/config.toml`), `cru init` automatically runs a first-run wizard that walks you through choosing an LLM provider, model, and embedding backend before creating the kiln or project.

### Manual configuration

You can also create the config file by hand. See [[Configuration]] for the full reference.

Create `~/.config/crucible/config.toml`:

```toml
default_kiln = "notes"

[kilns]
notes = "~/notes"

[chat]
provider = "ollama"
model = "llama3.2"

[embedding]
provider = "fastembed"
```

For multiple kilns and project bindings:

```toml
default_kiln = "vault"

[kilns]
vault = "~/vault"
docs = "~/crucible/docs"

[projects.crucible]
path = "~/crucible"
kilns = ["docs", "vault"]
default_kiln = "vault"
```

See [[Configuration#Migrating from `kiln_path` to `[kilns]`]] if you have an existing `kiln_path` setup.

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

Make sure your LLM provider is running and configured. For Ollama: `cru chat --provider ollama`. For other providers, check your `config.toml` settings.

## See Also

- `:h frontmatter` - YAML metadata format
- `:h wikilinks` - Link syntax and resolution
- `:h config` - Full configuration reference
