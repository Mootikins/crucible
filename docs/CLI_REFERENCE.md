# Crucible CLI Command Reference

> Comprehensive guide to all Crucible CLI commands and options

## Overview

The Crucible CLI provides a powerful command-line interface for knowledge management, service orchestration, and AI integration. This reference covers all available commands, options, and usage examples.

## Installation

```bash
# Build from source
cargo build -p crucible-cli

# Install globally
cargo install --path crates/crucible-cli

# Verify installation
crucible-cli --version
```

## Global Options

These options are available with all commands:

| Option | Short | Long | Description |
|--------|-------|------|-------------|
| Config File | `-C` | `--config <path>` | Config file path (default: ~/.config/crucible/config.toml) |
| Vault Path | `-p` | `--vault-path <path>` | Vault path (overrides config file) |
| Embedding URL | | `--embedding-url <url>` | Embedding service URL (overrides config) |
| Embedding Model | | `--embedding-model <model>` | Embedding model name (overrides config) |
| Database Path | | `--db-path <path>` | Database path to use (overrides config) |
| Tool Directory | | `--tool-dir <path>` | Tool directory path for Rune scripts |
| Output Format | `-f` | `--format <format>` | Output format (table, json, csv) |
| Verbose | `-v` | `--verbose` | Enable verbose logging |
| Help | `-h` | `--help` | Show help message |
| Version | `-V` | `--version` | Show version information |

## Core Commands

### REPL (Default)

**Description**: Starts the interactive REPL with SurrealQL support

**Usage**:
```bash
crucible-cli [global-options]
```

**Examples**:
```bash
# Start REPL with default settings
crucible-cli

# Start REPL with custom format and tool directory
crucible-cli --format json --tool-dir ~/my-tools

# Start REPL with verbose logging
crucible-cli --verbose

# Start REPL with custom database
crucible-cli --db-path ~/custom.db
```

**REPL Commands**:
Once in the REPL, you can use these built-in commands:

| Command | Description |
|---------|-------------|
| `:tools` | List available tools |
| `:run <tool> [args...]` | Execute a tool |
| `:rune <script> [args...]` | Run a Rune script |
| `:stats` | Show vault statistics |
| `:config` | Display configuration |
| `:log <level>` | Set log level (trace|debug|info|warn|error) |
| `:format <fmt>` | Set output format (table|json|csv) |
| `:help [command]` | Show help |
| `:history [limit]` | Show command history |
| `:clear` | Clear screen |
| `:quit` | Exit REPL |

**SurrealQL Queries**:
Any input not starting with `:` is treated as a SurrealQL query:

```sql
-- Basic queries
SELECT * FROM notes;
SELECT title, tags FROM notes WHERE tags CONTAINS '#project';

-- Advanced queries
SELECT ->links->note.title FROM notes WHERE path = 'foo.md';
SELECT * FROM notes ORDER BY created DESC LIMIT 10;

-- Aggregation
SELECT COUNT(*) as total FROM notes;
SELECT tags, COUNT(*) as count FROM notes GROUP BY tags;
```

### Search Commands

#### `search` - Interactive Search

**Description**: Interactive search through notes with fuzzy finder

**Usage**:
```bash
crucible-cli search [query] [options]
```

**Options**:
| Option | Short | Long | Default | Description |
|--------|-------|------|---------|-------------|
| Limit | `-n` | `--limit <N>` | 10 | Number of results to show |
| Format | `-f` | `--format <format>` | plain | Output format (plain, json, table) |
| Show Content | `-c` | `--show-content` | false | Show content preview in results |

**Examples**:
```bash
# Interactive search (opens picker)
crucible-cli search

# Search with specific query
crucible-cli search "machine learning"

# Search with more results and table format
crucible-cli search "research" --limit 50 --format table

# Search with content preview
crucible-cli search "project" --show-content

# Search with JSON output for scripting
crucible-cli search "AI" --format json | jq '.[] | .title'
```

**Search Validation & Safety Features**:

All search commands include built-in safety protections:

