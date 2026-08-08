# Crucible development recipes
# Run `just` to see available commands

# Throttles the COMPILE phase — nextest's `-j` caps test threads only, and
# rust-lld peaks ~7GB per link job. Raise it: `CARGO_BUILD_JOBS=12 just test`.
export CARGO_BUILD_JOBS := env_var_or_default("CARGO_BUILD_JOBS", "6")

# Property-test budget for crucible-oil (`tests/common/mod.rs`); per-file
# `.max(N)` floors still apply. CI raises it to 256 via the workflow `env:`.
export CRUCIBLE_PROPTEST_CASES := env_var_or_default("CRUCIBLE_PROPTEST_CASES", "64")

# Default recipe - show help
default:
    @just --list

# === Setup ===

# Idempotent. protoc and bun are reported, not installed: their package name
# differs per platform and guessing wrong is worse than saying so.
#
# Install everything `just ci` needs beyond a Rust toolchain
setup:
    #!/usr/bin/env bash
    set -euo pipefail

    missing=0

    if ! command -v protoc >/dev/null 2>&1; then
        missing=1
        echo "MISSING: protoc (protobuf-compiler) — crucible-cli, crucible-daemon and crucible-web pull prost-build and will not compile without it."
        case "$(uname -s)" in
            Darwin) echo "  brew install protobuf" ;;
            Linux)
                echo "  Debian/Ubuntu: sudo apt-get install -y protobuf-compiler"
                echo "  Fedora:        sudo dnf install -y protobuf-compiler"
                echo "  Arch:          sudo pacman -S protobuf"
                ;;
            *) echo "  Install the 'protobuf-compiler' package for your platform." ;;
        esac
    fi

    if ! command -v bun >/dev/null 2>&1; then
        missing=1
        echo "MISSING: bun — required for the web frontend (npm/yarn are NOT substitutes)."
        echo "  curl -fsSL https://bun.sh/install | bash    # or: brew install oven-sh/bun/bun"
    fi

    if ! command -v jq >/dev/null 2>&1; then
        missing=1
        echo "MISSING: jq — used by \`just test-plugin(s)\` and scripts/validate-demos.sh."
        echo "  apt-get / dnf / pacman / brew install jq"
    fi

    if [ "$missing" -ne 0 ]; then
        echo
        echo "Install the above, then re-run \`just setup\`."
        exit 1
    fi

    # cargo-nextest: every `just test*` recipe and every GitHub test job uses it.
    if ! cargo nextest --version >/dev/null 2>&1; then
        echo "== installing cargo-nextest"
        cargo install cargo-nextest --locked
    fi

    # cargo-deny: backs `just license-check` / the `deny` CI job.
    if ! cargo deny --version >/dev/null 2>&1; then
        echo "== installing cargo-deny"
        cargo install cargo-deny --locked
    fi

    echo "== installing web dependencies"
    cd crates/crucible-web/web && bun install

    # Chromium only — every Playwright project runs on chromium. `--with-deps`
    # is left off on purpose: it shells out to sudo apt-get.
    echo "== installing Playwright browsers (chromium)"
    bunx playwright install chromium

    echo
    echo "Setup complete. Run \`just ci\` to verify."

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

# Slow/external tests are gated with #[ignore], not cargo features; each ignore
# reason names its prerequisite (agent binary, podman, or an `.env.local` endpoint).
#
# Run tests: quick (default, skips #[ignore]d) | ignored | full
test tier="quick":
    #!/usr/bin/env bash
    set -euo pipefail
    case "{{tier}}" in
        quick)
            cargo nextest run --workspace
            ;;
        ignored)
            cargo nextest run --workspace --run-ignored ignored-only
            ;;
        full)
            cargo nextest run --workspace --run-ignored all
            ;;
        *)
            echo "Unknown tier: {{tier}}"
            echo "Valid tiers: quick, ignored, full"
            exit 1
            ;;
    esac

