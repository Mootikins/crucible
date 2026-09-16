---
title: Web Composition — 2026-09-16
description: An inventory of the repeated shapes in crucible-web, the branch count in its ten deepest components, why the tree grew this way, and a parts library with lanes to build it
tags: [meta, ux, web, architecture]
status: draft
updated: 2026-09-16
---

# Web Composition — 2026-09-16

Read-only pass over `crates/crucible-web/web/src` on 2026-09-16. No source changed. Design
values come from sections 3 and 4 of [[2026-09-15 t3code Design Foundations]]; the cost of a
sweep comes from [[2026-09-15 Why a Simple Change Costs a Day]].

The tree holds 141 non-test `.tsx` files and 133 exported components. Five shared parts exist,
and nine files import one. The rest write the class string again.

## 1. The repeated shapes

One component serves every site of each shape below. Section 4 gives the props.

**1.1 The hover icon button — 12 sites. Part: `IconButton`.**
`crates/crucible-web/web/src/components/ui/IconButton.tsx:26` holds a 24px and a 28px box with
`hit-32` and `focus-ring`. One file imports it,
`crates/crucible-web/web/src/components/graph/GraphPanel.tsx`. Six sites write `rounded p-1
text-muted-dark hover:text-shell-ink hover:bg-hover-wash transition-colors` by hand:
`Message.tsx:181`, `Message.tsx:192`, `SessionTree.tsx:112`, `AssistantTurn.tsx:308`,
`AssistantTurn.tsx:319`, `ChangesPanel.tsx:387`. Four drift from it:
`NotificationCenter.tsx:261` orders `rounded` last, `BacklinksPanel.tsx:196` hovers to
`text-shell-body`, and `ChangesPanel.tsx:189` and `ChangesPanel.tsx:203` hover to `text-ok`
and `text-error` with `hit()` appended. `hover:bg-hover-wash` appears on 139 lines across 62
files.

**1.2 The hover-revealed column — 6 sites. Part: `Reveal`.** `TurnGutter.tsx:25` keeps the
box, changes opacity, and drops pointer events while hidden. (The transcript's right-hand
gutter became the per-turn meta row `TurnMeta.tsx` later the same day; the opacity rule and the
pointer-event rule moved with it, and the count of copies did not change.) `SessionTree.tsx:106` repeats it
without the pointer-event rule. `InboxPanel.tsx:190`, `windowing/TabBar.tsx:148`,
`canvas/CanvasCardChrome.tsx:209` and `canvas/CanvasNodeView.tsx:222` write four more copies.

**1.3 The section header with a count — 14 sites. Part: `SectionHead`.** Four declarations of
one label already exist: `components/ui/SectionLabel.tsx:6`,
`components/tree/tree-style.ts:11`, `components/settings/primitives.tsx:43` and
`components/PanelHeader.tsx:16`. They disagree on three axes. The size is `text-floor` in nine
files and `text-xs` in five. The tracking is `tracking-wide`, `tracking-wider`,
`tracking-widest` or `tracking-[0.08em]`. The padding differs at every site. Ten more copies:
`SearchPanel.tsx:503`, `BacklinksPanel.tsx:233`, `SkillsPanel.tsx:143`,
`ChangesPanel.tsx:559`, `settings/SettingsModal.tsx:132`,
`settings/AppConfigSettings.tsx:305`, `settings/MobileSettings.tsx:57`,
`PluginSettings.tsx:210`, `blocks/GraphBlock.tsx:122`, `blocks/KanbanBlock.tsx:132`.

**1.4 The collapsible fold — 9 sites, 5 carets. Part: `Fold`.**
`components/tree/TreeSection.tsx:29` owns the pattern and two files use it,
`SessionsPanel.tsx:174` and `SessionTree.tsx:548`. `InboxPanel.tsx:281` draws its caret as the
text `▾` and `▸`; `ActivityPanel.tsx:101` uses `▼` and `▶`; `ToolCard.tsx:293`,
`SubagentCard.tsx:79` and `DelegationCard.tsx:80` rotate a `ChevronRight`;
`SessionTree.tsx:495` uses `treeChevron`; `ChangesPanel.tsx:140` uses a fifth. The `+N` button
in `composer/ChipRow.tsx` is the same state with another trigger.