| Feature | Limit | Behavior |
|---------|-------|----------|
| **Query Length** | 2-1000 characters | Empty/short queries show error, long queries are rejected |
| **File Size** | 10MB limit | Files >10MB are automatically skipped |
| **Content Memory** | 1MB limit | Large files processed with streaming reads |
| **UTF-8 Handling** | Automatic | Invalid UTF-8 sequences replaced safely |
| **Whitespace** | Normalized | Excessive whitespace cleaned automatically |

**Error Examples**:
```bash
# Empty query (shows validation error)
crucible-cli search ""
# Error: Search query cannot be empty or only whitespace.

# Too short query (shows validation error)
crucible-cli search "a"
# Error: Search query too short (1 < 2 characters).

# Very long query (shows validation error)
crucible-cli search "$(printf 'a%.0s' {1..1001})"
# Error: Search query too long (1001 > 1000 characters).
```

**Performance Notes**:
- Large files (>10MB) are skipped automatically to prevent memory issues
- UTF-8 encoding errors are handled gracefully with character replacement
- Search performance is optimized for typical markdown file sizes
- Memory usage stays constant regardless of file collection size

#### `fuzzy` - Fuzzy Search

**Description**: Fuzzy search across all metadata (tags, properties, content)

**Usage**:
```bash
crucible-cli fuzzy [query] [options]
```

**Options**:
| Option | Short | Long | Default | Description |
|--------|-------|------|---------|-------------|
| Content | | `--content <bool>` | true | Search in content |
| Tags | | `--tags <bool>` | true | Search in tags |
| Paths | | `--paths <bool>` | true | Search in file paths |
| Limit | `-n` | `--limit <N>` | 20 | Number of results |

**Examples**:
```bash
# Fuzzy search in all fields
crucible-cli fuzzy "projct"  # Typo tolerance

# Search only in tags and paths
crucible-cli fuzzy "ml" --content false

# Search with many results
crucible-cli fuzzy "research" --limit 100

# Search in content only
crucible-cli fuzzy "neural network" --tags false --paths false
```

#### `semantic` - Semantic Search

**Description**: Semantic search using AI embeddings

**Usage**:
```bash
crucible-cli semantic <query> [options]
```

**Options**:
| Option | Short | Long | Default | Description |
|--------|-------|------|---------|-------------|
| Top K | `-n` | `--top-k <N>` | 10 | Number of results |
| Format | `-f` | `--format <format>` | plain | Output format (plain, json, table) |
| Show Scores | `-s` | `--show-scores` | false | Show similarity scores |

**Examples**:
```bash
# Semantic search for concepts
crucible-cli semantic "machine learning algorithms"

# Search with similarity scores
crucible-cli semantic "data analysis techniques" --show-scores

# Search with JSON output
crucible-cli semantic "research methodology" --format json

# Search for more results
crucible-cli semantic "software architecture patterns" --top-k 20
```

### Note Management

#### `note create` - Create Note

**Description**: Create a new note

**Usage**:
```bash
crucible-cli note create <path> [options]
```

**Options**:
| Option | Short | Long | Description |
|--------|-------|------|-------------|
| Content | `-c` | `--content <text>` | Note content |
| Edit | `-e` | `--edit` | Open in $EDITOR after creation |

**Examples**:
```bash
# Create note with editor
crucible-cli note create projects/research.md --edit

# Create note with content
crucible-cli note create meeting-notes.md --content "Team meeting - $(date)"

# Create note in subdirectory
crucible-cli note create daily/2024-01-15.md --edit
```

#### `note get` - Get Note

**Description**: Retrieve and display a note

**Usage**:
```bash
crucible-cli note get <path> [options]
```

**Options**:
| Option | Short | Long | Default | Description |
|--------|-------|------|---------|-------------|
| Format | `-f` | `--format <format>` | plain | Output format (plain, json) |

**Examples**:
```bash
# Get note content
crucible-cli note get projects/research.md

# Get note with metadata as JSON
crucible-cli note get meeting-notes.md --format json

# Pipe note content to other tools
crucible-cli note get draft.md | pandoc -f markdown -t pdf -o draft.pdf
```

