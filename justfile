# Crucible development recipes
# Run `just` to see available commands

# Cap cargo's BUILD parallelism for every recipe below.
#
# The compile phase is what overloads a dev box, not the tests: `rust-lld`
# peaks around 7GB per link job, so an uncapped `--workspace` build runs ~30
# linkers at once and can take the machine down. Capping it is what makes a
# full-workspace `just test` / `just ci` safe to run.
#
# TRAP this encodes: nextest's `-j` caps TEST THREADS ONLY — the compile phase
# still uses every core. `CARGO_BUILD_JOBS` is the knob that actually throttles
# the build, and as an export it covers build/check/clippy/nextest uniformly.
#
# Raise it on a big or idle box: `CARGO_BUILD_JOBS=12 just test`
export CARGO_BUILD_JOBS := env_var_or_default("CARGO_BUILD_JOBS", "6")

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

# Run tests (quick|ignored|full). Slow/external tests are gated with
# #[ignore], not cargo features — there is no feature-based tier system.
# - quick: everything not #[ignore]d (default)
# - ignored: only #[ignore]d tests (need agent binaries, podman, or an LLM
#   endpoint; see each test's ignore reason for its prerequisites)
# - full: quick + ignored
#
# The ones needing a live LLM endpoint read `.env.local` at the repo root —
# `cp .env.local.example .env.local` and edit. Without it they print SKIPPED
# and pass, so the tier is green on a machine with no endpoint configured.
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

# Run all tests (full output, legacy alias)
test-full:
    cargo test --workspace

# Run tests for a specific crate
test-crate crate:
    cargo test -p {{crate}}

# Run a filtered subset of one crate's tests (nextest substring match)
test-crate-filter crate filter:
    cargo nextest run -p {{crate}} -E 'test(/{{filter}}/)'

# Run a Lua plugin's own test suite, e.g. `just test-plugin runtime/plugins/oci`
test-plugin dir:
    cargo build -q -p crucible-cli --bin cru
    ./target/debug/cru plugin test {{dir}}

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

# Check for oversized Rust files (enforces 1500-line ceiling via whitelist)
file-size-check:
    scripts/check-file-sizes.sh

# Format code
fmt:
    cargo fmt

# Format check (CI)
fmt-check:
    cargo fmt --all -- --check

# Validate the `docs/` documentation kiln against parser, frontmatter,
# wikilink, and code-reference invariants. Runs the 5 `#[ignore]`d tests in
# `crates/crucible-core/tests/dev_kiln.rs`.
#
# Run this BEFORE EVERY RELEASE and whenever `docs/` structure changes. The
# tests are slow (they walk and parse every markdown/script file under the
# repo's `docs/`) and `dev_kiln_root()` is `env!("CARGO_MANIFEST_DIR")`-anchored
# to that exact directory, so the suite cannot be pointed at a `/tmp` copy —
# failures must be reproduced by a trapped edit/rename INSIDE `docs/`.
#
# NOT part of `just ci`: `docs/` rarely changes shape, re-parsing ~150 notes
# on every local CI run is pure noise, and these invariants are user-facing
# docs quality rather than compile/runtime correctness.
#
# Syntax: `--test dev_kiln` selects the binary, then `-- --ignored` is passed
# through to the libtest harness to bypass the `#[ignore]` gate.
# `cargo test --ignored dev_kiln` is INVALID Cargo syntax — `dev_kiln` after
# `--ignored` parses as a positional test-name filter, not a binary selector.
lint-docs:
    cargo test -p crucible-core --test dev_kiln -- --ignored

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

# Re-record the ACP wire fixture replayed by tests/acp_fixture_replay.rs.
# REQUIRES the real agent binary on PATH (claude / opencode / codex / cursor /
# gemini) and a working login for it — the replay test itself is hermetic and
# needs none of this. The capture is sanitized ($HOME -> <HOME>) and installed
# over the existing fixture; diff it before committing, and update the case
# table in acp_fixture_replay.rs to match the new session id / usage numbers.
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

# Debug-build web server on a side port (default 3001) with fresh assets.
# ALWAYS --standalone: a debug client on the shared socket detects the git-SHA
# mismatch with the installed daemon and SHUTS IT DOWN to respawn its own
# (verify_or_restart) — killing the production instance on 3000 out from
# under you. Build parallelism comes from CARGO_BUILD_JOBS at the top.
# Debug web server on a side port. Binds 0.0.0.0 so the instance is reachable
# from another machine on the LAN, which is how it actually gets looked at;
# pass a host to narrow it. --standalone is NOT optional: a debug `cru` that
# talks to the installed daemon will kill it for a version mismatch.
web-debug port="3001" host="0.0.0.0": web-build-debug
    cargo build -p crucible-cli --bin cru
    cargo run -p crucible-cli -- --standalone web --host {{host}} --port {{port}} --static-dir crates/crucible-web/web/dist

# Fail on a dependency whose licence is not on the allowlist in deny.toml.
# Mirrored by the `deny` job in .github/workflows/ci.yml — GitHub does not
# invoke `just ci`, so every gate has to exist in both places or it only ever
# runs on whichever side you remembered.
license-check:
    cargo deny --all-features check licenses

# Regenerate THIRD-PARTY-NOTICES.md from the dependency graph that ships.
# Needs the web tree's node_modules present, since the font and icon notices
# are read from the packages themselves rather than transcribed.
notices:
    python3 scripts/gen-third-party-notices.py