**1.5 The row with a leading icon, a title and trailing meta — 12 sites. Part: `ListRow`.**
`components/tree/tree-style.ts:30` already solves this: `treeRow` carries no metrics and reads
`data-density` from `styles/refine-touch.css`. Three files import it. The rest write `px-3
py-1.5`, `px-3 py-2`, `px-2 py-1.5` or `px-2.5 py-1.5`: `SearchPanel.tsx:132`,
`SearchPanel.tsx:406`, `BacklinksPanel.tsx:260`, `BacklinksPanel.tsx:310`,
`ChangesPanel.tsx:575`, `SessionTree.tsx:59`, `SkillsPanel.tsx:150`,
`AutocompletePopup.tsx:107`, `canvas/NotePicker.tsx:132`, `composer/ChipSelect.tsx:576`,
`interactions/PopupInteraction.tsx:54`, `settings/MobileSettings.tsx:34`.

**1.6 The popover panel — 9 sites. Part: `PopoverPanel`.** `components/ui/menu-style.ts:22`
states the rule: a `hairline-strong` edge under `shadow-md`, a ratio of 8 to 1. Six files
import it. Nine draw a floating panel with a weaker edge and a bigger shadow:
`AutocompletePopup.tsx:97` (`border-hairline` under `shadow-xl`), `canvas/NotePicker.tsx:106`,
`canvas/LinkPrompt.tsx:83`, `canvas/CanvasPanel.tsx:1139`, `graph/GraphControls.tsx:59`,
`canvas/CanvasCardChrome.tsx:89`, `CommandPalette.tsx:163`, `composer/ChipSelect.tsx:472`,
`blocks/PluginCommandDialog.tsx:163`.

**1.7 The chip — 9 sites, 4 radii, 3 paddings. Part: `Chip`.** `SessionStatusChips.tsx:100`
writes `px-2 py-0.5 rounded-md border text-floor`. `ToolCard.tsx:308`, `ToolCard.tsx:320`,
`ToolCard.tsx:329`, `PluginPanel.tsx:251` and `PluginPanel.tsx:256` write `px-1.5 py-0.5
rounded` with `uppercase tracking-wider`. `SessionTree.tsx:91` and
`mobile/SessionsTab.tsx:100` share a byte-identical branch chip. `NotificationCenter.tsx:243`,
`canvas/CanvasNodeView.tsx:225` and `SearchPanel.tsx:323` use `rounded-full`.
`SessionStatusChips.tsx` already holds the tone table the part needs.

**1.8 The card with a header and a body — 11 sites. Part: `DisclosureCard`.** Six interaction
cards are byte-identical on `bg-surface-elevated rounded-lg p-4 mb-4 border border-hairline`:
`interactions/AskInteraction.tsx:36`, `AskBatchInteraction.tsx:64`, `PopupInteraction.tsx:38`,
`PanelInteraction.tsx:65`, `ShowInteraction.tsx:21`, `EditInteraction.tsx:27`.
`PermissionInteraction.tsx:143` writes `p-3`, and that one character is the drift.
`SubagentCard.tsx` and `DelegationCard.tsx` are twins: after I mask the words "subagent" and
"delegation", 149 and 151 lines differ on 59 lines. Both draw a header button, then Prompt,
Summary, Error and a running dot. `ActivityPanel.tsx:107` draws the same four sections with
other `max-h` values and the label "Result". `ToolCard.tsx:341` draws them a fourth time.

**1.9 The empty state — 15 bypass sites. Part: `EmptyState`, extended.**
`components/ui/EmptyState.tsx` exists and five files import it. Fifteen bypass it:
`CommandPalette.tsx:179`, `ActivityPanel.tsx:186`, `InboxPanel.tsx:299`,
`SurfacesPanel.tsx:113`, `SurfacesPanel.tsx:123`, `PluginPanel.tsx:209`,
`MessageList.tsx:269`, `SearchPanel.tsx:377`, `ChangesPanel.tsx:488`, `SkillsPanel.tsx:126`,
`SkillsPanel.tsx:135`, `composer/ChipSelect.tsx:569`, `composer/ChipSelect.tsx:697`,
`shell/ProjectMenu.tsx:77`, `mobile/SessionsTab.tsx:173`. Six say "Loading…", a third tone the
part lacks.