#### `note update` - Update Note

**Description**: Update note properties

**Usage**:
```bash
crucible-cli note update <path> -p <properties>
```

**Options**:
| Option | Short | Long | Description |
|--------|-------|------|-------------|
| Properties | `-p` | `--properties <json>` | Properties as JSON object |

**Examples**:
```bash
# Add tags to note
crucible-cli note update research.md -p '{"tags": ["#research", "#ml"]}'

# Update multiple properties
crucible-cli note update draft.md -p '{"status": "review", "priority": "high", "due_date": "2024-01-30"}'

# Update with complex JSON
crucible-cli note update project.md -p '{
  "tags": ["#project", "#active"],
  "metadata": {
    "client": "Acme Corp",
    "budget": 50000,
    "timeline": "Q1 2024"
  }
}'
```

#### `note list` - List Notes

**Description**: List all notes in the vault

**Usage**:
```bash
crucible-cli note list [options]
```

**Options**:
| Option | Short | Long | Default | Description |
|--------|-------|------|---------|-------------|
| Format | `-f` | `--format <format>` | table | Output format (plain, json, table) |

**Examples**:
```bash
# List notes in table format
crucible-cli note list

# List notes as JSON for scripting
crucible-cli note list --format json | jq '.[] | select(.tags | contains(["#project"]))'

# List notes in plain format
crucible-cli note list --format plain
```

### Vault Operations

#### `index` - Index Vault

**Description**: Index vault for search and embeddings

**Usage**:
```bash
crucible-cli index [path] [options]
```

**Options**:
| Option | Short | Long | Description |
|--------|-------|------|-------------|
| Force | `-F` | `--force` | Force re-indexing of all files |
| Glob | `-g` | `--glob <pattern>` | File pattern (default: **/*.md) |

**Examples**:
```bash
# Index current vault
crucible-cli index

# Index specific path
crucible-cli index ~/Documents/vault

# Force re-indexing
crucible-cli index --force

# Index only specific file types
crucible-cli index --glob "**/*.md" --glob "**/*.txt"

# Index with custom pattern
crucible-cli index --glob "notes/**/*.md"
```

#### `stats` - Vault Statistics

**Description**: Display comprehensive vault statistics

**Usage**:
```bash
crucible-cli stats
```

**Examples**:
```bash
# Show vault statistics
crucible-cli stats

# Export statistics as JSON
crucible-cli stats --format json | jq '.total_notes'
```

**Output includes**:
- Total number of notes
- Notes with embeddings
- Vault size
- Last indexed date
- Database statistics
- Tag distribution

### Script Execution

#### `run` - Execute Rune Script

**Description**: Run a Rune script as a command

**Usage**:
```bash
crucible-cli run <script> [options]
```

**Options**:
| Option | Short | Long | Description |
|--------|-------|------|-------------|
| Arguments | | `--args <json>` | Arguments to pass to the script |

**Examples**:
```bash
# Run script with no arguments
crucible-cli run hello-world.rn

# Run script with arguments
crucible-cli run data-analysis.rn --args '{"input_file": "data.csv", "output_format": "json"}'

# Run script by name (searches standard locations)
crucible-cli run search-tool

# Run script with complex arguments
crucible-cli run report-generator.rn --args '{
  "template": "monthly",
  "date_range": {
    "start": "2024-01-01",
    "end": "2024-01-31"
  },
  "include_charts": true
}'
```

#### `commands` - List Available Commands

**Description**: List available Rune commands

**Usage**:
```bash
crucible-cli commands
```

**Examples**:
```bash
# List all available commands
crucible-cli commands

