# Crucible development recipes
# Run `just` to see available commands
#
# Grouped by verb, not by tool: `build`, `lint`, `test` and `web-test` each take
# a sub-target rather than each owning a recipe of its own. Every sub-target
# prints its valid values when given one it does not know, so `just test bogus`
# is a usable index.

# Argument boundaries survive into recipe bodies as "$@", so a filter that
# contains a space — `just test quick -E 'test(/a b/)'` — reaches nextest as one
# argument instead of two.
set positional-arguments := true

# Throttles the COMPILE phase — nextest's `-j` caps test threads only, and
# rust-lld peaks ~7GB per link job. Raise it: `CARGO_BUILD_JOBS=12 just test`.
export CARGO_BUILD_JOBS := env_var_or_default("CARGO_BUILD_JOBS", "6")

# Property-test budget for crucible-oil (`tests/common/mod.rs`); per-file
# `.max(N)` floors still apply. CI raises it to 256 via the workflow `env:`.
export CRUCIBLE_PROPTEST_CASES := env_var_or_default("CRUCIBLE_PROPTEST_CASES", "64")

# Show available commands
default:
    @just --list

# Install CI prerequisites (bun, jq and ripgrep must already be on PATH)
setup:
    #!/usr/bin/env bash
    set -euo pipefail

    missing=0

    if ! command -v bun >/dev/null 2>&1; then
        missing=1
        echo "MISSING: bun — required for the web frontend (npm/yarn are NOT substitutes)."
        echo "  curl -fsSL https://bun.sh/install | bash    # or: brew install oven-sh/bun/bun"
    fi

    if ! command -v jq >/dev/null 2>&1; then
        missing=1
        echo "MISSING: jq — used by \`just test plugins\` and scripts/validate-demos.sh."
        echo "  apt-get / dnf / pacman / brew install jq"
    fi

    if ! command -v rg >/dev/null 2>&1; then
        missing=1
        echo "MISSING: rg (ripgrep) — the grep tool shells out to it, and four \`just test gated\` tests need it (\`#[ignore = \"requires: ripgrep\"]\`)."
        echo "  apt-get / dnf / pacman / brew install ripgrep"
    fi

    if [ "$missing" -ne 0 ]; then
        echo
        echo "Install the above, then re-run \`just setup\`."
        exit 1
    fi

    # cargo-nextest: every `just test` tier and every GitHub test job uses it.
    if ! cargo nextest --version >/dev/null 2>&1; then
        echo "== installing cargo-nextest"
        cargo install cargo-nextest --locked
    fi

    # cargo-deny: backs `just lint license` / the `deny` CI job.
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

# Build the frontend and locked shipping binary, then install to ~/.cargo/bin
install:
    #!/usr/bin/env bash
    set -euo pipefail
    just build release-web
    dest="${CARGO_HOME:-$HOME/.cargo}/bin/cru"
    install -m 755 "$(cargo metadata --format-version 1 --no-deps --offline \
        | jq -r .target_directory)/release/cru" "$dest"
    echo "installed $("$dest" --version) to $dest"

# Release uses LTO; reserve it for installation. release-web embeds a fresh frontend.
# Build: debug (default) | cli | release | release-web | fixtures
build target="debug":
    #!/usr/bin/env bash
    set -euo pipefail
    case "$1" in
        debug)   cargo build ;;
        cli)     cargo build -p crucible-cli ;;
        release) cargo build --release ;;
        release-web)
            just web-build
            cargo build -p crucible-cli --release
            ;;
        fixtures)
            # Separate graphs: enabling test-utils on the daemon in the cru
            # build needlessly invalidates the ordinary web/live binary later.
            cargo build -p crucible-daemon --features test-utils --bin mock-acp-agent
            cargo build -p crucible-cli --bin cru
            ;;
        *)
            echo "Unknown build target: $1"
            echo "Valid targets: debug cli release release-web fixtures"
            exit 1
            ;;
    esac

# Check every crate and test target (default-members contains only the CLI)
check:
    cargo check --workspace --all-targets

# Format Rust code
fmt:
    cargo fmt

