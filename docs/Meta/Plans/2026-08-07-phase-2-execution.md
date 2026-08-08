---
title: Phase 2 Execution — Repo Credibility
description: Task-by-task execution guide for Phase 2 of the public launch readiness plan.
tags:
  - meta
  - plan
  - release
status: active
created: 2026-08-07
---

# Phase 2 Execution — Repo Credibility

> Task-by-task execution guide for Phase 2 of the public launch readiness plan.
> Claims below were re-verified against `bdffb48aa` on 2026-08-07; where the plan's
> numbers had drifted, the verified number is used instead.

## Corrections to the plan's own figures

Re-verification before writing this doc:

- **Note count is 126, not 118** (`find docs -name '*.md' | wc -l`). The README says
  155. Rather than replace one stale number with another, the count is **removed** —
  a hand-maintained figure in a landing page rots by construction, and no reader is
  making a decision on it.
- **`crucible.search` is wrong in four places, not one.** Beyond `README.md:145`, it
  appears in `crates/crucible-lua/src/annotations.rs` at `:14`, `:25`, and `:642`
  (module doc comment, Fennel example, and a test fixture). The correct API is
  `cru.kiln.search`, documented in the module header of
  `crates/crucible-lua/src/vault.rs`.
- **`grafana/opencode` is wrong in four places, not three**: `README.md:118`,
  `acp/discovery.rs:229`, `acp/discovery.rs:260`, and
  `docs/Help/Concepts/Agent Client Protocol.md:157`. The replacement is *not*
  `sst/opencode` either — that repo 301s to `anomalyco/opencode`. Cite
  [opencode.ai](https://opencode.ai) and install with
  `curl -fsSL https://opencode.ai/install | bash` (or `npm install -g opencode-ai@latest`).
- **The `just clippy` gap is worse than "omits `--workspace`."** It reads
  `cargo clippy --all-targets -- -D warnings` (`justfile:113`). With
  `default-members = ["crates/crucible-cli"]`, `--all-targets` expands for the CLI
  alone. The GitHub job (`ci.yml:29`) *does* pass `--workspace` but with `-W warnings`,
  which does not fail. So the two gates each have exactly the half the other is
  missing, and between them nothing deny-lints daemon/core/lua/oil test code.
- **`cru acp` and `cru web` both ship** (`cli/mod.rs:203` and `:512`), confirming the
  Roadmap is stale. `cru acp` *is* the editor-embedding agent mode, not host-side
  plumbing — `crates/crucible-cli/src/commands/acp/mod.rs:1` opens "run Crucible as an
  ACP **agent** over stdio", spawned by Zed / JetBrains / Neovim / marimo. The web row
  is checkable. The ACP row stays unchecked for a different reason than the plan gave:
  the v1 surface is partial — session modes, model switching, and host-side
  filesystem/terminal capabilities are not wired. Say that in the roadmap rather than
  denying the command is agent mode.
- **`SECURITY.md` gate.** The plan defers it until 0.0a/0.0b are released; the latest
  release is `v0.22.0` (2026-08-05), which predates the Phase 0 fixes. Decision taken:
  **land it now.** The file discloses nothing about either defect — it opens a private
  channel — and a private channel is worth strictly more before the audience grows
  than after. It points at GitHub private vulnerability reporting, so no personal
  address is published.

## Task groups

Partitioned by file ownership so the groups can run concurrently without collisions.
No two groups write the same file.

---

### T1 — Gates that can actually fail (§2.1, §2.3-durable)

**Owns:** `.github/workflows/ci.yml`, `justfile`

1. `ci.yml:29` — `-W warnings` → `-D warnings`.
2. `justfile:113` — `just clippy` gains `--workspace`.
3. New `docs` job running the kiln invariant suite. This is the durable fix for §2.3:
   without it the one-time cleanup rots again within a month. **Not** `paths: docs/**`
   as first drafted — GitHub Actions has no per-job path filter, a skipped job blocks a
   required status check, and the code-reference invariant breaks on Rust changes that
   touch no doc. It runs unconditionally.
4. `test-plugins` (5 Lua suites, in `just ci`, in no GitHub job) gets a job.
5. `just web-typecheck` runs nowhere; fold into the existing `test-web-unit` job
   rather than paying for a sixth runner.
6. ~~`test-sqlite` gains `--profile ci`.~~ **Superseded:** `--profile ci` sets
   `retries = 1`, so this would have made the job looser, not stricter — and its test
   set is a strict subset of the `test` job's `--workspace` run (274 == 274 by
   `nextest list`). Two jobs running identical tests under different retry policies can
   disagree with no authoritative verdict. The job was deleted instead.
7. `just setup` bootstrap recipe — installs `cargo-nextest`, `cargo-deny`, `bun` deps,
   Playwright browsers; names `protobuf-compiler` as the one thing it cannot install.