# Filter commands with grep
crucible-cli commands | grep search
```

### AI Chat Integration

#### `chat` - Interactive AI Chat

**Description**: Interactive chat mode with AI agents

**Usage**:
```bash
crucible-cli chat [options]
```

**Options**:
| Option | Short | Long | Default | Description |
|--------|-------|------|---------|-------------|
| Agent | `-a` | `--agent <name>` | default | Agent name for conversation |
| Model | `-m` | `--model <model>` | | Model override |
| Temperature | `-t` | `--temperature <temp>` | | Temperature (0.0-2.0) |
| Max Tokens | | `--max-tokens <N>` | | Maximum response tokens |
| No Stream | | `--no-stream` | false | Disable streaming responses |
| Start Message | `-s` | `--start-message <msg>` | | Start with specific message |
| History | | `--history <file>` | | Load conversation history |

**Examples**:
```bash
# Start chat with default agent
crucible-cli chat

# Chat with researcher agent
crucible-cli chat --agent researcher

# Chat with custom temperature
crucible-cli chat --agent writer --temperature 0.8

# Start chat with specific message
crucible-cli chat --agent researcher --start-message "Help me analyze my research notes"

# Chat with token limit
crucible-cli chat --max-tokens 1000

# Load conversation history
crucible-cli chat --history conversation.json

# Non-streaming chat (wait for complete response)
crucible-cli chat --no-stream
```

**Available Agents**:
- `default` - General purpose conversation
- `researcher` - Research and analysis focused
- `writer` - Content creation and editing

### Configuration Management

#### `config init` - Initialize Configuration

**Description**: Initialize a new config file

**Usage**:
```bash
crucible-cli config init [options]
```

**Options**:
| Option | Short | Long | Description |
|--------|-------|------|-------------|
| Path | | `--path <path>` | Path for the config file |
| Force | `-F` | `--force` | Overwrite existing config file |

**Examples**:
```bash
# Initialize config in default location
crucible-cli config init

# Initialize config in custom location
crucible-cli config init --path ~/.crucible/config.toml

# Force overwrite existing config
crucible-cli config init --force
```

#### `config show` - Show Configuration

**Description**: Show the current effective configuration

**Usage**:
```bash
crucible-cli config show [options]
```

**Options**:
| Option | Short | Long | Default | Description |
|--------|-------|------|---------|-------------|
| Format | `-f` | `--format <format>` | toml | Output format (toml, json) |

**Examples**:
```bash
# Show configuration in TOML format
crucible-cli config show

# Show configuration in JSON format
crucible-cli config show --format json

# Export specific section
crucible-cli config show --format json | jq '.vault'
```

## Service Management Commands

### Service Health

#### `service health` - Service Health Status

**Description**: Show service health status

**Usage**:
```bash
crucible-cli service health [service] [options]
```

**Arguments**:
| Argument | Description |
|----------|-------------|
| service | Service name (optional - shows all services if not specified) |

**Options**:
| Option | Short | Long | Default | Description |
|--------|-------|------|---------|-------------|
| Format | `-f` | `--format <format>` | table | Output format (table, json) |
| Detailed | `-d` | `--detailed` | false | Show detailed health information |

**Examples**:
```bash
# Show health of all services
crucible-cli service health

# Show health of specific service
crucible-cli service health crucible-script-engine

# Show detailed health information
crucible-cli service health --detailed

# Show health in JSON format
crucible-cli service health --format json

# Show detailed health of specific service
crucible-cli service health crucible-script-engine --detailed
```

### Service Metrics

#### `service metrics` - Service Metrics

**Description**: Show service performance metrics

**Usage**:
```bash
crucible-cli service metrics [service] [options]
```

**Arguments**:
| Argument | Description |
|----------|-------------|
| service | Service name (optional - shows all services if not specified) |

**Options**:
| Option | Short | Long | Default | Description |
|--------|-------|------|---------|-------------|
| Format | `-f` | `--format <format>` | table | Output format (table, json) |
| Real Time | `-r` | `--real-time` | false | Show real-time metrics |

**Examples**:
```bash
# Show metrics for all services
crucible-cli service metrics

# Show metrics for specific service
crucible-cli service metrics crucible-script-engine

# Show real-time metrics (continuous updates)
crucible-cli service metrics --real-time

# Show metrics in JSON format
crucible-cli service metrics --format json

