---
title: Code Reduction Review — 2026-09-14
description: Evidence, implementation and validation of twelve code-reduction and reachability improvements.
tags: [meta, architecture, review]
status: complete
---

# Code Reduction Review — 2026-09-14

Reviewed at `53534ceea`. The findings below retain their baseline evidence;
all twelve implementation items are now applied on branch
`refactor/code-reduction-review`, with full CI passing.
Historical source citations use crate-relative paths and baseline line numbers;
deleted modules are not current code references. It follows
[[Consolidation Plan]], whose first five tiers have already run. The old
plan is historical evidence, not a current list of things to delete.

The best remaining reduction is whole implementations with no production
caller, plus machinery whose output nobody consumes. Several survived the
earlier review because tests used them. Those tests establish that the
implementation works in isolation; they do not establish that the product
uses it. Useful test doubles and fixtures are a different case and should
stay when they exercise a live path.

The initial review estimated roughly 2,000–3,000 physical lines of reduction
plus several hundred from web RPC forwarding. The implementation removes
**3,587 net source lines**: 4,636 removed and 1,049 added, including new tests,
comments and formatting. This measures Rust and TypeScript/TSX changes under
`crates/`, including new files; documentation and snapshots are excluded.

## Implementation

Applied in the suggested sequence: R1/R4/R5/R6/R12, then R3/R11, R2, R8/R7,
and R9/R10. No stored notes or transcripts were deleted, and no migration is needed.

- Removed the isolated ACP, indexer, raw-query and processing-outcome remnants.
- Kept native watch grouping, both debounce stages, overflow behavior and capture suppression.
- Shared the SQLite vector implementation and existing embedding-provider factory.
- Converted 79 web forwarders to explicit replay-policy rows. Mutations, including
  plugin actions and session sends, are single-attempt. The shared client also
  stops retrying undo and setters; web clients disable nested timeout retries.
- A disconnected reader releases pending calls promptly. Real-socket tests cover
  lost replies, a later safe read, one reconnect, sticky subscriptions and events.
- Mounted the voice provider in the app. Real composer/provider tests cover the
  configured server request, appended transcription and visible error without draft loss.
- Cleaned frontend exports, retained Tailwind dependencies by including CSS in
  analysis, and enabled the dead-code check in normal lint/CI.
- Retired the inert Lua message-panel API explicitly in user docs. Notification
  delivery remains live; panel visibility stays client-owned.

Validation: 13 focused retry/queue/vector tests and 17 voice tests passed.
Both voice tests failed before the provider was wired. The mutation replay test
and both queue tests were deliberately broken and failed, then restored.
All lint gates passed: Rust formatting and Clippy, documentation examples and
references, licenses, TypeScript and frontend dead-code analysis.
Full CI passed with `env -u NO_COLOR CARGO_TARGET_DIR=/home/moot/crucible/target just ci`:

- 9,112 workspace tests; 429 Oil feature tests and four Lua stub checks.
- 58 doctests, with the suite's explicit ignores retained.
- 2,492 frontend unit tests across 209 files.
- 137 browser tests and 17 live-server browser tests, with no retries needed.
- 77 gated process/documentation tests.

The first full workspace run passed 9,048 tests and failed 64 color/rendering
assertions with `NO_COLOR=1` inherited from the agent shell. All passed with
that variable unset for the test process. The earlier two-line ACP snapshot
adjustment was reverted too: the implementation changes no snapshots.

## Scope and evidence

The inventory covered all six crates and the shipped Luau runtime. Manual
tracing concentrated on ACP, watching, storage/retrieval, RPC forwarding,
Lua notifications, TUI state, and the browser import graph. This is not an
exhaustive correctness audit of every function, platform, or feature flag.

Physical source lines from `rg --files crates runtime`, restricted to Rust,
TypeScript/TSX, and Luau; includes comments, blank lines, tests, and examples.
Excludes snapshots, lockfiles, CSS, generated build output, and `vendor/`.

