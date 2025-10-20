# Integration Tests with Real Ollama Provider

This directory contains optional integration tests that use a real Ollama provider for testing embedding generation and semantic search functionality.

## Overview

The integration tests (`integration_test.rs`) provide comprehensive testing of the daemon's embedding pipeline with actual Ollama embeddings, allowing you to verify:

- Real Ollama provider connectivity
- Actual embedding generation and quality
- Semantic search with real embeddings
- Comparison between mock and real providers
- Error handling for invalid configurations

## Setup

### 1. Configure Ollama

You need access to an Ollama server with an embedding model installed:

**Local Ollama (Recommended for development):**
```bash
# Install Ollama
curl -fsSL https://ollama.ai/install.sh | sh

# Pull an embedding model
ollama pull nomic-embed-text-v1.5-q8_0

# Start Ollama server
ollama serve
```

**Remote Ollama (Alternative):**
- Use a remote Ollama instance like `https://llama.krohnos.io`

### 2. Configure Environment

Copy the example configuration file:
```bash
cp .env.example .env
```

Edit `.env` with your Ollama configuration:
```bash
# Local Ollama server
OLLAMA_ENDPOINT=http://localhost:11434
OLLAMA_MODEL=nomic-embed-text-v1.5-q8_0

# Remote Ollama server (uncomment if using remote)
# OLLAMA_ENDPOINT=https://llama.krohnos.io
# OLLAMA_MODEL=nomic-embed-text

# Optional settings
OLLAMA_TIMEOUT=30
```

### 3. Verify Configuration

Check that your Ollama server is accessible:
```bash
curl http://localhost:11434/api/tags
```

## Running Tests

### Run All Integration Tests
```bash
cargo test -p crucible-daemon --test integration_test --ignored
```

### Run Specific Tests
```bash
# Test basic Ollama connectivity
cargo test -p crucible-daemon --test integration_test test_integration_real_ollama_provider --ignored

# Test semantic search
cargo test -p crucible-daemon --test integration_test test_integration_real_semantic_search --ignored

# Test batch embedding
cargo test -p crucible-daemon --test integration_test test_integration_real_batch_embedding --ignored

# Test error handling
cargo test -p crucible-daemon --test integration_test test_integration_error_handling --ignored
```

## Test Descriptions

### `test_integration_real_ollama_provider`
- Tests basic connectivity to Ollama server
- Verifies embedding generation works
- Validates embedding dimensions and normalization

### `test_integration_real_semantic_search`
- Creates test documents with real embeddings
- Performs semantic search queries
- Validates search results and similarity scores

### `test_integration_real_vs_mock_comparison`
- Compares behavior between real and mock providers
- Tests embedding dimensions and norms
- Documents differences in provider behavior

### `test_integration_real_batch_embedding`
- Tests batch embedding operations
- Measures performance with real provider
- Validates batch result consistency

### `test_integration_error_handling`
- Tests behavior with invalid endpoints
- Tests missing configuration handling
- Verifies graceful error recovery

### `test_integration_embedding_quality`
- Tests semantic relationships in embeddings
- Validates search quality for different domains
- Measures similarity scores for related/unrelated content

## Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `OLLAMA_ENDPOINT` | Ollama server URL | `http://localhost:11434` |
| `OLLAMA_MODEL` | Embedding model name | `nomic-embed-text-v1.5-q8_0` |
| `OLLAMA_TIMEOUT` | Request timeout in seconds | `30` |
| `OLLAMA_DEBUG` | Enable debug logging | `false` |

## Expected Output

When running integration tests, you should see output like:

```
🔗 Testing real Ollama provider connectivity...
✓ Generated embedding: 768 dimensions
✓ Model: nomic-embed-text-v1.5-q8_0
✓ Tokens: Some(15)
✓ Embedding norm: 1.234

🔍 Testing real semantic search with Ollama embeddings...
📝 Creating test documents...
✓ Created rust_guide.md
✓ Created python_guide.md
...

🔍 Testing semantic search queries...

Query: 'programming languages' (Should find Rust and Python guides)
  1. /tmp/.tmpXXX/rust_guide.md (similarity: 0.892)
  2. /tmp/.tmpXXX/python_guide.md (similarity: 0.756)
  3. /tmp/.tmpXXX/database_guide.md (similarity: 0.423)
```

## Troubleshooting

### Tests are Skipped
If tests are skipped, ensure:
- `.env` file exists with required variables
- Environment variables are set correctly
- Check test output for skip messages

### Connection Errors
If you see connection errors:
- Verify Ollama server is running: `ollama ps`
- Check endpoint URL is correct
- Verify firewall settings allow the connection
- Test with curl: `curl $OLLAMA_ENDPOINT/api/tags`

### Model Not Found
If the model is not available:
- List available models: `ollama list`
- Pull the required model: `ollama pull $OLLAMA_MODEL`
- Use a different model in `.env`

### Timeouts
If tests timeout:
- Increase `OLLAMA_TIMEOUT` in `.env`
- Check server performance and network latency
- Reduce test data size

## Development Tips

### Running Tests During Development
```bash
# Watch mode for continuous testing
cargo watch -x "test -p crucible-daemon --test integration_test --ignored"

# Run with debug output
RUST_LOG=debug cargo test -p crucible-daemon --test integration_test test_integration_real_ollama_provider --ignored --nocapture
```

### Test Performance
Integration tests are slower than unit tests because they:
- Make real HTTP requests to Ollama
- Generate actual embeddings (can take 100-500ms per request)
- Process real vector operations

Typical test times:
- Basic provider test: 1-2 seconds
- Semantic search test: 5-10 seconds
- Full test suite: 30-60 seconds

### Debugging Failed Tests
1. Enable debug logging: `RUST_LOG=debug`
2. Use `--nocapture` to see test output
3. Check Ollama logs: `ollama logs`
4. Verify model availability: `curl $OLLAMA_ENDPOINT/api/tags`

## CI/CD Integration

These integration tests are marked as `#[ignore]` by default to avoid requiring Ollama in CI environments. To run them in CI:

1. Set up Ollama service in CI
2. Configure environment variables
3. Run tests without `--ignored` flag
4. Handle failures gracefully with proper timeouts

Example CI configuration:
```yaml
# GitHub Actions example
- name: Start Ollama
  run: |
    curl -fsSL https://ollama.ai/install.sh | sh
    ollama pull nomic-embed-text-v1.5-q8_0
    ollama serve &

- name: Run Integration Tests
  env:
    OLLAMA_ENDPOINT: http://localhost:11434
    OLLAMA_MODEL: nomic-embed-text-v1.5-q8_0
  run: |
    cargo test -p crucible-daemon --test integration_test --ignored
```

## Contributing

When adding new integration tests:

1. Mark tests with `#[ignore]` to keep them optional
2. Use descriptive test names with `test_integration_` prefix
3. Include proper error handling and graceful skipping
4. Add documentation for new test scenarios
5. Update this README with new test descriptions

Remember that these tests should complement, not replace, the existing unit test suite. They are meant to verify integration with real external services.