# Real-time metrics for specific service
crucible-cli service metrics crucible-script-engine --real-time
```

### Service Lifecycle

#### `service start` - Start Service

**Description**: Start a service

**Usage**:
```bash
crucible-cli service start <service> [options]
```

**Arguments**:
| Argument | Description |
|----------|-------------|
| service | Service name |

**Options**:
| Option | Short | Long | Description |
|--------|-------|------|-------------|
| Wait | | `--wait` | Wait for service to be ready |

**Examples**:
```bash
# Start service
crucible-cli service start crucible-script-engine

# Start service and wait for it to be ready
crucible-cli service start crucible-script-engine --wait
```

#### `service stop` - Stop Service

**Description**: Stop a service

**Usage**:
```bash
crucible-cli service stop <service> [options]
```

**Arguments**:
| Argument | Description |
|----------|-------------|
| service | Service name |

**Options**:
| Option | Short | Long | Description |
|--------|-------|------|-------------|
| Force | | `--force` | Force stop (graceful shutdown if false) |

**Examples**:
```bash
# Stop service gracefully
crucible-cli service stop crucible-script-engine

# Force stop service
crucible-cli service stop crucible-script-engine --force
```

#### `service restart` - Restart Service

**Description**: Restart a service

**Usage**:
```bash
crucible-cli service restart <service> [options]
```

**Arguments**:
| Argument | Description |
|----------|-------------|
| service | Service name |

**Options**:
| Option | Short | Long | Description |
|--------|-------|------|-------------|
| Wait | | `--wait` | Wait for service to be ready |

**Examples**:
```bash
# Restart service
crucible-cli service restart crucible-script-engine

# Restart service and wait for it to be ready
crucible-cli service restart crucible-script-engine --wait
```

### Service Discovery

#### `service list` - List Services

**Description**: List all registered services

**Usage**:
```bash
crucible-cli service list [options]
```

**Options**:
| Option | Short | Long | Default | Description |
|--------|-------|------|---------|-------------|
| Format | `-f` | `--format <format>` | table | Output format (table, json) |
| Status | `-s` | `--status` | false | Show service status |
| Detailed | `-d` | `--detailed` | false | Show detailed information |

**Examples**:
```bash
# List all services
crucible-cli service list

# List services with status
crucible-cli service list --status

# List services with detailed information
crucible-cli service list --detailed

# List services in JSON format
crucible-cli service list --format json

# List services with status and detailed info
crucible-cli service list --status --detailed
```

### Service Logs

#### `service logs` - Service Logs

**Description**: Show service logs

**Usage**:
```bash
crucible-cli service logs [service] [options]
```

**Arguments**:
| Argument | Description |
|----------|-------------|
| service | Service name (optional - shows all services if not specified) |

**Options**:
| Option | Short | Long | Default | Description |
|--------|-------|------|---------|-------------|
| Lines | `-n` | `--lines <N>` | 100 | Number of lines to show |
| Follow | `-f` | `--follow` | false | Follow log output |
| Errors | | `--errors` | false | Show only errors |

**Examples**:
```bash
# Show recent logs from all services
crucible-cli service logs

# Show logs from specific service
crucible-cli service logs crucible-script-engine

# Show more lines
crucible-cli service logs --lines 1000

# Follow logs in real-time
crucible-cli service logs --follow

# Show only error logs
crucible-cli service logs --errors

# Follow logs from specific service
crucible-cli service logs crucible-script-engine --follow

# Show error logs from specific service
crucible-cli service logs crucible-script-engine --errors --lines 50
```

## Migration Management Commands

### Migration Status

#### `migration status` - Migration Status

**Description**: Show migration status and statistics

**Usage**:
```bash
crucible-cli migration status [options]
```

**Options**:
| Option | Short | Long | Default | Description |
|--------|-------|------|---------|-------------|
| Format | `-f` | `--format <format>` | table | Output format (table, json) |
| Detailed | `-d` | `--detailed` | false | Show detailed migration information |
| Validate | | `--validate` | false | Validate migration integrity |

**Examples**:
```bash
# Show migration status
crucible-cli migration status

# Show detailed migration information
crucible-cli migration status --detailed

# Validate migration integrity
crucible-cli migration status --validate

