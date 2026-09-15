---
title: Test Suite Reduction — 2026-09-14
description: Coverage-preserving consolidation of test fixtures, CI ownership and development recipes.
tags: [meta, testing, review]
status: implemented
---

# Test Suite Reduction — 2026-09-14

Follows [[2026-09-14 Code Reduction Review]] and uses [[Product]] as
the behavioral baseline. Changes are on `refactor/code-reduction-review`.
The objective is fewer repeated executions and fixtures, not fewer assertions
or smaller property-test budgets.

## Coverage ownership

| Area | Change | Coverage retained |
| --- | --- | --- |
| CI overlap | Remove repeated Oil/Lua execution, docs reruns and two plugin jobs | Workspace nextest executes feature-enabled Oil tests, every shipped plugin suite and both exhaustive Lua typecheck gates; docs lint owns the ignored docs tests |
| Fixture builds | Nextest setup owns fresh process fixtures; build mock agent and shipping CLI with separate feature graphs | CLI and daemon packages still trigger setup; no freshness checks are removed |
| Copied Rust tests | Remove repeated configuration, theme, socket-path, connection and no-context cases | Canonical domain tests remain; one drawer adapter test explicitly crosses the CLI trait boundary |
| Browser setup | Share model-menu assertions with selection; keep the strongest ended-session scenario; combine six shell boot tests | Controls, absent legacy UI, layout, toggle state, errors, file/chat coexistence, pop-out and docking assertions remain |
| Weak slow tests | Replace sleep-based PTY responsiveness with rendered typing/backspace; test ACP timeout with an open silent transport | Real PTY input and output remain; ACP distinguishes timeout from EOF and exercises configured timeout bounds |
| Web environments | Pure fuzzy, canvas-document and table-format suites use Node | Component suites retain jsdom and browser shims |
| Repeated I/O | Index docs filenames once per sweep, cache regexes, share one real search server/index, advance idle time virtually | Case/extension/path-hint resolution, watcher-fed body/title/text/asset/query searches and real socket lifecycle behavior remain |
| Replay cost | Cache terminal text between updates and avoid repeated line-vector allocation | Every replay fixture, terminal size, frame and semantic assertion remains |

The idle timer's two observations now use Tokio's clock (converted to the
existing standard-library instant type). They previously mixed a Tokio
scheduled probe with a wall-clock observation, making virtual-time tests
incorrect. Outside a paused runtime they use the same monotonic clock.

The delegation process test previously accepted a malformed SSE exchange and
could spend ten seconds waiting for a request that never arrived. Its mock
now frames SSE correctly, reads complete requests and shuts down explicitly.
The assertions cover a correlated unavailable-tool error returning to the
model and successful turn completion. This is deliberately not called a
user-visible "delegation disabled" error: that text does not reach the CLI on
this path. The daemon's specific disabled-delegation test remains separate.

## Simpler recipes

The justfile shrinks from 698 to 424 lines without moving its complexity into
another dispatcher. Existing command names remain available. Lint uses one
direct case table; plugin typechecking reuses the two exhaustive Rust gates;
the optional process-path plugin loop builds the CLI once. `just --list`
shows the entry points and sub-targets.
Rust test tiers stay together, before the frontend build can invalidate the
embedded-web binary and force another test-fixture rebuild.

- `just test quick`: ordinary tests, scoped with `-p` or `-E` when useful.
- `just test gated`: ignored local-prerequisite tests except docs, which lint owns.
- `just test external`: tests requiring external services or manual prerequisites.
- `just test features`: compile the minimal production Oil library; feature-enabled tests already run in the main tier.
- `just web-test unit`: fast frontend iteration; `coverage` runs the same suite with coverage thresholds.
- `just plugin-check`: both shipped-plugin and other shipped-Lua typecheck gates.
- `just ci`: lint, ordinary and gated Rust tests, minimal-feature compilation,
  doctests, frontend coverage, mocked browser tests and live-server tests.

## Verification

Replacement drawer, terminal-cache, docs-resolution and ACP-timeout tests were
each observed failing under a deliberate mutation and restored. The virtual
idle tests fail with the former wall-clock observations. The API contract
matrix fails when its plugin-options endpoint is changed.

Frontend coverage reporting is now part of CI rather than an optional local
measurement. Its existing thresholds were not relaxed. The baseline suite
passed its tests but failed API/notification coverage thresholds; explicit
contract and notification-action assertions close those gaps. A deterministic
math case preserves a branch that had depended on random property inputs.

Full `just ci` passes: 9,062 ordinary Rust tests, 65 gated tests, 58 doctests,
2,510 frontend tests, 129 mocked browser tests and 17 live-server tests, plus
lint, docs and minimal-feature compilation. The frontend
comparison of the full coverage maps finds no previously covered statement,
function or branch lost. This change removes 29 Rust tests and eight browser
tests, and adds seven frontend tests. The source change is a net reduction of
304 Rust/TypeScript lines, in addition to the 274-line justfile reduction.
Frontend line coverage increases from 77.15% to 77.99%; branch coverage from
65.31% to 65.88%. The simplified `plugin-check` separately passes both gates.
The reordered Rust tiers pass again with cached fixtures: 13.18 seconds for
ordinary tests and 1.77 seconds for gated tests, including nextest setup.

Keep runtime claims separate from coverage claims. Whole-run elapsed time is
affected by build-cache warmth and machine load; removing repeated tiers is a
structural saving, not an additive benchmark. No Rust coverage percentage is
claimed, and external-service tests were not discarded or treated as locally
verified.