# Run tests for a specific crate
test-crate crate:
    cargo test -p {{crate}}

# Run a filtered subset of one crate's tests (nextest substring match)
test-crate-filter crate filter:
    cargo nextest run -p {{crate}} -E 'test(/{{filter}}/)'

# Run a Lua plugin's own test suite, e.g. `just test-plugin runtime/plugins/oci`
test-plugin dir:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build -q -p crucible-cli --bin cru
    # Worktrees share the primary checkout's target dir, so ./target/debug/cru
    # need not exist here. Ask cargo where the binary actually landed.
    cru="$(cargo metadata --format-version 1 --no-deps --offline | jq -r .target_directory)/debug/cru"
    "$cru" plugin test "{{dir}}"

# Run tests with output
test-verbose:
    cargo test -- --nocapture

# === Check & Lint ===

# `--workspace` is mandatory: `default-members` is crucible-cli alone, so a bare
# `--all-targets` silently skips daemon/core/lua/oil/web. The second line covers
# oil's feature-gated test files, which `--all-targets` alone never compiles.
#
# Check compilation without building
check:
    cargo check --workspace --all-targets
    cargo check -p crucible-oil --all-targets --features serde,test-utils

# `--all-targets` covers target kinds, not feature combinations, so the second
# line lints the feature-gated surface — it pairs 1:1 with `test-features`.
#
# Lint every crate's every target, denying warnings
clippy:
    cargo clippy --workspace --all-targets -- -D warnings
    cargo clippy -p crucible-oil --all-targets --features serde,test-utils -- -D warnings

# Check for oversized Rust files (enforces 1500-line ceiling via whitelist)
file-size-check:
    scripts/check-file-sizes.sh

# Format code
fmt:
    cargo fmt

# Format check (CI)
fmt-check:
    cargo fmt --all -- --check

# These tests are `CARGO_MANIFEST_DIR`-anchored to this repo's `docs/`, so a
# failure has to be reproduced by editing `docs/` in place, not a copy.
#
# Validate the `docs/` kiln (parser, frontmatter, wikilinks, code refs, config)
lint-docs:
    cargo test -p crucible-core --test dev_kiln --test docs_config -- --ignored

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

# === Demo & Recording ===

# Generate demo GIF from replay fixture
demo name speed="3":
    @printf '#!/bin/sh\nexec cru chat --replay assets/fixtures/%s.jsonl --replay-speed %s --replay-auto-exit 3000\n' "{{name}}" "{{speed}}" > /tmp/cru-demo-wrapper && chmod +x /tmp/cru-demo-wrapper
    vhs assets/{{name}}.tape

# Generate all demo GIFs
demo-all: (demo "demo") (demo "acp-demo") (demo "delegation-demo") (demo "overview")
    cp assets/demo.gif docs-site/public/demo.gif
    cp assets/delegation-demo.gif docs-site/public/delegation-demo.gif
    @echo "Copied demo GIFs to docs-site/public/"

# Validate demo fixtures render without duplication or styling issues
demo-validate:
    cargo test -p crucible-cli -- fixture_replay
    cargo test -p crucible-oil --test style_wrap_tests

# Record a new demo fixture (requires live agent)
demo-record name *args:
    cargo run -p crucible-cli -- chat --record assets/fixtures/{{name}}.jsonl {{args}}

