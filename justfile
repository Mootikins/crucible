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

# Run tests by tier (quick|fixtures|infra|slow|full|all)
# - quick: Fast unit tests, no external deps (default)
# - fixtures: Tests using docs/ or examples/test-kiln
# - infra: Tests requiring Ollama, ACP agents
# - slow: Performance benchmarks and timing-sensitive tests
# - full: All tests including ignored
# - all: Quick + fixtures + infra + slow (no ignored)
test tier="quick":
    #!/usr/bin/env bash
    set -euo pipefail
    case "{{tier}}" in
        quick)
            echo "Running quick tests (no features)..."
            cargo nextest run --workspace 2>/dev/null || cargo test --workspace
            ;;
        fixtures)
            echo "Running fixture tests..."
            cargo nextest run --workspace --features test-fixtures 2>/dev/null || \
            cargo test --workspace --features test-fixtures
            ;;
        infra)
            echo "Running infrastructure tests..."
            cargo nextest run --workspace --features test-infrastructure 2>/dev/null || \
            cargo test --workspace --features test-infrastructure
            ;;
        slow)
            echo "Running slow tests..."
            cargo nextest run --workspace --features test-slow 2>/dev/null || \
            cargo test --workspace --features test-slow
            ;;
        full)
            echo "Running ALL tests including ignored..."
            cargo nextest run --workspace --features test-fixtures,test-infrastructure,test-slow -- --include-ignored 2>/dev/null || \
            cargo test --workspace --features test-fixtures,test-infrastructure,test-slow -- --ignored
            ;;
        all)
            echo "Running all tiered tests (quick + fixtures + infra + slow)..."
            cargo nextest run --workspace --features test-fixtures,test-infrastructure,test-slow 2>/dev/null || \
            cargo test --workspace --features test-fixtures,test-infrastructure,test-slow
            ;;
        *)
            echo "Unknown tier: {{tier}}"
            echo "Valid tiers: quick, fixtures, infra, slow, full, all"
            exit 1
            ;;
    esac

# Run all tests (full output, legacy alias)
test-full:
    cargo test --workspace

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

# === Web Interface ===

# Build Svelte frontend and run web server (for production-like dev)
web: web-build
    cargo run -p crucible-cli -- web --host 0.0.0.0 --port 3000

# Build only the Svelte frontend
web-build:
    cd crates/crucible-web/web && bun install && bun run build

# Run Vite dev server (hot reload, localhost only)
web-vite:
    cd crates/crucible-web/web && bun run dev

# Run Vite dev server exposed to network
web-vite-host:
    cd crates/crucible-web/web && bun run dev --host

# Run web server pointing to Vite dev server (for API + hot reload)
web-dev:
    cargo run -p crucible-cli -- web --host 0.0.0.0 --port 3000 --static-dir crates/crucible-web/web/dist

# Build release with embedded web assets
release-web: web-build
    cargo build -p crucible-cli --release

# === Coverage ===

# Run code coverage with tarpaulin (uses tarpaulin.toml config)
coverage:
    cargo tarpaulin --config tarpaulin.toml

# Run quick coverage on core crates only
coverage-quick:
    cargo tarpaulin --config tarpaulin.toml --run-types lib

# Run coverage for a specific crate
coverage-crate crate:
    cargo tarpaulin -p {{crate}} --skip-clean --timeout 120 --exclude-files 'vendor/*' --out html --output-dir target/tarpaulin

# Open coverage report in browser
coverage-open: coverage
    xdg-open target/tarpaulin/tarpaulin-report.html 2>/dev/null || open target/tarpaulin/tarpaulin-report.html 2>/dev/null || echo "Open target/tarpaulin/tarpaulin-report.html manually"

# === CI ===

# Run full CI check
ci: fmt-check clippy test
    @echo "CI checks passed!"