# Lint: all (default) | fmt | clippy | docs | license | types | dead
lint what="all":
    #!/usr/bin/env bash
    set -euo pipefail

    case "$1" in
        all)
            for target in fmt clippy docs license types dead; do just lint "$target"; done
            ;;
        fmt) cargo fmt --all -- --check ;;
        clippy) cargo clippy --workspace --all-targets -- -D warnings ;;
        docs)
            cargo test -p crucible-core --test dev_kiln --test docs_config -- --ignored
            cargo test -p crucible-lua --test docs_lua_config -- --ignored
            ;;
        license) cargo deny --all-features check licenses ;;
        types)
            # bunx otherwise falls back to a cached, incompatible TypeScript.
            cd crates/crucible-web/web
            test -d node_modules/typescript || {
                echo "Missing frontend dependencies; run just setup." >&2
                exit 1
            }
            bunx tsc --noEmit -p tsconfig.json
            ;;
        dead)
            # Include tests: --production misclassifies lazy-loaded dependencies.
            cd crates/crucible-web/web
            bunx knip --no-progress
            ;;
        *) echo "Valid lint targets: all fmt clippy docs license types dead" >&2; exit 1 ;;
    esac

# Reports only: the index cannot see dynamic Lua, serde or RPC uses.
# Inspect a rust-analyzer SCIP index: unread (default) | index | symbol | check | orphans
refs what="unread" *args:
    #!/usr/bin/env bash
    set -euo pipefail
    case "{{what}}" in
        index)  rust-analyzer scip . --output /tmp/scip/index.scip ;;
        unread) python3 scripts/scip-refs.py --unread-fields {{args}} ;;
        symbol) python3 scripts/scip-refs.py --symbol {{args}} ;;
        check)  python3 scripts/scip-refs.py --self-test ;;
        orphans) python3 scripts/orphan-types.py {{args}} ;;
        *)
            echo "Unknown refs target: {{what}}"
            echo "Valid targets: index unread symbol check orphans"
            exit 1
            ;;
    esac

