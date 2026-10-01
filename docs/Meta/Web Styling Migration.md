---
title: Web Styling Migration
description: Integration plan and acceptance criteria for the agreed shell styling.
tags: [meta, web, plan]
status: complete
---

# Web styling migration

The agreed shell design is integrated. Production components remain the
behavior owners. This is browser presentation work; the TUI and daemon retain
the shared session, transcript, review and file protocols.

## Sequence

1. Rebase onto local master (`d27152968`). Preserve the untracked handoff.
2. Prove and fix the expanded rail close/move/pop-out invariant.
3. Move shared palette and shell chrome into a production stylesheet, imported
   by the app. Use leaf tabs, right-rail cards, gaps and radii,
   without shadows or optional inner lines. Preserve restored layouts.
4. Extend Appearance in the existing settings owner. Keep the existing theme
   owner for dark/light. Add true black, contrast, navigation tint, RGB accent,
   gap, radius, note text size and file labels. Repaint graph and terminal.
5. Style the existing session list, file tree, transcript, composer, permissions,
   editor and review. Preserve their queries, commands and event subscriptions.
6. Run focused regressions, the quick suite, UI/story coverage and full CI.
   Inspect changed visual baselines individually. Record actual results here.

## Acceptance

- The full CodeMirror editor remains: source line numbers, live preview,
  cursor motion, syntax, undo, save/autosave and conflict/hash checks.
- Session history and streaming still render the daemon transcript, with one
  copy footer per completed assistant turn and working permission responses.
- Review keeps its opening triggers, stale/conflict/supersede handling and
  comment anchors. Its rows are numbered, monospace and borderless, with quiet
  +/- tints, bright inline changes and caret hunk controls.
- Appearance persists and applies without remounting editors, graphs or shells.
- Compact layout, keyboard focus, tab dragging, overflow and rail stowing work.
- The Look experiment is not a replacement for production settings. No mock
  session/note types, fixture decisions or mock datastore enter production.

## Initial migration validation

Rebased without conflicts onto local master `d27152968`. The original handoff
remains untracked. Production owns the shared palette, chrome and permission
rows. Appearance, navigation/conversation placement,
source-mode line numbers, review markers and file comments are integrated.

Expanded-rail close/move/pop-out and folded-pane ribbon regressions failed
before their fixes and pass afterward. The real editor/transcript browser case
proves that appearance changes preserve the mounted editor, unsaved text, undo
history and transcript, and that settings survive reload. Saved-layout tests
cover the former default, swapped rails and idempotent restoration.

Validation of the initial styling pass (not proof of full control integration):

- `just test quick`: 9,779 Rust tests passed.
- `flock /tmp/crucible-ci.lock just ci`: lint, Rust, 68 gated tests, feature/doc
  checks, frontend coverage and UI stages passed. The live stage exposed the
  folded-terminal bug and a wrapped-line test-input assumption. After fixing
  them, all affected frontend gates were rerun against the final code:
  `just web-test coverage` (3,616 tests), all 177 UI/story tests, and
  `just web-test live` (81 passed, 30 skipped).
- Changed permission-card and editor-properties baselines were inspected
  individually. Transcript captures were corrected to fit their containing
  rail; no clipped images were accepted.
- The required docs-kiln check passes all 19 tests.

The live suite exercises real daemon turns, transcript replay, shared streams,
editor save/autosave and conflict resolution on desktop and compact layouts,
terminal sockets, and the built application's security headers.


## Independent visual comparison and interactive review

The comparison oracle is the pre-migration mockup at immutable commit
`94df484751a69a46aea55b237445f94ac023a027`, served from a separate worktree.
The temporary current-tree mockup has been retired. The reference remains
separate from production styling and serves only as an independent oracle. The reference server uses its
own Vite dependency cache and permits its font assets; the test verifies that
Geist Mono loaded successfully before accepting the comparison.

Run the comparison from `crates/crucible-web/web` with the current application
on port 5391 and the original mockup on port 5393:

```sh
CRUCIBLE_WEB_PORT=5391 CRUCIBLE_MOCKUP_REFERENCE_URL=http://127.0.0.1:5393 bunx playwright test e2e/mockup-parity.spec.ts --project=ui --reporter=line
```

The reference URL is required to enable these two tests. They capture both
pages at 1600 × 960 with the same dark/light appearance, the same minimal
transcript, and the same TypeScript edit. The real production components run
with mocked API data for this visual comparison. No screenshot masks hide
style differences. Paired images, difference images, overlays and measurements
are written to `/tmp/crucible-visual-parity`; each theme has a generated HTML
report.

The gate allows fewer than 0.5% differing pixels in each matched region,
counting a pixel when its largest RGBA channel difference exceeds 16:

