# Crucible MCP Server

A Model Context Protocol (MCP) server for the Crucible knowledge management system. This server provides AI agents with tools to search, index, and manage documents using real embeddings from Ollama or OpenAI providers.

> **✨ Now with real embeddings!** Choose between Ollama (free, local) or OpenAI for semantic search capabilities.

## Features

### 🤖 Embedding Providers
- **Ollama Provider** - Free, local, privacy-preserving (nomic-embed-text, 768 dims)
- **OpenAI Provider** - Cloud-based, high-quality (text-embedding-3-small, 1536 dims)
- **Provider Abstraction** - Easy to add new providers (Cohere, Azure, Hugging Face, etc.)
- **Batch Processing** - Efficient bulk embedding generation
- **Retry Logic** - Automatic exponential backoff on failures

### Native MCP Tools
- **search_by_properties** - Search notes by frontmatter properties via Obsidian API
- **search_by_tags** 🌐 - Search notes by tags via Obsidian API (real-time results)
- **list_notes_in_folder** - Search notes in specific folders via Obsidian API
- **search_by_filename** - Search notes by filename patterns
- **search_by_content** - Full-text search in note contents
- **semantic_search** 🔥 - Vector similarity search using real embeddings
- **build_search_index** 🔥🌐 - Generate real embeddings for vault notes via Obsidian API
- **get_note_metadata** - Get metadata for specific notes
- **update_note_properties** - Update note frontmatter properties
- **get_vault_stats** - Get statistics about indexed documents

🔥 = Uses real embeddings from configured provider
🌐 = Uses Obsidian plugin HTTP API for live data

### Rune Tools (Dynamic Extensions)
When `RUNE_TOOL_DIR` is configured:
- **create_note** 📝 - Create new markdown notes with frontmatter and tags
- Custom tools can be added as `.rn` scripts without recompiling the server

📝 = Rune-scripted tool (hot-reloadable)

### Crucible Integration Tools
- **index_document** 🔥 - Index Crucible documents with real embeddings
- **search_documents** 🔥 - Semantic search across indexed documents
- **get_document_stats** - Get statistics about indexed documents
- **update_document_properties** - Update Crucible document properties

## Architecture

### Obsidian Integration Layer 🆕
- **HTTP API Client** - Direct integration with Obsidian plugin REST API
- **Live Data Access** - Real-time access to vault files, tags, and metadata
- **No Filesystem Access** - All data fetched via Obsidian HTTP API (port 27123)
- **Tag Search** - Direct queries to Obsidian for instant tag-based filtering
- **Metadata Sync** - Automatic sync of frontmatter properties and tags

### Embedding Layer
- **Provider Abstraction** - Trait-based design for multiple backends
- **Ollama Support** - Local, free embedding generation
- **OpenAI Support** - Cloud-based premium embeddings
- **Configuration** - Environment-based provider selection
- **Error Handling** - Comprehensive retry and fallback logic

### Database Layer
- **DuckDB** - High-performance analytical database with vector operations
- **VSS Extension** - Vector similarity search capabilities
- **JSON Storage** - Flexible metadata and embedding storage (variable dimensions)

### Protocol Layer
- **rmcp SDK** - Official Rust MCP SDK with tool_router macro
- **JSON-RPC 2.0** - Standard protocol for MCP communication
- **stdio Transport** - Communication over standard input/output
- **Error Handling** - Comprehensive error responses and logging

### Integration Layer
- **Tauri Integration** - Native desktop app communication
- **Core Types** - Integration with Crucible document types
- **Async Runtime** - Tokio-based async operations

## Usage

### As a Library

```rust
use crucible_mcp::{McpServer, EmbeddingConfig, create_provider};

// Create embedding provider (Ollama by default)
let config = EmbeddingConfig::default();
let provider = create_provider(config).await?;

// Create a server instance with provider
let server = McpServer::new("crucible.db", provider.clone()).await?;

// Handle tool calls
let result = server.handle_tool_call(
    "semantic_search",
    serde_json::json!({
        "query": "machine learning",
        "top_k": 5
    })
).await?;

// Start stdio MCP server with provider
McpServer::start_stdio("crucible.db", provider).await?;
```

### As a Standalone Binary

```bash
# Build the server
cargo build --bin crucible-mcp-server

# Run with default database
./target/debug/crucible-mcp-server

# Run with custom database path
./target/debug/crucible-mcp-server /path/to/database.db
```

### With MCP Clients

The server implements the standard MCP protocol and can be used with any MCP-compatible client:

```json
{
  "tools": [
    {
      "name": "semantic_search",
      "description": "Semantic search using embeddings",
      "inputSchema": {
        "type": "object",
        "properties": {
          "query": {"type": "string"},
          "top_k": {"type": "integer", "default": 10}
        },
        "required": ["query"]
      }
    }
  ]
}
```

## Database Schema

### Embeddings Table
```sql
CREATE TABLE embeddings (
    id BIGINT PRIMARY KEY,
    file_path TEXT UNIQUE NOT NULL,
    content TEXT NOT NULL,
    embedding JSON NOT NULL,  -- Vector embeddings stored as JSON
    metadata JSON NOT NULL,   -- File metadata and properties
    created_at TIMESTAMP,
    updated_at TIMESTAMP
);
```

