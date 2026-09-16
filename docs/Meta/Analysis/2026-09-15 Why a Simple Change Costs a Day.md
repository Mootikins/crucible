---
title: Why a Simple Change Costs a Day — 2026-09-15
description: A retrospective on one day of web UI fixes, why each defect had the same shape, why no test lane caught a 422, and five changes to make a data-shaped change cheap
tags: [meta, retrospective, testing, architecture]
status: draft
updated: 2026-09-15
---

# Why a Simple Change Costs a Day — 2026-09-15

Status at filing: cause 2's daemon rule landed in `714d68423` (a stored session answers `session.get`, `list_modes` and `list_models`, and the client revive path is gone). Change 1 (a session in the live tier) and change 2 (one rule for listed and attachable kiln names) are in progress. Related: [[2026-09-15 t3code Design Reference]], [[Product Decision Log]], [[Web User Stories]].

Date: 2026-09-15. Repository: /home/moot/crucible. Read-only analysis.

The owner asked two questions. Why does a change that looks orthogonal and
simple take so long? Why do the tests not catch a 422? The two questions have
one answer. A rule lives in more than one layer, and no test crosses a layer.

## (a) The causes, in the order of the time they cost

### 1. One rule, many layers, no owner (largest cost)

Every defect today has the same shape. A layer states a rule. Another layer
states a different rule for the same thing. Neither layer reads the other.

- `crates/crucible-daemon/src/server/kiln.rs:137` derives a kiln name from the
  directory basename when the registry knows no name.
  `crates/crucible-daemon/src/server/session/scope.rs:181` refuses any name the
  registry does not know. So `kiln.list` publishes a name that
  `session.connect_kiln` refuses. That is the owner's 422.
- `crates/crucible-daemon/src/server/fs/mod.rs` admitted registered projects
  only. `session_manager.rs` `create_session` puts a project-less session in a
  scratch folder that no registry lists. The file tree of every such session
  got 422.
- `crates/crucible-web/src/routes/session/mod.rs:598` `get_session` answers for
  a live session. `get_session_history` at line 613 revives a stored one. So a
  reload of a stored session failed, and a history read fixed it.
- `runtime/plugins/oci/init.luau` knew the default runtime. No API published
  it, so the chip read "Project default".

The cost is not the fix. The cost is the search. An agent must find every
layer that states the rule before it can change one of them.

### 2. The client patches a daemon rule, and the patch does not spread

`crates/crucible-web/web/src/contexts/SessionContext.tsx` held a revive-on-
failure workaround inline in `selectSession`. `adoptSession` at line 255 did
not hold it. The same daemon rule, patched at one call site of two. Today's
fix extracts `loadSession` and calls it from both. The daemon rule stays
wrong: three red tests wait in
`crates/crucible-daemon/src/server/session/modes.rs:163`
(`stored_session_tests`), with no implementation yet.

A workaround in a client multiplies by the number of call sites. A rule in the
daemon does not.

### 3. Every test lane mocks the layer below it

- 225 unit test files under `crates/crucible-web/web/src`. 51 of them call
  `vi.mock('@/lib/api')`.
- 28 Playwright specs under `crates/crucible-web/web/e2e`. 40 files use
  `setupBasicMocks`, which answers every route with a fixture.
- `crates/crucible-web/tests/route_contract_tests` runs the real axum router
  against a mock daemon.
- The daemon tests run real handlers against per-test fixtures.

Only `crates/crucible-web/web/e2e/live` runs a real daemon over real storage.
It holds 16 tests in 4 files, and 12 of them are one kiln-truth suite. It
never creates a session.

`crates/crucible-web/web/src/components/__tests__/SessionScopeChips.test.tsx:35`
shows the failure exactly. It mocks `listKilns` to return `main` and `extra`,
two names the registry would know. The mock states the assumption that the
defect breaks. The test can never fail on it.

### 4. Comments argue for a choice, so an agent defends the choice

