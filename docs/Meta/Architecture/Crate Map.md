---
title: Crate Map
description: The six-crate Rust workspace — dependency order, each crate's root module, its build and example files, and the crate boundary AGENTS.md sets.
tags: [meta, architecture, workspace, crates]
status: as-built
as_of: 582c5e6c1
---

# Crate Map

The workspace is six crates: `crucible-core`, `crucible-oil`, `crucible-lua`,
`crucible-daemon`, `crucible-web`, `crucible-cli`. This page names each
crate's root module (`src/lib.rs`), its build script where it has one, and
its example and benchmark binaries — the files that state, in code, what a
crate exposes and how it compiles. It does not re-describe what each crate's
own modules do; the other architecture pages ([[Core Config]],
[[Agent Manager]], [[Tools and Admission]], [[Luau Host]], [[Oil Renderer]],
[[Web Server]], [[CLI Commands]]) cover that. This page only covers the
crate boundary itself: what a crate declares as `pub`, what it re-exports,
what its `Cargo.toml` depends on, and what its build/example files do
outside the library.

## Purpose and ownership

This subsystem owns the workspace's compilation boundaries: which crate a
module lives in, what each crate's root file exposes to the crates above it,
and the one-way dependency order between crates. It does not own any
subsystem's internal logic — a crate root only declares and re-exports; it
must hold no business logic of its own. AGENTS.md states the boundary
directly: "Crates are compilation boundaries, not folders. Prefer fewer,
larger crates," and gives each crate one line of ownership (`crucible-core`
for canonical domain types, config and the parser; `crucible-daemon` for
sessions, admission, tools, storage, retrieval, review and plugin lifecycle;
`crucible-cli`/`crucible-web` for input, presentation and client-local state;
`crucible-oil` for terminal rendering primitives; `crucible-lua`/`runtime/`
for the Luau host, bindings and plugin behavior). Every `src/lib.rs` file on
this page matches its crate's row in that table: `crucible-core/src/lib.rs`
re-exports domain types and nothing that reads a socket or a terminal;
`crucible-daemon/src/lib.rs` re-exports the RPC and session surface, not a
second config parser; `crucible-web/src/lib.rs` re-exports `WebConfig` from
`crucible-core` rather than redefining it.

## Module map

Crate roots (`src/lib.rs`), grouped by crate in dependency order (`core` is
the base; `cli` and `web` are the two frontends):

| Path | Lines | Role |
| --- | --- | --- |
| `crates/crucible-core/src/lib.rs` | 152 | Crate root for `crucible-core`. Declares every domain submodule — including `diff`, `git`, `note_frontmatter`, `proposal`, `sources`, `status_color` and `bases` — and re-exports the canonical types every other crate imports. Defines the legacy `CrucibleError`/`Result<T>` pair. |
| `crates/crucible-oil/src/lib.rs` | 84 | Crate root for `crucible-oil`. Declares the rendering submodules — including `screen`, for the full-screen `ScreenDiff`/`PresentStats` row-diff surface — and re-exports the `Node`/`Style`/`Terminal`/render-pipeline surface, plus `ScreenMode` from `terminal`; states the "Lean-JSON contract" (default-valued fields omitted on serialize). |
| `crates/crucible-lua/src/lib.rs` | 280 | Crate root for `crucible-lua`. Declares around sixty-five Luau-host submodules, re-exports the crate's full host API — including a `vault::bases` API re-exported as `bases_api`, and `tool:render` in place of the old `tool:display_start`/`tool:display_complete` hook pair — embeds the fallback `init.luau` as `BUILTIN_INIT_LUA`, and gates the build against `panic = "abort"`. |
| `crates/crucible-daemon/src/lib.rs` | 166 | Crate root for `crucible-daemon`. Declares around seventy submodules — including `diff`, `proposals`, `bases` and the crate-private `lossless_queue`, and no longer `permission_bridge` — and re-exports the RPC, session, plugin-lifecycle and wire-type surface that `crucible-cli` and `crucible-web` consume. Sets `#![recursion_limit = "256"]`. |
| `crates/crucible-web/src/lib.rs` | 16 | Crate root for `crucible-web`. Declares `routes`, `server`, `services`, `fs_events`, `middleware` as public and `assets`/`error`/`events` as private, re-exporting only `WebConfig`, `Result`, `WebError`, `ChatEvent`, `start_server`. |
| `crates/crucible-cli/src/lib.rs` | 22 | Crate root for `crucible-cli`. Declares `cli`, `commands`, `config`, `factories`, `output`, `tui` as public and `chat`, `common`, `formatting`, `kiln_attach`, `kiln_discover`, `kiln_validate`, `provider_detect`, `status_line` as crate-private. |

Build scripts:

