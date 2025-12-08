# Crucible development recipes
# Run `just` to see available commands

# Default recipe - show help
default:
    @just --list

# === Build ===

# Build all crates (debug)
build:
    cargo build

# Build CLI only (debug)
build-cli:
    cargo build -p crucible-cli

# Build release
release:
    cargo build --release

# Build release CLI only
release-cli:
    cargo build --release -p crucible-cli

# === Test ===

# Run all tests
test:
    cargo test

# Run tests for a specific crate
test-crate crate:
    cargo test -p {{crate}}

# Run tests with output
test-verbose:
    cargo test -- --nocapture

# === Check & Lint ===

# Check compilation without building
check:
    cargo check --all-targets

# Run clippy
clippy:
    cargo clippy --all-targets -- -D warnings

# Format code
fmt:
    cargo fmt

# Format check (CI)
fmt-check:
    cargo fmt -- --check

# === Documentation ===

# Build docs
docs:
    cargo doc --no-deps

# Build and open docs
docs-open:
    cargo doc --no-deps --open

# === Clean ===

# Clean build artifacts
clean:
    cargo clean

# === MCP Server ===

# Start MCP server (SSE on port 3847)
mcp:
    cargo run --release -p crucible-cli -- mcp --port 3847

# Start MCP server with verbose logging
mcp-debug:
    cargo run --release -p crucible-cli -- mcp --port 3847 -v

# === Benchmarks (future) ===

# Run benchmarks (placeholder)
bench:
    @echo "Benchmarks not yet configured"
    # cargo bench

# === CI ===

# Run full CI check
ci: fmt-check clippy test
    @echo "CI checks passed!"