Comment density is 21.6% in the web sources (11347 of 52636 lines) and 18.3%
in the daemon (31091 of 170258). Many comments are not a description. They are
an argument.

- `crates/crucible-daemon/src/rpc/missing_session_contract.rs (the module comment)` argues for
  eight different answers to one question, and tells the reader not to change
  them.
- `crates/crucible-web/web/src/components/SessionTree.tsx (the row comment)` argues for the
  indent rule of one row.
- `SessionScopeChips.tsx` argued for chips in the composer control row.

An agent that must change the choice reads the argument first. It then spends
its effort on a justification for the change. A factual comment costs nothing
to change. An argument costs a paragraph.

### 5. A test pins the design, so a design change becomes a test rewrite

Commit 8539dbff8 (sessions rail) changed 711 lines. About 386 of them are
tests: `SessionTree.test.tsx` 151, `SessionsPanel.test.tsx` 118,
`session-inbox.test.ts` 55, `SessionsTab.test.tsx` 31, e2e 31. The behaviour
is about 322 lines. More than half the work went to tests that named the old
design.

Commit c3519fac8 removed 239 lines and added 145 across 16 files, in the
daemon, the core and the web, to fix one session field at creation.

Commit f947e01aa touched 9 files to delete one draft field.

### 6. Structure sits in the markup, not in data

Before today, `CenterComposer.tsx` and `ChatInput.tsx` each placed their chips
by hand, in different places. The draft drew chips above the field. The
session drew them below. `composer/ChipRow.tsx` now builds both rows from one
list, and `SessionScopeChips.tsx` became a hook that returns data.

The same hand-placed shape stays elsewhere:

- `components/settings/sections.tsx` imports 12 section components by name and
  builds one list by hand.
- `lib/register-panels.tsx` calls `registry.register(...)` 19 times in a row.
- `SessionTree.tsx` places each row part in the markup.
- `SessionStatusChips.tsx` places each chip in the markup.

A hand-placed surface has no single place to change. So a consistency change
must visit every surface, and an agent must find them all first.

### 7. The daemon's message did not reach the user

`crates/crucible-web/src/error.rs` passed `RPC error: {"code":-32602,
"message":"…"}` to the browser as the body. The daemon wrote a plain sentence.
The user saw a status and a JSON blob. So every defect above presented as
"422", and the owner had to read the source to learn the cause. Today's fix
adds `rpc_error_parts` and unwraps the envelope.

### 8. The brief started from a symptom

The lead wrote "the composer still isn't pill shaped" to parallel agents. The
agents built a full wrong answer: they moved the chips inside the capsule. The
comment in `composer/ComposerCard.tsx` records the result: "Chips inside the
field made the field look like a toolbar." A screenshot after the first
element would have stopped that build. The second attempt was correct.

## (b) The tests question

### Which lane would have caught each 422

| Defect | Lane that could catch it | Why it did not |
|---|---|---|
| `connect_kiln` refuses a `kiln.list` name | live | The live lane creates no session and attaches no kiln. The daemon test at `kiln.rs:1736` asserts the basename fallback and passes. No test sends that name onward. |
| `fs.list_dir` refuses a session folder | live | The live lane creates no session, so no scratch folder exists. |
| `session.get` refuses a stored session | live | The live lane creates no session and never restarts the daemon. |
| `list_modes` / `list_models` refuse a stored session | daemon unit | The fixtures always hold a live session. The red tests exist now; the code does not. |
| The body carries the RPC envelope | route contract | The mock daemon returned what the test wrote, so no test wrote a real envelope. |

The vitest and Playwright lanes cannot catch any of them. Both mock the API,
so a 422 only exists when an author writes one.

### The lane that would catch them

The chassis already exists. `crates/crucible-web/web/e2e/live/global-setup.ts`
boots a real `cru web` with its own daemon, an isolated socket, scrubbed
provider credentials and a TempDir kiln. It runs in `just ci`
(`justfile:340`). It does not cover sessions.