8. `just web-test-unit` (`justfile:309`) is bare `bunx vitest run` with no `bun install`.

**Proof (approval criterion 1).** Introduce a deliberate warning in *daemon test* code,
confirm `just clippy` fails, confirm the `ci.yml` invocation fails, revert. Record both
outcomes. This is the only check that proves both halves; today neither catches it.

---

### T2 — CONTRIBUTING, templates, conduct (§2.2, part of §2.3)

**Owns:** `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `SECURITY.md`,
`.github/ISSUE_TEMPLATE/`, `.github/pull_request_template.md`

- `CONTRIBUTING.md:227-280` documents `just test fixtures|infra|slow|all` and cargo
  features `test-fixtures`/`test-infrastructure`/`test-slow`. The recipe accepts only
  `quick|ignored|full`; those features have zero occurrences in any manifest. Rewrite
  against `AGENTS.md` and the real `#[ignore]` convention.
- Prerequisites (`:8-9`) list Rust and `just`. Actually required: `protobuf-compiler`,
  `bun`, `cargo-nextest`, `cargo-deny`, Playwright browsers. Point at `just setup`.
- Replace `cargo test --workspace` guidance with `just ci` — the justfile's own header
  warns raw workspace cargo can OOM the box.
- `SECURITY.md`: supported versions, GitHub private vulnerability reporting as the
  channel, response-time expectation, embargo expectations.
- `CODE_OF_CONDUCT.md:48` routes harassment reports to a public issue tracker. Replace
  with the private channel.
- Issue templates and a PR template lifted from the CONTRIBUTING checklist.

**Rule:** every command this file names must exist in the `justfile`. That is
approval criterion 2, and it is checkable mechanically.

---

### T3 — The landing page (§2.3 README items, §2.4)

**Owns:** `README.md`

- Plugin example (`:140-147`) is wrong three ways: `description=` → `desc=`
  (`annotations.rs:389-393`), `@param` takes unquoted `name type Description`, and
  `crucible.search` → `cru.kiln.search`. This is the only code sample on the landing
  page and it cannot work as written.
- `:101` "Agent **Context** Protocol" + `agentcontextprotocol.org` → Agent **Client**
  Protocol, `agentclientprotocol.com`. Contradicted by the project's own
  `docs/Help/Concepts/Agent Client Protocol.md:21`.
- `:154` "155 interlinked notes" → drop the count (see corrections above).
- `:42` claims Linux aarch64 binaries; dist builds only `x86_64-unknown-linux-gnu` and
  `aarch64-apple-darwin` (`Cargo.toml` `[workspace.metadata.dist]`).
- `:118` `go install github.com/grafana/opencode@latest` → the real opencode install,
  `curl -fsSL https://opencode.ai/install | bash` (or `npm install -g opencode-ai@latest`).
  The repo is `anomalyco/opencode`; `sst/opencode` is a redirect, not the home.
- Roadmap: check the web row; leave ACP agent mode unchecked.
- Comparison table (`:29-38`): cut the OpenClaw column and the setup-time row.
  Compare on architecture.
- Dual-license note so GitHub's Apache-only detection and the MIT/Apache claim agree.
- Still images: `assets/chat-demo.png` and `assets/chat-response.png` exist and are
  unused. `.gitignore` blanket-ignores `*.png`, so landing them needs an exception.
  **Verify they show current UI before using them** — a stale screenshot is worse than
  none.

---

### T4 — Documentation truth pass (§2.3 docs items)

**Owns:** `docs/Guides/`, `docs/Help/` (except `docs/Help/Workflows/Index.md`,
owned by T5), `docs/Config.toml`

- `kiln.db` → `crucible-sqlite.db` (`Getting Started.md:205`, `Windows Setup.md:34`;
  real paths in `kiln_manager.rs:443,1031-1050`).
- "exactly five checks" → 8 (`Help/CLI/Index.md:72`, `Help/CLI/doctor.md:21,53`).
- `.crucible/config.toml` → `.crucible/kiln.toml` (`Help/Config/agents.md:21`,
  `Config.toml:9`, `Help/Configuration.md:276`).
- `brew uninstall crucible` (`Getting Started.md:257`) — no tap exists.
- `Help/Concepts/Agent Client Protocol.md:157` — opencode upstream is
  `anomalyco/opencode` / opencode.ai, not `grafana/opencode` and not `sst/opencode`.
- Six placeholder notes bodied "This note exists to provide a stable link target":
  `Agents/Tool Capabilities.md`, `Help/CLI/skills.md`, `Help/Config/LLM Providers.md`,
  `Help/TOON Format.md`, `Help/Lua/Ask Module.md`,
  `Help/Extending/Scripts/Daily Summary.md`. Fill or delete — the five under `Help/`
  publish as empty pages. **Filling requires reading the implementation**; a note that
  invents behaviour is worse than the placeholder.