| Area | Files | Physical source lines |
|---|---:|---:|
| Daemon | 519 | 207,828 |
| Web server and browser | 627 | 123,196 |
| CLI/TUI | 303 | 83,510 |
| Core | 230 | 59,127 |
| Lua host | 140 | 55,955 |
| Shipped runtime | 85 | 21,835 |
| Oil | 43 | 13,924 |
| Total | 1,947 | 565,375 |

Do not rank files by these numbers alone: many large Rust files contain
substantial inline test modules. Moving those modules would change file
sizes without removing any maintenance burden.

Checks performed:

- `just refs orphans`: **0 findings**. Its deliberately narrow definition
  requires a type name to occur exactly once. Re-exports, impl blocks, and
  tests keep all the larger islands below out of this report.
- `just lint dead`: **failed with findings** — 19 unused exports, 23 unused
  exported types, four duplicate exports, and two dependency warnings.
  The dependency warnings are false positives; see R9.
- `scripts/inert-settings.py`: 27 candidates, mostly requiring wire or
  serialization analysis. These are not a deletion list.
- `just refs index`, then `just refs unread --crate crucible-daemon`: a fresh
  index reported 133 unread field-shaped symbols. The indexer completed but
  emitted duplicate-symbol/unnamed-definition diagnostics, so its output is
  supporting evidence only. Many hits are live serialized request fields or
  drop guards (`socket_lock`, background-job guards), which must stay. The
  first index attempt failed because its fixed output directory did not exist;
  creating `/tmp/scip` and rerunning succeeded.
- A temporary lexical census identified public functions with no other
  apparent production reference. Every recommendation below was separately
  checked against declarations, call sites, tests, and relevant entry points.
- Documentation checks: **4 passed** — all notes parse, required frontmatter,
  wikilink resolution, and code-reference validity. Run through `just test quick
  -p crucible-core --test dev_kiln --run-ignored ignored-only` with those four
  tests selected. `git diff --check` also passed.

The initial documentation-only review did not run application tests or
`just ci`; subsequent implementation validation is recorded above. Public
Rust API removals are verified against this repository's consumers, not
unknown downstream libraries. The reachability conclusions concern this
repository's product.

## Ranked work queue

| ID | Proposed action | Evidence / expected reduction | Risk |
|---|---|---|---|
| R1 | Delete the obsolete ACP tool facade | 805-line module; only test callers | Low for product; check public Rust API |
| R2 | Remove unreachable watcher alternatives and selection machinery | About 750–950 lines including selection tests | Medium; preserve native watch behavior |
| R3 | Delete unread watch metrics and simplify queue accounting | About 110–150 lines | Low; retain overflow semantics |
| R4 | Delete the retired session indexer | 283-line module plus two CLI tests | Low; keep retirement guidance |
| R5 | Remove error-only raw-query interface and test-only SQL DTO machinery | About 180–260 lines | Low; keep scoped storage coverage |
| R6 | Remove old ACP formatting facade and unused core ACP error | About 350–420 lines including tests | Low; preserve actual streaming types |
| R7 | Consolidate web RPC forwarding with explicit retry policy | Roughly 90 methods to classify; several hundred lines possible | Medium; correctness issue present |
| R8 | Share SQLite vector helpers and use existing embedding factory entry point | Tens of lines, fewer divergent implementations | Low for codecs; preserve failure policy |
| R9 | Triage frontend exports, then enable the existing dead-code gate | Mostly export cleanup; mic is a wiring bug | Mixed |
| R10 | Resolve the inert Lua message-panel API | Small removal or actual feature work | Public Lua contract decision |
| R11 | Delete the unconsumed TUI shell-history store | About 70–100 lines including tests | Low; preserve real input history |
| R12 | Remove the unproduced processing outcome | Small enum/constructor/match cleanup | Low within the workspace |

Ranges include tests and comments and are not independent promises. Keep
production-line and test-line deltas separate when implementing; deleting a
test is justified by the retired contract, never by the size target.

## R1. Delete the obsolete ACP tool facade