| Path | Lines | Role |
| --- | --- | --- |
| `crates/crucible-daemon/build.rs` | 41 | Stamps `CRUCIBLE_BUILD_SHA` (short git SHA) via `cargo:rustc-env` so a client and a running daemon can detect a version mismatch and trigger a restart. |
| `crates/crucible-web/build.rs` | 81 | Checks whether the SolidJS bundle (`web/dist/index.html`) exists and sets `CRUCIBLE_WEB_UI_EMBEDDED`; panics only when `CRUCIBLE_REQUIRE_WEB_UI` is set and the bundle is absent (release CI). Never invokes `bun` itself. |

Examples and benchmarks (`cargo run --example ...` / `cargo bench`, not linked
into any binary):

| Path | Lines | Role |
| --- | --- | --- |
| `crates/crucible-cli/examples/fullscreen_demo.rs` | 177 | Runs `FullscreenShell` (two fake chat panes plus a 10,000-line plugin buffer) against a real `crucible_oil::terminal::Terminal` with no daemon, to check scroll, selection and copy under `ScreenMode::Fullscreen`. |
| `crates/crucible-cli/examples/test_mcp_server.rs` | 78 | Spawns `cru mcp` as a child process, lists its tools over the MCP stdio transport, and asserts the count is exactly 12. |
| `crates/crucible-daemon/benches/llm_embedding_comparison.rs` | 130 | Criterion benchmark of the FastEmbed embedding provider's throughput across batch sizes (1, 10, 50, 100, 500) and single-call versus batch-call embedding. |

## Key types and traits

- `CrucibleError` / `Result<T>` (`crates/crucible-core/src/lib.rs`) — a
  three-variant `thiserror` error type (`DocumentNotFound`, `InvalidOperation`,
  `DatabaseError`) defined at the crate root. Later submodules define more
  specific `thiserror` enums (`HttpError`, `JobError`, and so on); this type
  is the oldest one, but it still has live callers outside this crate:
  `crucible-daemon`'s `storage/sqlite/repository.rs` and `rpc_client/storage.rs`
  construct `CrucibleError::DatabaseError`, and `crucible-lua`'s `vault/mod.rs`
  converts it into an `mlua::Error`.
- `BUILTIN_INIT_LUA: &str` (`crates/crucible-lua/src/lib.rs`) — the compiled-in
  default `init.luau` text (`include_str!`), created once at compile time and
  held as a crate constant. It is the last-resort baseline behind the
  daemon's own default-loading path; `crucible-daemon` decides when to use it,
  `crucible-lua` only carries the bytes.
- `is_default<T: Default + PartialEq>` (`crates/crucible-oil/src/lib.rs`) — a
  `pub(crate)` helper behind the `serde` feature, used via
  `skip_serializing_if` across every node and style type so a default-valued
  field is omitted on serialize.
- `WebConfig`, `Result`, `WebError`, `ChatEvent` (`crates/crucible-web/src/lib.rs`)
  — re-exports, not new definitions. `WebConfig` is created inside
  `crucible-core`'s config tree; `crucible-web`'s server holds it for the
  lifetime of one `cru web` process; `crucible-cli`'s `cru web` command and
  the SolidJS frontend's generated types consume `ChatEvent`.

One exception aside — `crucible-core/src/lib.rs`'s own `CrucibleError` enum,
which `crucible-daemon` and `crucible-lua` construct at runtime (see Key types
and traits, above) — no other crate-root file in this page defines a struct,
trait or enum that another module constructs, holds or mutates at runtime;
every other one is declaration and re-export. The types that matter at
runtime (`AgentManager`, `Server`, `DaemonClient`, `Cli`, node/style types)
are defined in the submodules these roots declare, and are covered on the
pages named above.

## Flows

There are two flows this page's files participate in: the compile-time
module/re-export wiring, and the build-time SHA/asset flags. Neither crosses
more than two modules on its own, so no diagram is needed.

**Compile-time wiring.** Each `src/lib.rs` runs once, at compile time, not at
process runtime: it declares `pub mod` / `mod` for its crate's submodules and
`pub use`s the subset of each submodule's items that its dependents need. A
consumer crate's `use crucible_core::config::WebConfig` resolves through
`crucible-core/src/lib.rs`'s `pub mod config` declaration and its config
submodule's own `pub` items; `crucible-web/src/lib.rs`'s
`pub use crucible_core::config::WebConfig;` line then makes the same type
available as `crucible_web::WebConfig` without redefining it.

**`crucible-daemon/build.rs` → `CRUCIBLE_BUILD_SHA`.** Cargo runs
`crucible-daemon/build.rs` before compiling the crate. It runs
`git rev-parse --short HEAD`; on success it emits
`cargo:rustc-env=CRUCIBLE_BUILD_SHA=<sha>`, which the daemon binary reads
later via `option_env!` when comparing its own build against a connecting
client's. If git is unavailable (a CI archive, `cargo publish`), it sets
nothing and both sides fall back to the literal string `"dev"`.