# Show status in JSON format
crucible-cli migration status --format json

# Show detailed status with validation
crucible-cli migration status --detailed --validate
```

### Migration Operations

#### `migration migrate` - Start Migration

**Description**: Start migration of Rune tools to ScriptEngine service

**Usage**:
```bash
crucible-cli migration migrate [options]
```

**Options**:
| Option | Short | Long | Default | Description |
|--------|-------|------|---------|-------------|
| Tool | | `--tool <name>` | Tool name to migrate (migrates all if not specified) |
| Force | | `--force` | false | Force migration even if tool already exists |
| Security Level | | `--security-level <level>` | safe | Security level for migrated tools |
| Dry Run | | `--dry-run` | false | Dry run - show what would be migrated |

**Security Levels**:
- `safe` - Limited capabilities, sandboxed (default)
- `development` - Full access for testing and development
- `production` - Balanced security and functionality

**Examples**:
```bash
# Preview migration (dry run)
crucible-cli migration migrate --dry-run

# Migrate all tools
crucible-cli migration migrate

# Migrate specific tool
crucible-cli migration migrate --tool search-tool

# Force migration
crucible-cli migration migrate --force

# Set security level for migrated tools
crucible-cli migration migrate --security-level production

# Preview migration of specific tool
crucible-cli migration migrate --tool search-tool --dry-run

# Migrate with production security level
crucible-cli migration migrate --security-level production --force
```

#### `migration validate` - Validate Migration

**Description**: Validate migration integrity

**Usage**:
```bash
crucible-cli migration validate [options]
```

**Options**:
| Option | Short | Long | Default | Description |
|--------|-------|------|---------|-------------|
| Tool | | `--tool <name>` | Tool name to validate (validates all if not specified) |
| Auto Fix | | `--auto-fix` | false | Fix issues automatically if possible |
| Format | `-f` | `--format <format>` | table | Output format (table, json) |

**Examples**:
```bash
# Validate all migrations
crucible-cli migration validate

# Validate specific tool
crucible-cli migration validate --tool search-tool

# Auto-fix validation issues
crucible-cli migration validate --auto-fix

# Validate in JSON format
crucible-cli migration validate --format json

# Validate specific tool with auto-fix
crucible-cli migration validate --tool search-tool --auto-fix
```

#### `migration rollback` - Rollback Migration

**Description**: Rollback migrated tools

**Usage**:
```bash
crucible-cli migration rollback [options]
```

**Options**:
| Option | Short | Long | Default | Description |
|--------|-------|------|---------|-------------|
| Tool | | `--tool <name>` | Tool name to rollback (rollbacks all if not specified) |
| Confirm | | `--confirm` | false | Confirm rollback without prompt |
| Backup | | `--backup` | false | Keep backup of migrated tools |

**Examples**:
```bash
# Rollback all tools (with confirmation prompt)
crucible-cli migration rollback

# Rollback specific tool
crucible-cli migration rollback --tool search-tool

# Confirm rollback without prompt
crucible-cli migration rollback --confirm

# Keep backup during rollback
crucible-cli migration rollback --backup

# Rollback specific tool with confirmation
crucible-cli migration rollback --tool search-tool --confirm
```

### Migration Information

#### `migration list` - List Migrated Tools

**Description**: List migrated tools

**Usage**:
```bash
crucible-cli migration list [options]
```

**Options**:
| Option | Short | Long | Default | Description |
|--------|-------|------|---------|-------------|
| Format | `-f` | `--format <format>` | table | Output format (table, json) |
| Active | | `--active` | false | Show only active tools |
| Inactive | | `--inactive` | false | Show only inactive tools |
| Metadata | `-m` | `--metadata` | false | Show migration metadata |

**Examples**:
```bash
# List all migrated tools
crucible-cli migration list

# Show only active tools
crucible-cli migration list --active

# Show only inactive tools
crucible-cli migration list --inactive

# Show migration metadata
crucible-cli migration list --metadata

# List in JSON format
crucible-cli migration list --format json

