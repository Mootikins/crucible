---
description: Search past chat sessions to find conversations and build episodic memory
tags:
  - guide
  - sessions
  - search
---

# Session Search

This guide covers searching your past chat sessions — finding conversations by content, building on previous context, and using session history as episodic memory.

## What is Session Search?

Every Crucible chat session is persisted as a JSONL file (and optionally a human-readable markdown file). Session search lets you find past conversations by searching through this history, turning your chat sessions into a searchable knowledge base.

This enables an **episodic memory pattern**: instead of starting every conversation from scratch, you can recall what you discussed before and build on it.

## Prerequisites

- Crucible CLI installed
- At least one past chat session (run `cru chat` to create one)
- [ripgrep](https://github.com/BurntSushi/ripgrep) installed (optional, but recommended for speed)

## Usage

### Basic Search

```bash
cru session search "authentication"
```

This searches all session JSONL files for the query and displays matching lines with context:

```
Sessions matching 'authentication':

  chat-20260115-1430-a1b2 (line 42)
    {"type":"user","content":"How does the authentication flow work?"}...

  chat-20260118-0900-c3d4 (line 87)
    {"type":"assistant","content":"The authentication uses JWT tokens..."}...
```

### Limit Results

```bash
cru session search "database migration" -n 5
```

The `-n` flag limits the number of results returned (default: 20).

## How It Works

Session search uses a **two-tier strategy**:

1. **Ripgrep (fast path)**: If `rg` is installed, Crucible delegates to ripgrep for blazing-fast search across all session files. Ripgrep uses parallelism and memory-mapped I/O, making it fast even with thousands of sessions.

2. **In-memory fallback (slow path)**: If ripgrep isn't available, Crucible reads session files directly and performs line-by-line text matching. Slower, but works everywhere.

You don't need to configure anything — Crucible automatically detects which path to use.

### Search Behavior

- **Case-insensitive**: Searches ignore case by default
- **Context lines**: Results include 2 lines of context before and after each match
- **JSONL files**: Searches the raw session event log (`.jsonl`), not markdown
- **Truncation**: Long matching lines are truncated to 100 characters for readability

## Search Tips

### Find Conversations About a Topic

```bash
cru session search "kubernetes deployment"
```

### Find What You Asked

Search for your own messages by looking for user content patterns:

```bash
cru session search "how do I"
```

### Find Agent Responses

Search for specific information the agent provided:

```bash
cru session search "the solution is"
```

### Combine with Other Tools

Pipe results to other commands for further processing:

```bash
# Count how many sessions mention a topic
cru session search "refactoring" | grep -c "line"

# List just the session IDs
cru session search "API design" | grep -oP 'chat-\S+'
```

## Session Markdown

Alongside the JSONL event log, Crucible generates a human-readable `session.md` file for each chat session. This markdown file contains the conversation flow (user and assistant messages) with timestamps and frontmatter metadata.

You can read these files directly:

```bash
# Find session markdown files
ls ~/your-kiln/sessions/*/session.md

# Read a specific session
cat ~/your-kiln/sessions/chat-20260115-1430-a1b2/session.md
```

Or search them with standard tools:

```bash
# Search markdown files with ripgrep
rg "authentication" ~/your-kiln/sessions/*/session.md
```

## Troubleshooting

### No results found

- Check that you have past sessions: `cru session list`
- Try a broader search term
- Verify your kiln path is correct

### Search is slow

Install [ripgrep](https://github.com/BurntSushi/ripgrep) for significantly faster search:

```bash
# macOS
brew install ripgrep

# Ubuntu/Debian
sudo apt install ripgrep

# Cargo
cargo install ripgrep
```

### Results are hard to read

The raw JSONL format can be noisy. For human-readable session history, read the markdown files directly or use `cru session show <id>` to view a formatted session.

## See Also

- [[Guides/Getting Started|Getting Started Guide]]
- [[Guides/Basic Commands|Basic Commands]]
- [[Help/Config/LLM Providers|LLM Providers Reference]]