`crucible-daemon/src/acp/tools.rs:30` defines `ToolRegistry`, line 138
defines a hand-maintained ten-tool catalog, and line 306 defines
`AcpToolExecutor`. The file is 805 lines. The executor is constructed only
in that file's tests. The catalog's external caller is the test at
`crucible-daemon/tests/acp_integration_e2e.rs:30`.

Production ACP instead starts `InProcessMcpHost` in
`crucible-daemon/src/acp_handle.rs:200`; the host builds
`CrucibleMcpServer` in `mcp_host.rs:73`. The old facade is not an
implementation of the live `ToolExecutor` trait. It implements only a subset
of the note operations its catalog advertises, and its search branch at
`acp/tools.rs:515` always returns empty results.

Delete the module and its declaration in `acp/mod.rs`, along with tests
specific to this facade. Replace the catalog assertion in the integration
test with an assertion against the MCP server the ACP agent actually gets,
if existing MCP coverage does not already cover its useful intent. Keep the
rest of `acp_integration_e2e.rs`: it also tests live dispatcher/delegation code.

The old plan explicitly retained this catalog because integration tests use
it (`Consolidation Plan`, T1-B8 and the refuted-claims list). This review
revisits that rationale, rather than claiming the earlier search missed
those tests. Transfer any unique containment regression to the live note/MCP
path before deleting the old executor's tests.

Validation: all-target compilation; real MCP tool listing and execution;
ACP kiln-versus-workspace routing; note containment and protected-file tests.

## R2. Remove watcher alternatives unreachable through the manager

Both manager construction sites,
`crucible-daemon/src/watch/manager.rs:195` and `:240`, request
`WatcherRequirements::high_performance()`. That requires fine-grained events,
recursive watching, and at most 50 ms latency. The table in
`watch/backends/mod.rs:94` assigns polling a 1,000 ms floor and editor a
5,000 ms floor; polling also lacks fine-grained events, and editor lacks
recursive watching. Neither can be selected. No production caller supplies
different requirements or directly constructs these alternatives.

The alternatives are also unfinished: `polling_backend.rs:102` performs no
scan on a tick; only `watch()` scans once. `editor_backend.rs:107` logs a
placeholder tick and never emits changes. Their files total 501 lines.
`select.rs` adds 167, and much of the 281-line backend enum/table module exists
to select and forward among them.

Make the manager own `NotifyWatcher` directly. Delete the two alternatives,
their capability-selection types, and obsolete selection tests. The unread
`WatchConfig.backend_options` field (`watch/traits.rs:47`) can go with them.
Preserve any intended platform refusal explicitly; silently widening platform
support is not part of this reduction.

Validation: file create/modify/delete, recursive filtering, one-backend-per-
watch-group resource behavior, group removal, reindex delivery, and external
change capture suppression. Update the multi-backend claims in `watch/mod.rs`
and [[Meta/Product]].

Do not merge the two debounce stages in this batch. The capture-suppression
window explicitly depends on both at `watch/external_changes.rs:61`.

## R3. Stop computing watch statistics nobody observes

`crucible-daemon/src/watch/utils/monitor.rs` is an 89-line
`PerformanceMonitor`. Its only public operations construct it and record an
event. There is no report, getter, emission, or external consumer of the
computed totals/history/rate. Even the estimated memory usage is derived from
elapsed milliseconds, not an actual memory measurement.

`watch/manager.rs:60` stores it behind `Arc<Mutex<_>>`, threads it through
four processing functions, and locks it after dispatch at line 465. Delete
the monitor and this plumbing. These writes are executed, but their results
have no observable product use.

In `watch/utils/queue.rs:18`, dropped/processed counters are also never read.
The atomic `size` duplicates `VecDeque::len()` while every mutation already
requires `&mut self` behind the manager's mutex. Remove the unused counters
and use the deque's length. Preserve DropOldest, the zero-capacity error,
and the queue-capacity limit in this batch. Replacing the entire queue with
direct batch dispatch would be a separate behavior change.