# REQUIRES the agent binary on PATH and logged in. Diff the result before
# committing and update the case table in acp_fixture_replay.rs to match.
#
# Re-record the ACP wire fixture replayed by tests/acp_fixture_replay.rs
record-acp-fixture agent prompt="say hello in exactly 3 words":
    #!/usr/bin/env bash
    set -euo pipefail
    dir=$(mktemp -d)
    dest="crates/crucible-daemon/tests/fixtures/acp/recorded/{{agent}}/basic-chat.jsonl"
    # The recorder lives in the daemon, not the CLI, so the env var only takes
    # effect on a daemon this command spawns itself. An already-running daemon
    # started without it records nothing — stop it first.
    cargo run -p crucible-cli -- daemon stop >/dev/null 2>&1 || true
    echo "Recording {{agent}} into $dir (agent binary must be installed and logged in)"
    export CRUCIBLE_ACP_RECORD_DIR="$dir" CRUCIBLE_ACP_RECORD_SCENARIO=basic-chat
    session=$(cargo run -p crucible-cli -- session create -a {{agent}} --permissions allow -q)
    cargo run -p crucible-cli -- session send "$session" "{{prompt}}" --permissions allow
    capture=$(ls -t "$dir"/{{agent}}-*.jsonl | head -n1)
    mkdir -p "$(dirname "$dest")"
    sed "s|$HOME|<HOME>|g" "$capture" > "$dest"
    echo "Wrote $dest — review it (secrets, absolute paths) before committing."

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

# Build SolidJS frontend and run web server (for production-like dev)
web: web-build
    cargo run -p crucible-cli -- web --host 0.0.0.0 --port 3000

# Build only the SolidJS frontend
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

# `--standalone` is NOT optional: a debug `cru` on the shared socket detects the
# git-SHA mismatch and shuts the installed daemon down to respawn its own.
#
# Debug web server on a side port (default 3001, bound 0.0.0.0)
web-debug port="3001" host="0.0.0.0": web-build-debug
    cargo build -p crucible-cli --bin cru
    cargo run -p crucible-cli -- --standalone web --host {{host}} --port {{port}} --static-dir crates/crucible-web/web/dist

# Fail on a dependency whose licence is not on deny.toml's allowlist
license-check:
    cargo deny --all-features check licenses

# Needs the web tree's node_modules present: font and icon notices are read from
# the packages themselves rather than transcribed.
#
# Regenerate THIRD-PARTY-NOTICES.md from the dependency graph that ships
notices:
    python3 scripts/gen-third-party-notices.py

# Same build minus the service worker, which otherwise keeps serving the PREVIOUS
# bundle until the update toast is accepted. Never ship this output.
#
# Build the frontend for debugging (no PWA service worker)
web-build-debug:
    cd crates/crucible-web/web && bun install && VITE_DISABLE_PWA=1 bun run build

# Build release with embedded web assets
release-web: web-build
    cargo build -p crucible-cli --release

# Always runs in a private output dir: two concurrent `playwright test` runs
# otherwise share `test-results/` and wipe each other's traces mid-run, which
# reads exactly like a flake. Args pass through:
# `just web-test e2e/cross-zone-dnd.spec.ts --project=chromium`.
#
# Run web E2E tests (Playwright)
web-test *args:
    #!/usr/bin/env bash
    set -euo pipefail
    out="$(mktemp -d /tmp/crucible-pw-XXXXXX)"
    trap 'rm -rf "$out"' EXIT
    cd crates/crucible-web/web && \
      PLAYWRIGHT_HTML_OUTPUT_DIR="$out/html" \
      bunx playwright test --reporter=line --output "$out/results" {{args}}

# `bun install` first so a fresh clone or a pulled lockfile bump fails on the
# tests, not on a missing vitest.
#
# Run web unit tests (Vitest); args pass through for scoped runs
web-test-unit *args:
    cd crates/crucible-web/web && bun install && bunx vitest run {{args}}

# Typecheck the web frontend (no emit)
web-typecheck:
    cd crates/crucible-web/web && bunx tsc --noEmit -p tsconfig.json

# Report at crates/crucible-web/web/coverage/index.html; thresholds in
# vite.config.ts gate against regressions below the baseline.
#
# Run web unit tests with coverage (Vitest + v8)
web-test-coverage:
    cd crates/crucible-web/web && bun run test:coverage

# Run the web user-story suites only (video + trace + per-step screenshots)
web-test-stories:
    cd crates/crucible-web/web && bunx playwright test --project=stories --reporter=line