**`crucible-web/build.rs` → `CRUCIBLE_WEB_UI_EMBEDDED`.** Cargo runs
`crucible-web/build.rs` before compiling the crate. It checks for
`web/dist/index.html` and emits `cargo:rustc-env=CRUCIBLE_WEB_UI_EMBEDDED=0`
or `=1`. `crates/crucible-web/src/assets.rs` (outside this page) does not
read this variable directly — `rust-embed`'s own missing-file handling covers
the absent case — so the flag is informational for CI rather than a runtime
branch. A missing bundle only becomes a hard failure when
`CRUCIBLE_REQUIRE_WEB_UI` is also set, which release builds set and ordinary
local builds do not.

## State, concurrency and lifecycle

Nothing on this page runs a task, holds a lock, or opens a channel. Every
file is either compiled once and produces no runtime value (`src/lib.rs`,
`build.rs`) or is a separate binary target that Cargo never links into `cru`
(`examples/`, `benches/`). The one runtime-adjacent fact any of these files
states is `crucible-daemon/src/lib.rs`'s `#![recursion_limit = "256"]`: this
is a compiler attribute, not a runtime state; it exists because
`spawn_delegation`'s future (defined elsewhere in `crucible-daemon`) nests
deeply enough that the compiler's auto-trait resolution can exceed the
default recursion limit, per the file's own comment ("Not a real cycle —
rustc just needs more headroom").

Lifecycle that does matter here is the workspace's own dependency order,
read from each crate's `Cargo.toml`: `crucible-core` depends on none of the
other five; `crucible-oil` depends only on `crucible-core`; `crucible-lua`
depends on `crucible-core` and `crucible-oil`; `crucible-daemon` depends on
`crucible-core` and `crucible-lua` (feature `send`); `crucible-web` depends
on `crucible-core`, `crucible-daemon` and `crucible-lua`; `crucible-cli`
depends on all five others, with `crucible-web` behind its own `web` feature
(on by default) so a slim CLI build can drop it. This order is one-way:
`crucible-core` never depends on any of the other five crates. The
cross-crate import matrix's handful of `core -> daemon`, `core -> lua` and
`core -> web` matches are doc comments naming a downstream consumer for
context (for example `crates/crucible-core/src/turn/mod.rs`'s comment naming
`crucible_web::ChatEvent`), not `use` imports; grep against
`crates/crucible-core/src` for `crucible_daemon`, `crucible_lua` and
`crucible_web` turns up only comments, confirming the dependency graph has no
reverse edge.

## Boundaries and invariants

- A crate root only declares and re-exports. None of the six `src/lib.rs`
  files defines executable logic beyond a re-export, a constant, or (for
  `crucible-oil`) one small serialization helper. Business logic stays in the
  submodules the root declares.
- `crucible-lua/src/lib.rs` enforces one build-time invariant with a
  `compile_error!`: the crate must never build under `panic = "abort"`. Its
  comment states the reason directly — Luau raises a host callback's `Err` by
  throwing, and an abort-panic build turns every Rust stack frame `nounwind`,
  so the process dies on the spot instead of unwinding. The workspace
  `Cargo.toml` repeats the same trap warning just above `[profile.release]`.
  `crucible-lua/src/lib.rs`'s own top-of-file comment states this bit a
  release daemon at boot twice in one day, for two unrelated reasons (a
  misspelled plugin field and a char-boundary bug in the vendored markdown
  parser).
  Root symbol: `crucible-lua`'s `#[cfg(panic = "abort")] compile_error!(...)`.
- `crucible-web/build.rs` enforces that it never modifies the source tree
  itself: it only reports whether the frontend bundle exists and, in the
  release-CI case, fails loudly rather than silently shipping an empty UI. It
  does not invoke `bun` and does not write outside `OUT_DIR`.
  `crates/crucible-daemon/build.rs` enforces the matching rule for the
  version-SHA stamp: it degrades to `"dev"` rather than failing the build
  when git is unavailable.
  `crates/crucible-web/src/lib.rs`'s `test_support` module is gated behind
  `#[cfg(any(test, feature = "test-utils"))]`, keeping test-only code out of
  a release binary while still letting `crucible-web`'s own integration tests
  (`crates/crucible-web/tests/`) depend on it: the crate's `dev-dependencies`
  re-add itself with the `test-utils` feature enabled.