| Matched region | Dark | Light |
| --- | ---: | ---: |
| Numbered diff rows | 0% | 0% |
| User and assistant transcript | 0% | 0% |
| Completed-turn copy/regenerate controls | 0% | 0% |

The comparison exposed and corrected diff gutter/header geometry, composer
sizing, user-bubble width, transcript spacing, action sizing and bold text
weight. These percentages establish parity for the measured regions, not for
the whole application. The full conversation rail differs by 1.807% in dark
and 1.824% in light, including different real header/footer controls. Full
permission cards retain production request labels, scope controls and decision
layout, including an amber Allow action instead of the mockup's blue action.
Those differences are shown in the report. Full-page captures have different
navigation/review fixtures and are diagnostic images without a parity score.
The complete working editor remains the explicitly agreed exception to the
mock note renderer. The mode control is at the far left below the composer,
as requested during the spot check.

A separate agent also drove the running application against a disposable real
daemon and fake model provider. It created and sent a session turn, reloaded
and reopened its transcript, edited a scratch note, undid edits, manually saved
and autosaved to disk, changed and persisted appearance while retaining
buffers, expanded and restored the session, and ran a terminal echo command
through localhost. At a 390px viewport it exercised drawers, transcript
viewing, note editing/saving and the reading toggle. No uncaught JavaScript
errors were observed. Permission and review queues were not populated in this
manual walkthrough; their behavior remains covered by the automated suites.
The terminal retains the existing localhost access restriction.

Follow-up validation: all 3,618 frontend coverage tests and both independent
visual comparison tests passed. The 178-test UI/story rerun reached 176 passes
and two visual baseline differences. The changed images were inspected
individually and accepted; all six affected stories then passed. After the
requested mode-control ordering change, all 46 targeted chip/ChatInput unit
tests passed. Type checking and the diff whitespace check passed. The previous
live/served result remains 81 passed and 30 skipped; the walkthrough adds direct
interaction with the running application. The final documentation change
passes all 19 required docs-kiln tests.

Live-preview line numbers were removed after visual review; source mode retains
them. Paragraph joining and the default 768px readable column remain intact.

The note toolbar now owns the desktop Live/Source/Reading switch; the floating
editor buttons are removed. The production switch drives the existing editor mode owner.

## Control integration audit

The initial completion report was premature: appearance comparison and preserved
editor/transcript behavior did not prove that every intended mockup control was
present in production. The follow-up audit checks controls individually,
including buttons that were inert fixtures in the mockup. Experimental Look
variants, plugin stylesheet delivery and experimental hover variants remain the
explicitly agreed omissions; existing review opening triggers remain supported.

The audit found missing note view/history controls, queue actions, recalled-note
links, Files toolbar actions, rail actions and session header controls. Completion
requires production-owner wiring and interaction evidence for these controls;
a screenshot match alone is insufficient.

The audit is grouped by the mockup's component families:

| Surface | Intended controls | Production owner / verification |
|---|---|---|
| Note | Live / Source / Reading | `EditorWithPreview` and shared `NoteViewSwitch`; mode transitions, Vim editing and paragraph reflow browser stories |
| Note | Back / Forward; plain, new-tab and split navigation | Canonical file actions and per-tab history; document-history browser test covers dirty buffers, undo, save base hashes, Back/Forward and new tabs |
| Note | Properties fold, tasks, links, callouts and tables | Existing live-preview decorations and Markdown preview; editor stories |
| Files | Root picker, manage roots, new note/folder, sort, collapse, labels, refresh, reveal | Files query/mutation and display owners; toolbar browser test, shared lazy filesystem tree for project and kiln roots |
| Sessions list | Open, group fold, older/archived sessions | Existing unbounded session roster and group folds; session and windowing browser suites |
| Rails | Search, new session, theme, settings, inbox, centre swap, expansion exit | Shell/window actions; rail affordances, centre swap and expansion-exit setting wired and covered by windowing tests |
| Session | Expand, menu, edit/copy/regenerate, tool details and file links | Chat/session and tool-card owners; header menu and canonical tool-file links exercised in chat stories |
| Composer | Draft, send/stop, dictate, mode/model/scope, comment attachments | Existing ChatInput, speech, settings and session knob owners |
| Queue | Send now and remove | Shared transcript queue and turn cancellation; chat stories exercise remove and send-now without duplicate delivery |
| Recalled notes | Open each note | Session-kiln note resolution; recalled-note browser story opens the real editor |
| Permissions / inbox | Allow once/session/project/user, deny, open session, review | Existing interaction response and proposal owners; no fixture decisions |
| Review | Fold file/hunk, open file, per-file/all decisions, comments/cancel/attach/resolve | Existing DiffPanel and review query owners; separate filename open added and tested |
| Window chrome | Tabs, overflow, move/split/float, pin, close, resize, fold/expand | Existing windowing core and regression suites |
| Terminal | Interactive shell | Existing terminal socket and emulator; live suite and independent echo walkthrough |