Validation: overflow and zero-capacity behavior, delivery of every ready
debounced event, and unchanged file-event integration tests.

## R4. Finish retiring the session-to-note indexer

`crucible-daemon/src/observe/indexer.rs` is 283 lines. Its
`SessionContent`, extraction function, embedding-content helper, and
`to_note_record` have no production caller. External references are re-exports
and two tests in `crucible-cli/src/commands/session/tests/reindex.rs:33`.

The actual command at `commands/session/reindex.rs:16` prints retirement
guidance. [[Help/Core/Sessions]] already describes its retirement. The product
map explicitly records that the indexer's last production caller is gone.

Delete the indexer, re-exports, and the two tests exercising its old data
model. Keep the retired-command response and its coverage, and keep transcript
loading, rendering, and rebuilding. Update [[Systems]]'s current claim that
this adapter indexes sessions: it describes an implementation with no entry
point.

Validation: transcript replay/export and the retired CLI command. This should
require no migration and should not delete any stored transcript or note.

## R5. Remove the raw-query contract that only refuses calls

`crucible-core/src/traits/storage_client.rs:20` has one method,
`query_raw`. Its sole implementation at
`crucible-daemon/src/rpc_client/storage.rs:46` always returns an error.
The only call is the test asserting that refusal. There is no raw-query RPC
being removed. Delete this trait, impl, re-export, and obsolete test; retain
`DaemonStorageClient`, which has live `KnowledgeRepository` consumers.

Separately, `storage/sqlite/adapters.rs:115` contains a test-only generic SQL
query/row-to-JSON adapter. Only two tests in the same file use it. It keeps
`Record`, `RecordId`, and the core `QueryResult` in
`crucible-core/src/types/database.rs:22` alive, despite having no product
consumer. `StorageRecord`/`StorageRecordId` are unused re-export aliases.

Delete this test-only adapter and its two self-tests, then remove those three
DTOs and their re-exports. Preserve `DocumentId`, `SearchResult`, `BlockRef`,
and the unrelated CLI evaluation type also named `QueryResult`. Keep
`as_knowledge_repository_is_kiln_scoped`: it exercises a live security contract.

Validation: typed daemon storage RPCs, scoped repository tests, and CLI
search. This is deletion of obsolete Rust surface, not restoration of raw SQL.

## R6. Remove the old ACP formatter and unused error vocabulary

`crucible-daemon/src/acp/streaming.rs:157` defines `StreamConfig` and
`StreamHandler`, a formatting facade separate from the actual event renderer.
Every caller is a unit test or the formatting tests in
`tests/acp_integration/concurrent_sessions.rs:411`. Its `use_colors` branch
produces identical output on both sides; `normalize_chunk` returns its input.

Delete these types and their private formatting helpers, associated tests,
and re-exports. Keep `StreamingChunk`, `TurnSummary`, `StreamingCallback`,
`channel_callback`, and `humanize_tool_title`: these have production users.
Keep the real concurrency tests sharing the integration-test file.

The 90-line `crucible-core/src/traits/acp.rs` is another complete island:
`AcpError`/`AcpResult` are referenced only by their own declarations and tests.
Other occurrences of the spelling `AcpResult` are aliases of different error
types, including the live daemon client error. Delete the core module and
repair its mention in `crucible-daemon/src/acp/error.rs`.

Validation: actual ACP streaming/translation and both frontends' presentation
parity coverage; compilation of examples as well as tests.

## R7. Consolidate forwarding around an explicit replay decision

The web's `services/daemon*.rs` files contain roughly 90 public async methods.
For example, `services/daemon.rs:584` through the lifecycle forwarders repeat
the same ownership/cloning/boxed-future wrapper, changing the client method,
label, and return type. Existing helpers already own connection replacement,
event-router replacement, and sticky subscription restoration.