# Show active tools with metadata
crucible-cli migration list --active --metadata
```

#### `migration reload` - Reload Migrated Tool

**Description**: Reload a migrated tool from its original source

**Usage**:
```bash
crucible-cli migration reload <tool> [options]
```

**Arguments**:
| Argument | Description |
|----------|-------------|
| tool | Tool name to reload |

**Options**:
| Option | Short | Long | Description |
|--------|-------|------|-------------|
| Force | | `--force` | Force reload even if source unchanged |

**Examples**:
```bash
# Reload tool
crucible-cli migration reload search-tool

# Force reload
crucible-cli migration reload search-tool --force
```

#### `migration cleanup` - Clean Up Migration Artifacts

**Description**: Clean up migration artifacts

**Usage**:
```bash
crucible-cli migration cleanup [options]
```

**Options**:
| Option | Short | Long | Description |
|--------|-------|------|-------------|
| Inactive | | `--inactive` | Remove inactive migrations |
| Failed | | `--failed` | Remove failed migrations |
| Confirm | | `--confirm` | Confirm cleanup without prompt |

**Examples**:
```bash
# Clean up migration artifacts (with confirmation)
crucible-cli migration cleanup

# Remove inactive migrations
crucible-cli migration cleanup --inactive

# Remove failed migrations
crucible-cli migration cleanup --failed

# Confirm cleanup without prompt
crucible-cli migration cleanup --confirm

# Remove inactive and failed migrations
crucible-cli migration cleanup --inactive --failed --confirm
```

## Configuration

### Configuration File

The CLI uses a hierarchical configuration system:

1. **Defaults**: Built-in sensible defaults
2. **Config File**: `~/.config/crucible/config.toml`
3. **Environment Variables**: `CRUCIBLE_VAULT_PATH`, etc.
4. **Command Line Arguments**: Highest priority

### Sample Configuration

```toml
[vault]
path = "~/Documents/vault"

[llm]
provider = "ollama"
model = "llama3.1"
base_url = "http://localhost:11434"

[embedding]
provider = "ollama"
model = "nomic-embed-text"
base_url = "http://localhost:11434"

[network]
timeout = 30
retries = 3

[services]
# ScriptEngine service configuration
[services.script_engine]
enabled = true
security_level = "safe"
max_source_size = 1048576  # 1MB
default_timeout_secs = 30
enable_caching = true
max_cache_size = 1000
max_memory_mb = 100
max_cpu_percentage = 80.0
max_concurrent_operations = 50

# Service discovery configuration
[services.discovery]
enabled = true
endpoints = ["localhost:8080"]
timeout_secs = 5
refresh_interval_secs = 30

# Service health monitoring configuration
[services.health]
enabled = true
check_interval_secs = 10
timeout_secs = 5
failure_threshold = 3
auto_recovery = true

[migration]
enabled = true
default_security_level = "safe"
auto_migrate = false
enable_caching = true
max_cache_size = 500
preserve_tool_ids = true
backup_originals = true

# Migration validation settings
[migration.validation]
auto_validate = true
strict = false
validate_functionality = true
validate_performance = false
max_performance_degradation = 20.0

[cli]
default_format = "table"
enable_colors = true
enable_syntax_highlighting = true
history_size = 1000
```

### Environment Variables

| Variable | Description |
|----------|-------------|
| `CRUCIBLE_VAULT_PATH` | Override vault path |
| `CRUCIBLE_CONFIG_PATH` | Override config file path |
| `CRUCIBLE_LOG_LEVEL` | Set log level (trace,debug,info,warn,error) |
| `CRUCIBLE_TEST_MODE` | Enable test mode (skip user config loading) |
| `CRUCIBLE_EMBEDDING_URL` | Override embedding service URL |
| `CRUCIBLE_EMBEDDING_MODEL` | Override embedding model |
| `CRUCIBLE_LLM_PROVIDER` | Override LLM provider |
| `CRUCIBLE_LLM_MODEL` | Override LLM model |
| `CRUCIBLE_LLM_BASE_URL` | Override LLM base URL |

## Advanced Usage

### Pipeline Operations

```bash
# Chain multiple operations
crucible-cli search "project" --format json | \
  jq '.[] | .path' | \
  xargs crucible-cli note get --format json | \
  jq '.content' | \
  crucible-cli run summarize.rn --args '{"mode": "bullet-points"}'

