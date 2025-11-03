# Crucible CLI

A powerful command-line interface for the Crucible knowledge management system, providing interactive REPL, search capabilities, and AI agent integration.

## Features

- 🔍 **Interactive Search**: Fuzzy search across all metadata with real-time results
- 🎯 **Semantic Search**: AI-powered semantic search using embeddings
- 🖥️ **Interactive REPL**: Full-featured REPL with SurrealQL support, syntax highlighting, and tool execution
- 🔄 **Kiln Processing**: Manage embedding generation directly from the CLI
- 📝 **Note Management**: Create, update, and list notes with full metadata support
- 📊 **Statistics**: Comprehensive kiln statistics and analytics
- 🔧 **Rune Scripting** (via crucible-tools, in development): Execute custom Rune scripts as commands
- ⚡ **Performance**: Fast, responsive CLI with async operations

## Installation

```bash
# Build from source
cargo build -p crucible-cli

# Install globally (optional)
cargo install --path crates/crucible-cli
```

## Quick Start

```bash
# Start interactive REPL (default behavior)
crucible-cli

# Show help and available commands
crucible-cli --help

# Show kiln statistics
crucible-cli stats

# Search notes interactively
crucible-cli search "your query"

# Semantic search
crucible-cli semantic "conceptual understanding"

# Start REPL with custom settings
crucible-cli --format json --tool-dir ~/my-tools
```

**Default Behavior**: Running `crucible-cli` without any arguments starts the interactive REPL with SurrealQL support, syntax highlighting, and tool execution capabilities.

## Commands

### Search & Discovery

#### Interactive Search
```bash
crucible-cli search [query] [options]
```
- `query`: Optional search query (opens picker if omitted)
- `-n, --limit <N>`: Number of results (default: 10)
- `-f, --format <format>`: Output format (plain, json, table)
- `-c, --show-content`: Show content preview

#### Fuzzy Search
```bash
crucible-cli fuzzy [query] [options]
```
- `query`: Search query (optional - shows all results if omitted)
- `--content`: Search in content (default: true)
- `--tags`: Search in tags (default: true)
- `--paths`: Search in file paths (default: true)
- `-n, --limit <N>`: Number of results (default: 20)

#### Semantic Search
```bash
crucible-cli semantic <query> [options]
```
- `query`: Search query (required)
- `-n, --top-k <N>`: Number of results (default: 10)
- `-f, --format <format>`: Output format (plain, json, table)
- `-s, --show-scores`: Show similarity scores

### Interactive REPL (Default)

The interactive REPL starts by default when running `crucible-cli` without arguments. You can customize the REPL with global options:

**Global Options:**
- `--db-path <path>`: Database path override
- `--tool-dir <path>`: Tool directory for Rune scripts
- `-v, --verbose`: Enable verbose output
- `-f, --format <format>`: Output format (table, json, csv)

```bash
# Start with custom settings
crucible-cli --format json --tool-dir ~/my-tools --db-path ~/custom.db
```

**REPL Commands:**
- `:tools` - List available tools
- `:run <tool> [args...]` - Execute a tool
- `:rune <script> [args...]` - Run a Rune script
- `:stats` - Show statistics
- `:config` - Display configuration
- `:log <level>` - Set log level (trace|debug|info|warn|error)
- `:format <fmt>` - Set output format (table|json|csv)
- `:help [command]` - Show help
- `:history [limit]` - Show command history
- `:clear` - Clear screen
- `:quit` - Exit REPL

**SurrealQL Queries:**
Any input not starting with `:` is treated as a SurrealQL query:

```sql
SELECT * FROM notes;
SELECT title, tags FROM notes WHERE tags CONTAINS '#project';
SELECT ->links->note.title FROM notes WHERE path = 'foo.md';
```

### Note Management

#### Create Note
```bash
crucible-cli note create <path> [options]
```
- `-c, --content <text>`: Note content
- `-e, --edit`: Open in $EDITOR after creation

#### Get Note
```bash
crucible-cli note get <path> [options]
```
- `-f, --format <format>`: Output format (plain, json)

#### Update Note
```bash
crucible-cli note update <path> -p <properties>
```
- `-p, --properties <json>`: Properties as JSON object

