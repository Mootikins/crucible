# MCP Embeddings Integration - Work In Progress Plan

## 🎯 Project Goal

Implement real embeddings support in the Crucible MCP server with provider abstraction for both Ollama API (https://llama.terminal.krohnos.io) and OpenAI API.

## ✅ Completed Work

### Phase 1: Architecture & Provider Implementation (COMPLETE)

**1. Provider Architecture** ✅
- **File**: `src/embeddings/provider.rs` (482 lines)
- Implemented `EmbeddingProvider` trait with async methods
- Created `EmbeddingResponse` struct with validation and similarity methods
- Object-safe trait for `Arc<dyn EmbeddingProvider>` usage
- Comprehensive documentation with examples

**2. Configuration System** ✅
- **File**: `src/embeddings/config.rs` (225 lines)
- `EmbeddingConfig` struct with environment variable loading
- `ProviderType` enum (Ollama, OpenAI)
- Validation logic with API key requirement checks
- Default endpoints and model configurations

**3. Error Handling** ✅
- **File**: `src/embeddings/error.rs` (116 lines)
- Comprehensive error types with `thiserror`
- Retryable error classification
- Retry delay calculation for exponential backoff

**4. Ollama Provider** ✅
- **File**: `src/embeddings/ollama.rs` (348 lines)
- Full implementation of `EmbeddingProvider` trait
- HTTP client with timeout and retry logic
- API endpoint: `POST {endpoint}/api/embeddings`
- Default model: `nomic-embed-text` (768 dimensions)
- Health check and batch processing
- 5 unit tests, 3 integration tests

**5. OpenAI Provider** ✅
- **File**: `src/embeddings/openai.rs` (280 lines)
- Full implementation of `EmbeddingProvider` trait
- Bearer token authentication
- API endpoint: `POST {endpoint}/embeddings`
- Default model: `text-embedding-3-small` (1536 dimensions)
- Error handling for rate limits, auth errors, etc.
- Batch processing with index sorting

**6. Module Organization** ✅
- **File**: `src/embeddings/mod.rs` (55 lines)
- Public API exports
- Factory function `create_provider(config)`
- Module exposed in `src/lib.rs`

### Build Status
- ✅ **Compiles successfully** with no errors
- ✅ **All tests pass** (Ollama: 5 unit tests, OpenAI: complete)
- ✅ **Zero warnings** in provider implementations

## 🔄 Remaining Work

### Phase 2: Integration into MCP Server

**Task 1: Update Protocol Layer**
- **File**: `src/protocol.rs`
- **Status**: NOT STARTED
- **Changes Needed**:
  - Add `provider: Arc<dyn EmbeddingProvider>` field to `StdioMcpServer`
  - Update `initialize()` to accept and store provider
  - Pass provider to tools during tool calls

**Task 2: Update Tools Layer**
- **File**: `src/tools/mod.rs`
- **Status**: NOT STARTED
- **Changes Needed**:
  - Replace `generate_dummy_embedding()` function (line 624) with provider-based approach
  - Update `semantic_search()` (line 218) - accept provider parameter, use `provider.embed()`
  - Update `index_vault()` (line 253) - use provider for embeddings
  - Update `index_document()` (line 416) - use provider for embeddings
  - Update `search_documents()` (line 482) - use provider for embeddings
  - Modify function signatures to accept provider: `&Arc<dyn EmbeddingProvider>`

**Task 3: Update Main Entry Point**
- **File**: `src/main.rs`
- **Status**: NOT STARTED
- **Changes Needed**:
  - Import embedding modules
  - Load `EmbeddingConfig::from_env()`
  - Create provider via `create_provider(config).await?`
  - Log provider configuration (provider name, model, dimensions)
  - Pass provider to `StdioMcpServer::initialize()`
  - Handle provider initialization errors gracefully

### Phase 3: Testing & Documentation

**Task 4: Integration Testing**
- **Status**: NOT STARTED
- **Tests Needed**:
  - Test semantic search with real embeddings
  - Test index_vault with provider
  - Test both Ollama and OpenAI providers end-to-end
  - Test error handling (invalid API key, network failures)
  - Test configuration loading from environment

**Task 5: Documentation**
- **Status**: NOT STARTED
- **Documents Needed**:
  - Configuration guide with environment variables
  - Claude Desktop config example
  - API endpoint setup instructions
  - Model selection guide
  - Troubleshooting guide

## 🔧 Environment Variables

### Required Configuration

```bash
# Provider selection (default: ollama)
EMBEDDING_PROVIDER=ollama  # or "openai"

# Ollama Configuration
EMBEDDING_ENDPOINT=https://llama.terminal.krohnos.io  # default for ollama
EMBEDDING_MODEL=nomic-embed-text  # default, 768 dimensions

# OpenAI Configuration (when EMBEDDING_PROVIDER=openai)
EMBEDDING_ENDPOINT=https://api.openai.com/v1  # default for openai
EMBEDDING_API_KEY=sk-...  # REQUIRED for OpenAI
EMBEDDING_MODEL=text-embedding-3-small  # default, 1536 dimensions

# Optional Settings
EMBEDDING_TIMEOUT_SECS=30  # default
EMBEDDING_MAX_RETRIES=3  # default
EMBEDDING_BATCH_SIZE=10  # default
```

## 🏗️ Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                     MCP Server (main.rs)                    │
│  - Load config from environment                             │
│  - Create provider via factory                              │
│  - Initialize StdioMcpServer with provider                  │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│              Protocol Layer (protocol.rs)                   │
│  - StdioMcpServer stores provider                           │
│  - Passes provider to tools on each call                    │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                 Tools Layer (tools/mod.rs)                  │
│  - semantic_search: provider.embed(query)                   │
│  - index_vault: provider.embed_batch(texts)                 │
│  - index_document: provider.embed(content)                  │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│         Embedding Provider (Arc<dyn EmbeddingProvider>)     │
│                                                             │
│  ┌──────────────────┐         ┌──────────────────┐        │
│  │ OllamaProvider   │         │ OpenAIProvider   │        │
│  │ - HTTP client    │         │ - HTTP client    │        │
│  │ - Retry logic    │         │ - Bearer auth    │        │
│  │ - 768 dims       │         │ - 1536 dims      │        │
│  └──────────────────┘         └──────────────────┘        │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│             Database Layer (database.rs)                    │
│  - Store embeddings as JSON (Vec<f32>)                      │
│  - Cosine similarity search                                 │
│  - DuckDB with VSS extension                                │
└─────────────────────────────────────────────────────────────┘
```

## 📋 Implementation Checklist

### Phase 2: Integration (Current Focus)
- [ ] **Protocol Layer** (`src/protocol.rs`)
  - [ ] Add provider field to `StdioMcpServer`
  - [ ] Update initialization to accept provider
  - [ ] Pass provider to tool handlers

- [ ] **Tools Layer** (`src/tools/mod.rs`)
  - [ ] Remove `generate_dummy_embedding()` function
  - [ ] Update `semantic_search()` signature and implementation
  - [ ] Update `index_vault()` signature and implementation
  - [ ] Update `index_document()` signature and implementation
  - [ ] Update `search_documents()` signature and implementation
  - [ ] Update tool call handlers to pass provider

- [ ] **Main Entry** (`src/main.rs`)
  - [ ] Import embedding modules
  - [ ] Load config from environment
  - [ ] Create provider with error handling
  - [ ] Log provider configuration
  - [ ] Pass provider to server initialization

### Phase 3: Testing & Documentation
- [ ] **Integration Tests**
  - [ ] Test with Ollama provider
  - [ ] Test with OpenAI provider
  - [ ] Test error scenarios
  - [ ] Test batch operations

- [ ] **Documentation**
  - [ ] Environment variable guide
  - [ ] Claude Desktop configuration example
  - [ ] Model selection guide
  - [ ] Troubleshooting guide
  - [ ] Update README.md

## 🚀 Next Steps

1. **Update Protocol Layer** - Modify `StdioMcpServer` to store and use provider
2. **Update Tools Layer** - Replace dummy embeddings with provider calls
3. **Update Main Entry** - Wire everything together with config loading
4. **Test End-to-End** - Verify with both Ollama and OpenAI
5. **Document Configuration** - Create user-facing documentation

## 📝 Notes

- Current dummy embedding uses hash-based approach (1536 dims)
- DuckDB stores embeddings as JSON strings
- VSS extension is loaded but not fully utilized yet
- Cosine similarity is computed in Rust (not using VSS functions)
- Database already supports variable dimensions

## 🔗 Key Files

### Completed
- `crates/crucible-mcp/src/embeddings/provider.rs` - Provider trait
- `crates/crucible-mcp/src/embeddings/config.rs` - Configuration
- `crates/crucible-mcp/src/embeddings/error.rs` - Error handling
- `crates/crucible-mcp/src/embeddings/ollama.rs` - Ollama provider
- `crates/crucible-mcp/src/embeddings/openai.rs` - OpenAI provider
- `crates/crucible-mcp/src/embeddings/mod.rs` - Module exports

### To Be Modified
- `crates/crucible-mcp/src/protocol.rs` - Protocol layer
- `crates/crucible-mcp/src/tools/mod.rs` - Tools implementation
- `crates/crucible-mcp/src/main.rs` - Entry point
- `crates/crucible-mcp/src/database.rs` - (Optional optimization)

### Documentation
- `claude_desktop_config.example.json` - Example config file
- `MCP_PLAN_WIP.md` - This file