**1.10 The segmented control — 4 sites. Part: `Segmented`.** `SearchPanel.tsx:341` uses
`role="group"` in an `inline-flex rounded-full border overflow-hidden`. `ChangesPanel.tsx:397`
uses `role="group"` over a `For`. `editor/EditorWithPreview.tsx:137` wants one and builds two
toggle buttons over a shared `modeButton` string. `ChatModeControl.tsx:238` is the same axis
as a circle.

## 2. The ten deepest components

| File | Depth | Lines | Branches | Variant | Structural |
|---|---|---|---|---|---|
| `blocks/GraphBlock.tsx` | 16 | 219 | 8 | 3 | 5 |
| `SkillsPanel.tsx` | 15 | 230 | 8 | 4 | 4 |
| `ChangesPanel.tsx` | 14 | 663 | 14 | 9 | 5 |
| `ToolCard.tsx` | 14 | 511 | 12 | 8 | 4 |
| `settings/AppConfigSettings.tsx` | 14 | 503 | 17 | 11 | 6 |
| `AssistantTurn.tsx` | 14 | 330 | 11 | 5 | 6 |
| `composer/ChipSelect.tsx` | 13 | 706 | 20 | 13 | 7 |
| `CenterComposer.tsx` | 11 | 732 | 1 | 0 | 1 |
| `SessionTree.tsx` | 11 | 664 | 8 | 4 | 4 |
| `canvas/CanvasPanel.tsx` | 10 | 1628 | 15 | 4 | 11 |

`settings/AppConfigSettings.tsx` is the clearest case. Lines 182, 192, 212, 233 and 249 test
`props.node.type` against `'toggle'`, `'select'`, `'range'`, `'text'` and a negated list of
all four. That is a closed set as five sequential `<Show>` calls with no completeness gate. A
record keyed by node type removes all five.

`ToolCard.tsx` lines 345, 366, 382, 394, 482 and 494 pick the body: a bash command, formatted
arguments, an error, a diff, a result, a spinner. Six exclusive bodies behind six independent
predicates. Line 55 picks the icon through seven sequential `String.includes` tests. Both are
tables.

`composer/ChipSelect.tsx` lines 447, 547 and 681 each render an optional icon; lines 554 and
688 an optional hint; lines 551 and 685 a tick. The flyout at line 644 redraws the option row
that line 514 already draws. One `OptionRow` part removes six branches.

`SkillsPanel.tsx` lines 124, 128, 130 and 210 are a load ladder: no kiln, then loading, then
error, then empty. `ChangesPanel.tsx` lines 486, 490, 501, 537 and 549 repeat it, and every
panel writes its own.

True structure: `ChangesPanel.tsx:452` and `ChangesPanel.tsx:619` list conflicts and comments,
which are separate data; `canvas/CanvasPanel.tsx` holds a viewport, a marquee and three
dialogs, so it is long rather than branchy. `AssistantTurn.tsx` lines 294, 298, 312 and 316
are all meta-row contents, and they became branches only because the row arrived as a wrapper
and not as a list. `CenterComposer.tsx` holds one branch in 732 lines, so its depth needs a
layout lane, not this one.

## 3. Why the tree grew this way

I read every commit that touched five representative files.

| File | Commits | Added a branch or a class | Imported a part |
|---|---|---|---|
| `ToolCard.tsx` | 26 | 16 | 0 |
| `ChangesPanel.tsx` | 9 | 7 | 0 |
| `composer/ChipSelect.tsx` | 23 | 14 | 2 |
| `SessionTree.tsx` | 16 | 11 | 3 |
| `settings/AppConfigSettings.tsx` | 6 | 4 | 0 |
| Total | 80 | 52 | 5 |

Two subjects show the mechanism.

> `1156ec613 feat(web): refine the composer, empty states and touch density; add the theme token contract`

That commit changed 101 files under `crates/crucible-web/web/src`.

> `9526ec046 feat(web)!: set the app in Geist and unify the type scale`

That one changed 40.

The mechanism is this. A feature lands as a branch inside the component that already draws the
nearest thing, because a branch costs one commit and a part costs a name, a file and a
migration. Each branch copies the class string beside it, so the recipe lives in two places
and the second copy is free to drift. The drift stays invisible until somebody states a rule
for the whole app. That rule has no single place to land, so it lands as a sweep over 40 or
101 files. A sweep is expensive, hard to review, and it never reaches the copies nobody
grepped for. The next feature branches a swept file. The loop ran 52 times in 80 commits.

