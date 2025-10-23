# Frequently Asked Questions (FAQ)

> **Status**: Active FAQ
> **Version**: 1.0.0
> **Date**: 2025-10-23
> **Purpose**: Common questions and answers about Crucible

## Table of Contents

- [General Questions](#general-questions)
- [Installation and Setup](#installation-and-setup)
- [Usage and Features](#usage-and-features)
- [Technical Questions](#technical-questions)
- [Troubleshooting](#troubleshooting)
- [Development](#development)

## General Questions

### Q: What is Crucible?

**A**: Crucible is a high-performance knowledge management system that combines hierarchical organization, real-time collaboration, and AI agent integration. It's built for linked thinking - the seamless connection and evolution of ideas across time and context.

### Q: What makes Crucible different from other knowledge management tools?

**A**: Key differentiators include:
- **ScriptEngine Architecture**: Production-ready script execution with security isolation
- **Semantic Search**: Advanced search with embeddings and fuzzy matching
- **AI Integration**: Multiple AI agents for research, writing, and analysis
- **Real-time Collaboration**: CRDT-based collaboration with Yjs
- **CLI-First Design**: Powerful command-line interface with interactive REPL
- **High Performance**: 83% complexity reduction with simplified architecture

### Q: Is Crucible open source?

**A**: Crucible is currently proprietary software. Please see the license in the repository for specific terms.

### Q: What platforms does Crucible support?

**A**: Currently supported platforms:
- **Linux**: Primary development platform
- **macOS**: Supported with minor variations
- **Windows**: Limited support (development in progress)

## Installation and Setup

### Q: How do I install Crucible?

**A**: Basic installation:
```bash
# Clone the repository
git clone https://github.com/matthewkrohn/crucible.git
cd crucible

# Build and install
cargo build -p crucible-cli
cargo install --path crates/crucible-cli
```

For detailed installation instructions, see the [Developer Guide](./DEVELOPER_GUIDE.md).

### Q: What are the system requirements?

**A**: Minimum requirements:
- **Rust**: 1.70.0 or later
- **Memory**: 4GB RAM (8GB recommended)
- **Storage**: 1GB free space
- **OS**: Linux (Ubuntu 20.04+), macOS (10.15+)

### Q: Installation fails with compilation errors

**A**: This is a known issue with the current version. Try these solutions:

1. **Install specific components**:
   ```bash
   cargo build -p crucible-cli
   cargo build -p crucible-core
   ```

2. **Update Rust toolchain**:
   ```bash
   rustup update
   ```

3. **Use the working version**:
   ```bash
   cargo run -p crucible-cli -- --help
   ```

For more detailed troubleshooting, see the [Troubleshooting Guide](./TROUBLESHOOTING.md).

## Usage and Features

### Q: How do I get started with Crucible?

**A**: Quick start:
```bash
# Start the interactive REPL (default behavior)
crucible-cli

# Show available commands
crucible-cli --help

# Search your notes
crucible-cli search "your query"
```

### Q: What can I do with the REPL?

**A**: The REPL provides:
- **SurrealQL Queries**: Direct database queries
- **Built-in Commands**: `:tools`, `:stats`, `:config`, `:help`
- **Script Execution**: Run Rune scripts
- **Interactive Search**: Fuzzy search and semantic search

Example:
```sql
-- Search in REPL
SELECT * FROM notes ORDER BY created DESC LIMIT 10;

-- Built-in command
:tools
```

### Q: How does semantic search work?

**A**: Semantic search uses embeddings to find conceptually similar content:

```bash
# Basic semantic search
crucible-cli semantic "machine learning concepts"

# With similarity scores
crucible-cli semantic "data analysis" --show-scores

# Limit results
crucible-cli semantic "project management" --top-k 5
```

### Q: What are Rune scripts?

**A**: Rune is a scripting language integrated with Crucible for:
- **Custom Tools**: Create specialized tools
- **Automation**: Script repetitive tasks
- **Data Processing**: Transform and analyze data
- **Integration**: Connect with external services

Example:
```bash
# Run a Rune script
crucible-cli run my-script.rn

# List available scripts
crucible-cli commands
```

### Q: How do I manage services?

**A**: Crucible provides service management commands:

```bash
# Check service health
crucible-cli service health --detailed

# List all services
crucible-cli service list --status

# Start a service
crucible-cli service start crucible-script-engine

# View service logs
crucible-cli service logs --follow
```

### Q: What is the migration system?

**A**: The migration system handles tool migrations between versions:
- **Automated Migration**: Zero-touch tool migration
- **Validation**: Ensures migration integrity
- **Rollback**: Revert problematic migrations
- **Multiple Security Levels**: Safe, Development, Production modes

```bash
# Check migration status
crucible-cli migration status --detailed

# Run migration (dry run first)
crucible-cli migration migrate --dry-run
crucible-cli migration migrate

# Validate migration
crucible-cli migration validate --auto-fix
```

## Technical Questions

### Q: What databases does Crucible use?

**A**: Crucible uses multiple database systems:
- **SurrealDB**: Primary database for documents and metadata
- **DuckDB**: Analytics and vector operations
- **File System**: File-based storage for notes and media

### Q: How does real-time collaboration work?

**A**: Real-time collaboration uses:
- **Yjs**: CRDT library for conflict-free replication
- **WebSockets**: Real-time communication
- **Operational Transforms**: Efficient conflict resolution

### Q: What is the ScriptEngine architecture?

**A**: The ScriptEngine architecture provides:
- **Simplified Design**: 83% complexity reduction
- **Security Isolation**: Sandboxed script execution
- **Performance Monitoring**: Resource limits and metrics
- **Event System**: Service coordination and health monitoring

### Q: How are embeddings generated?

**A**: Crucible supports multiple embedding providers:
- **Local Models**: Ollama and local transformers
- **Cloud Services**: OpenAI, Cohere, etc.
- **Custom Models**: Your own embedding models

### Q: Can I use Crucible without AI features?

**A**: Yes, all AI features are optional:
- **Local Search**: Full-text and fuzzy search work locally
- **CLI Tools**: All CLI commands work without AI
- **Database Operations**: Direct database access available

## Troubleshooting

### Q: Crucible is running slowly, what can I do?

**A**: Performance optimization tips:

1. **Rebuild search index**:
   ```bash
   crucible-cli index --rebuild
   ```

2. **Limit concurrent operations**:
   ```bash
   crucible-cli --max-concurrent 2
   ```

3. **Use specific search types**:
   ```bash
   crucible-cli fuzzy "query" --no-content
   ```

4. **Clear cache**:
   ```bash
   crucible-cli cache --clear
   ```

### Q: Search returns no results

**A**: Common solutions:

1. **Check vault path**:
   ```bash
   crucible-cli --vault-path /correct/path search "query"
   ```

2. **Rebuild index**:
   ```bash
   crucible-cli index --force-rebuild
   ```

3. **Check file permissions**:
   ```bash
   ls -la /path/to/vault
   ```

### Q: Database connection errors

**A**: Database troubleshooting:

1. **Check database status**:
   ```bash
   crucible-cli db test
   ```

2. **Recreate database**:
   ```bash
   rm ~/.local/share/crucible/db/*
   crucible-cli index --rebuild
   ```

3. **Check database path**:
   ```bash
   crucible-cli --db-path /custom/path
   ```

## Development

### Q: How can I contribute to Crucible?

**A**: See the [Contributing Guide](../CONTRIBUTING.md) for detailed information:
- Code contributions
- Documentation improvements
- Bug reports
- Feature requests

### Q: What are the development requirements?

**A**: Development setup:
```bash
# Install dependencies
rustup update
rustup component add clippy rustfmt
npm install -g pnpm

# Clone and setup
git clone https://github.com/your-username/crucible.git
cd crucible
./scripts/setup.sh
```

### Q: How do I run tests?

**A**: Testing commands:
```bash
# Run all tests
cargo test

# Run specific test
cargo test test_name

# Run with coverage
cargo tarpaulin --out Html

# Run benchmarks
cargo bench
```

### Q: How do I build documentation?

**A**: Documentation build:
```bash
# Build Rust documentation
cargo doc --no-deps

# Open in browser
cargo doc --open

# Build specific crate docs
cargo doc -p crucible-cli
```

### Q: What are the coding standards?

**A**: Code standards:
- **rustfmt**: Automatic formatting
- **clippy**: Linting with `cargo clippy -- -D warnings`
- **Documentation**: All public APIs must have documentation
- **Tests**: New features require comprehensive tests

See the [Contributing Guide](../CONTRIBUTING.md) for detailed standards.

## Advanced Questions

### Q: Can I extend Crucible with custom tools?

**A**: Yes, you can create custom tools:
- **Rune Scripts**: Dynamic tools using Rune language
- **Rust Plugins**: Compile-time tool extensions
- **Service Integration**: Connect external services

### Q: How does Crucible handle large datasets?

**A**: Large dataset handling:
- **Streaming**: Processes large files without loading everything into memory
- **Indexing**: Efficient indexing for fast search
- **Pagination**: Limits result sets for memory efficiency
- **Background Processing**: Heavy operations run in background

### Q: Can I integrate Crucible with other tools?

**A**: Integration options:
- **CLI API**: Command-line interface for scripting
- **Service APIs**: RESTful APIs for external integration
- **Webhooks**: Event-driven integrations
- **Database Access**: Direct database queries

### Q: How secure is Crucible?

**A**: Security features:
- **Sandboxing**: Isolated script execution
- **Access Control**: Multiple security levels
- **Encryption**: Data encryption at rest and in transit
- **Audit Logging**: Comprehensive activity logging

---

## Still Have Questions?

If your question isn't answered here:

1. **Check Documentation**:
   - [API Documentation](./API_DOCUMENTATION.md)
   - [CLI Reference](./CLI_REFERENCE.md)
   - [Architecture Guide](./ARCHITECTURE.md)

2. **Search Issues**: Check existing [GitHub Issues](https://github.com/matthewkrohn/crucible/issues)

3. **Ask the Community**: Create a [GitHub Discussion](https://github.com/matthewkrohn/crucible/discussions)

4. **Report Problems**: [File an issue](https://github.com/matthewkrohn/crucible/issues/new) for bugs

---

*Last updated: 2025-10-23*