There is a concrete correctness concern to fix before mechanical compression:
`services/daemon_plugins.rs:94` and `:110` use `call_with_reconnect` for
`plugin.option_execute` and `plugin.run_command`. A plugin action can mutate
files or start work; replay after an ambiguous connection failure can repeat
that effect. `services/daemon.rs:684` also retries `session.send_message`.
The typed daemon-client methods for these calls use a single `typed_call`,
so the web layer adds this replay behavior. Review writes already use
`call_once` for exactly this reason (`services/daemon_review.rs:9`).

Use a small declarative forwarding table/macro for the repeated wrappers,
requiring an explicit read/replay-safe versus single-attempt policy per row.
Reuse the existing reconnect body. Keep methods with substantial argument
conversion explicit. The existing `RpcMethod` vocabulary is a useful source
for method names; do not build another independent string catalog.

There is also a timeout-retry loop in the shared daemon client at
`rpc_client/client/mod.rs:569`. Classify which layer retries each operation
before moving any behavior: consolidating wrappers must not multiply retries.

Validation needs a transport that records an applied mutation, loses its
response, and allows reconnection. Assert that a mutation is submitted once
and a replay-safe read can retry, while SSE and sticky subscriptions recover.
A mock returning an unrelated error string cannot establish this contract;
the existing review-forwarder doc records that exact testing trap.

This is a medium-size refactor with a useful invariant, not just a file split.
The reduction estimate remains provisional until a representative set of
wrappers has been converted and measured.

## R8. Share the small vector implementation; preserve retrieval semantics

`storage/sqlite/note_store.rs:56` and `storage/sqlite/block_store.rs:46` both
implement little-endian vector encoding, decoding, and blob cosine similarity.
Their computational bodies match. A private SQLite vector module can own
these helpers and their numerical edge-case tests. Keep the note and block
queries separate: their identities, filtering, and result materialization
differ.

The separate `EmbeddingResponse::cosine_similarity` at
`llm/embeddings/provider.rs:484` has only test callers. Delete it and its
specific tests instead of adding a third representation to the shared helper.
Keep the live `EmbeddingResponse` type used by Ollama.

`KilnManager::embedding_provider` already exists at `kiln_manager.rs:431`,
but `agent_manager/mod.rs:1184`, `agent_manager/messaging/send.rs:781`, and
`agent_manager/precognition/mod.rs:446` still fetch its config and invoke the
global factory separately. Route provider construction through the existing
method. Preserve the callers' different policies: no configured provider is
silent for precognition; creation failure there emits an event; dispatcher
setup can fall back; agent construction may propagate an error.

Validation: existing cosine tests including malformed/truncated blobs, note
and block retrieval order, cached embedding reuse, and the distinct missing-
provider/failure behaviors. Do not remove the block subsystem: it is live,
despite older architecture text saying otherwise.

## R9. Frontend dead exports include a missing provider, not just deletions

Two clear obsolete functions are
`web/src/lib/session-actions.ts:7` (`findTabBySessionId`) and `:16`
(`focusTabInPlace`). The live `openSessionInChat` uses `tabHost.find/activate`.
Remove those helpers and update the stale comment mentioning the old path.
The unused `api.getMode` can also be removed as a browser helper without
removing the daemon RPC contract.

Many other Knip findings mean **remove `export`**, not remove the implementation:
`fileTab` is called inside `file-actions.ts`; `sectionPage` and
`SettingsNavGroup` are used inside `MobileSettings.tsx`; the interaction
variant types compose the live `InteractionBody` union in `lib/types.ts`.
The four duplicate component exports can lose their unused default export.

`WhisperProvider` is never mounted: `App.tsx` has no provider import, and no
other production source imports it. But the composer renders `MicButton`,
which calls `useWhisperSafe` at `components/MicButton.tsx:20`. With no
provider, the fallback at `contexts/WhisperContext.tsx:234` resolves
transcription to an empty string. Recording can therefore complete without
producing text or reporting an error. The microphone tests mock this context.

Recommend wiring the provider and testing a real composer/provider pairing
with mocked audio/network boundaries. If voice input is deliberately retired,
remove the microphone/settings/dependencies as one product decision. Deleting
only the provider would preserve the misleading visible control.