## 4. The parts library

New files under `crates/crucible-web/web/src/components/ui/`, by sites replaced.

| Part | Sites | Class recipe |
|---|---|---|
| `EmptyState` (extend) | 15 | keep the file; add a loading tone |
| `SectionHead` | 14 | `text-floor font-semibold uppercase tracking-wide text-muted-dark` |
| `IconButton` (extend) | 12 | keep `IconButton.tsx:26`; add `tone`, `hit` |
| `ListRow` | 12 | `treeRow` plus `data-density`; no `text-*` on the row |
| `DisclosureCard` | 11 | `bg-surface-elevated rounded-lg p-4 border border-hairline` |
| `Chip` | 9 | `inline-flex items-center gap-1 px-2 py-0.5 rounded-md border text-floor` |
| `Fold` | 9 | the `TreeSection.tsx:29` header plus `treeChevron` |
| `PopoverPanel` | 9 | `menu-style.ts:22`: `hairline-strong` under `shadow-md` |
| `Reveal` | 6 | the `TurnMeta.tsx` opacity and pointer-event rule (formerly `TurnGutter.tsx:25`) |
| `LoadLadder` | 6 | no classes; it renders `EmptyState` per state |
| `Segmented` | 4 | `inline-flex rounded-md border border-hairline overflow-hidden` |
| `StatusDot` (promote) | 4 | move `shell/SessionStatusDot.tsx` into `ui/` |

```ts
interface IconButtonProps extends JSX.ButtonHTMLAttributes<HTMLButtonElement> {
  size?: 'sm' | 'md'; tone?: 'default' | 'ok' | 'error'; hit?: boolean }  // 24|28px box; hover colour; 44px target
interface ListRowProps { icon?: Component<{ class?: string }>; title: JSX.Element;
  subtitle?: JSX.Element; meta?: JSX.Element; trailing?: JSX.Element;  // trailing reveals on hover
  selected?: boolean; onSelect?: () => void; testid?: string }
interface SectionHeadProps { label: string; count?: number; urgent?: boolean;
  actions?: JSX.Element; density?: 'tight' | 'normal' }
interface DisclosureCardProps { tone?: 'neutral' | 'running' | 'error';
  header: JSX.Element; open: boolean; onToggle: () => void;
  sections: { label: string; tone?: 'muted' | 'ok' | 'error'; body: JSX.Element }[] }
interface ChipProps { tone?: 'neutral' | 'ok' | 'warn' | 'error' | 'precog' | 'primary';
  icon?: Component<{ class?: string }>; dot?: boolean; caps?: boolean;
  title?: string; children: JSX.Element }
interface FoldProps { label: string; count?: number; open: boolean;
  onToggle: () => void; hideWhenEmpty?: boolean; testid: string; children: JSX.Element }
interface PopoverPanelProps { children: JSX.Element; width?: 'menu' | 'panel' | 'dialog';
  maxHeight?: number; anchor?: PopupPlacement; class?: string }
interface RevealProps { children: JSX.Element; align?: 'start' | 'center' | 'end'; class?: string }
interface LoadLadderProps<T> { state: { loading: boolean; error?: string; data?: T[] };
  empty: { title: string; body?: string }; children: (data: T[]) => JSX.Element }
interface SegmentedProps<T extends string> { value: T; label: string; size?: 'sm' | 'md';
  options: { value: T; label: string; icon?: Component<{ class?: string }> }[];
  onChange: (value: T) => void }
```

Three surfaces become data. `components/settings/sections.tsx:48` already declares the SECTION
record; the controls inside one are still five `<Show>` calls. `lib/register-panels.tsx:23`
registers 19 panels and each draws its own header and empty state.
`crates/crucible-core/src/types/tool_display.rs` owns the tool projection the daemon sends,
and `ToolKind` is its web half.

```ts
type ConfigControl = 'toggle' | 'select' | 'range' | 'text';
type ControlTable = Record<ConfigControl, Component<{ node: PluginOptionNode }>>;
interface PanelDefinition {  // widened; the shell then draws header and empty state
  id: string; title: string; component: Component; defaultZone: Zone;
  icon?: Component<{ class?: string }>; headerActions?: Component;
  empty?: { title: string; body?: string } }
interface ToolKind { match: (name: string) => boolean;
  icon: Component<{ class?: string }>; body: Component<{ call: ToolCallDisplay }> }
```