#### List Notes
```bash
crucible-cli note list [options]
```
- `-f, --format <format>`: Output format (plain, json, table)

### Utilities

#### Statistics
```bash
crucible-cli stats
```
Shows comprehensive kiln statistics including note count, embeddings status, and metadata.

#### Kiln Processor
```bash
crucible-cli process <subcommand> [options]
```
- `start`: Run the processor (use `--wait` to block until complete)
- `status`: Check processor status and embedding availability
- `stop`: Request processor shutdown (force flag supported)
- `restart`: Restart processing in one command

#### Rune Scripts
```bash
# Run a specific script
crucible-cli run <script> [options]
crucible-cli run script.rn --args '{"key": "value"}'

# List available commands
crucible-cli commands
```

#### Configuration
```bash
# Initialize config
crucible-cli config init [options]
crucible-cli config init --path ~/.crucible/config.toml --force

# Show current config
crucible-cli config show -f json
```

## Configuration

The CLI uses a hierarchical configuration system:

1. **Defaults**: Built-in sensible defaults
2. **Config File**: `~/.config/crucible/config.toml`
3. **Environment Variables**: `CRUCIBLE_KILN_PATH`, etc.
4. **Command Line Arguments**: Highest priority

### Sample Configuration

```toml
[kiln]
path = "~/Documents/kiln"

[llm]
provider = "ollama"
model = "llama3.1"
base_url = "http://localhost:11434"

[network]
timeout = 30
retries = 3
```

## Examples

### Research Workflow

```bash
# 1. Search for related notes
crucible-cli search "machine learning" -n 20

# 2. Semantic exploration
crucible-cli semantic "neural network architectures" --show-scores

# 3. Interactive REPL for deep analysis
crucible-cli
# In REPL:
# :stats
# SELECT title, created FROM notes WHERE tags CONTAINS '#ml' ORDER BY created DESC;
# :run semantic_search "deep learning patterns"

# 4. Summarize findings with a Rune script
crucible-cli run summarize.rn --args '{"source": "projects/ml-research.md"}'
```

### Note Management

```bash
# Create a new note
crucible-cli note create projects/ml-research.md -e

# Add metadata
crucible-cli note update projects/ml-research.md -p '{"tags": ["#ml", "#research"], "status": "active"}'

# Search in context
crucible-cli fuzzy ml -n 10
```

### Tool Development

```bash
# Create a custom tool directory
mkdir -p ~/.crucible/tools

# Create a Rune script
echo 'println!("Hello from custom tool!");' > ~/.crucible/tools/hello.rn

# Run in REPL
crucible-cli repl
# In REPL:
# :tools
# :run hello
```

## Advanced Features

### Output Formats

Most commands support multiple output formats:

```bash
# Table format (human-readable)
crucible-cli search "query" -f table

# JSON format (machine-readable)
crucible-cli search "query" -f json | jq '.[] | .title'

# CSV format (spreadsheet compatible)
crucible-cli search "query" -f csv > results.csv
```

### Pipe Integration

```bash
# Chain operations
crucible-cli fuzzy "project" -f json | jq '.[].path' | xargs crucible-cli note get

# Export data
crucible-cli stats -f json | jq '.total_notes' > note_count.txt

# Semantic search with processing
crucible-cli semantic "important concepts" -f json | jq -r '.[].title' | while read title; do
  crucible-cli search "$title" -n 1
done
```

## Troubleshooting

### Common Issues

1. **Database Lock Error**: Ensure no other Crucible processes are running
2. **Missing Embeddings**: Run `crucible-cli index` to generate embeddings
3. **Configuration Issues**: Use `crucible-cli config show` to verify settings

### Debug Mode

```bash
# Enable verbose logging
RUST_LOG=debug crucible-cli --verbose search "query"

# Check configuration
crucible-cli config show -f toml
```

## Development

```bash
# Run tests
cargo test -p crucible-cli

# Run with debug output
RUST_LOG=debug cargo run -p crucible-cli -- repl

# Build for release
cargo build -p crucible-cli --release
```

## License

Copyright (c) 2024 Crucible. All Rights Reserved.