# Export and process data
crucible-cli stats --format json | \
  jq '.total_notes, .notes_with_embeddings' | \
  crucible-cli run create-report.rn --args '{"format": "markdown"}'

# Batch processing
for tag in "#research" "#project" "#idea"; do
  crucible-cli fuzzy "$tag" --format json | \
    jq -r '.[].path' | \
    xargs -I {} crucible-cli note update {} -p "{\"last_reviewed\": \"$(date -I)\"}"
done
```

### Integration with Other Tools

```bash
# Integration with text editors
crucible-cli search "TODO" --format plain | \
  vim - +":set ft=markdown" -

# Integration with git hooks
#!/bin/sh
# .git/hooks/pre-commit
crucible-cli index --force
crucible-cli migration validate --auto-fix

# Integration with backup systems
crucible-cli stats --format json > backup/stats-$(date +%Y%m%d).json
crucible-cli migration list --metadata > backup/migrations-$(date +%Y%m%d).json
```

### Performance Optimization

```bash
# Use JSON output for better performance with large datasets
crucible-cli search "query" --format json --limit 1000 | \
  jq '.[] | select(.created > "2024-01-01")'

# Limit output fields for faster processing
crucible-cli note list --format json | \
  jq '.[] | {path, title, tags}'

# Use parallel processing for batch operations
crucible-cli fuzzy "pattern" --format json | \
  jq -r '.[].path' | \
  xargs -P 4 -I {} crucible-cli note get {} --format plain
```

## Troubleshooting

### Common Issues

1. **Service Not Found**
   ```
   Error: Service not found: crucible-script-engine
   ```
   **Solution**: Check available services:
   ```bash
   crucible-cli service list
   ```

2. **Migration Disabled**
   ```
   Error: Migration is disabled in configuration
   ```
   **Solution**: Enable migration in config:
   ```toml
   [migration]
   enabled = true
   ```

3. **Database Lock Error**
   ```
   Error: Database is locked
   ```
   **Solution**: Ensure no other Crucible processes are running

4. **Configuration Issues**
   ```
   Error: Invalid configuration
   ```
   **Solution**: Show and validate configuration:
   ```bash
   crucible-cli config show
   ```

### Debug Mode

Enable debug logging for troubleshooting:

```bash
# Enable debug logging
RUST_LOG=debug crucible-cli --verbose service health

# Enable trace logging for detailed debugging
RUST_LOG=trace crucible-cli --verbose migration status

# Debug specific operations
RUST_LOG=debug crucible-cli --verbose run script.rn
```

### Test Mode

Use test mode to avoid loading user configuration:

```bash
# Run in test mode
CRUCIBLE_TEST_MODE=1 crucible-cli service list

# Test mode with debug logging
CRUCIBLE_TEST_MODE=1 RUST_LOG=debug crucible-cli migration status
```

## Performance Tips

1. **Use JSON Output**: Faster for scripting and large datasets
2. **Limit Results**: Use `--limit` to avoid overwhelming output
3. **Enable Caching**: Ensure script caching is enabled for better performance
4. **Batch Operations**: Process multiple items at once when possible
5. **Use Appropriate Formats**: Choose output formats based on use case

## Getting Help

- **Command Help**: Use `--help` with any command
- **REPL Help**: Use `:help` in the interactive REPL
- **Configuration Help**: Use `crucible-cli config show` to see current settings
- **Debug Information**: Use `--verbose` and `RUST_LOG=debug` for troubleshooting

---

For more information, see:
- [Architecture Documentation](./ARCHITECTURE.md)
- [ScriptEngine API Documentation](./SCRIPTENGINE_API.md)
- [Migration Guide](./MIGRATION_GUIDE.md)
- [Service Integration Guide](../crates/crucible-cli/CLI_SERVICE_INTEGRATION.md)