## 5. The lanes

One session on `opus` each. File-disjoint except where stated.
1. **IconButton.** Add `tone` and `hit`; migrate the 12 sites in 1.1. Tests:
   `ui/__tests__/IconButton.test.tsx` gains a tone case, and `style-consistency.test.ts` gains a
   gate that fails on a raw `hover:bg-hover-wash` on a `<button>`. Check the transcript meta row,
   the Changes accept/reject pair, the Backlinks refresh.
2. **SectionHead.** Replace the four declarations and the ten copies in 1.3;
   delete `components/ui/SectionLabel.tsx` and the `treeSectionHeader` export. Tests: the
   `SearchPanel`, `BacklinksPanel`, `SkillsPanel` and `SettingsModal` specs. Check every panel
   header at one size and one tracking.
3. **EmptyState and LoadLadder.** Add the loading tone, build the ladder, migrate
   the 15 sites in 1.9 and the six ladders in section 2. Tests: `__tests__/EmptyState.test.tsx`,
   `MessageList.empty-state.test.tsx`. Check Skills with no kiln, Changes with no session, Search
   with no match.
4. **Fold.** Widen `components/tree/TreeSection.tsx` with `hideWhenEmpty`;
   migrate the nine folds. Tests: the `InboxPanel`, `SessionTree` and `ChangesPanel` specs. Check
   one caret everywhere, and no text triangles.
5. **Chip.** Build it from the `SessionStatusChips.tsx` tone table; migrate the
   nine sites. Tests: the `SessionStatusChips` and `SessionScopeChips` specs. Check the status
   strip, the branch chip and the plugin badge at one radius.
6. **DisclosureCard.** Reduce `SubagentCard.tsx` and `DelegationCard.tsx` to thin
   callers; migrate the seven interaction cards and the Activity task card. Tests:
   `__tests__/InteractionHandlers.test.tsx`, `interaction-coverage.test.ts`. Check a running
   subagent, a failed delegation, a permission prompt.
7. **PopoverPanel.** Fold `menu-style.ts` into the part; migrate the nine panels.
   Tests: the `RootDropdown`, `ChipSelect` and `CommandPalette` specs. Check that the project
   menu, the chip popout and the palette share one edge and shadow.
8. **ListRow and Reveal.** Migrate the 12 rows and the six reveals. Run AFTER
   lane 4: both touch `SessionTree.tsx`. Tests: the `SearchPanel`, `BacklinksPanel` and
   `FileTreeView` specs. Check row height and indent at both densities, phone and desktop.
9. **ToolCard kinds.** Replace `iconForTool` and the six body branches with the
   `ToolKind` table. Tests: the three `ToolCard` specs. Check a bash call, an edit with a diff,
   an error, an MCP result.
10. **Settings controls.** Replace the five branches at
    `settings/AppConfigSettings.tsx:182` with an exhaustive record. Test:
    `settings/__tests__/AppConfigSettings.test.tsx`. Check a toggle, a select, a range and a
    text row, on a phone and on a desktop.
11. **Panel registry.** Widen `PanelDefinition`; move the header and the empty
   state into the shell. Tests: `lib/__tests__/panel-registry.test.ts`,
   `lib/__tests__/register-panels.test.tsx`. Check all 19 panels open with one header treatment.
12. **Segmented.** Migrate the four sites. Tests: the `EditorWithPreview` and
   `ChangesPanel` specs. Check the editor Read and Write pair, the Changes scope, the search
   mode.

## 6. The rule block for CLAUDE.md

```
## Web composition

- Before you add a <Show>, ask if its two arms are VARIANTS of one part.
  If they are, add a record entry. Only true structure gets a branch.
- A component over 250 lines or 10 JSX levels takes no new branch. Split it.
- No class recipe is written twice. A second site means a part under
  `components/ui/`, and the first site migrates in the same commit.
- A new surface is one record plus one renderer, never a list of `if` tests.
- A closed set needs an exhaustive record the compiler checks.
- Add the gate to `components/__tests__/style-consistency.test.ts` in the
  commit that adds the part.
```