### Metadata Structure
```json
{
  "file_path": "document.md",
  "title": "Document Title",
  "tags": ["tag1", "tag2"],
  "folder": "folder/path",
  "properties": {
    "key": "value",
    "document_id": "uuid",
    "type": "crucible_document"
  },
  "created_at": "2024-01-01T00:00:00Z",
  "updated_at": "2024-01-01T00:00:00Z"
}
```

## Configuration

### Environment Variables

#### Embedding Provider Configuration
- `EMBEDDING_PROVIDER` - Provider type: "ollama" (default) or "openai"
- `EMBEDDING_MODEL` - Model name (e.g., "nomic-embed-text-v1.5-q8_0" for Ollama)
- `EMBEDDING_ENDPOINT` - API endpoint URL (e.g., "https://llama.terminal.krohnos.io")
- `EMBEDDING_BATCH_SIZE` - Batch size for bulk operations (default: 1)
- `OPENAI_API_KEY` - API key for OpenAI provider (if using OpenAI)

#### Obsidian Plugin Configuration
- `OBSIDIAN_API_PORT` - Obsidian plugin HTTP API port (default: 27123)
- `OBSIDIAN_VAULT_PATH` - Path to Obsidian vault (for display purposes)

#### Rune Tools Configuration
- `RUNE_TOOL_DIR` - Directory containing .rn script tools for dynamic extensibility (optional)
  - If not set, only native tools will be available
  - Example: `/home/user/crucible/crates/crucible-mcp/tools`
  - Enables scripted tools like `create_note` without recompiling

#### General Configuration
- `RUST_LOG` - Set logging level (debug, info, warn, error)
- `MCP_DB_PATH` - Default database path (default: "crucible.db")

### Logging
```bash
# Enable debug logging
RUST_LOG=debug ./crucible-mcp-server

# Enable info logging for specific modules
RUST_LOG=crucible_mcp=info ./crucible-mcp-server
```

## Development

### Running Tests
```bash
# Run all tests
cargo test

# Run specific test module
cargo test test_rmcp_tools

# Run integration tests (requires Obsidian running)
cargo test --test test_tag_search -- --ignored

# Run with logging
RUST_LOG=debug cargo test
```

### Building
```bash
# Build library
cargo build

# Build binary
cargo build --bin crucible-mcp-server

# Build release
cargo build --release
```

### Dependencies
- **crucible-core** - Core Crucible types and functionality
- **duckdb** - Database with vector search capabilities
- **tokio** - Async runtime
- **serde/serde_json** - Serialization
- **jsonrpc-core** - JSON-RPC 2.0 implementation
- **tracing** - Structured logging
- **anyhow** - Error handling

## MCP Protocol Compliance

This server implements the MCP (Model Context Protocol) specification:

- ✅ **Initialize** - Server initialization and capability negotiation
- ✅ **List Tools** - Enumerate available tools
- ✅ **Call Tool** - Execute tools with proper error handling
- ✅ **JSON-RPC 2.0** - Standard transport protocol
- ✅ **stdio Transport** - Standard I/O communication
- ✅ **Error Handling** - Comprehensive error responses
- ✅ **Notifications** - Support for client notifications

## Performance

### Benchmarks
- **Indexing**: ~1000 documents/second (dummy embeddings)
- **Search**: ~100ms for semantic search across 10k documents
- **Memory**: ~50MB base memory usage
- **Storage**: ~1KB per document (excluding embeddings)

### Optimizations
- Connection pooling for concurrent operations
- Batch processing for bulk indexing
- Efficient vector similarity calculations
- JSON-based flexible metadata storage

## Recent Improvements

### ✅ Completed (October 2025)
- [x] Real embedding model integration (Ollama + OpenAI providers)
- [x] Obsidian HTTP API integration for all vault operations
- [x] Tag search via Obsidian API (real-time results)
- [x] Vault indexing via Obsidian API (no filesystem access)
- [x] rmcp SDK integration with tool_router pattern
- [x] Comprehensive integration tests for tag search

### Documented Plans
See `/docs/plans/VAULT_INDEXING_API_MIGRATION.md` for detailed optimization roadmap

## Roadmap

### Short Term
- [ ] Parallel file fetching for large vault indexing (5-10x speedup)
- [ ] Incremental indexing with modification time detection
- [ ] Full-text search with FTS5
- [ ] Structured error reporting with partial success details

### Medium Term
- [ ] Provider-specific content length optimization
- [ ] Progress reporting for long operations
- [ ] Vector quantization for storage efficiency
- [ ] Prometheus metrics and monitoring

### Long Term
- [ ] Distributed search across multiple nodes
- [ ] Real-time collaborative indexing
- [ ] Advanced RAG (Retrieval Augmented Generation) features
- [ ] Multi-provider embedding ensemble search

## Contributing

1. Fork the repository
2. Create a feature branch
3. Add tests for new functionality
4. Ensure all tests pass
5. Submit a pull request

## License

MIT OR Apache-2.0