## Final control validation — 2026-10-01

The control audit above is integrated. The final validation stages passed:

| Gate | Result |
|---|---|
| Rust CI suite | 9,780 passed |
| Gated Rust suite | 68 passed |
| Formatting, clippy, contracts/types, unused code, feature/doc checks | Passed |
| Frontend coverage | 3,646 passed across 324 files; coverage thresholds passed |
| UI and user-story browser suite | 187 passed; the two independent comparison cases require their separate reference server |
| Independent frozen-mockup comparison | 2 passed, dark and light; all three matched regions have zero pixel differences |
| Real daemon and served bundle | 82 passed, 30 explicitly skipped, with retries disabled |

The serialized `just ci` run is recorded in
`/tmp/crucible-controls-final-ci3.log`. Its final served-app stage still targeted
the removed preview button; after changing that test to use the real toolbar's
Reading view control, the entire live/served tier was rebuilt and rerun using
`just web-test live --retries=0`. That clean result is in
`/tmp/crucible-controls-live-clean.log`. Earlier partial/failed runs are not
counted as successful final runs. The live skips are declared contract-sweep
omissions and the installed-plugin prerequisite; they are not passes.

Validation found and fixed an actual Files integration defect: each pane
invalidated a directory already refreshed by the shared filesystem route.
Two panes caused three browser reads for one event. The shared route now owns
invalidation; panes rebuild expanded children through its cache. A failing
regression was observed before the fix, and both top-level and expanded-child
regressions pass afterward. The live test proves one read updates both panes.
Empty-folder creation and Refresh of non-note assets also pass against the
real daemon and survive reload. The kiln watcher still watches indexable
formats; external assets outside that filter require Refresh.

A diff-comment browser test also read gutter coordinates before CodeMirror's
layout settled (8 failures in 20 reproductions). The locator now waits for the
actual line number before pressing/releasing; all 20 verification runs passed.
The full browser suite passed afterward. These fixes did not relax screenshot
thresholds or replace interaction assertions with fixtures.

The image report is served at `http://192.168.0.16:5394/`. Matched diff rows,
transcript and turn actions are identical; the whole conversation rail has
1.807% / 1.824% differing pixels in dark/light. The whole application is not
claimed pixel-identical: the real editor, production permission content and
other differences described above remain visible in the report.

The spot-check preview uses a separate disposable daemon/kiln and fake model
provider at `http://192.168.0.16:5391/`; it does not use the developer's daemon.
Its launcher is `/tmp/crucible-final-preview.ts`, with process details in the
private `/tmp/crucible-final-preview-state.json` and startup output in
`/tmp/crucible-final-preview.log`. Preview and image artifacts are temporary
local services, not a deployment. No integration commit was created.


The final LAN walkthrough signed in through the real authentication prompt,
created a session and received its transcript, switched Source/Reading/Live,
verified source-only line numbers, edited in Vim, saved to disk, and reopened
the editor and transcript after reload. It waited for the normal debounced
layout save before reloading. No uncaught page errors occurred. Four current
Vim captures are under `/tmp/crucible-visual-parity/vim/` (heading, bold,
inline code and table). The preview's disposable sign-in key is in
`/tmp/crucible-preview-key.txt`; localhost access needs no key. The recorded
walkthrough is `/tmp/crucible-preview-check.log`.


## Mobile styling follow-up

The compact shell now shares the desktop stylesheet and appearance tokens.
Its working surface is a MAIN card on NAV ground, drawers use leaf tabs, and
bottom sheets/settings cards use the same radius and surface colors without
shadows. Phone edge gaps cap the shared gap at 8px. Navigation and the title
stay in the app bar; Read/Write/Save occupy a separate note toolbar so a dirty
file remains usable at 320px. Shared transcript/composer actions read a single
size token (22px desktop, 44px compact), with phone-sized send/mic and session
menu targets. The composer input is 16px to avoid iOS focus zoom.

These are presentation changes in the existing mobile components and
`shell-theme.css`; desktop/mobile retain their separate layout owners and share
editor, transcript, settings and daemon behavior. No second theme/settings
store was introduced. Regression tests first reproduced the squeezed note
title at both 320px and 390px. The added browser cases exercise dirty-buffer
retention and touch geometry, and capture dark/light phone surfaces.


Mobile follow-up validation: `just test quick` passed 9,780 Rust tests;
193 focused frontend tests and type checking passed; the complete UI/story
suite passed 190 tests, with the two separate reference comparisons also
passing. All five live-compact/served tests passed with retries disabled.
The added short-viewport composer geometry check passed with the three mobile
appearance tests. A phone-emulated LAN walkthrough of the rebuilt application
verified sign-in, real transcript viewing, Read/Write, shared Appearance
changes without remounting the editor, drawers and the tab overview. No
uncaught page errors occurred. This is Chromium phone emulation, not a claim
of physical iOS/Android keyboard testing.

