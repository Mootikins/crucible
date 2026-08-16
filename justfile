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

# Default recipe - show help
default:
    @just --list

# === Setup ===

# Idempotent. bun is reported, not installed: its package name
# differs per platform and guessing wrong is worse than saying so.
#
# Install everything `just ci` needs beyond a Rust toolchain
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

# === Build & Check ===

# Two reasons this is a build-and-copy rather than `cargo install --path`.
#
# `cargo install` ignores `Cargo.lock` unless you pass `--locked`, and a fresh
# resolve picks `jaq-std 3.0.1` against `jaq-json 2.0.0-alpha`, which do not
# compile together; the lock pins `jaq-std 3.0.0-beta`. So `--locked` is
# mandatory, not optional. And `cargo install` builds in its own target dir, so
# it would redo the whole LTO link instead of reusing the release build below.
#
# The frontend has to exist first either way: `crucible-web` embeds `web/dist`
# with rust-embed, and `allow_missing = true` means a build without it succeeds
# and silently serves a placeholder rather than failing.

# Install the shipping build to ~/.cargo/bin
install:
    #!/usr/bin/env bash
    set -euo pipefail
    just build release-web
    dest="${CARGO_HOME:-$HOME/.cargo}/bin/cru"
    install -m 755 "$(cargo metadata --format-version 1 --no-deps --offline \
        | jq -r .target_directory)/release/cru" "$dest"
    echo "installed $("$dest" --version) to $dest"


# Don't build `release` unless you are installing — LTO takes 5-10 minutes.
# `release-web` is the shipping build: it builds the frontend first because
# rust-embed bakes `web/dist` into the binary at compile time. `fixtures`
# builds the mock ACP agent that the acp_smoke tests spawn.
#
# Build the workspace: debug (default) | cli | release | release-web | fixtures
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
        fixtures) cargo build -p crucible-daemon --features test-utils --bin mock-acp-agent ;;
        *)
            echo "Unknown build target: $1"
            echo "Valid targets: debug cli release release-web fixtures"
            exit 1
            ;;
    esac

# `--workspace` is mandatory: `default-members` is crucible-cli alone, so a bare
# `--all-targets` silently skips daemon/core/lua/oil/web. The second line covers
# oil's feature-gated test files, which `--all-targets` alone never compiles.
#
# Check compilation without building
check:
    cargo check --workspace --all-targets
    cargo check -p crucible-oil --all-targets --features serde,test-utils

# Format code
fmt:
    cargo fmt

# Every gate that reads code without running it. Notes on the non-obvious ones:
#
# - `clippy` lints the feature-gated surface separately, because `--all-targets`
#   covers target kinds, not feature combinations — it pairs 1:1 with
#   `test features`.
# - `docs` validates the `docs/` kiln (parser, frontmatter, wikilinks, code
#   refs, config). The tests are `CARGO_MANIFEST_DIR`-anchored to this repo's
#   `docs/`, so a failure has to be reproduced by editing `docs/` in place, not
#   a copy.
# - `types` is the only target that needs no Rust toolchain — the web-unit CI
#   job runs it with bun alone.
#
# - `dead` is frontend-only, and deliberately NOT in `all`. Rust needs no
#   equivalent: `crucible-cli`'s modules are `pub(crate)` precisely so `dead_code`
#   reports unused items itself, which `clippy -D warnings` then fails on. Knip is
#   the same idea for TypeScript, where `pub` has no analogue. It is out of `all`
#   until its existing findings are triaged — wiring it in with a backlog would
#   just teach everyone to ignore it.
#
# Lint: all (default) | fmt | clippy | docs | license | size | types | dead
lint what="all":
    #!/usr/bin/env bash
    set -euo pipefail

    lint_fmt()    { cargo fmt --all -- --check; }
    lint_clippy() {
        cargo clippy --workspace --all-targets -- -D warnings
        cargo clippy -p crucible-oil --all-targets --features serde,test-utils -- -D warnings
    }
    lint_docs()    { cargo test -p crucible-core --test dev_kiln --test docs_config -- --ignored; }
    lint_license() { cargo deny --all-features check licenses; }
    lint_size()    { scripts/check-file-sizes.sh; }
    lint_types()   { (cd crates/crucible-web/web && bunx tsc --noEmit -p tsconfig.json); }
    # Import-dead frontend code: unused files, exports and dependencies. Run it
    # WITHOUT `--production`: that mode drops test files from the graph and then
    # reports three dozen lazy-loaded dependencies as unused, which is noise, not
    # a finding. Note the limit — knip sees imports, so a component that IS
    # imported and rendered behind a condition that is always false (the
    # `FilesPanel` root dropdown under `embedded`) is invisible to it. That class
    # needs a reachability test, not a linter.
    lint_dead()    { (cd crates/crucible-web/web && bunx knip --no-progress); }

    case "$1" in
        all) lint_fmt; lint_clippy; lint_docs; lint_license; lint_size; lint_types ;;
        fmt|clippy|docs|license|size|types|dead) "lint_$1" ;;
        *)
            echo "Unknown lint target: $1"
            echo "Valid targets: all fmt clippy docs license size types dead"
            exit 1
            ;;
    esac