- New `docs/Help/Config/acp.md` and `docs/Help/Config/web.md` documenting every field
  on `AcpConfig` and `WebConfig`. Scope cut per the plan: the other undocumented
  sections are deferred.

**Constraint:** every TOML block written here must survive `CliAppConfig::load`
(T6 enforces this).

---

### T5 — Scrub internal leakage (§2.5 do-now)

**Owns:** `docs/Meta/Plans/2026-08-03-acp-presentation-parity.md`,
`docs/Help/Workflows/Index.md`, `docs/Meta/TUI User Stories.md`,
`docs/Meta/Web User Stories.md`, `.file-size-whitelist`, `Journal/`,
and the `thoughts/` references in:
`crates/crucible-cli/src/commands/workflow.rs`,
`crates/crucible-core/src/parser/types/workflow.rs`,
`crates/crucible-core/src/workflow/mod.rs`,
`crates/crucible-core/tests/dev_kiln.rs`,
`crates/crucible-daemon/src/acp/client/recording.rs`,
`crates/crucible-daemon/src/server/fs.rs`,
`crates/crucible-daemon/src/tool_dispatch.rs`,
`crates/crucible-oil/tests/sequencing_proofs.rs`,
`crates/crucible-web/web/vite.config.ts`

- Scrub the personal path `/home/moot/.crucible` and the
  `> **For Claude:** REQUIRED SUB-SKILL:` harness directive from the one tracked plan.
- 13 substantive `thoughts/` references (14 files, one of which is `.gitignore`
  itself). Each is a dead link for a public contributor. **Judge each individually** —
  some are load-bearing defaults in production code, not doc references, and those
  need a real fix rather than a text edit.
- Delete root `Journal/2025-03-20.md`, `Journal/2025-06-15.md`, `Journal/2026-07-24.md`
  (daily-notes test artifacts violating the repo-root-clean rule).

Deferred, as the plan directs: the `docs/Meta/` publication *policy*.

---

### T6 — The config-truth test (§2.3, approval criterion 6)

**Owns:** `crates/crucible-core/tests/` (new test file)

Extract every TOML block from `docs/Help/**` and `docs/Guides/**` and feed each
through `CliAppConfig::load`. Same shape as `dev_kiln_code_references_exist`, same
`#[ignore]` convention, wired into the same new CI docs job as T1's item 3.

Fragments that are deliberately partial (a bare `[acp.agents.my-claude]` stanza) must
still parse as a whole config — if they do not, the doc is teaching config the loader
rejects, which is exactly the thing under test. Blocks explicitly marked as non-config
(shell, output samples) are excluded by fence language, not by an allowlist.

---

### T7 — Source-side truth (§2.3 items outside docs)

**Owns:** `crates/crucible-daemon/src/acp/discovery.rs`,
`crates/crucible-lua/src/annotations.rs`,
`runtime/crucible-help/skills/crucible-help/SKILL.md`

- `discovery.rs:229,260` — opencode upstream, in user-facing error text. Point at
  opencode.ai / `anomalyco/opencode`.
- `annotations.rs:14,25,642` — `crucible.search` → `cru.kiln.search` in the module doc
  comment, the Fennel example, and the test fixture.
- `SKILL.md:23` — "Agent Context Protocol" in the support agent's own knowledge, so
  the shipped help agent currently teaches the wrong protocol name.

---

## Out of band

- **Branch protection** — enable required status checks on `master` via `gh api`.
  Not a file change; done once, verified by re-reading the protection endpoint.

## Approval criteria

1. **2.1** — a deliberate clippy warning in daemon *test* code fails both `just ci`
   and the GitHub `check` job. Revert after proving it.
2. **2.2** — every command named in `CONTRIBUTING.md` exists in the `justfile`.
3. **2.3 durable** — the dev_kiln invariant suite runs in CI on any `docs/**` change
   and passes, `dev_kiln_code_references_exist` included.
4. **2.3 cleanup** — zero placeholder notes under `docs/Help/`; the README plugin
   example is copy-pasteable and produces a working tool; `SECURITY.md` and the issue
   and PR templates exist; no tracked file references `thoughts/`; root `Journal/` is
   gone.
5. **2.4** — no comparison row names a competitor project. *(Discord/X card rendering
   needs a real paste; out of scope for this execution.)*
6. **2.6** — nothing in `docs/Help/` or `docs/Guides/` teaches config the loader
   rejects, proven by T6's test rather than by inspection.

## Finishing

One commit per task group, `just ci` before the final commit, adversarial review,
fast-forward merge.