The dependency warnings for `tailwindcss` and `@tailwindcss/typography` are
false positives: `src/index.css:9` imports the former and line 10 loads the
latter as a plugin. Knip's project excludes CSS. Correct that analysis gap or
document a narrowly justified exclusion; retain both dependencies.

Once findings are resolved, include `lint dead` in normal lint/CI. It is
currently deliberately excluded at `justfile:158`, so new dead imports and
exports can accumulate without failing the main gate.

## R10. The Lua message-panel actions have no production consumer

`crucible-lua/src/notify.rs:222` registers
`cru.log.messages.{toggle,show,hide,clear}`. All four write
`__crucible_messages_action__`. Its only reader,
`get_messages_action` at line 359, is `#[cfg(test)]`; there is no runtime or
frontend consumer of that global. The comment promising TUI delivery is false.

This is a public Lua surface documented in [[Help/Lua/Language Basics]], so
silently deleting registration is not a mechanical cleanup. Either retire
it explicitly, removing its declaration/tests/docs together, or send a real
client UI action through the daemon's existing UI surface. First decide what
the target client means when several are connected. Do not add another queue
with no consumer.

Do not delete `cru.log.notify` or `notify_once`: those now have a real
`NotificationSink` installed by the daemon. The older claim that all Lua
notifications are unconsumed no longer describes those functions.

## R11. Remove the shell-history collection that never supports recall

`crucible-cli/src/tui/oil/chat_app/popup_state.rs:52` stores 100 shell
commands and a recall cursor. `chat_app/shell.rs:27` reads the last command
only to avoid storing it twice, appends a command, and resets the cursor to
`None`. No production code reads the stored commands for display, navigation,
or completion; nothing reads the cursor at all. The two tests at
`chat_app/tests.rs:258` only verify collection insertion and eviction.

Delete `ShellHistoryState`, its constant, constructor/field/plumbing, append
helper, and those two tests. Preserve `ShellHistoryItem`, shell output capture,
and the input buffer's actual command recall. [[Meta/Product]] already records
that this separate shell-history store does not provide a user feature.

Validation: TUI shell submit/close/output insertion and real input-history
navigation, including `!` commands. Update the corresponding history story
and product entry if they still describe the removed storage implementation.

## R12. Delete the outcome no pipeline produces

`crucible-core/src/processing.rs:20` retains `NoChanges`, described
in terms of a Merkle tree. Its only constructor use is its own test. The two
production matches at `kiln_manager.rs:766` and `:944` treat it identically
to `Skipped`. The actual unchanged-file path at
`pipeline/note_pipeline.rs:509` returns `Skipped`.

Remove the variant, convenience constructor, dedicated test, and those match
arms. `ProcessingResult` has no serde derive here, so this is not removal of
a serialized outcome. Validate unchanged-note processing and reported counts.

## Suggested sequence and completion criteria

1. **Delete isolated remnants:** R1, R4, R5, R6, and R12 as small separate
   changes. Preserve useful regression intent on the actual product paths.
2. **Reduce unnecessary state:** R3 and R11. Measure production and test
   deltas independently and retain behavior coverage.
3. **Collapse the watcher alternatives:** R2, with native-watcher and capture
   regression coverage before changing ownership.
4. **Share proven implementation:** R8, then R7 with a discriminating retry
   test and explicit operation policy.
5. **Close public-surface decisions:** R9 and R10, with actual entry-point
   coverage and updated user documentation; enable the frontend dead-code gate.

For each implementation batch: trace the surviving entry point, compile all
targets, run focused behavioral tests plus the repository's required checks,
and run `just ci` before committing. A cross-process or cross-language change
needs a test crossing that boundary. Snapshot changes require visual review.

Track removed implementations, removed state, affected call sites, and net
production/test lines. Avoid a blanket LoC target: compression that hides
ownership or weakens a gate is not completion. Keep the permission/intercept
ordering, the four different wire bindings, parser-versus-resolver ownership,
the live block store, and migration readers intact unless separately reviewed.