# === Test ===

# Slow/external tests are gated with #[ignore], not cargo features; each ignore
# reason names its prerequisite in a closed vocabulary, enforced by gate A5
# (`crates/crucible-daemon/tests/architecture_tests.rs`).
# `quick`/`ignored`/`full` differ only in which of those they run.
#
# `gated` and `external` split `ignored` along that vocabulary: `gated` is
# everything whose prerequisites this repo can satisfy by building itself,
# `external` is everything needing a network, a model, a container runtime or a
# human. 98 of the 106 ignored tests ran in NO pre-commit tier before these
# existed — the whole process-boundary surface was outside `ci`.
#
# The rest are the CI tiers, and each exists because the one before it is blind
# to something: `ci` builds every crate with its DEFAULT features, so `features`
# covers what a non-default flag gates; nextest cannot execute doctests, so
# `doc` covers the examples in doc comments (they had rotted to 62 failures
# before it existed). The shipped Lua suites are NOT a tier of their own any
# more: `oci` decides which environment to build and whether config is
# trustworthy, so a regression there is a sandbox regression no Rust suite
# covers — but `shipped_plugin_lua_suite_passes` now runs every plugin's suite
# in-process under `test ci`, so `test plugins` was running them a second time
# through a daemon for no added signal. It remains as a manual recipe for the
# process-boundary path; see the comment on that arm.
#
# Anything unrecognised that starts with `-` is passed straight to nextest, so
# `just test -p crucible-core -E 'test(parser)'` scopes a run without a recipe.
#
# Test: quick (default) | ignored | gated | external | full | ci | tiers | features | doc | plugin <dir> | plugins
test tier="quick" *args:
    #!/usr/bin/env bash
    set -euo pipefail
    tier="$1"; shift

    # `--workspace` beats a `-p` rather than combining with it: cargo selects
    # every crate and the package flag is silently ignored, so a scoped run
    # turns into the full 8000-test one and looks merely slow. Naming a package
    # is therefore what drops `--workspace`. (`-E` still needs it — a filter
    # with no package means "across the workspace".)
    scope="--workspace"
    for arg in "$@"; do
        case "$arg" in -p|-p=*|--package|--package=*) scope="" ;; esac
    done

    # An `-E` filterset matching exactly the tests in assets/test-tiers/external.txt.
    # The list is GENERATED from the #[ignore] reason strings (`just test tiers`)
    # and gated by A5, because a hand-written filter here is precisely what
    # drifts: the reasons move, the filter does not, and the mismatch is silent
    # in both directions.
    # `test(name)`, not `test(=name)`: a nextest test name is MODULE-QUALIFIED
    # (`chat::chat_ctrl_c_exits`, `embeddings::test_ollama_basic`), while the
    # generated list holds bare fn names because A5 derives it from source
    # without a build. Exact match therefore silenced 26 of the 35 entries —
    # they matched nothing and stayed in the blocking gate. Substring match is
    # safe because A5 also proves each name is unique among all test fns and is
    # not a substring of another one.
    external_filter() {
        sed -e 's/#.*//' -e '/^[[:space:]]*$/d' assets/test-tiers/external.txt \
            | sed 's/^/test(/; s/$/)/' | paste -sd'+' -
    }

    case "$tier" in
        quick)   cargo nextest run $scope "$@" ;;
        ignored) cargo nextest run $scope --run-ignored ignored-only "$@" ;;
        full)    cargo nextest run $scope --run-ignored all "$@" ;;
        gated)
            # The ignored tests whose prerequisites are hermetic: a built `cru`,
            # `mock-acp-agent`, ripgrep, this repo's docs/ kiln, or wall-clock
            # time. Selection is NEGATIVE — everything ignored except the
            # generated external list — so a NEWLY ADDED ignored test lands in
            # this blocking gate by default. That is deliberate (fail-closed)
            # and it will surprise someone once; when it does, A5 has already
            # run in `test ci` and told them either "unknown prerequisite" or
            # "regenerate the tier file", with the fix in the message.
            just build fixtures
            cargo build -p crucible-cli --bin cru
            # The PTY harness gives the child a hermetic HOME with no kiln, so
            # `ensure_valid_kiln` falls back to ascending to the git root. A
            # working tree usually has a `.crucible/` there and a fresh clone
            # never does (it is gitignored), so without this the PTY tests hit
            # the interactive "No kiln found" wizard and every wait_for_* times
            # out. Doing it here rather than in the CI job keeps local and CI on
            # the same path — a prerequisite only CI satisfies is how a tier
            # starts passing in one place and failing in the other.
            mkdir -p .crucible
            cargo nextest run --profile ci $scope --run-ignored ignored-only \
                -E "not ($(external_filter))" "$@"
            ;;
        external)
            # Needs a network, a model download, a live LLM, a container
            # runtime, a real DB, the Playwright harness, or a human reading
            # numbers. NOT a blocking gate — run it when you have the
            # prerequisites.
            just build fixtures
            cargo build -p crucible-cli --bin cru
            cargo nextest run --profile ci $scope --run-ignored ignored-only \
                -E "$(external_filter)" "$@"
            ;;
        tiers)
            # Regenerate assets/test-tiers/external.txt from the #[ignore]
            # reason strings. Same test that checks the file, so generation and
            # checking share one parser and cannot disagree.
            CRUCIBLE_WRITE_TEST_TIERS=1 cargo nextest run -p crucible-daemon \
                --test architecture_tests --no-capture \
                -E 'test(external_test_tier_file_matches_the_ignore_reasons)'
            ;;
        ci)
            just build fixtures
            cargo nextest run --profile ci $scope "$@"
            ;;
        features)
            cargo nextest run --profile ci -p crucible-oil --features serde,test-utils
            cargo nextest run --profile ci -p crucible-lua -E 'test(stubs)' --no-capture
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
            # NOT in `just ci` — the nextest gate
            # `shipped_plugin_lua_suite_passes` runs the same suites through the
            # same handler, in-process, needing neither a built binary nor a
            # live daemon, and `every_shipped_plugin_with_a_suite_is_gated`
            # proves it covers every plugin that has one. What this recipe adds
            # over that is the process boundary: `cru plugin test` -> RPC ->
            # daemon. Worth running by hand when you touch that path.
            #
            # `.fnl` as well as `.lua`: the runner compiles Fennel suites, and a
            # Fennel-only plugin (graph-view) was silently skipped by a
            # Lua-only glob.
            for dir in runtime/plugins/*/; do
                if compgen -G "${dir}tests/*.lua" > /dev/null \
                    || compgen -G "${dir}tests/*.fnl" > /dev/null; then
                    echo "== ${dir}"
                    just test plugin "${dir%/}"
                fi
            done
            ;;
        -p|-p=*|--package|--package=*) cargo nextest run "$tier" "$@" ;;
        -*) cargo nextest run $scope "$tier" "$@" ;;
        *)
            echo "Unknown test tier: $tier"
            echo "Valid tiers: quick ignored gated external full ci tiers features doc plugin plugins"
            exit 1
            ;;
    esac

# === Web ===

# `--standalone` is NOT optional: a debug `cru` on the shared socket detects the
# git-SHA mismatch and shuts the installed daemon down to respawn its own.
#
# Bound to every interface by default so a headless box is reachable; pass a
# host to narrow it (`just web 3000 127.0.0.1` for localhost-only). Any name or
# address a LAN client reaches it by works with no configuration — those
# clients authenticate with the key from `cru web key`.
#
# `--static-dir` is what makes this recipe's `web-build` dependency take effect:
# the binary's own assets are embedded at COMPILE time, so without the flag a
# `bun run build` would change nothing until the Rust crate was rebuilt too.
# Serving `dist/` from disk is a dev choice, not a build-profile one.
#
# For frontend hot reload, run `bun run dev` in crates/crucible-web/web
# alongside this. Its proxy is hardcoded to localhost:3000, so changing the
# port here leaves that dev server's /api pointing at nothing.
#
#     just web / just web 3001 / just web 3000 127.0.0.1
#
# Build the frontend and serve it (default 0.0.0.0:3000)
web port="3000" host="0.0.0.0": (web-build "off")
    cargo build -p crucible-cli --bin cru
    cargo run -p crucible-cli -- --standalone web --host {{host}} --port {{port}} --static-dir crates/crucible-web/web/dist

# `off` disables the PWA service worker, which otherwise serves stale assets
# from its cache and makes a rebuild look like it did nothing — always what you
# want locally, never what you want in a release.
#
# Build the SolidJS frontend
web-build pwa="on":
    cd crates/crucible-web/web && bun install && {{ if pwa == "off" { "VITE_DISABLE_PWA=1" } else { "" } }} bun run build

# The tiers, and why they are not interchangeable:
#
# - `unit` — Vitest/jsdom. `bun install` first so a fresh clone or a pulled
#   lockfile bump fails on the tests, not on a missing vitest.
# - `e2e` — Playwright against the Vite dev server. Always runs in a private
#   output dir: two concurrent runs otherwise share `test-results/` and wipe
#   each other's traces mid-run, which reads exactly like a flake.
# - `live` — the ONLY tier that exercises real HTTP responses from `cru web`
#   (CSP, nosniff, Content-Disposition, host validation, file serving); the
#   dev server sends none of those headers. It builds `cru` and `web/dist`
#   rather than skipping without them: the suite skips *green* when the binary
#   is absent, so as a CI gate an unbuilt `cru` would report success while
#   asserting nothing. Both are prerequisites: the setup starts `cru web` with
#   `--static-dir <web>/dist`, because rust-embed bakes the bundle in at COMPILE
#   time and `cru` is built here before the frontend is.
# - `hero` — the cross-surface flow (TUI -> web -> TUI, one session), made
#   deterministic by a fake Ollama server.
#
# Args pass through: `just web-test e2e cross-zone-dnd.spec.ts --project=chromium`.
#
# Web tests: e2e (default) | unit | live | stories | hero
web-test tier="e2e" *args:
    #!/usr/bin/env bash
    set -euo pipefail
    tier="$1"; shift
    web=crates/crucible-web/web
    case "$tier" in
        unit)
            cd "$web" && bun install && bunx vitest run "$@"
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
            echo "Valid tiers: unit e2e stories live hero"
            exit 1
            ;;
    esac

# === CI ===

# Every job in .github/workflows/ci.yml invokes one of these targets, so the two
# cannot drift. CI-only: `build-from-clean-clone` (needs a tree with no
# web/dist) and the sharded Playwright matrix.
#
# `test gated` runs LAST and deliberately: it is the only target that spawns
# real processes (37 PTY tests, 12 daemon-spawning ones), so it is the slowest
# to fail and the most useful to see after everything cheap has passed. It was
# added once its cost was measured — 72 tests in 31–56s across four consecutive
# green runs on a loaded box. `test ci` precedes it so gate A5 reports an
# unparseable `#[ignore]` reason before the tier derived from those reasons runs.
#
# Run every gate GitHub runs — do this before committing
ci: (lint "all") (test "ci") (test "features") (test "doc") (web-test "unit") (web-test "e2e") (web-test "live") (test "gated")
    @echo "CI checks passed!"

# === Daemon & tooling ===

# Build and restart daemon (kills stale daemon so next cru auto-spawns fresh)
dev:
    -pkill -f "cru daemon serve" 2>/dev/null
    cargo build

# Start the MCP server on port 3847; args pass through (`just mcp -v`)
mcp *args:
    cargo run --release -p crucible-cli -- mcp --port 3847 "$@"

# === Demos & fixtures ===

# `all` re-renders every tape and copies the two GIFs the docs site serves.
#
# Render demo GIFs from replay fixtures: all (default) | <fixture name>
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

# Needs the web tree's node_modules present: font and icon notices are read from
# the packages themselves rather than transcribed.
#
# Regenerate THIRD-PARTY-NOTICES.md from the dependency graph that ships
notices:
    python3 scripts/gen-third-party-notices.py