# Flags pass through, e.g. just test quick -p crucible-core -E 'test(parser)'.
# Nextest setup builds process fixtures; profiles only change timeouts/retries.
# The pinned checker is needed by the shipped-Luau gates.
# Rust tests: quick (default) | ci | gated | external | ignored | full | features | doc | tiers | plugin <dir> | plugins
test tier="quick" *args: luau-lsp
    #!/usr/bin/env bash
    set -euo pipefail
    tier="$1"; shift

    # -p must replace --workspace: cargo otherwise silently ignores the package.
    scope="--workspace"
    for arg in "$@"; do
        case "$arg" in -p|-p=*|--package|--package=*) scope="" ;; esac
    done

    # Generated from ignore reasons and checked by architecture gate A5.
    # Bare names need substring matching against module-qualified test names.
    external_filter() {
        sed -e 's/#.*//' -e '/^[[:space:]]*$/d' assets/test-tiers/external.txt \
            | sed 's/^/test(/; s/$/)/' | paste -sd'+' -
    }

    case "$tier" in
        quick)   cargo nextest run $scope "$@" ;;
        ignored) cargo nextest run $scope --run-ignored ignored-only "$@" ;;
        full)    cargo nextest run $scope --run-ignored all "$@" ;;
        gated)
            # Fail closed: new ignored tests run here unless classified external.
            # Docs have their own lint gate. PTY children need a kiln marker.
            mkdir -p .crucible
            cargo nextest run --profile ci $scope --run-ignored ignored-only \
                -E "not ($(external_filter)) and not (binary(=dev_kiln) + binary(=docs_config) + binary(=docs_lua_config))" "$@"
            ;;
        external)
            # Requires external services, models, containers or a human.
            cargo nextest run --profile ci $scope --run-ignored ignored-only \
                -E "$(external_filter)" "$@"
            ;;
        tiers)
            # Regeneration and validation share the architecture gate's parser.
            CRUCIBLE_WRITE_TEST_TIERS=1 cargo nextest run -p crucible-daemon \
                --test architecture_tests --no-capture \
                -E 'test(external_test_tier_file_matches_the_ignore_reasons)'
            ;;
        ci)
            cargo nextest run --profile ci $scope "$@"
            ;;
        features)
            # Oil's dev-dependency enables test features in the main run.
            # Separately prove the production library needs neither feature.
            cargo check -p crucible-oil --lib --no-default-features
            ;;
        doc) cargo test --workspace --doc ;;
        plugin)
            [ "$#" -ge 1 ] || { echo "usage: just test plugin <dir>"; exit 1; }
            cargo build -q -p crucible-cli --bin cru
            # Worktrees share the primary checkout's target dir, so ./target/debug/cru
            # need not exist here. Ask cargo where the binary actually landed.
            cru="$(cargo metadata --format-version 1 --no-deps --offline | jq -r .target_directory)/debug/cru"
            "$cru" plugin test "$1"
            ;;
        plugins)
            # Optional full process-path run; CI uses exhaustive in-process
            # suites plus one CLI/RPC contract. Isolate from the user's daemon.
            cargo build -q -p crucible-cli --bin cru
            cru="$(cargo metadata --format-version 1 --no-deps --offline | jq -r .target_directory)/debug/cru"
            scratch="$(mktemp -d)"
            mkdir -p "$scratch/home"
            export CRUCIBLE_SOCKET="$scratch/daemon.sock"
            export CRUCIBLE_HOME="$scratch/home"
            trap '"$cru" daemon stop >/dev/null 2>&1 || true; rm -rf "$scratch"' EXIT
            ran=0
            for dir in runtime/plugins/*/; do
                if compgen -G "${dir}tests/*.luau" > /dev/null \
                    || compgen -G "${dir}tests/*.lua" > /dev/null; then
                    echo "== ${dir}"
                    "$cru" plugin test "${dir%/}"
                    ran=$((ran + 1))
                fi
            done
            if [ "$ran" -eq 0 ]; then
                echo "no plugin suites matched — the glob is stale" >&2
                exit 1
            fi
            echo "ran $ran plugin suites"
            ;;
        -p|-p=*|--package|--package=*) cargo nextest run "$tier" "$@" ;;
        -*) cargo nextest run $scope "$tier" "$@" ;;
        *)
            echo "Unknown test tier: $tier"
            echo "Valid tiers: quick ignored gated external full ci tiers features doc plugin plugins"
            exit 1
            ;;
    esac

# Run the frontend with hot reload and a standalone API (never replace the installed daemon)
web api_port="3000" host="127.0.0.1":
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build -p crucible-cli --bin cru
    # The BINARY, not `cargo run`: killing cargo can leave the server it
    # spawned holding the port, and the next run then fails to bind.
    ./target/debug/cru --standalone web --host {{host}} --port {{api_port}} &
    api=$!
    trap 'kill "$api" 2>/dev/null || true' EXIT INT TERM
    cd crates/crucible-web/web
    bun install
    CRUCIBLE_API_PORT={{api_port}} exec bun run dev

# Serve a fresh frontend bundle from cru (PWA disabled to avoid stale local caches)
web-static port="3000" host="0.0.0.0": (web-build "off")
    cargo build -p crucible-cli --bin cru
    cargo run -p crucible-cli -- --standalone web --host {{host}} --port {{port}} --static-dir crates/crucible-web/web/dist

# Build the frontend; pass off to disable the PWA for local development
web-build pwa="on":
    cd crates/crucible-web/web && bun install && {{ if pwa == "off" { "VITE_DISABLE_PWA=1" } else { "" } }} bun run build

# Prove the test suite writes nothing under the developer's own directories
test-hermetic tier="quick":
    @scripts/check-test-hermeticity.sh {{tier}}

# Args pass through. live covers real server headers; e2e uses Vite.
# Web tests: e2e (default) | unit | coverage | live | stories | hero
web-test tier="e2e" *args:
    #!/usr/bin/env bash
    set -euo pipefail
    tier="$1"; shift
    web=crates/crucible-web/web
    case "$tier" in
        unit|coverage)
            extra=()
            if [ "$tier" = coverage ]; then extra=(--coverage); fi
            cd "$web" && bun install && bunx vitest run "${extra[@]}" "$@"
            ;;
        e2e)
            out="$(mktemp -d /tmp/crucible-pw-XXXXXX)"
            trap 'rm -rf "$out"' EXIT
            cd "$web"
            PLAYWRIGHT_HTML_OUTPUT_DIR="$out/html" \
                bunx playwright test --reporter=line --output "$out/results" "$@"
            ;;
        stories)
            cd "$web" && bunx playwright test --project=stories --reporter=line "$@"
            ;;
        live)
            cargo build -p crucible-cli --bin cru
            cd "$web" && bun install && bun run build
            bunx playwright test --config=playwright.live.config.ts "$@"
            ;;
        hero)
            cargo build -p crucible-cli --bin cru
            cargo test -p crucible-cli --test tui_e2e_tests --no-run
            cd "$web" && bun install && bun run build
            bunx playwright test --config=playwright.hero.config.ts "$@"
            ;;
        *)
            echo "Unknown web test tier: $tier"
            echo "Valid tiers: unit coverage e2e stories live hero"
            exit 1
            ;;
    esac

# Keep Rust tiers before web builds; clean-clone packaging/sharding are workflow-specific.
# Run all local CI gates before committing; GitHub uses the same recipes
ci: luau-lsp (lint "all") (test "ci") (test "gated") (test "features") (test "doc") (web-test "coverage") (web-test "e2e") (web-test "live")
    @echo "CI checks passed!"

# Keep this in sync with the checker required by the shipped-Lua gates.
LUAU_LSP_VERSION := "1.69.0"

# Install the pinned Luau checker into target/tools (idempotent)
luau-lsp:
    #!/usr/bin/env bash
    set -euo pipefail
    dest="target/tools/luau-lsp"
    if [ -x "$dest" ] && "$dest" --version 2>/dev/null | grep -q "{{LUAU_LSP_VERSION}}"; then
        exit 0
    fi
    case "$(uname -s)-$(uname -m)" in
        Linux-x86_64)  asset=luau-lsp-linux-x86_64.zip ;;
        Linux-aarch64) asset=luau-lsp-linux-arm64.zip ;;
        Darwin-*)      asset=luau-lsp-macos.zip ;;
        *) echo "no pinned luau-lsp for $(uname -s)-$(uname -m)" >&2; exit 1 ;;
    esac
    mkdir -p target/tools
    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"' EXIT
    curl -sSfL -o "$tmp/lsp.zip" \
        "https://github.com/JohnnyMorganz/luau-lsp/releases/download/{{LUAU_LSP_VERSION}}/$asset"
    unzip -q -o "$tmp/lsp.zip" -d "$tmp"
    mv "$tmp/luau-lsp" "$dest"
    chmod +x "$dest"
    "$dest" --version

# Typecheck all shipped Lua against generated declarations for its VM profile
plugin-check: luau-lsp
    just test quick -p crucible-daemon --lib -E 'test(=server::lua_plugin_suite::shipped_plugin_tests::every_shipped_lua_file_typechecks) | test(=server::lua_plugin_suite::shipped_plugin_tests::every_shipped_plugin_typechecks)'

# Build and restart the development daemon
dev:
    -pkill -f "cru daemon serve" 2>/dev/null
    cargo build

# Start the MCP server on port 3847; args pass through
mcp *args:
    cargo run --release -p crucible-cli -- mcp --port 3847 "$@"

# Render replay GIFs: all (default) | <fixture name>
demo name="all" speed="3":
    #!/usr/bin/env bash
    set -euo pipefail
    render() {
        printf '#!/bin/sh\nexec cru chat --replay assets/fixtures/%s.jsonl --replay-speed %s --replay-auto-exit 3000\n' \
            "$1" "{{speed}}" > /tmp/cru-demo-wrapper
        chmod +x /tmp/cru-demo-wrapper
        vhs "assets/$1.tape"
    }
    if [ "$1" = all ]; then
        for name in demo acp-demo delegation-demo overview; do render "$name"; done
        cp assets/demo.gif docs-site/public/demo.gif
        cp assets/delegation-demo.gif docs-site/public/delegation-demo.gif
        echo "Copied demo GIFs to docs-site/public/"
    else
        render "$1"
    fi

# Review the capture for secrets and update the replay case table before committing.
# Re-record an ACP wire fixture; requires the authenticated agent binary
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
    session=$(cargo run -p crucible-cli -- session create --acp {{agent}} --permissions allow -q)
    cargo run -p crucible-cli -- session send "$session" "{{prompt}}" --permissions allow
    capture=$(ls -t "$dir"/{{agent}}-*.jsonl | head -n1)
    mkdir -p "$(dirname "$dest")"
    sed "s|$HOME|<HOME>|g" "$capture" > "$dest"
    echo "Wrote $dest — review it (secrets, absolute paths) before committing."

# Regenerate third-party notices (requires frontend node_modules)
notices:
    python3 scripts/gen-third-party-notices.py