Extend the same setup:

1. Register a kiln whose `[kilns]` name differs from its directory basename.
   Example: the directory `My Vault`, the name `notes`.
2. Open a second directory with `kiln.open` and no registration. That is the
   directory `kiln.list` names by basename.
3. Create one session with no project, so the daemon makes a scratch folder.
4. Write one session to storage, then stop and start the daemon, so the
   session is in storage only.

Then add a `live-session` project to `playwright.live.config.ts` with about
ten specs: change the kiln from the chip, open the file tree of the
project-less session, reload the page onto the stored session, read the model
list and the mode list of the stored session, and read the text of one refusal
body.

Cost: the live tier already builds `cru` and the web bundle, which dominates
its runtime. The extra specs add about 30 to 60 seconds. The setup adds one
daemon restart, about 5 seconds. It goes where `web-test live` already sits,
at the end of the `ci` recipe.

## (c) Five changes, in priority order

**1. Give the live tier a session.** The single largest gap is that no test
creates a session against a real daemon. Add the four setup steps above to
`e2e/live/global-setup.ts` and publish the ids in `.live-state.json`. Then
every cross-layer rule about a session has one place that can fail. This
change alone would have caught four of today's eight defects, and it needs no
new infrastructure.

**2. Make a name that a lister publishes a name that a taker accepts.** Today
`kiln.list` invents a basename and `connect_kiln` refuses it. Pick one rule.
Either registration happens when a kiln opens, so every listed kiln has a
registry name, or `kiln.list` marks an unregistered entry `connectable:
false` and the picker shows it as inert. Write the rule once in
`kiln_registry.rs` and let both handlers call it. Add a daemon test that feeds
every `kiln.list` name to `connect_kiln`.

**3. Move each client workaround into the daemon.** `session.get`,
`session.list_modes` and `session.list_models` must read storage, as
`session_resume_from_storage` already does. The red tests in
`server/session/modes.rs:163` state the rule. When the daemon holds it, delete
`loadSession`'s revive path in `SessionContext.tsx`. A rule in the daemon
serves the TUI, the web, ACP and MCP at once. A rule in a client serves one
call site.

**4. Build the remaining hand-placed surfaces from data.** `ChipRow.tsx` now
proves the pattern: a list of records, one renderer, one `Switch` over a
`render` field. Apply the same shape to `components/settings/sections.tsx`,
`lib/register-panels.tsx`, `SessionStatusChips.tsx` and the `SessionTree` row.
Each becomes one array and one component. A consistency change then edits one
array, and a test asserts on the array rather than on the markup of four
surfaces.

**5. Separate the description from the argument in comments, and in tests.**
A comment above a function must say what the function does and what it
refuses. The reason for the choice belongs in
`docs/Meta/Product Decision Log.md`, with a one-line reference from the code.
Apply the same rule to tests: a test asserts a behaviour the product promises,
not the design that delivers it. Commit 8539dbff8 spent more than half its
lines on tests that named the old rail design. A test named for the promise
survives a redesign.

## (d) What the lead must put in a brief

1. **The target, not the symptom.** Give the shape as data or as a sketch.
   Write the chip list, or attach an image. Never write "it still isn't pill
   shaped".
2. **One probe before the full build.** Tell the agent to build one element,
   take one screenshot, and report. Then let it build the rest.
3. **The layer that owns the rule.** Name the file that must hold the rule,
   and name the layers that must only read it.
4. **The lane that must go red first.** Name the test file and the lane. Tell
   the agent to run it, to see the failure, and to report the failure text
   before it writes the fix.
5. **Every surface that shares the shape.** List them. An agent that must find
   them spends its budget on the search.
6. **The non-goals.** Name what must not change. Today's first composer
   attempt moved the chips inside the capsule because nothing forbade it.
7. **The evidence to return.** Ask for a screenshot, a diff stat, and the test
   output. Do not accept a claim of success without them.