# This tier is the ONLY one that exercises real HTTP responses from `cru web` —
# CSP, nosniff, Content-Disposition, host validation, and file serving. The mock
# tier runs against the Vite dev server, which sends none of those headers.
#
# It builds `cru` and `web/dist` rather than skipping without them: the suite
# skips *green* when the binary is absent, so as a CI gate an unbuilt `cru`
# would report success while asserting nothing. Debug `cru` reads `web/dist`
# from disk (rust-embed has no `debug-embed`), so both are prerequisites.
# Set CRU_BIN to point at an existing binary and the build is still cheap (noop).
#
# Run the live web tier (real `cru web` + daemon + temp kiln)
web-test-live:
    cargo build -p crucible-cli --bin cru
    cd crates/crucible-web/web && bun install && bun run build
    cd crates/crucible-web/web && bunx playwright test --config=playwright.live.config.ts

# Deterministic via a fake Ollama server. Builds `cru`, the web assets and the
# TUI test binary first; skips cleanly if `cru` is absent.
#
# Run the cross-surface hero flow (TUI -> web -> TUI, one session)
hero:
    cargo build -p crucible-cli --bin cru
    cd crates/crucible-web/web && bun install && bun run build
    cargo test -p crucible-cli --test tui_e2e_tests --no-run
    cd crates/crucible-web/web && bunx playwright test --config=playwright.hero.config.ts

# === Daemon Management ===

# Build and restart daemon (kills stale daemon so next cru auto-spawns fresh)
dev:
    -pkill -f "cru daemon serve" 2>/dev/null
    cargo build

# Start the background daemon
daemon-start:
    cru daemon start

# Stop the background daemon
daemon-stop:
    cru daemon stop

# Restart daemon with current binary
daemon-restart:
    -pkill -f "cru daemon serve" 2>/dev/null
    @echo "Daemon killed. Next cru command will auto-spawn fresh."

# Check daemon status
daemon-status:
    cru daemon status

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

# Every job in .github/workflows/ci.yml invokes one of these recipes, so the two
# cannot drift. CI-only: `build-from-clean-clone` (needs a tree with no web/dist).
#
# Run every gate GitHub runs — do this before committing
ci: fmt-check clippy lint-docs license-check file-size-check test-ci test-features test-doc web-test-unit web-typecheck web-test web-test-live test-plugins
    @echo "CI checks passed!"

# In `ci` because `oci` decides which environment to build and whether config is
# trustworthy — a regression there is a sandbox regression no Rust suite covers.
#
# Run every shipped plugin's own Lua test suite
test-plugins:
    #!/usr/bin/env bash
    set -euo pipefail
    for dir in runtime/plugins/*/; do
        if compgen -G "${dir}tests/*.lua" > /dev/null; then
            echo "== ${dir}"
            just test-plugin "${dir%/}"
        fi
    done

# Run tests with CI profile (matches GitHub Actions)
test-ci: build-test-fixtures
    cargo nextest run --profile ci --workspace

# `--workspace` builds every crate with its DEFAULT features, so anything behind
# a non-default flag is not even type-checked by `test-ci`.
#
# Run the feature-gated suites the default build never compiles
test-features:
    cargo nextest run --profile ci -p crucible-oil --features serde,test-utils
    cargo nextest run --profile ci -p crucible-lua -E 'test(stubs)' --no-capture

# nextest cannot execute doctests, so `test-ci` alone leaves every example in a
# doc comment unverified — which is how they rotted unnoticed.
#
# Run doctests
test-doc:
    cargo test --workspace --doc

# Build test fixtures required by integration tests
build-test-fixtures: build-mock-acp-agent
    @echo "Test fixtures built"


# Build the mock-acp-agent binary the acp_smoke tests spawn
build-mock-acp-agent:
    cargo build -p crucible-daemon --features test-utils --bin mock-acp-agent