Current screenshots: `http://192.168.0.16:5394/mobile/`. The preview remains at
port 5391 with the same disposable notes, sessions and sign-in key. Mobile
styling and layout rules live in the shared production stylesheet; desktop's
independent mockup parity remains unchanged.

### Navigation styling correction

The first mobile styling pass missed legacy borders and small/square corners
inside the navigation controls. Removed the root trigger, picker and menu borders,
replaced ruled menu groups with spacing, and applied shared control/card radii
to Files/Sessions rows, toolbar controls and picker options. Open popups use a
quiet tonal step. `flat-navigation.spec.ts` reproduced the border bug at 390px
and 1440px before the fix; it checks open picker, hovered rows, context menus
and session rows, with dark/light screenshots.

Visual inspection also caught context menus behind the phone drawer. The shared
menu layer now sits above it; the browser gate requires a menu item to receive
a pointer hover. Focused validation: 47 unit tests, phone/editor/root-picker
browser checks, two independent mockup comparisons, source typecheck, production
build and a real LAN phone-viewport walkthrough passed.

The drawer leaf tabs now use curved concave joins into the content surface.
The exposed top corner opposite the selected tab is rounded; both Sessions
and Files states are covered by a browser regression and light/dark captures.

### Desktop spot-check fixes

Live-preview tables normalize rendered HTML whitespace, removing the trailing
newline that added a blank line below the table; document blank lines remain
editable. The shared root-picker wrapper can shrink so long names truncate
before toolbar actions. App rail chrome replaces the core fallback tail, leaving
one centre/right swap button and keeping left navigation in place. Browser
regressions measure table height, picker/action separation and a swap round trip.

### Interaction follow-up

Added vertical rail insertion feedback through the tab-bar reorder owner.
Removed compact transcript size/visibility overrides; shared turn components
reveal actions by hover/focus or touch tap. Extended the drawer gesture owner
to accept main-content swipes with direction locking and vertical-scroll
cancellation. Centered rail resize lines and hit targets in the pane gutter
(the right line measured 4px off-center before correction). Validation: 29
browser tests covering touch gestures, transcript reveal, rail/tab reordering
and resize geometry; 244 focused unit tests passed.

Session selection reuses a conversation tab group even when the user groups
Backlinks, Activity or other supporting tabs with it. It activates the selected
conversation without adding a split, preferring an existing conversation over
an empty group. Navigation groups containing Sessions or Files remain excluded.
Selecting either an existing or new session unfolds its conversation pane;
activation owns revealing the tab so focus cannot land in hidden content.

## Retiring the design fixture

The production migration removes `shell-mockup.html` and the entire
`src/test-harness/shell-mockup/` implementation, including its experimental
Look toolbox, fixture state and duplicate views. Fixture-only browser tests
are removed with it; production editor, review, transcript and windowing tests
continue to exercise their existing owners. `e2e/permission-layout.spec.ts`
retains the viewport regression against a real permission card populated by a
mocked daemon interaction event. The shared scroll-fade tests move
to `src/lib/__tests__/scroll-fade.test.tsx`.

`e2e/mockup-parity.spec.ts` remains an optional independent comparison against
the immutable pre-migration worktree above. No runtime reference implementation
is copied into the production tree. Editor, review and windowing harnesses
remain because they mount production components.

Production appearance choices remain in Settings → Appearance, persisted by
SettingsContext: theme, true black, contrast, navigation tint, accent, pane gap,
corner radius, note text size, file labels and fonts. Shared range and boolean
controls render these preferences for both shells. The shell stylesheet imports
small palette, window chrome, content and compact modules; there is one theme
owner rather than separate desktop and mobile palettes.

## Final Sol review and commit validation

Three Sol reviewers covered correctness, component boundaries and removal of the
design fixture. Their verified folded-conversation finding is fixed through the
shared tab activation owner, with regressions for existing and new sessions.
FilesToolbar, ChipOptionRow and shared settings controls reduce duplicate view
markup without introducing another state owner. The final read-only follow-up
found no further verified regression.

Full `flock -o /tmp/crucible-ci.lock just ci` passed: lint, 9,780 Rust tests,
68 gated tests, feature/doc checks, 3,632 frontend tests with coverage,
187 browser/story checks and 82 live/served checks. The suites skipped
124 Rust cases, two optional visual-reference cases and 30 live cases.
Both optional dark/light visual-reference cases passed separately against the
frozen mockup. The docs-kiln gate passed all 19 tests. Obsolete menu assertions
requiring borders and shadows were retired; the real-browser flat-navigation
checks cover the requested appearance.