# Same build, minus the service worker. See the `selfDestroying` note in
# vite.config.ts: with the production SW a rebuild keeps serving the PREVIOUS
# bundle until the update toast is accepted, so changes look like they did not
# deploy. Never use this output for a release.
web-build-debug:
    cd crates/crucible-web/web && bun install && VITE_DISABLE_PWA=1 bun run build

# Build release with embedded web assets
release-web: web-build
    cargo build -p crucible-cli --release

# Run web E2E tests (Playwright). Args pass through for scoped runs:
# `just web-test e2e/cross-zone-dnd.spec.ts --project=chromium`
web-test *args:
    cd crates/crucible-web/web && bunx playwright test --reporter=line {{args}}

# Playwright run isolated from any other in-flight run. Two concurrent
# `playwright test` invocations share `test-results/`, and one wipes the
# other's trace files mid-run — which surfaces as ENOENT on
# `.playwright-artifacts-N/traces/*.network` and reads exactly like a flake.
# Use this whenever measuring flakiness (`--repeat-each=N`) alongside anything
# else, or when running two suites at once.
web-test-isolated *args:
    #!/usr/bin/env bash
    set -euo pipefail
    out="$(mktemp -d /tmp/crucible-pw-XXXXXX)"
    trap 'rm -rf "$out"' EXIT
    cd crates/crucible-web/web && \
      PLAYWRIGHT_HTML_OUTPUT_DIR="$out/html" \
      bunx playwright test --reporter=line --output "$out/results" {{args}}

# Run web unit tests (Vitest). Args pass through for scoped runs:
# `just web-test-unit src/stores/__tests__`
web-test-unit *args:
    cd crates/crucible-web/web && bunx vitest run {{args}}

# Typecheck the web frontend (no emit)
web-typecheck:
    cd crates/crucible-web/web && bunx tsc --noEmit -p tsconfig.json

# Run web unit tests with coverage (Vitest + v8). Report at crates/crucible-web/web/coverage/index.html.
# Thresholds in vite.config.ts gate against regressions below the 2026-05-17 baseline.
web-test-coverage:
    cd crates/crucible-web/web && bun run test:coverage

# Run the web user-story suites only (video + trace + per-step screenshots)
web-test-stories:
    cd crates/crucible-web/web && bunx playwright test --project=stories --reporter=line

# Run the live web tier (real `cru web` + daemon + temp kiln). Needs a `cru`
# binary: set CRU_BIN or build target/debug/cru first. Skips cleanly if absent.
web-test-live:
    cd crates/crucible-web/web && bunx playwright test --config=playwright.live.config.ts

# Run the cross-surface hero flow (TUI → web → TUI, one session; deterministic
# via a fake Ollama server). Builds `cru`, the web assets, and the TUI test
# binary, then runs only the hero spec. Skips cleanly if `cru` is absent.
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

# Run full CI check (mirrors GitHub CI workflow)
ci: fmt-check clippy license-check file-size-check test-ci test-features test-doc web-test-unit web-test test-plugins
    @echo "CI checks passed!"

# Every shipped plugin's own Lua suite.
#
# In `ci` because these guard behaviour nothing in the Rust suites covers —
# `oci` decides which environment to build and whether config is trustworthy,
# and a regression there is a sandbox regression.
test-plugins:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build -q -p crucible-cli --bin cru
    for dir in runtime/plugins/*/; do
        if compgen -G "${dir}tests/*.lua" > /dev/null; then
            echo "== ${dir}"
            ./target/debug/cru plugin test "${dir%/}"
        fi
    done

# Run tests with CI profile (matches GitHub Actions).
#
# `CRUCIBLE_PROPTEST_CASES=256` overrides the local 64-case floor for
# property tests (see `crucible-oil/tests/common/mod.rs`); per-file `.max(N)`
# floors still apply. GitHub Actions sets the same value via workflow `env:`.
test-ci: build-test-fixtures
    CRUCIBLE_PROPTEST_CASES=256 cargo nextest run --profile ci --workspace

# Feature-gated suites the default build never compiles.
#
# `--workspace` builds every crate with its DEFAULT features, so anything
# behind a non-default flag is not even type-checked by `test-ci`. That is how
# a `Border::Custom(BorderChars)` variant reached CI inside a `Serialize`
# derive without `BorderChars` implementing it: green locally, red on the one
# step that turns the feature on. GitHub runs these; so must `just ci`.
#
# `CRUCIBLE_PROPTEST_CASES=256` matches `test-ci` so the local run exercises
# the same high-budget path the oil property tests see in CI.
test-features:
    CRUCIBLE_PROPTEST_CASES=256 cargo nextest run --profile ci -p crucible-oil --features serde,test-utils
    cargo nextest run --profile ci -p crucible-lua -E 'test(stubs)' --no-capture

# Run doctests. nextest cannot execute them, so `test-ci` alone leaves every
# example in a doc comment unverified — which is how they rotted unnoticed.
test-doc:
    cargo test --workspace --doc

# Build test fixtures required by integration tests
build-test-fixtures: build-mock-acp-agent
    @echo "Test fixtures built"


# Build mock-acp-agent binary for acp_smoke tests
# Required before running: cargo nextest run -p crucible-daemon --test acp_smoke
build-mock-acp-agent:
    cargo build -p crucible-daemon --features test-utils --bin mock-acp-agent