- The visibility split in each `src/lib.rs` is itself an enforced boundary:
  `crucible-cli/src/lib.rs` marks `chat`, `common`, `formatting`,
  `kiln_attach`, `kiln_discover`, `kiln_validate`, `provider_detect` and
  `status_line` crate-private, leaving only `cli`, `commands`, `config`,
  `factories`, `output` and `tui` as this crate's public surface;
  `crucible-web/src/lib.rs` marks `assets`, `error` and `events`
  private, re-exporting their public types (`WebError`, `ChatEvent`) at the
  crate root so a caller never writes `crucible_web::error::WebError`.

## Extension seams

- A new domain type, config field or parser rule lands in `crucible-core`
  and is re-exported from `crates/crucible-core/src/lib.rs`'s `pub use` block
  so every dependent crate sees it without a second copy. See [[Core Config]]
  and [[Parser]] for where inside `crucible-core` a given kind of type lands.
- A new daemon-owned capability (a tool, an RPC method, a plugin-lifecycle
  hook) lands inside `crucible-daemon`'s own submodules and, if a frontend
  needs to reach it, gets a matching re-export line added to
  `crates/crucible-daemon/src/lib.rs`. The `diff`, `proposals` and `bases`
  submodules are recent examples of a capability landing this way. See
  [[Agent Manager]], [[Tools and Admission]], [[Review]] and [[RPC Client]]
  for where inside `crucible-daemon` a given capability lands.
- A new terminal rendering primitive lands in `crucible-oil`'s own modules
  and gets re-exported from `crates/crucible-oil/src/lib.rs` following the
  Lean-JSON contract (`skip_serializing_if = "is_default"` on any new
  default-valued field). See [[Oil Renderer]].
  A new Luau host API surface follows the matching path through
  `crates/crucible-lua/src/lib.rs`'s `pub use` block. See [[Luau Host]] and
  [[Luau APIs]].
- A new web route or SSE event lands inside `crucible-web`'s `routes`,
  `server`, `services`, `fs_events` or `middleware` modules; only a new
  crate-root-level export (an error variant, an event variant) touches
  `crates/crucible-web/src/lib.rs` itself. See [[Web Server]] and
  [[Web Windowing]] for the frontend half.
- A new `cru` subcommand lands inside `crucible-cli`'s `cli`/`commands`
  modules, which are already public from `crates/crucible-cli/src/lib.rs`; no
  change to the root file itself is normally needed. See [[CLI Commands]].
- A new manual smoke test follows the pattern in
  `crates/crucible-cli/examples/test_mcp_server.rs` and
  `crates/crucible-cli/examples/fullscreen_demo.rs`: an `examples/`
  binary that is never linked into `cru` and is run by hand via
  `cargo run --example <name>`. A new throughput measurement follows
  `crates/crucible-daemon/benches/llm_embedding_comparison.rs` and runs only
  via `cargo bench`.

## Tests

None of the twelve files on this page has a dedicated unit test, and none
needs one: each is either declarative wiring with no branch to cover, a build
script exercised implicitly by every `cargo build` in CI, or a manually-run
example/benchmark binary that is explicitly not part of the automated suite.

- `crates/crucible-cli/examples/test_mcp_server.rs` is itself a manual
  regression check (asserts an exact tool count of 12) run by hand, not by
  `just test` or CI; its own oddity is that the count is a hardcoded magic
  number with no shared source of truth, so an added or removed tool will
  silently need a manual update here to keep passing.
  `crates/crucible-cli/examples/fullscreen_demo.rs` is the same kind of
  manual check for the full-screen prototype: a person runs it and reads
  its stderr frame-time/byte-size percentiles by eye, with no assertion.
- The workspace's dependency direction (`crucible-core` never depending on
  the other five) is not gated by a compiler or CI check specific to this
  page; the crate graph is enforced only by each crate's own `Cargo.toml`,
  which is a manual invariant, not a test.
- Gap: nothing in the tree asserts that each `src/lib.rs`'s `pub`/private
  module split matches its crate's own `AGENTS.md` table (for example that
  `crucible-cli`'s `chat`/`common`/`formatting` stay crate-private). A
  renamed module could silently widen a crate's public surface without a
  failing test naming it.

## Findings

- `crates/crucible-daemon/benches/llm_embedding_comparison.rs` carries a
  comment about a removed Burn backend it once compared against — dead
  history in a comment, not dead code, and does not affect the benchmark's
  current behavior.
- `crates/crucible-core/src/lib.rs` declares `pub mod bases;` after the
  `Result<T>` type alias, at the very end of the file, apart from the
  alphabetized block of every other `pub mod` line above it. The placement
  does not change what the module exports; it is a minor ordering
  inconsistency, not a boundary violation.
- No conflict with AGENTS.md's crate-boundary rules was found: every
  crate-root file re-exports rather than duplicates, the one-way dependency
  order holds (confirmed against `Cargo.toml` and against a grep of
  `crucible-core/src` for reverse imports), and the two build scripts stay
  out of business logic as their own comments require.
