---
title: t3code Sessions Rail — 2026-09-15
description: The thread list of T3 Code, row by row and state by state, compared with the crucible-web sessions rail and ranked P1 to P3; section 13 gives the inbox + tree structure
tags: [meta, ux, web, design, reference, sessions]
status: draft
updated: 2026-09-15
---

# t3code Sessions Rail — 2026-09-15

Source: [T3 Code](https://github.com/pingdotgg/t3code) at commit `3efdcc52`, read on 2026-09-15.
The comparison target is `crates/crucible-web/web` at `7d96a3a81` on master.
Part of [[2026-09-15 t3code Design Reference]].


Reference tree root (read-only):
`t3code (checkout 3efdcc52)`

Target tree:
`crates/crucible-web/web/src`

Everything below is copied from the source. Class strings are verbatim.

---

## 0. File map

### T3 Code — the sidebar is not in `components/sidebar/`

`components/sidebar/` holds only chrome (brand header, footer pills, update
notices). The thread list itself is one very large file plus five logic
siblings.

| Path | Lines | Role |
|---|---|---|
| `apps/web/src/components/Sidebar.tsx` | 4916 | The whole thread list: rows, shelves, drag, search, context menu, keyboard |
| `apps/web/src/components/Sidebar.logic.ts` | 1253 | Pure model: status resolution, recede rule, list-item model, drop planning, sorting, search |
| `apps/web/src/components/Sidebar.drag.ts` | 259 | dnd-kit collision detection + sorting strategy + modifiers |
| `apps/web/src/components/Sidebar.motion.ts` | 215 | FLIP animation for row reordering |
| `apps/web/src/components/Sidebar.pointer.ts` | 141 | Pointer sensor with distance constraint, drag lifecycle |
| `apps/web/src/components/Sidebar.snooze.ts` | 57 | Snooze preset list and wake labels |
| `apps/web/src/components/ThreadStatusIndicators.tsx` | 606 | PR badge, terminal badge, worktree glyph, status pill, compact leading/trailing status |
| `apps/web/src/components/sidebar/SidebarThreadHeader.tsx` | 209 | The one header row: search + scope + new-project + new-thread |
| `apps/web/src/components/sidebar/SidebarChrome.tsx` | 233 | Brand header, utility footer (settings / PRs / usage / back) |
| `apps/web/src/components/ui/sidebar.tsx` | 920 | Shell primitives: provider, rail, trigger, content, group, menu button |
| `apps/web/src/components/AppSidebarLayout.tsx` | 262 | Mounts the sidebar, owns width + resize + keyboard toggle |
| `apps/web/src/components/threadSidebarWidth.ts` | 22 | Width constants and clamping |
| `apps/web/src/components/threadActionMenu.logic.ts` | 146 | The single context-menu item table, shared by sidebar and chat header |
| `apps/web/src/hooks/useThreadActionMenu.ts` | ~340 | Dispatcher for that table on non-sidebar surfaces |
| `apps/web/src/components/chat/ChatHeader.tsx` | 443 | Breadcrumb, title-as-menu-trigger, inline rename |
| `apps/web/src/components/ThreadNotificationCoordinator.tsx` | 210 | Sound, desktop notification, in-app toast, badge |
| `apps/web/src/threadNotifications.ts` | 100 | Audio unlock, buffer cache, favicon/taskbar badge |
| `apps/web/src/index.css` | 2122 | All tokens |
| `apps/mobile/src/features/threads/threadListV2.ts` | 528 | Mobile port of the same model |
| `apps/mobile/src/features/threads/thread-list-v2-items.tsx` | 1146 | Mobile rows, shelf headers, swipe actions |

### Crucible — what exists today

| Path | Lines | Role |
|---|---|---|
| `components/SessionsPanel.tsx` | ~200 | Inbox + tree + Reflections + Archived |
| `components/SessionTree.tsx` | ~520 | Project grouping, group header, `SessionRow`, one hoisted context menu |
| `components/shell/SessionStatusDot.tsx` | ~75 | The 7px dot |
| `lib/session-status.ts` | ~55 | Three-value status |
| `lib/session-inbox.ts` | ~30 | Inbox = newest 5 |
| `components/tree/TreeSection.tsx` | ~40 | Collapsible counted section |
| `components/tree/tree-style.ts` | ~40 | Shared tree classes |
| `components/SessionStatusChips.tsx` | ~130 | Chips for the ACTIVE session (not the list) |
| `components/SessionScopeChips.tsx` | ~200 | Workspace + kiln pickers under the composer |
| `components/PanelShell.tsx` / `PanelHeader.tsx` | 17 / 22 | Trivial wrappers |
| `index.css` | ~2100 | Tokens |

---

## 1. Information architecture

### 1.1 What the sidebar shows, top to bottom

```
SidebarChromeHeader        brand only: hamburger (mobile) + T3 wordmark + "Code" + optional stage pill
─────────────────────────  (52px, --workspace-topbar-height, drag-region on Electron)
SidebarThreadHeader        [search..............] [scope] [new project] [new thread]
─────────────────────────  (fixedHeader: does not scroll)
  draft rows               unsent composer drafts, newest first  <- above everything
  ─ draft divider ─
  (pinned-header marker)   zero-height; grows a "Pinned" label only while dragging
  pinned rows              CARD variant
  (pinned-divider marker)  zero-height; grows an "Active" label only while dragging
  (active-placeholder)     zero-height drop target
  active rows              CARD variant
  v Snoozed (3)            collapsible shelf header, mt-auto -> pushed to bottom
  snoozed rows             SLIM variant, collapsed by default
  v Settled (41)           collapsible shelf header
  settled rows             SLIM variant, collapsed by default, paged 10 then +25
  + Show 25 more
─────────────────────────
SidebarChromeFooter        provider update pill, arch warning, then settings / PRs / usage + update pill
```

`apps/web/src/components/Sidebar.tsx:4367-4916` is the whole return.

### 1.2 The hierarchy is FLAT

This is the single most important structural decision, and it is the opposite
of Crucible's. **There is no project tier in the list.** Projects exist only as
a *filter* — one combobox in the header — never as a nesting level. Each row
carries its project as a favicon + name on its own first line.

`Sidebar.tsx:2333-2352`:

```tsx
// Project scope: one menu above the list. Scoping filters the list without
// making the header width depend on the number or length of project names.
const projectScopeItems = useMemo(
  () => [
    { value: "all", label: "All projects" },
    ...projectGroups.map((project) => ({
      value: project.projectKey,
      label: project.displayName,
    })),
  ],
  [projectGroups],
);
```

Grouping is instead by **lifecycle**: pinned -> active -> snoozed -> settled.
That is a user-controlled, four-state inbox, not a directory.

### 1.3 Collapsed by default

Both shelves. `Sidebar.tsx:250-254`:

```ts
// Fresh keys deliberately reset both shelves to collapsed for existing users.
const SETTLED_SHELF_EXPANDED_KEY = "t3code:sidebar:settled-expanded";
const SNOOZED_SHELF_EXPANDED_KEY = "t3code:sidebar:snoozed-expanded";
```

`useLocalStorage(SETTLED_SHELF_EXPANDED_KEY, false, Schema.Boolean)` — default
`false`.

Two exceptions are hard-coded, and both are worth stealing:

1. **The open thread never hides.** A settled or snoozed thread that is the
   current route is rendered even while its shelf is collapsed
   (`Sidebar.tsx:2703-2714` and `2738-2751`).
2. **"Show more" never hides the open thread** either — `visibleSettledThreads`
   pulls the route thread up out of the deep tail (`Sidebar.tsx:2662-2680`).

### 1.4 Hover-only

| Element | Rest | Hover / focus |
|---|---|---|
| Time label / status label | visible | fades to `opacity-0` and goes `absolute` |
| Settle button (check + "Settle") | `opacity-0`, `pointer-events-none`, `absolute` | `static`, `opacity-100` |
| Snooze button (clock) | hidden the same way | shown |
| Discard-draft X | hidden | shown |
| Un-settle / Wake (slim) | hidden | shown |
| Settled row favicon | `opacity-40 grayscale` | full colour |
| `Woke` pill | **always visible** — it is an action, not a label | stays |
| PR badge | **always visible and clickable** | stays |

The comment at `Sidebar.tsx:1778-1781` states the rule explicitly:

```
{/* The visible state owns this slot's width: status at rest,
    actions on hover/keyboard focus or while the popover is open. Keeping
    the hidden state out of flow lets the project label reclaim
    space without either state overlapping it. */}
```

The two states swap `static` <-> `absolute` rather than one being
`display:none`, so the row never reflows and the project name reclaims the
freed width.

---

## 2. Row anatomy

Two variants only. `Sidebar.tsx:4620-4625`:

```
// Settled and snoozed are the ONLY things that collapse a
// row: every other thread is a full card. Density comes
// from users (or the auto rules) actually parking work,
// not from the sidebar second-guessing what still matters.
const isCard = section === "active" || section === "pinned";
```

### 2.1 The CARD row (pinned + active) — 78px content box

```
+---------------------------------------------------------------+
| pen  (o) crucible                  [pin]   (o) Working 2m 14s |  h-5, gap-1.5
| Fix the transcript wikilink resolution                        |  mt-1, text-sm
| Y feat/wikilinks                    T   PR12  +48 -9    C  M  |  mt-0.5, text-xs
+---------------------------------------------------------------+
  ^                                   ^    ^     ^         ^  ^
  worktree glyph                    term  PR   diffstat  env provider
```

Outer `<li>` — `Sidebar.tsx:1720-1729`:

```tsx
<li
  data-thread-item
  {...sortableRootProps}
  {...(fileDropHandlers ?? {})}
  className={cn(
    // Matches the h-[4.875rem] content box; the py-0.5 padding is added on top.
    "list-none py-0.5 [content-visibility:auto] [contain-intrinsic-size:auto_78px]",
    sortable?.isDragging && "relative z-20",
  )}
>
```

Note `content-visibility:auto` with a matching `contain-intrinsic-size` —
off-screen rows are not laid out, and the reserved size is exact so nothing
shifts on paint.

Inner content box — `Sidebar.tsx:1758`:

```tsx
<div className="relative z-10 h-[4.875rem] px-[var(--sidebar-row-content-inset)] py-[var(--sidebar-content-inset)]">
```

`--sidebar-row-content-inset: 0.625rem` (10px), `--sidebar-content-inset: 0.5rem` (8px).

**Line 1** (`Sidebar.tsx:1759-1774`): `flex h-5 min-w-0 items-center gap-1.5`

```tsx
<div className="flex h-5 min-w-0 items-center gap-1.5">
  {draftIndicator}
  {props.project ? (
    <ProjectFavicon project={props.project} className="size-4 shrink-0" />
  ) : null}
  {props.projectDisplayName ? (
    <span
      className={cn(
        "min-w-0 flex-1 truncate text-secondary-label text-xs",
        shouldRecede ? "font-normal" : "font-medium",
      )}
    >
      {props.projectDisplayName}
    </span>
  ) : (
    <span className="flex-1" />
  )}
  {pinIndicator}
  ...status/action slot...
</div>
```

**Line 2 — the title** (`Sidebar.tsx:1452-1494`):

```tsx
<span
  className={cn(
    "min-w-0 flex-1 text-sm transition-opacity motion-reduce:transition-none",
    shouldRecede ? "font-normal" : "font-medium",
    variant === "card"
      ? cn(
          "truncate",
          shouldRecede
            ? "text-secondary-label"
            : isUnread || isWoke || status === "input"
              ? "text-foreground"
              : status === "failed"
                ? "text-foreground/95"
                : "text-foreground/90",
        )
      : cn(...slim...),
    isRegeneratingTitle && "opacity-[0.55]",
  )}
>
  {thread.title}
</span>
```

Five title inks on the card alone. Weight AND colour both move with attention.

**Line 3 — the metadata strip** (`Sidebar.tsx:1901-1946`):

```tsx
<div className="mt-0.5 flex min-w-0 items-center gap-1.5 text-secondary-label text-xs">
  {/* Always the branch. The plan step used to take this slot while
      working, but it truncated to a half-sentence and dropped the
      branch, so the row lost its most stable identifier. */}
  {thread.branch ? (
    <>
      <ThreadWorktreeIndicator thread={thread} />
      <span className="min-w-0 flex-1 truncate whitespace-nowrap text-muted-foreground/40">
        {thread.branch}
      </span>
    </>
  ) : (
    <span className="flex-1" />
  )}
  {terminalStatusIcon}
  {prBadge}
  {diff ? (
    <span className="shrink-0 font-mono">
      <span className="text-diff-addition-foreground">+{diff.insertions}</span>{" "}
      <span className="text-diff-deletion-foreground">-{diff.deletions}</span>
    </span>
  ) : null}
  <span aria-hidden className="pointer-events-none ml-auto inline-flex shrink-0 items-center gap-1">
    {isRemote ? <EnvironmentMachineIcon kind={...} className="size-3.5" /> : null}
    {driverKind ? <ProviderInstanceIcon ... iconClassName="size-3.5 opacity-60" /> : null}
  </span>
</div>
```

The branch is deliberately at `text-muted-foreground/40` — 40% opacity — and it
is the *only* thing that permanently owns that slot. The comment records that
they tried putting the live plan step there and reverted: a truncated
half-sentence beat the row's most stable identifier.

### 2.2 Truncation rules

- Project name: `min-w-0 flex-1 truncate` — yields first, because it is the
  least identifying of the three lines.
- Title: `min-w-0 flex-1 truncate`, single line, never wraps (web). Mobile
  allows `numberOfLines={2}`.
- Branch: `min-w-0 flex-1 truncate whitespace-nowrap`.
- Every trailing badge is `shrink-0` — status, PR, diffstat, provider, machine
  never truncate. They win the width fight; the text yields.
- The environment label is a **glyph only**, never a word, in the row. The word
  lives in the tooltip.

### 2.3 The SLIM row (snoozed + settled) — 36px flat

`Sidebar.tsx:1566-1620`:

```tsx
<li
  data-thread-item
  {...sortableRootProps}
  className={cn(
    // Matches the h-9 row so unrendered rows never shift the list when they paint.
    "list-none [content-visibility:auto] [contain-intrinsic-size:auto_36px]",
    sortable?.isDragging && "relative z-20",
  )}
>
  <div
    ref={rowRef}
    role="button"
    tabIndex={0}
    data-testid="sidebar-row-slim"
    className={cn(rowSurfaceClassName, "flex h-9 items-center gap-2.5 px-2.5")}
  >
    <span className={cn(
      "shrink-0 transition-opacity",
      (!props.isActive || variantAction === "unsettle") &&
        "opacity-40 grayscale group-focus-within/sidebar-row:opacity-100 group-focus-within/sidebar-row:grayscale-0 group-hover/sidebar-row:opacity-100 group-hover/sidebar-row:grayscale-0",
    )}>
      <ProjectFavicon project={props.project} className="size-4" />
    </span>
    {draftIndicator}
    {title}
    {pinIndicator}
    {terminalStatusIcon}
    {prBadge}
    <span className="relative ml-auto flex h-6 min-w-8 shrink-0 items-center justify-end">
      ...time label / Woke pill, and the hover action over it...
    </span>
  </div>
</li>
```

`favicon -> title -> pin -> terminal -> PR -> time`. One line, 36px, project
favicon dimmed and desaturated at rest. Same surface classes as the card; only
the geometry and the leading dimming differ.

### 2.4 Exact surface class strings by state

This is the whole surface model — `Sidebar.tsx:1398-1422`:

```tsx
// All sidebar rows share one surface model. Live threads used to look
// like elevated cards while settled threads were plain rows, leaving neither
// a useful hierarchy nor a reliable hover cue. Status now lives in the row
// content; surface is reserved for interaction (hover, multi-select, route).
const rowSurfaceClassName = cn(
  "group/sidebar-row relative w-full cursor-pointer overflow-hidden rounded-md text-left outline-none select-none",
  variantAction === "unsettle" && "[&:not(:hover):not(:focus-within)_*]:text-secondary-label/70",
  props.isActive
    ? "bg-sidebar-row-active text-sidebar-foreground"
    : isSelected
      ? "bg-sidebar-row-selected text-sidebar-foreground"
      : hasUnsentDraft
        ? cn(draftSurfaceClassName, "text-sidebar-foreground")
        : shouldRecede
          ? "text-sidebar-muted-foreground/75 hover:bg-sidebar-row-hover hover:text-sidebar-foreground"
          : "bg-transparent text-sidebar-foreground hover:bg-sidebar-row-hover",
  isFileDragOver && "ring-1 ring-inset ring-primary/70",
  // The hover tint must not clobber an active/selected row's own surface.
  isFileDragOver && !props.isActive && !isSelected && "bg-sidebar-row-hover",
  // The lifted row is an opaque card so the rows beneath it never show
  // through. The row tint is translucent in dark themes and the pointer
  // keeps the hover color applied, so both the tint and the solid sidebar
  // color are stacked as background images.
  props.sortable?.isDragging &&
    "bg-[linear-gradient(var(--sidebar-row-active),var(--sidebar-row-active)),linear-gradient(var(--sidebar),var(--sidebar))] text-sidebar-foreground opacity-100 shadow-lg",
);
```

Where `draftSurfaceClassName` is (`Sidebar.tsx:546`):

```ts
const draftSurfaceClassName = "bg-amber-400/[0.04] hover:bg-amber-400/[0.08]";
const draftPenClassName = "size-3 shrink-0 text-amber-600 dark:text-amber-300/80";
```

Six mutually exclusive surfaces in priority order: **active route > multi-select
> unsent draft > receded > normal**, then two additive modifiers (file drag,
being dragged).

Note: NO status colour on the surface. Ever. The comment says why — it was tried
and it produced neither hierarchy nor a reliable hover cue.

### 2.5 The token values behind those class names

`apps/web/src/index.css:1024-1031` (light) and `1116-1124` (dark):

```css
/* light */
--sidebar: var(--color-zinc-50);
--sidebar-row-hover: var(--color-zinc-25);      /* oklch(99.2% 0 0) */
--sidebar-row-active: var(--color-white);
--sidebar-row-selected: var(--color-white);

/* dark */
--sidebar-row-hover:    color-mix(in srgb, var(--contrast-foreground)  8%, transparent);
--sidebar-row-active:   color-mix(in srgb, var(--contrast-foreground) 11%, transparent);
--sidebar-row-selected: color-mix(in srgb, var(--contrast-foreground)  7%, transparent);
```

Three tints within 4 percentage points of each other. The distinction is
carried by *content* (title ink, weight, status label), not by a strong fill.
Light mode inverts the logic entirely: hover is *lighter* than the panel, active
is pure white.

---

## 3. Status representation

### 3.1 The status model

`Sidebar.logic.ts:806-866`:

```ts
// ── Sidebar thread status model ─────────────────────────────────────
// Five visual states, three colors: color is reserved for "act now"
// (approval), "in motion" (working), and "broken" (failed). Ready is the
// unlabeled resting state — the agent stopped and is waiting on the user,
// whether it finished, asked a question, or proposed a plan.
// Unread completion is tracked separately: it describes whether a ready
// thread needs attention, not what the thread is currently doing.
export type SidebarThreadStatus =
  | "approval" | "input" | "working" | "monitoring" | "failed" | "ready";

export function resolveSidebarThreadStatus(thread: SidebarThreadStatusInput): SidebarThreadStatus {
  if (thread.hasPendingApprovals) return "approval";
  if (thread.hasPendingUserInput) return "input";
  if (thread.session?.status === "running" || thread.session?.status === "starting") return "working";
  // A failed session outranks lingering background liveness: the user must
  // see the failure, not a stale Working (review finding).
  if (thread.session?.status === "error") return "failed";
  // Background work outlives the turn: fleets read as working; monitoring
  // only when watch loops are the sole live work.
  if (thread.backgroundLiveness === "working") return "working";
  if (thread.backgroundLiveness === "monitoring") return "monitoring";
  return "ready";
}
```

### 3.2 The label + icon + colour table

`Sidebar.tsx:1332-1401`. This is the whole visual vocabulary:

| Status | Label | Icon (lucide, `size-4`) | Class |
|---|---|---|---|
| working | `Working` + live `2m 14s` | `CircleDashedIcon` | `text-sky-600 dark:text-sky-400` |
| monitoring | `Monitoring` | `EyeIcon` | `text-foreground dark:text-white` |
| approval | `Approval` | `ShieldQuestionIcon` | `text-amber-700 dark:text-amber-300` |
| input | `Input` | `MessageCircleQuestionIcon` | `text-indigo-600 dark:text-indigo-300` |
| failed | `Failed` | `CircleAlertIcon` | `text-red-700 dark:text-red-300` |
| woke | `Woke` (button) | `AlarmClockIcon` | `text-amber-700 dark:text-amber-300` |
| unread done | `Done` | `CircleCheckIcon` | `text-emerald-700 dark:text-emerald-300` |
| ready + read | *(no label — the timestamp shows instead)* | — | `text-secondary-label` |

Verbatim (`Sidebar.tsx:1332-1345`):

```tsx
// Status hues follow the system-wide convention set by sidebar v1 and the
// mobile Live Activity/widgets (amber approval, indigo input, sky working)
// so a thread reads the same color everywhere it surfaces.
const topStatus =
  status === "working"
    ? {
        label: "Working",
        icon: "working" as const,
        // No shimmer: a label that animates forever is noise in a sidebar
        // full of them (and repaints every vsync on high-refresh displays).
        className: "text-sky-600 dark:text-sky-400",
      }
    : ...
```

**There is no pulse, no spinner, no shimmer on a working thread in v2.** The
signal is a static ring icon plus a *ticking number*. The comment names both
reasons: visual noise when many rows are live, and GPU cost on 120 Hz displays.

The one exception is the terminal glyph, which does pulse
(`Sidebar.tsx:1516-1526`):

```tsx
<TerminalIcon className={cn("size-3.5", terminalStatus.pulse && "animate-status-pulse")} />
```

and the pulse keyframe is duty-cycled with `steps()` to avoid per-vsync repaints
(`index.css:232-247`):

```css
@keyframes status-pulse {
  0%, 40%  { opacity: 1;   animation-timing-function: steps(6); }
  50%, 90% { opacity: 0.5; animation-timing-function: steps(6); }
  100%     { opacity: 1; }
}
```

### 3.3 The live duration counter

`Sidebar.tsx:293-302`:

```tsx
// Self-ticking so only this span re-renders each second, not the whole row.
function WorkingDuration(props: { startedAt: string | null }) {
  const startedMs = props.startedAt !== null ? Date.parse(props.startedAt) : Number.NaN;
  const [, setTick] = useState(0);
  useEffect(() => {
    if (Number.isNaN(startedMs)) return;
    const id = window.setInterval(() => setTick((tick) => tick + 1), 1_000);
    return () => window.clearInterval(id);
  }, [startedMs]);
  if (Number.isNaN(startedMs)) return null;
  return <span className="tabular-nums">{formatWorkingDurationLabel(Date.now() - startedMs)}</span>;
}
```

Format (`Sidebar.logic.ts:977-983`): `< 60s -> "12s"`, `< 60m -> "7m"`, else
`"1h 23m"`.

The whole status label is the aria live region, but the ticking span is
`aria-hidden` — otherwise a screen reader announces every second
(`Sidebar.tsx:1836-1845`).

### 3.4 "Needs your input" — three distinct signals

1. **`Input`** (indigo, question-mark-in-bubble): the agent asked a question.
2. **`Approval`** (amber, shield-question): a tool wants permission.
3. **`Woke`** (amber, alarm clock, **is a button**): a snooze expired.

`input` is the only status that is exempt from receding
(`Sidebar.logic.ts:819-830`):

```ts
export function shouldRecedeSidebarThread(input: {...}): boolean {
  if (input.isActive || input.isSelected || input.status === "input") return false;
  if (input.status === "working" || input.status === "monitoring") return true;
  if (input.status === "ready" || input.status === "approval") {
    return !input.isUnread && !input.isWoke;
  }
  return false;
}
```

Read that carefully: **a working thread RECEDES.** Background work is explicitly
demoted so that it cannot outrank an unread completion. `input` is the only
state that is unconditionally prominent. That is the inbox-zero posture —
the list ranks *what needs you*, not *what is busy*.

`Woke` as an interactive pill (`Sidebar.tsx:1795-1813`):

```tsx
<button
  type="button"
  aria-label="Dismiss Woke notification"
  onClick={handleAcknowledgeWokeClick}
  className={cn(
    "inline-flex cursor-pointer items-center gap-1 rounded-sm font-medium outline-none hover:underline focus-visible:ring-2 focus-visible:ring-ring",
    topStatus.className,
  )}
>
  <AlarmClockIcon aria-hidden className="size-4 shrink-0" />
  <span role="status">{topStatus.label}</span>
</button>
```

### 3.5 Unread completion

`Sidebar.logic.ts:634-643`:

```ts
export function hasUnseenCompletion(thread: ThreadStatusInput): boolean {
  if (!thread.latestTurn?.completedAt) return false;
  const completedAt = Date.parse(thread.latestTurn.completedAt);
  if (Number.isNaN(completedAt)) return false;
  if (!thread.lastVisitedAt) return false;   // never-visited counts as READ
  const lastVisitedAt = Date.parse(thread.lastVisitedAt);
  if (Number.isNaN(lastVisitedAt)) return true;
  return completedAt > lastVisitedAt;
}
```

Never-visited counts as read, deliberately — otherwise switching to the new
sidebar would light up every historical thread at once.

### 3.6 Secondary status carriers in the row

| Carrier | Component | Colour |
|---|---|---|
| Terminal processes running | `TerminalIcon size-3.5` + pulse | `text-teal-600 dark:text-teal-300/90` |
| PR open | `GitPullRequestArrowIcon size-3` + number | `text-emerald-600 dark:text-emerald-300/90` |
| PR merged | same | `text-violet-600 dark:text-violet-300/90` |
| PR closed | same | `text-red-600 dark:text-red-300/90` |
| PR draft | same | `text-zinc-500 dark:text-zinc-400/80` |
| PR stack | `LayersIcon` + layer count | state colour above |
| Worktree | `FolderGit2Icon size-3` | `text-muted-foreground/40` |
| Unsent draft | `SquarePenIcon size-3` | `text-amber-600 dark:text-amber-300/80` |
| Remote machine | `EnvironmentMachineIcon size-3.5` | `text-sidebar-muted-foreground/70` |
| Provider | `ProviderInstanceIcon size-3.5` | `opacity-60` + accent badge |
| Pinned | `PinIcon size-3` | `text-muted-foreground/65` |

`PR_STATE_COLOR_CLASS` — `ThreadStatusIndicators.tsx:259-264`:

```ts
const PR_STATE_COLOR_CLASS: Record<ThreadPullRequestBadge["state"], string> = {
  open:   "text-emerald-600 dark:text-emerald-300/90",
  merged: "text-violet-600 dark:text-violet-300/90",
  closed: "text-red-600 dark:text-red-300/90",
  draft:  "text-zinc-500 dark:text-zinc-400/80",
};
```

### 3.7 The hover tooltip — the row's overflow valve

`Sidebar.tsx:313-440`. Right-side, `variant="glass"`, `max-w-80`, and it carries
everything the row could not fit:

```
+-----------------------------------+
| Fix the transcript wikilink...    |  title, text-xs font-medium
|                                   |
|  (o) crucible                     |  project favicon + name
|  [C] build-box                    |  machine glyph + environment label
|  Y   feat/wikilinks               |  branch
|  !   You're currently checked out |  branch mismatch warning (text-warning)
|      on another branch.           |
|  M   Opus 5 . Anthropic (work)    |  provider + instance
|  T   2 terminal processes running |  terminal, in its status colour
|  !   Error occurred               |  text-red-600 dark:text-red-400
| --------------------------------- |
|  PR #421  Fix wikilink resolution |  PR mini-list, indented by stack depth
+-----------------------------------+
```

Delay is 150ms, close delay 0
(`TooltipProvider key="sidebar-thread-tooltips-150" delay={150} closeDelay={0} timeout={400}`).

### 3.8 Notifications and sound

`ThreadNotificationCoordinator.tsx:104-197`. Two MP3s:
`assets/notification-completion.mp3`, `assets/notification-input.mp3`.

Four modes (`threadNotifications.ts:7-12`):

```ts
export const NOTIFICATION_MODE_LABELS = {
  off: "Off",
  notifications: "Notifications only",
  sound: "Sound only",
  "notifications-and-sound": "Notifications with sound",
};
```

Escalation ladder, in order:
1. Sound always plays if the mode allows.
2. If the window is **visible and focused** and the thread is **not** the open
   one -> in-app toast with an "Open thread" action.
3. Otherwise -> desktop `Notification` with `silent: true` (the sound already
   played) and `tag: "${environmentId}:${threadId}"` so a second event replaces
   rather than stacks.
4. A badge count on the taskbar / favicon; on non-Electron it draws a red
   circle with the count onto a 64x64 canvas and swaps the favicon
   (`threadNotifications.ts:25-64`).

Audio is unlocked on the first `pointerdown`/`keydown` so background playback is
permitted later.

---

## 4. Grouping and ordering

### 4.1 The four sections

`Sidebar.tsx:2512-2570` partitions in one pass:

```ts
if (optimisticDrop?.key === threadKey) { ...projected... }
else if (supportsSnooze && effectiveSnoozed(thread, { now: preciseNow })) {
  // Snooze outranks settlement and pinning until the thread wakes.
  snoozed.push(thread);
} else if (supportsSettlement && thread.settledOverride === "settled") {
  settled.push(thread);
} else if (thread.pinnedAt != null) {
  pinned.push(thread);
} else {
  active.push(thread);
}
```

Precedence: **snoozed > settled > pinned > active.**

### 4.2 Ordering inside each section

| Section | Order | Source |
|---|---|---|
| pinned | user-arranged `orderKey` first, then creation order | `sortPinnedThreadsByOrderKey` |
| active | user-arranged `orderKey`; **activity does not move a row** | `sortActiveThreadsByOrderKey` |
| snoozed | soonest wake first | `firstValidTimestampMs(snoozedUntil)` asc |
| settled | when the work *ended*, newest first | `sortSettledThreadsForSidebar` |

The active-order rule is a strong opinion (`threadListV2.ts:158-160`):

```
/** The active order shared by web and native: new/reopened rows, then the
    saved arrangement. Activity does not move a thread. */
```

A thread does **not** jump to the top when its agent speaks. The list is stable
furniture. That is the opposite of a recency-sorted chat list and it is
deliberate: the row you were looking at stays where you left it.

Settled ordering is bound to its label so the two cannot disagree
(`Sidebar.tsx:266-271`):

```ts
// Settled rows read "how long ago did this wrap up", matching their sort
// key: both go through resolveSettledThreadTimestamp so label and order can't
// disagree.
function settledTimeLabel(thread: SidebarThreadSummary): string {
  const timestamp = resolveSettledThreadTimestamp(thread);
  return timestamp === null ? "" : compactSidebarTimeLabel(formatRelativeTimeLabel(timestamp));
}
```

### 4.3 There is no Today / Yesterday grouping

Time buckets do not exist. Relative ages are compacted to the smallest readable
form (`Sidebar.tsx:256-259`):

```ts
function compactSidebarTimeLabel(label: string): string {
  if (label === "just now") return "now";
  return label.endsWith(" ago") ? label.slice(0, -4) : label;
}
```

`"2 hours ago" -> "2 hours"`, `"just now" -> "now"`.

### 4.4 Shelf headers

`Sidebar.tsx:618-702`:

```tsx
function SidebarSectionHeader(props: {
  marker: "snoozed-header" | "settled-header";
  label: string;
  className?: string;
  dragging?: boolean;
  isDropTarget?: boolean;
  toggle: { expanded: boolean; onToggle: () => void };
}) {
  const snoozed = props.marker === "snoozed-header";
  const className = cn(
    "flex h-full w-full items-center gap-2 px-2 text-left text-xs font-medium",
    snoozed ? "text-blue-600 dark:text-blue-400" : "text-sidebar-muted-foreground/60",
    props.dragging && "text-sidebar-foreground/80",
    props.isDropTarget && "text-primary",
  );
  const content = (
    <>
      <span className="shrink-0">{props.label}</span>
      <span aria-hidden className={cn(
        "h-px min-w-2 flex-1",
        snoozed ? "bg-blue-500/20 dark:bg-blue-400/15" : "bg-sidebar-border/60",
        props.dragging && "bg-sidebar-foreground/25",
        props.isDropTarget && "bg-primary/50",
      )} />
      <ChevronDownIcon aria-hidden className={cn(
        "size-3 shrink-0 transition-transform",
        props.toggle.expanded && "rotate-180",
      )} />
    </>
  );
  return (
    <SortableSidebarMarker marker={props.marker} className={cn("mx-0.5 h-8", props.className)}>
      <button type="button" onClick={props.toggle.onToggle} aria-expanded={props.toggle.expanded}
        className={cn(className, "cursor-pointer")}>{content}</button>
    </SortableSidebarMarker>
  );
}
```

Shape: **label — hairline rule that eats the remaining width — chevron.**
h-8 (32px), `text-xs font-medium`. The rule is the visual separator; there is no
box, no fill, no uppercase, no letter-spacing.

The count only appears when **collapsed** (`Sidebar.tsx:4815-4819`):

```tsx
label={settledShelfExpanded ? "Settled" : `Settled (${settledThreads.length})`}
```

Expanded, the rows themselves say how many there are, so the number is dropped.

Both shelves carry `mt-auto` so they sink to the bottom of a short list
(`className="mt-auto"` on the snoozed header, and on the settled header when
there are no snoozed threads). The inbox floats at the top; parked work sits on
the floor.

### 4.5 Drag and reorder

Full dnd-kit sortable list where **section markers are sortable items too**.
`Sidebar.logic.ts:110-135`:

```
// Rows and section markers share one sortable list. The separators resolve
// the lifecycle action; Sidebar.drag previews the resulting layout. Pinned
// and active threads keep the dragged position; settled threads use time
// order. Snoozed rows can leave the shelf, but dropping into it is not
// supported because snoozing requires a wake time.
```

Dropping across a boundary performs a lifecycle action, and the lifted row shows
the verb it will perform (`Sidebar.tsx:911-940`):

```tsx
const dropVerbBadge: Record<SidebarDropVerb, ReactNode> = {
  pin:      (<><PinIcon aria-hidden className="size-3" />Pin</>),
  unpin:    (<><PinOffIcon aria-hidden className="size-3" />Unpin</>),
  settle:   (<><CircleCheckIcon aria-hidden className="size-3" />Settle</>),
  unsettle: (<><Undo2Icon aria-hidden className="size-3" />Un-settle</>),
  wake:     (<><AlarmClockOffIcon aria-hidden className="size-3" />Wake</>),
};
```

rendered as (`Sidebar.tsx:1443-1450`):

```tsx
<span role="status"
  className="pointer-events-none ml-auto inline-flex h-5 shrink-0 items-center gap-1 rounded-sm border border-primary/40 bg-primary/10 px-1.5 text-[11px] font-medium text-primary">
  {dropVerbBadge[props.dropVerb]}
</span>
```

Boundary labels are **zero-height at rest** and grow only during a drag
(`Sidebar.tsx:583-616`):

```tsx
// Zero-height markers reserve no label space at rest. During a drag the
// sorting strategy opens 24px for a 16px label with 4px clearance on each side.
const SIDEBAR_DRAG_LABEL_HEIGHT = 24;
```

That is the trick worth stealing even without drag: a section boundary that
costs nothing until it means something.

---

## 5. Actions

### 5.1 The context menu — one table, two surfaces

`components/threadActionMenu.logic.ts:44-146`. Rendered natively via
`api.contextMenu.show(items, position)` (Electron) with a browser fallback.

```
New thread on feat/wikilinks        (only when the thread has a branch)
Pin thread / Unpin thread           (capability-gated)
Settle thread / Un-settle thread    (capability-gated)
Snooze > / Wake thread              (capability-gated, disabled when unsnoozable)
   |- In 1 hour        (3:42 PM)
   |- This evening     (6:00 PM)
   |- Tomorrow morning (9:00 AM)
   `- ---------------
      Custom...
---------------------------------
Rename thread
Regenerate title / Regenerating...  (disabled while in flight)
Mark unread
---------------------------------
Copy >
   |- Path
   |- Branch          (only with a branch)
   `- Thread ID
Project settings
---------------------------------
Archive thread                      (disabled while a turn is running)
Delete                              (destructive: true)
```

Three design points:

- The whole thing is **data**, shared verbatim with the chat header
  (`hooks/useThreadActionMenu.ts`). The doc comment says why:
  `"so labels, ordering, and capability gating cannot drift between the two surfaces."`
- Every item is **capability-gated on the server the thread lives on**, not on a
  global flag: a mixed-version fleet shows different menus per row.
- `Archive` and `Delete` are separated by a rule and only `Delete` is
  `destructive: true`. The comment distinguishes the three disposals precisely:

```
// Archive removes the thread from the sidebar while keeping its
// conversation under Settings > Archived threads — distinct from Settle
// (stays visible in the Settled shelf) and Delete (clears history for
// good), so it sits beside Delete without borrowing its destructive styling.
```

### 5.2 Confirmation

Two independent settings: `confirmThreadDelete`, `confirmThreadArchive`
(`useThreadActionMenu.ts:290-320`):

```ts
case "delete": {
  if (confirmThreadDelete) {
    const confirmed = await settlePromise(() =>
      api.dialogs.confirm(
        [
          `Delete thread "${thread.title}"?`,
          "This permanently clears conversation history for this thread.",
        ].join("\n"),
        { variant: "destructive" },
      ),
    );
    if (confirmed._tag === "Failure" || !confirmed.value) return;
  }
  ...
}
```

Confirmation names the thread and states the consequence in a second line.
It is a *setting*, defaulting on, not a hard-coded modal.

### 5.3 Undo instead of confirmation, where possible

Snooze does not confirm — it toasts with an Undo
(`useThreadActionMenu.ts:160-180`):

```ts
toastManager.add(
  stackedThreadToast({
    type: "success",
    title: `Snoozed until ${snoozeWakeDescription(preset.snoozedUntil, new Date(), timestampFormat)}`,
    timeout: 5_000,
    actionProps: {
      children: "Undo",
      onClick: () => { void unsnoozeThread(threadRef).then(...); },
    },
  }),
);
```

### 5.4 Inline hover actions

- **Card**: discard-draft X (only with an unsent draft) - snooze clock (opens a
  popover) - `Settle` (icon + word).
- **Slim/settled**: un-settle.
- **Slim/snoozed**: wake now.
- **Draft row**: discard.

Settle button, exact string (`Sidebar.tsx:1885-1893`):

```tsx
<button
  type="button"
  aria-label="Settle thread"
  onClick={handleSettleClick}
  className="-mr-1 inline-flex cursor-pointer items-center gap-1 rounded-md bg-transparent px-1.5 text-xs text-muted-foreground hover:text-foreground"
>
  <CheckIcon className="size-3.5" />
  Settle
</button>
```

The wrapper that reveals them (`Sidebar.tsx:1852-1863`):

```tsx
<span className={cn(
  // focus-visible, not focus-within: a mouse click leaves
  // the Settle button focused, and a plain focus-within
  // would keep the controls pinned over the status label
  // once the pointer moves away (e.g. after a failed
  // settle) instead of cross-fading back.
  "pointer-events-none absolute inset-y-0 right-0 flex items-stretch opacity-0 transition-opacity has-[:focus-visible]:pointer-events-auto has-[:focus-visible]:static has-[:focus-visible]:opacity-100 group-hover/sidebar-row:pointer-events-auto group-hover/sidebar-row:static group-hover/sidebar-row:opacity-100",
  snoozeMenuOpen && "pointer-events-auto static opacity-100",
)}>
```

Note the `snoozeMenuOpen &&` clause: while the popover is open the pointer has
left the row, so the actions are *pinned* rather than fading out from under the
open menu. The same signal suppresses the row tooltip.

### 5.5 Rename

Double-click the row (`Sidebar.tsx:1247-1258`):

```tsx
const handleDoubleClick = useCallback((event: ReactMouseEvent) => {
  if (isRenaming || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
  if ((event.target as HTMLElement).closest("button, a, input")) return;
  event.preventDefault();
  onStartRename(threadRef, thread.title);
}, [...]);
```

And the trailing click of a double-click must not navigate
(`Sidebar.logic.ts:650-656`):

```ts
// A double-click dispatches two `click` events before `dblclick`: the first has
// `detail === 1`, the second `detail === 2`. The second click must not run the
// row's single-click navigation, otherwise double-click-to-rename would also
// navigate. `MouseEvent.detail` is 0 for synthetic/keyboard activations, which
// still count as a normal single activation.
export function isTrailingDoubleClick(detail: number): boolean { return detail > 1; }
```

The input itself (`Sidebar.tsx:1453-1465`):

```tsx
<input autoFocus value={renamingTitle} aria-label="Thread title"
  onFocus={(event) => event.currentTarget.select()}
  className="min-w-0 flex-1 rounded-sm border border-input bg-card px-1 text-sm font-medium text-card-foreground outline-none focus:border-foreground" />
```

Enter commits, Escape cancels, blur commits (unless already committed), empty
is rejected with a toast, unchanged is a no-op.

### 5.6 Multi-select

Mod+click toggles, Shift+click ranges over the rendered order
(`Sidebar.tsx:2975-2996`). The selection has its own surface tint
(`bg-sidebar-row-selected`) and its own context menu
(`buildMultiSelectThreadContextMenuItems`) with bulk unpin and bulk title
regeneration. Selection clears when the project scope flips, because rows
selected under the old scope may now be invisible.

### 5.7 Keyboard

| Chord | Action |
|---|---|
| `Cmd+1..9` | jump to the Nth row of the flat rendered order |
| `thread.previous` / `thread.next` | traverse the same order, wrapping |
| `sidebar.toggle` (default `Cmd+B`) | show/hide the sidebar, captured before editors |
| `chat.new` | new thread (picker when multi-project) |
| `chat.newLocal` | new thread directly in the current project |
| `Enter` / `Space` on a row | open |
| Arrows / `Enter` / `Esc` in search | move highlight, open, clear |

The jump hint overlay is the nicest touch. Hold the modifier and each of the
first nine rows grows a floating chip (`Sidebar.tsx:273-288`):

```tsx
// Floats at the row's right edge, vertically centered, while the jump
// modifier is held. An overlay pill instead of an inline slot: the hint
// must neither displace the status/time label (holding Cmd used to blank
// out "Working") nor shift any layout when it appears. pointer-events-none
// so it never swallows clicks meant for the settle/un-settle buttons it
// can overlap.
function JumpHintBadge(props: { label: string }) {
  return (
    <span aria-hidden
      className="pointer-events-none absolute right-1.5 top-1/2 z-10 inline-flex h-5 -translate-y-1/2 items-center rounded-full border border-border/80 bg-background/95 px-1.5 font-mono text-[10px] font-medium tracking-tight text-foreground shadow-sm">
      {props.label}
    </span>
  );
}
```

It appears after a 200ms hold (`THREAD_JUMP_HINT_SHOW_DELAY_MS = 200`) so a
transient modifier press does not flash it.

### 5.8 Files can be dropped on a row

Dropping OS files on a thread row opens that thread and hands the files to its
composer (`Sidebar.tsx:1266-1288`). The row rings while the drag is over it:
`"ring-1 ring-inset ring-primary/70"`. Files are queued by id, so a second drop
before navigation lands keeps both.

### 5.9 New-thread affordance

One pen button at the far right of the header. Shift+click skips the project
picker (`Sidebar.logic.ts:681-687`), and the tooltip is a two-line explainer:

```tsx
tooltip={showNewThreadInProjectHint ? (
  <span className="flex flex-col gap-0.5">
    <span>{newThreadLabel}</span>
    <span className="text-muted-foreground">
      New thread in current project: Shift+click{newThreadInProjectShortcutLabel ? ` (${newThreadInProjectShortcutLabel})` : ""}
    </span>
  </span>
) : newThreadLabel}
```

---

## 6. Header, footer, width

### 6.1 Header — one row, four controls

`sidebar/SidebarThreadHeader.tsx:80-158`. The search field spans; the three
icon buttons sit at the end as an unfilled cluster.

```tsx
<div className="flex items-center gap-1">
  <div ref={searchFieldRef}
    className="flex h-8 min-w-0 flex-1 items-center gap-2 rounded-md px-2 py-1.5 text-sm font-medium text-sidebar-muted-foreground hover:bg-sidebar-row-hover hover:text-sidebar-foreground">
    <SearchIcon className="size-4 shrink-0 text-[var(--sidebar-icon-color)]" />
    <Input ref={searchInputRef} nativeInput unstyled type="search" placeholder="Search"
      role="combobox" aria-autocomplete="list" aria-expanded={resultsVisible}
      aria-controls={resultsVisible ? "sidebar-thread-search-results" : undefined}
      aria-activedescendant={activeResultExists ? `sidebar-thread-search-result-${activeSearchResultIndex}` : undefined}
      className="min-w-0 flex-1 [&_[data-slot=input]]:h-auto [&_[data-slot=input]]:p-0 ..." />
    {isSearching ? <Button size="icon-micro" variant="ghost" aria-label="Clear thread search">...</Button> : null}
  </div>
  {/* Unfilled like the search field beside it: the buttons carry their own
      hover states, and a background well reads far louder on themed
      palettes than on the base light and dark ones. */}
  <div className="flex shrink-0 items-center">
    {projectScope}                                       {/* folder or project favicon */}
    <SidebarHeaderIconButton label="New project"><FolderPlusIcon /></SidebarHeaderIconButton>
    <SidebarHeaderIconButton label="New thread"><SquarePenIcon /></SidebarHeaderIconButton>
  </div>
</div>
```

The search field has **no border and no fill at rest** — only a hover wash.
Icon buttons are `size-7` (28px) and carry a `pointer-events-none` 48px
pseudo-target for coarse pointers:

```tsx
<span aria-hidden
  className="pointer-events-none absolute left-1/2 top-1/2 size-[max(100%,3rem)] -translate-1/2 pointer-fine:hidden" />
```

**The project scope trigger swaps its icon to the selected project's favicon.**
That is the whole "project switcher" — no label, no chevron, no name. The popup
anchors to the *search field's* width, not to the 28px trigger, so project names
have room:

```tsx
<ComboboxPopup
  align="start"
  // Anchored to the search field, not the 28px trigger: the
  // popup opens under the field, is at least as wide as it,
  // and grows to fit project names up to a cap, past which
  // the rows truncate.
  anchor={headerSearchRef}
  className="max-w-[min(18rem,var(--available-width))] overflow-hidden"
>
```

Each project row in the popup is `h-8 min-h-8 py-0 font-medium` with favicon,
truncating name, an environment badge when the catalog spans machines, and a
`size-6` ghost settings button that appears at `ml-auto`.

### 6.2 Search behaviour

- Searching replaces the list entirely with a `role="listbox"` of flat 36px
  result rows (`Sidebar.tsx:2069-2126`): favicon - title - relative time. No
  status, no branch — a search result is being *picked*, not monitored.

```tsx
className={cn(
  "flex h-9 w-full cursor-pointer items-center gap-2.5 rounded-md px-2.5 text-left text-sm outline-none",
  props.isHighlighted || props.isRouteActive
    ? "bg-sidebar-row-active text-sidebar-foreground"
    : "text-sidebar-muted-foreground/75 hover:bg-sidebar-row-hover hover:text-sidebar-foreground",
  isFileDragOver && "ring-1 ring-inset ring-primary/70",
)}
```

- Results preserve lifecycle order, not relevance order (`Sidebar.logic.ts:898-914`):
  `"Keeping the input order means lifecycle ordering (active, snoozed, settled) remains stable while the user narrows the list."`
- Matches title **and PR terms** (number, title, repository).
- Empty: `<p role="status" className="px-2 py-6 text-center text-xs text-sidebar-muted-foreground">No threads found</p>`

### 6.3 Footer

`sidebar/SidebarChrome.tsx:205-215`:

```tsx
<SidebarFooter className="px-[var(--sidebar-content-inset)] py-1">
  <SidebarProviderUpdatePill />
  <SidebarUpdateArchitectureWarning />
  <SidebarUtilityMenu />
</SidebarFooter>
```

`SidebarUtilityMenu` is a horizontal row of icon-only buttons: **Settings -
Pull Requests (capability-gated) - Usage**, then an update pill. On a
settings/usage/PR route the whole row collapses to a single `<- Back` button.

There is **no user avatar, no version string, no account row** in the sidebar
footer. Version lives in the update pill only when there is an update.

### 6.4 Brand header

`SidebarChrome.tsx:82-107`. The wordmark is rendered as an SVG sized to
`h-[1cap]` and baseline-aligned with the word "Code":

```tsx
<span className="inline-flex min-w-0 items-baseline gap-1 text-sm font-medium tracking-tight">
  <T3Wordmark aria-label="T3" className="h-[1cap] w-auto shrink-0" />
  <span className="truncate [text-box:trim-both_cap_alphabetic] text-muted-foreground">Code</span>
</span>
```

`h-[1cap]` and `text-box: trim-both cap alphabetic` — the logo is sized to the
cap height of the adjacent text and the text box is trimmed to its capitals, so
the two optically align rather than share a bounding box. That is a level of
typographic care worth noting.

The header is 52px (`--workspace-topbar-height`), `drag-region` on Electron, and
carries an optional **stage backdrop** — a coloured/artwork band identifying a
non-production environment, with every control flipped to white-on-backdrop:

```tsx
backdropVariant &&
  "focus-visible:ring-white/90 [&_svg]:stroke-white/90! [&_svg]:opacity-100! [&_svg]:hover:stroke-white! [:hover,[data-pressed]]:bg-white/15",
```

### 6.5 Width, resize, collapse

`components/threadSidebarWidth.ts`:

```ts
export const THREAD_SIDEBAR_WIDTH_STORAGE_KEY = "chat_thread_sidebar_width";
const THREAD_SIDEBAR_DEFAULT_WIDTH = 16 * 16;   // 256px
export const THREAD_SIDEBAR_MIN_WIDTH = 13 * 16; // 208px
export const THREAD_MAIN_CONTENT_MIN_WIDTH = 40 * 16; // 640px

export function resolveThreadSidebarMaximumWidth(viewportWidth: number): number {
  return Math.max(THREAD_SIDEBAR_MIN_WIDTH, Math.floor(viewportWidth) - THREAD_MAIN_CONTENT_MIN_WIDTH);
}
```

Wired at `AppSidebarLayout.tsx:224-236`:

```tsx
resizable={{
  maxWidth: sidebarMaximumWidth,
  minWidth: THREAD_SIDEBAR_MIN_WIDTH,
  shouldAcceptWidth: ({ currentWidth, nextWidth, wrapper }) =>
    nextWidth <= currentWidth ||
    wrapper.clientWidth - nextWidth >= THREAD_MAIN_CONTENT_MIN_WIDTH,
  storageKey: THREAD_SIDEBAR_WIDTH_STORAGE_KEY,
  onResize: setSidebarWidth,
}}
```

`shouldAcceptWidth` lets you always shrink but only grow while the main content
keeps 640px. The max is recomputed live from a `useSyncExternalStore` on window
width, because a clamped drag ends with an unchanged width and therefore skips
the re-render that would refresh a render-time snapshot.

**Double-clicking the rail resets to default**
(`<SidebarRail onDoubleClick={resetSidebarWidth} />`).

Collapse is `collapsible="offcanvas"` — the sidebar slides fully out, it does not
shrink to an icon rail. There is no partially-collapsed state for the thread
list. The toggle button is a fixed-position control in the titlebar
(`AppSidebarLayout.tsx:100-128`), and on macOS it offsets by the traffic lights:

```ts
const MACOS_TRAFFIC_LIGHTS_LEFT_INSET = "var(--desktop-window-controls-inset, 90px)";
```

Animation is opt-in via `data-panel-animations`:
`"[[data-panel-animations=true]_&]:transition-[left,right,width] [[data-panel-animations=true]_&]:[transition-duration:var(--panel-animation-duration)] [[data-panel-animations=true]_&]:ease-out"`

On mobile it is a `Sheet` at `calc(100vw - var(--spacing(3)))` with safe-area padding.

---

## 7. Empty, loading, error

### 7.1 Empty list

`Sidebar.tsx:4875-4903`:

```tsx
<div className="flex flex-col items-center gap-2 px-2 py-6 text-center text-xs text-muted-foreground/60">
  {projects.length === 0 ? (
    <>
      <span>No projects yet</span>
      <button type="button" onClick={openAddProjectCommandPalette}
        className="inline-flex cursor-pointer items-center gap-1.5 rounded-md border border-sidebar-border px-2.5 py-1 text-[11px] font-medium text-sidebar-muted-foreground transition-colors hover:bg-sidebar-row-hover hover:text-sidebar-foreground">
        <PlusIcon className="-mx-0.5 size-3" />
        Add project
      </button>
    </>
  ) : scopedProjectGroup ? (
    `No threads in ${scopedProjectGroup.displayName} yet`
  ) : (
    "No threads yet"
  )}
</div>
```

Three distinct empties. The scoped one **names the project**, which tells the
user the filter is why the list is blank — the single most common confusion a
scoped list causes.

### 7.2 Loading

There is no skeleton in the thread list. The design is
**"last-known data paints, then updates"** — atoms hold the last snapshot, and a
disconnected environment keeps rendering its cached rows. Emptiness is only
asserted once every environment has a live snapshot
(`useAllEnvironmentProjectSnapshotsReady`, `Sidebar.tsx:2418-2426`):

```ts
// A persisted scope whose project is gone falls back to all projects, but
// only after every catalog environment has a live project snapshot. Cached
// or disconnected environments cannot establish that the project is gone.
```

### 7.3 Error

Per-row, never per-list. A failed session gets the `Failed` status on the row
and `Error occurred` in the tooltip. Every mutation failure is a stacked toast
(`stackedThreadToast`), never an inline error state, and interruptions are
squashed so a cancelled command does not toast a false error
(`isAtomCommandInterrupted`).

### 7.4 Paging, not infinite scroll

`Sidebar.tsx:246-249`:

```ts
// Settled-tail paging: recent history is the common lookup; the deep tail
// stays behind an explicit Show more.
const SETTLED_TAIL_INITIAL_COUNT = 10;
const SETTLED_TAIL_PAGE_COUNT = 25;
```

The "Show more" row is styled exactly like a slim row so the list has no seam:

```tsx
<button type="button" onClick={showMoreSettled}
  className="flex h-9 w-full cursor-pointer items-center gap-2.5 rounded-md px-2.5 text-left text-sm text-sidebar-muted-foreground/55 hover:bg-sidebar-row-hover hover:text-sidebar-foreground">
  <PlusIcon aria-hidden className="size-4 shrink-0" />
  Show {Math.min(hiddenSettledCount, SETTLED_TAIL_PAGE_COUNT)} more
</button>
```

---

## 8. Density — exact numbers

| Thing | Value |
|---|---|
| Card row `<li>` | `py-0.5` (2px) + 78px content = **82px** pitch |
| Card content box | `h-[4.875rem]` = **78px** |
| Card padding | `px-[var(--sidebar-row-content-inset)]` = **10px**, `py-[var(--sidebar-content-inset)]` = **8px** |
| Slim row | `h-9` = **36px**, `px-2.5` = 10px, `gap-2.5` = 10px |
| Search result row | `h-9` = 36px, `px-2.5`, `gap-2.5` |
| Draft row | `h-[4.875rem]` = 78px |
| Shelf header | `h-8` = **32px**, `px-2`, `gap-2`, `mx-0.5` |
| Header row | `h-8` = 32px; header group padding `p-[var(--sidebar-content-inset)] pt-1` |
| Header icon buttons | `size-7` = **28px**, coarse-pointer target 48px |
| Brand header | **52px** (`--workspace-topbar-height`) |
| Gap between rows | `gap-px` — **1px**, from `<ul className="relative flex flex-col gap-px">` |
| Row radius | `rounded-md` |
| Card line 1 | `h-5` = 20px, `gap-1.5` = 6px |
| Card line 2 offset | `mt-1` = 4px |
| Card line 3 offset | `mt-0.5` = 2px, `gap-1.5` = 6px |
| Title | `text-sm` = 14px, `font-medium` (500) or `font-normal` when receded |
| Project name | `text-xs` = 12px |
| Branch / metadata | `text-xs` = 12px |
| Status label | `text-xs` = 12px, `font-medium` |
| Drop verb badge | `text-[11px]` |
| Jump hint | `text-[10px] font-mono` |
| Status icon | `size-4` = 16px |
| Project favicon | `size-4` = 16px |
| Terminal / machine / provider glyph | `size-3.5` = 14px |
| Pin / worktree / PR / draft-pen glyph | `size-3` = 12px |
| Chevron | `size-3` = 12px |
| Settle / un-settle icon | `size-3.5` |
| List side padding | `ps-[calc(var(--sidebar-content-inset)+1px)] pe-[var(--sidebar-content-inset)]` = 9px / 8px |
| Sidebar default / min width | 256px / 208px |
| Main content min width | 640px |

The 1px inter-row gap with a `rounded-md` fill is what makes hover read as a
discrete chip rather than a table stripe. That plus the asymmetric list padding
(9px start, 8px end) so the rounded fill optically centres.

Type scale in the whole sidebar: **14 / 12 / 11 / 10**. Four sizes. Hierarchy
above 14px is carried by weight and ink, never by size.

---

## 9. The chat header — session-related parts

`components/chat/ChatHeader.tsx:314-400`.

Structure: `[project] > [Thread title v]` ... `[scripts] [Open in v] [git actions]`

- **The project leads**, and it is a button that starts a new thread in that
  project. Comment: `"knowing which project a thread lives in is priority zero,
  and the thread title alone doesn't answer it."`
- **The title IS the menu trigger.** Click opens the thread action menu — the
  same table as the sidebar's right-click. A chevron fades in on hover:

```tsx
<ChevronDownIcon aria-hidden data-thread-title-chevron
  className="size-3.5 shrink-0 text-muted-foreground opacity-0 transition-opacity group-hover/thread-title:opacity-100 group-focus-visible/thread-title:opacity-100" />
```

- Click-to-open-menu is **delayed 500ms** so a double-click can cancel it and
  rename instead (`TITLE_MENU_OPEN_DELAY_MS = 500`, native menus only, because a
  native menu swallows the second click; the browser fallback opens immediately).
- Inline rename in place, same commit rule as the sidebar, factored into
  `resolveRenameCommit` and exported so both surfaces share it.
- **No model picker, no mode picker, no branch name, no token/cost display in the
  header.** Those live in the composer footer: `ContextWindowMeter`,
  `ComposerUsageLimits`, `ModelPickerSidebar`, `ComposerControl`. The header is
  identity + workspace actions only.
- Right-side actions get `pr-16` when the right panel is closed, transitioning,
  to clear the floating panel controls.

---

## 10. Mobile — what differs and what to learn

Ported deliberately from the web model (`threadListV2.ts:29-36`):
`"Thread List v2 model, ported from the web sidebar v2."`

| Aspect | Web | Mobile |
|---|---|---|
| Status values | 6 (incl. `monitoring`) | 5 — `monitoring` dropped |
| Status colour | `text-sky-600 dark:text-sky-400` etc. | semantic aliases: `text-warning-foreground`, `text-adaptive-sky-600-400`, `text-danger-foreground` |
| `input` colour | indigo | `text-foreground-secondary` — **no hue at all** |
| Title lines | 1 (`truncate`) | 2 (`numberOfLines={2}`) |
| Row actions | hover | **swipe**: primary Settle/Un-settle/Wake, secondary Snooze |
| Full swipe | n/a | commits the primary action only, never snooze |
| Context menu | right-click | long-press `ControlPillMenu` |
| Slim row height | 36px | `min-h-[44px]` — touch floor |
| Card padding | `px-2.5` | `px-3` (sidebar pane) / `px-5` (full screen) |
| Branch + machine | separate slots | one truncating line: `branch  .  machine`, machine last so a tight fit cuts the repetitive label |
| Branch font | sans | **monospace** (`style={{ fontFamily: MONO_FONT }}`) |
| Time label font | tabular sans | **monospace** |
| Extra section | — | `PENDING` — queued new-tasks awaiting reconnect |
| Section headers | `mx-0.5 h-8` | `mb-1.5 mt-4` + label + hairline + chevron |
| Selected row | tint | full `--color-user-bubble` fill, with every text node switched to the paired foreground |

Three things worth importing to desktop:

1. **`STATUS_LABEL_BY_STATUS` as a flat lookup table** rather than a nested
   ternary (`thread-list-v2-items.tsx:60-67`):

```ts
const STATUS_LABEL_BY_STATUS: Partial<Record<ThreadListV2Status, { label: string; className: string }>> = {
  approval: { label: "Approval", className: "text-warning-foreground" },
  input:    { label: "Input",    className: "text-foreground-secondary" },
  working:  { label: "Working",  className: "text-adaptive-sky-600-400" },
  failed:   { label: "Failed",   className: "text-danger-foreground" },
};
```

Absent key = no label = fall through to the timestamp. Much cleaner than the
web's 8-deep ternary at `Sidebar.tsx:1332-1401`.

2. **The branch/machine single-line rule**, verbatim from the comment:

```
/* "branch . machine" share one truncating line. The machine sits
   last so a tight fit cuts the repetitive label, not the branch —
   and machine-only fills the row for non-git projects. The glyph
   hugs the label (it cannot live inside the Text without breaking
   truncation), and the wrapper takes the slack so the trailers
   stay pinned right. */
```

3. **Monospace on the branch and on the time label.** It makes the metadata line
   read as data rather than as a second sentence, and it stops a ticking
   timestamp from changing width.

The mobile shelf header, for reference (`thread-list-v2-items.tsx:155-190`) —
same label/rule/chevron shape as web, different spacing:

```tsx
className={cn("mb-1.5 mt-4 flex-row items-center gap-2.5", props.pane === "sidebar" ? "px-3" : "px-5")}
...
<Text className="text-xs font-t3-medium text-foreground-tertiary">
  {props.expanded ? "Settled" : `Settled (${props.count})`}
</Text>
<View className="h-px flex-1 bg-border" />
<SymbolView name="chevron.down" size={10}
  style={{ transform: [{ rotate: props.expanded ? "180deg" : "0deg" }] }} />
```

---

## 11. Comparison against Crucible, with recommendations

### 11.1 Summary scorecard

| Dimension | T3 Code | Crucible today | Gap |
|---|---|---|---|
| Row information density | 3 lines, ~9 facts | 1 line, 4 facts | **P1** |
| Status vocabulary | 6 states, 5 labels, 5 hues, 8 glyphs | 3 states, one 7px dot | **P1** |
| Lifecycle sections | pinned / active / snoozed / settled, user-driven | Inbox(5) / tree / Reflections / Archived | **P2** |
| Hover actions | 3, with a non-shifting swap slot | 1 (archive), visibility toggle | **P1** |
| Search | in-header, listbox, PR-aware | **none** | **P1** |
| Project treatment | filter (combobox) | hierarchy (tree tier) | **P2** (a real design fork) |
| Keyboard | Cmd+1-9 + prev/next + hint overlay | none | **P2** |
| Context menu | 15 items, shared table, capability-gated | 3 items | **P2** |
| Confirmation | settings-gated, names the thread | none on delete | **P1** (correctness) |
| Empty states | 3 variants, scope-aware | 1 | **P3** |
| Tooltip overflow valve | rich 7-line hover card | `title=` attribute | **P2** |
| Notifications | sound + desktop + toast + badge | attention store only | **P3** |
| Unread / seen | `lastVisitedAt` vs `completedAt` | none | **P2** |
| Rename from the rail | double-click + menu | **none** | **P2** |
| Row pitch | 82px card / 36px slim | 28px, one size | — (different, see below) |

### 11.2 The big structural question: tree vs. filter

Crucible nests sessions under projects. `SessionTree.tsx:158-166` argues for it:

```
/**
 * All sessions grouped by project — worktree checkouts fold into their main
 * repo's group (`repository.root`), with the branch shown as a row chip
 * instead of a tree level (branch is a FILTER, not a hierarchy: user call).
 */
```

T3 makes the *same* call one level up: **project is a filter, not a hierarchy.**
Its reasoning (`Sidebar.tsx:2333-2337`) is that scoping "filters the list without
making the header width depend on the number or length of project names."

I do **not** recommend ripping out Crucible's tree. It is well-argued, it is
already built, sticky headers work, and per-project "New session" is genuinely
better than a panel-wide button. But it costs three things T3 gets for free:

- Every row loses 24px of width to `pl-6` indent, in a rail that is 250px wide.
- Sessions cannot be ranked globally by what needs you — ranking is per-group.
- The project name is *above* the row, so a scrolled row has no project on it
  unless the sticky header is in view.

**Recommendation (P2):** keep the tree, but add a **project scope combobox** to
a new sidebar header, mirroring T3's. When a scope is active, collapse the tree
to a flat list (no indent, no group headers) because the scope already answers
"which project". Crucible already half-does this — `SessionTree.tsx:303-320`
scopes to `currentProjectPath` — but the control that sets it is elsewhere and
the scoped view still draws group headers.

### 11.3 P1 — Row anatomy

**Today** (`SessionTree.tsx:29-57`):

```tsx
class={`group relative flex items-center gap-2 w-full h-(--cru-row-sm) pl-6 pr-2 rounded transition-colors cursor-pointer ${
  props.selected
    ? 'bg-primary/10 text-shell-ink'
    : 'hover:bg-hover-wash text-shell-body'
}`}
```

28px tall, one line: `dot - title - [kiln] - [branch chip] - age`. Three
problems.

**(a) The row cannot say what the session is doing.** A 7px dot with three
values is the entire status channel. `working` is not even reliable — the
comment in `lib/session-status.ts:20-30` admits it: *"Streaming is reported by
mounted chat panels only, so a session running under another client reads as
`idle` here."* T3's answer is a **word** plus an **elapsed counter**, which is
both legible and self-verifying.

**(b) The branch chip is a bordered pill.** `SessionTree.tsx:71-79`:

```tsx
<span class="shrink-0 inline-flex items-center gap-1 px-1 rounded bg-surface-elevated border border-hairline text-floor text-muted-dark">
  <GitBranch class="w-2.5 h-2.5 shrink-0" />
  <span class="truncate max-w-[80px]">{b}</span>
</span>
```

A fill + a border + an icon + 11px text, all to say "feat/x". T3 renders the
branch as bare text at `text-muted-foreground/40` with no box. In a 250px rail
the chip's chrome costs more than the information it carries.

**(c) 80px caps on both kiln and branch.** At 250px rail width, `pl-6` indent +
dot + 80px kiln + 80px branch + 32px age leaves the title roughly 40px. The
title — the only thing that identifies the session — is the first casualty.

**Recommendation:** adopt the two-variant model. A **card** for live/inbox
sessions, a **slim** row for archived, settled and reflections.

Proposed card, ASCII (250px rail):

```
+----------------------------------------------+
| [/] crucible                  (o) Working 2m14s|  20px, gap 6px
| Fix transcript wikilink resolution            |  13px, medium, truncate
| Y feat/wikilinks                    [k] docs M|  11px, muted/40
+----------------------------------------------+
   72px content + 2px pad = 74px pitch

   line 1: project glyph + name .......... status word + elapsed
   line 2: session title (the only 13px text in the row)
   line 3: branch (mono, 60% ink) ........ kiln chip (only if odd) + agent glyph
```

Proposed slim (archived / reflections / settled):

```
| [/] Fix transcript wikilink resolution     3d |  28px, --cru-row-sm
```

JSX-ish, with Crucible's own token names:

```tsx
// ── card ────────────────────────────────────────────────────────────
<li class="list-none py-px [content-visibility:auto] [contain-intrinsic-size:auto_72px]">
  <div
    role="button" tabindex="0"
    data-testid={`session-item-${s.id}`}
    data-session-id={s.id}
    class={cn(
      "group/session-row relative w-full cursor-pointer overflow-hidden rounded",
      "text-left outline-none select-none transition-colors",
      selected   ? "bg-primary/10 text-shell-ink"
      : recedes  ? "text-muted-dark hover:bg-hover-wash hover:text-shell-ink"
                 : "bg-transparent text-shell-body hover:bg-hover-wash",
    )}
  >
    <div class="relative h-[4.5rem] px-2.5 py-2">

      {/* line 1 — project + status/action swap slot */}
      <div class="flex h-5 min-w-0 items-center gap-1.5">
        <FolderGit2 class="w-3.5 h-3.5 shrink-0 text-muted-dark" />
        <span class="min-w-0 flex-1 truncate text-floor text-muted font-medium">
          {projectName}
        </span>
        <Show when={pinned}><Pin class="w-3 h-3 shrink-0 text-muted-dark" /></Show>

        {/* status at rest — goes absolute + transparent on hover */}
        <span class="pointer-events-none flex items-center tabular-nums text-floor
                     text-muted-dark transition-opacity
                     group-hover/session-row:absolute group-hover/session-row:right-0
                     group-hover/session-row:opacity-0">
          <Show when={status} fallback={age()}>
            <span class={cn("inline-flex items-center gap-1 font-medium", STATUS[status].class)}>
              <StatusIcon class="w-3.5 h-3.5 shrink-0" />
              <span role="status">{STATUS[status].label}</span>
              <Show when={status === 'working'}>
                <span aria-hidden><WorkingDuration startedAt={turnStartedAt} /></span>
              </Show>
            </span>
          </Show>
        </span>

        {/* actions on hover — absolute at rest, static on hover */}
        <span class="pointer-events-none absolute inset-y-0 right-0 flex items-stretch
                     opacity-0 transition-opacity
                     has-[:focus-visible]:pointer-events-auto has-[:focus-visible]:static has-[:focus-visible]:opacity-100
                     group-hover/session-row:pointer-events-auto group-hover/session-row:static group-hover/session-row:opacity-100">
          <button class="inline-flex cursor-pointer items-center gap-1 rounded px-1.5 text-floor
                         text-muted-dark hover:text-shell-ink" aria-label="Settle session">
            <CircleCheck class="w-3.5 h-3.5" />
            Settle
          </button>
          <button class="inline-flex cursor-pointer items-center rounded px-1.5 text-floor
                         text-muted-dark hover:text-shell-ink" aria-label="Archive session">
            <Archive class="w-3.5 h-3.5" />
          </button>
        </span>
      </div>

      {/* line 2 — title */}
      <div class="mt-1 flex min-w-0">
        <span class={cn(
          "min-w-0 flex-1 truncate text-reading",
          recedes ? "font-normal text-muted-dark" : "font-medium text-shell-ink",
        )}>{sessionDisplayTitle(s)}</span>
      </div>

      {/* line 3 — branch, kiln, agent */}
      <div class="mt-0.5 flex min-w-0 items-center gap-1.5 text-floor text-muted-dark">
        <Show when={branch} fallback={<span class="flex-1" />}>
          <GitBranch class="w-3 h-3 shrink-0 opacity-50" />
          <span class="min-w-0 flex-1 truncate whitespace-nowrap font-mono opacity-60">{branch}</span>
        </Show>
        <Show when={showKiln && kilnLabel}>
          <span class="shrink-0 inline-flex items-center gap-1">
            <FlaskConical class="w-3 h-3" /><span class="truncate max-w-[70px]">{kilnLabel}</span>
          </span>
        </Show>
        <Show when={reviewBlocked}>
          <span class="shrink-0 text-attention" title="waiting on review">
            {unreviewed} unreviewed
          </span>
        </Show>
        <span aria-hidden class="pointer-events-none ml-auto inline-flex shrink-0 items-center gap-1">
          <Show when={agentCard}><AgentGlyph class="w-3.5 h-3.5 opacity-60" /></Show>
        </span>
      </div>
    </div>
  </div>
</li>
```

```tsx
// ── slim ────────────────────────────────────────────────────────────
<li class="list-none [content-visibility:auto] [contain-intrinsic-size:auto_28px]">
  <div role="button" tabindex="0"
    class={cn(
      "group/session-row relative flex h-(--cru-row-sm) w-full items-center gap-2 rounded px-2.5",
      "cursor-pointer text-left outline-none select-none transition-colors",
      "[&:not(:hover):not(:focus-within)_*]:text-muted-dark",
      selected ? "bg-primary/10" : "hover:bg-hover-wash",
    )}>
    <FolderGit2 class="w-3.5 h-3.5 shrink-0 opacity-40 transition-opacity
                       group-hover/session-row:opacity-100" />
    <span class="min-w-0 flex-1 truncate text-reading">{sessionDisplayTitle(s)}</span>
    <span class="relative ml-auto flex h-6 min-w-8 shrink-0 items-center justify-end">
      <span class="inline-flex justify-end tabular-nums text-floor
                   group-hover/session-row:opacity-0">{age()}</span>
      <button aria-label="Un-settle session"
        class="pointer-events-none absolute inset-y-0 right-0 -mr-1 inline-flex cursor-pointer
               items-center rounded px-1.5 text-floor text-muted-dark opacity-0
               transition-opacity hover:text-shell-ink
               focus-visible:pointer-events-auto focus-visible:opacity-100
               group-hover/session-row:pointer-events-auto group-hover/session-row:opacity-100">
        <Undo2 class="w-3.5 h-3.5" />
      </button>
    </span>
  </div>
</li>
```

Two mechanics to copy exactly:

- `content-visibility: auto` + a matching `contain-intrinsic-size`. Crucible
  renders every session row of every group; this makes off-screen rows free.
- The `absolute <-> static` / `opacity-0 <-> opacity-100` swap instead of
  Crucible's current `group-hover:invisible` on the age plus a separately
  positioned absolute action cluster. Crucible's version is close but the age
  column reserves a fixed `w-8` forever; T3's slot lets the *visible* state own
  the width, so the project/title reclaims it.

### 11.4 P1 — Status vocabulary

**Today**: `SessionStatusDot.tsx` — 7px, three values, fill-vs-ring first, hue
second. The accessibility reasoning is excellent and should be kept. But it
cannot carry six states, it cannot say *how long*, and it is invisible at a
glance across a 40-row list.

**Recommendation:** promote the dot to a **labelled status** on card rows; keep
the bare dot only on slim rows and in the command palette.

Concretely, extend `lib/session-status.ts`:

```ts
export type SessionStatus =
  | 'approval'   // a tool write is gated on review / permission
  | 'input'      // the agent asked a question
  | 'working'    // a turn is in flight
  | 'failed'     // the last turn errored
  | 'done'       // completed since you last looked
  | 'idle';      // resting, seen

export const STATUS_PRESENTATION: Partial<Record<SessionStatus,
  { label: string; icon: Component; class: string }>> = {
  approval: { label: 'Approval', icon: ShieldQuestion,        class: 'text-attention' },
  input:    { label: 'Input',    icon: MessageCircleQuestion, class: 'text-precog' },
  working:  { label: 'Working',  icon: CircleDashed,          class: 'text-ok' },
  failed:   { label: 'Failed',   icon: CircleAlert,           class: 'text-error' },
  done:     { label: 'Done',     icon: CircleCheck,           class: 'text-ok' },
  // idle: absent — the row falls through to its timestamp
};
```

Crucible's palette already has exactly the hues T3 uses, under better names, and
`index.css:195-205` makes the same "one colour, one meaning" argument:

```
 *   attention — something is waiting on YOU
 *   ok        — the machine is busy, and it is fine
 *   primary   — this is the one you are on (selection and binding)
```

`attention` (#d4a72c) is T3's amber approval. `ok` (#7bc47f) covers working and
done. `precog` (#a78bda) is a natural fit for `input` — a question is
speculative. `error` (#ef4444) is failed. **No new tokens needed.**

Add the elapsed counter. Crucible knows `isStreaming` from the chat panel, and
the session record carries `last_activity`; a `startedAt` for the running turn
is the one missing field, and `last_activity` is a serviceable fallback.

And add the recede rule, which is the highest-leverage single function in T3:

```ts
/** A working session RECEDES. Background work must not outrank a question. */
export function shouldRecedeSessionRow(input: {
  status: SessionStatus;
  isUnread: boolean;
  isActive: boolean;
}): boolean {
  if (input.isActive || input.status === 'input') return false;
  if (input.status === 'working') return true;
  if (input.status === 'idle' || input.status === 'approval') return !input.isUnread;
  return false;
}
```

Also: **no pulse on the status.** Crucible's one animated status today is
`SessionStatusChips.tsx`'s `animate-pulse` dot on "waiting on review". T3's
`animate-status-pulse` is duty-cycled with `steps(6)` precisely so a rail full
of them does not repaint every vsync. If Crucible keeps a pulse anywhere, copy
the stepped keyframe:

```css
@keyframes cru-status-pulse {
  0%, 40%  { opacity: 1;   animation-timing-function: steps(6); }
  50%, 90% { opacity: 0.5; animation-timing-function: steps(6); }
  100%     { opacity: 1; }
}
```

### 11.5 P1 — Search

Crucible's sessions rail has none. With an Inbox capped at 5 and everything else
behind project folds, finding a session by name means unfolding groups by hand.

**Recommendation:** add `SessionsPanelHeader` modelled on `SidebarThreadHeader`:

```tsx
<div class="flex items-center gap-1 px-2 pt-1 pb-2">
  <div ref={searchFieldRef}
    class="flex h-8 min-w-0 flex-1 items-center gap-2 rounded px-2
           text-reading text-muted-dark hover:bg-hover-wash hover:text-shell-ink">
    <Search class="w-4 h-4 shrink-0 text-muted-dark" />
    <input type="search" placeholder="Search" aria-label="Search sessions"
      role="combobox" aria-autocomplete="list"
      aria-expanded={resultsVisible()}
      aria-controls={resultsVisible() ? 'session-search-results' : undefined}
      aria-activedescendant={activeResultExists() ? `session-search-result-${activeIndex()}` : undefined}
      class="min-w-0 flex-1 bg-transparent outline-none text-reading
             text-shell-ink placeholder:text-muted-dark" />
    <Show when={query()}>
      <button aria-label="Clear search" class="shrink-0 rounded p-0.5
        text-muted-dark hover:bg-hover-wash hover:text-shell-ink"><X class="w-3 h-3" /></button>
    </Show>
  </div>
  <div class="flex shrink-0 items-center">
    <ScopeCombobox anchor={searchFieldRef} />          {/* FolderGit2 -> project icon */}
    <IconButton label="New session"><SquarePen class="w-4 h-4" /></IconButton>
  </div>
</div>
```

While searching, replace the whole body with a flat `role="listbox"` of slim
rows — project glyph, title, age. Preserve section order, not relevance order.
Match title, project name and branch. Empty:

```tsx
<p role="status" class="px-2 py-6 text-center text-floor text-muted-dark">No sessions found</p>
```

This also gives Crucible somewhere to put the new-session button, which today is
a bare `window.dispatchEvent` reachable only from the empty state and from a
project group header.

### 11.6 P1 — Delete has no confirmation

`SessionTree.tsx:339-343`:

```ts
if (value === 'open-session') props.onSelectSession(t.session.id);
else if (value === 'archive-session') props.onArchiveSession(t.session.id);
else if (value === 'delete-session') props.onDeleteSession(t.session.id);
```

A `delete-session` menu pick destroys a transcript with no prompt. T3 gates it
behind a named confirm that states the consequence. This is a correctness fix,
not a polish one.

```ts
else if (value === 'delete-session') {
  const ok = await confirmDialog({
    title: `Delete session "${sessionDisplayTitle(t.session)}"?`,
    body: 'This permanently clears the transcript for this session.',
    variant: 'destructive',
  });
  if (ok) props.onDeleteSession(t.session.id);
}
```

Make it a setting (`confirmSessionDelete`, default on) the way T3 does, so a
power user can turn it off rather than route around it.

### 11.7 P2 — Lifecycle sections instead of a recency Inbox

Crucible's Inbox is "newest 5" (`lib/session-inbox.ts:26-30`). The file's own
comment records that it *used* to be status-based and was changed because it went
empty on a quiet morning. T3 solves the same problem differently: the
**Active** section is never empty because membership is user-controlled
(a session leaves only when you settle, snooze, archive or delete it), and
ordering is stable so the row you were on stays put.

**Recommendation:** rename and re-scope.

| Crucible today | Proposed |
|---|---|
| Inbox (newest 5) | **Active** — every non-archived, non-settled session; ordered by a stable key, not recency |
| *(none)* | **Settled** — collapsed, counted, slim rows; a manual "I'm done with this for now" |
| Archived | keep, collapsed, counted, slim |
| Reflections | keep |

Settle is cheaper than archive (reversible, still in the list, one hover click)
and it is what makes an inbox reach zero. Crucible's `archiveSession` is close
but it is a one-way trip to a fold with `opacity-60`.

If a daemon-side `settled` flag is too much for a first pass, a client-side
`settledUntilActivity` set in localStorage keyed by session id, cleared on any
new message, gets most of the value.

Shelf header, matching T3's shape, using Crucible's `TreeSection` as the base but
changing the visual from uppercase-label-plus-count to label-rule-chevron:

```tsx
<button type="button" aria-expanded={props.open} onClick={props.onToggle}
  class="mx-0.5 flex h-8 w-full cursor-pointer items-center gap-2 px-2 text-left
         text-floor font-medium text-muted-dark hover:text-shell-body">
  <span class="shrink-0">{props.open ? props.label : `${props.label} (${props.count})`}</span>
  <span aria-hidden class="h-px min-w-2 flex-1 bg-hairline" />
  <ChevronDown class={`w-3 h-3 shrink-0 transition-transform ${props.open ? 'rotate-180' : ''}`} />
</button>
```

Three changes from today's `TreeSection`:

- The count appears **only when collapsed**. Expanded, the rows say how many.
- `uppercase tracking-wide font-semibold` goes away. Today's header
  (`'... text-floor font-semibold uppercase tracking-wide text-muted-dark'`)
  is louder than the rows it heads.
- The hairline rule replaces the right-aligned count as the separator, so the
  header reads as a divider rather than as a row.

Add `mt-auto` on the first shelf header so parked work sinks to the floor of the
rail — a one-class change with a large perceptual payoff, and the same trick
T3 uses at `Sidebar.tsx:4806` and `4814`.

### 11.8 P2 — Unread / seen

Crucible has no concept of "this session finished since I last looked". T3's is
about 20 lines of client state (`lastVisitedAt` per thread id in a persisted
store) and it drives the `Done` label, the row's ink weight, and the recede rule.

```ts
// stores/uiStateStore.ts
sessionLastVisitedAtById: Record<string, string>;
markSessionVisited(id: string, at?: string): void;
markSessionUnread(id: string, before?: string): void;   // for the context menu
```

Copy T3's never-visited-counts-as-read rule verbatim, or every historical
session lights up the first time the feature ships.

Pair it with a `Mark unread` context-menu item, which T3 has and which is the
thing that makes an unread model trustworthy.

### 11.9 P2 — Context menu

Crucible's is three items (`SessionTree.tsx:105-113`). Bring it to T3's shape —
but note that Crucible already made T3's key architectural choice (one hoisted
`Menu.ContextTrigger` with a capture-phase router, `SessionTree.tsx:328-364`),
so this is purely a table change:

```ts
const SESSION_ITEMS: MenuItem[] = [
  { value: 'open-session',     label: 'Open',                      icon: MessageCircle },
  { value: 'new-here',         label: `New session in ${project}`, icon: Plus },
  { value: 'pin-session',      label: 'Pin session',               icon: Pin },
  { value: 'settle-session',   label: 'Settle session',            icon: CircleCheck, separatorBefore: true },
  { value: 'rename-session',   label: 'Rename session',            icon: Pencil,      separatorBefore: true },
  { value: 'mark-unread',      label: 'Mark unread',               icon: MailOpen },
  { value: 'copy-workspace',   label: 'Copy workspace path',       icon: Folder,      separatorBefore: true },
  { value: 'copy-branch',      label: 'Copy branch',               icon: GitBranch },
  { value: 'copy-session-id',  label: 'Copy session ID',           icon: Hash },
  { value: 'archive-session',  label: 'Archive session',           icon: Archive,     separatorBefore: true },
  { value: 'delete-session',   label: 'Delete',                    icon: Trash2, danger: true },
];
```

Missing today and cheap: **rename**. `sessionDisplayTitle` already falls back to
`Untitled . Sep 15 14:32`, so untitled sessions are common and there is no way
to fix one from the rail. Add double-click-to-rename with T3's
`isTrailingDoubleClick` guard, which Crucible does not have and will need the
moment a row has both a click action and a double-click action:

```ts
export function isTrailingDoubleClick(detail: number): boolean { return detail > 1; }
```

Also worth copying: gate items on what the daemon actually supports, per session,
rather than showing an action that will fail. T3's per-row capability check is
the pattern.

### 11.10 P2 — Keyboard

Nothing in the rail is reachable by keyboard beyond tabbing. Add:

- `Cmd+1..9` over the flat rendered order (inbox rows, then group rows in order).
- `Cmd+[` / `Cmd+]` or a configurable previous/next.
- The hold-modifier hint overlay. Copy `JumpHintBadge` almost verbatim; the only
  change is the token names:

```tsx
<span aria-hidden
  class="pointer-events-none absolute right-1.5 top-1/2 z-10 inline-flex h-5 -translate-y-1/2
         items-center rounded-full border border-hairline-strong bg-surface-overlay/95 px-1.5
         font-mono text-floor font-medium tracking-tight text-shell-ink shadow-sm">
  {label}
</span>
```

Keep the 200ms delay before showing it.

### 11.11 P2 — Hover card instead of `title=`

`SessionTree.tsx:35`:

```tsx
title={props.session.kilns.length ? `kilns . ${props.session.kilns.join(', ')}` : undefined}
```

A native `title` is slow (about 1s), unstyleable, and cannot show the seven
facts a session has. Crucible already has the popover machinery
(`@ark-ui/solid`, `ChipSelect`, `WikilinkHoverPreview`). Port
`SidebarThreadTooltip`'s structure:

```
title                                  text-reading font-medium text-shell-ink
[/] project
 Y  branch
[k] kiln, kiln, kiln                   ALL of them, not just the odd one out
 M  model / agent card
 !  waiting on review (3 unreviewed)
 !  last error
```

150ms delay, right side, `align="start"`, `max-w-80`, glass surface. This is also
where the kiln list belongs — which lets the row drop its 80px kiln column
entirely except for the odd-one-out case, freeing width for the title.

### 11.12 P2 — The rail has no header

`SessionsPanel.tsx:135-140` explicitly removed one:

```
{/* No switcher header. It was a dropdown listing sessions, sitting on
    top of a list of sessions — its one unique offer was a GLOBAL
    "active" group, and the Inbox below is that group, in the open,
    without a click. The waiting count rides the Inbox header instead. */}
```

The removal was right; the *replacement* is what T3 has and Crucible does not. A
header that holds search + scope + new-session is not a switcher — it is the
rail's toolbar. Note that `PanelHeader.tsx` exists and is unused here, and its
style (`text-sm font-semibold uppercase tracking-wide`) is wrong for this
anyway; do not reach for it.

### 11.13 P3 — Empty states

Crucible has one (`SessionsPanel.tsx:163-177`) and it only fires when there are
no projects **and** no sessions. Split it three ways like T3:

```tsx
<Show when={!projects().length}>
  <EmptyState title="No projects yet" body="Add one to give an agent a workspace."
    action={{ label: 'Add project', onClick: openAddProject }} testid="sessions-empty-projects" />
</Show>
<Show when={projects().length && scopedProject() && !rows().length}>
  <EmptyState compact title={`No sessions in ${scopedProject()!.name} yet`}
    action={{ label: 'New session', onClick: () => newSessionIn(scopedProject()!.path) }} />
</Show>
<Show when={projects().length && !scopedProject() && !rows().length}>
  <EmptyState compact title="No sessions yet"
    body="Start one to give an agent a workspace and a kiln."
    action={{ label: 'New session', onClick: newSession }} testid="sessions-empty" />
</Show>
```

The scoped variant naming the project is the one that matters. `EmptyState`
already supports `compact` and an action with a `kbd` label, so this is
composition only.

### 11.14 P3 — Notifications

Crucible has `attentionStore` and `notificationStore` but no sound, no desktop
notification, and no favicon badge for the session rail. T3's
`threadNotifications.ts` is 100 lines and mostly reusable as-is, including the
canvas favicon badge for non-Electron. The four-mode setting
(`off / notifications / sound / notifications-and-sound`) is the right shape,
and the escalation ladder (sound always, toast when focused and elsewhere,
desktop notification when unfocused, `silent: true` so it never double-sounds,
`tag` so it replaces rather than stacks) is worth copying exactly.

### 11.15 P3 — Density and the type floor

Crucible's rail is denser than T3's, and that is defensible for a rail that sits
beside a file tree rather than being the app's primary navigation. Two
adjustments regardless:

- **Row gap.** Crucible uses `flex flex-col` with no gap, so rounded hover fills
  touch. Add `gap-px` on the row containers so hover reads as a discrete chip.
  `SessionsPanel.tsx:147` and `SessionTree.tsx` group bodies:
  `<div class="flex flex-col">` -> `<div class="flex flex-col gap-px">`.
- **Indent vs. width.** `pl-6` (24px) on every session row in a 250px rail is
  about 10% of the width, for a tier the sticky header already states. T3 spends
  0. Drop to `pl-4` (16px), or to `pl-2.5` with the group's own left hairline
  guide — the file tree idiom — and give the 8-14px back to the title.

Also: Crucible's `EdgePanel` defaults to 250px with `EDGE_PANEL_MIN_WIDTH = 120`.
120px is below T3's 208px floor and far below what a card row needs. If card rows
land, raise the sessions panel's own minimum to about 200px.

### 11.16 Things Crucible already does BETTER — keep them

Worth saying plainly, because a "copy T3" pass could delete them:

1. **`SessionStatusDot`'s fill-vs-ring argument.** T3 has no equivalent
   reasoning; it relies on hue plus a differing glyph. Crucible's dot is more
   robust for colour-blind readers at 7px. Keep the dot for slim rows and the
   palette; add the labelled status only to card rows.
2. **`dominantKiln` / `showKiln`.** T3 has no concept of "only show the field
   when it distinguishes this row from its siblings". That is a genuinely better
   idea than T3's unconditional environment glyph, and it should be generalised:
   apply the same rule to the branch (hide it when every sibling shares it) and
   to the project name in a scoped view.
3. **Per-group "New session".** T3's single header button has to route through a
   command-palette picker in multi-project setups. Crucible's per-project `+` is
   a shorter path. Keep it; add T3's Shift+click-to-skip-the-picker on a global
   button as well.
4. **The capture-phase context router.** `SessionTree.tsx:328-364` — one trigger,
   resolved from the event target, with a veto path. T3 does the same thing; the
   implementations converged independently, which is a good sign about both.
5. **Sticky group headers.** T3 has nothing like it because it has no groups.
   With a tree, sticky is correct, and painting `bg-shell-bg` rather than
   `bg-shell-panel` (so the header reads as no fill at all) is the right call.
6. **Design-token discipline.** `index.css`'s `--cru-*` public-name contract, the
   four-size type scale, the AA-verified ink ramp, the `rem`-not-`px` rule for
   anything carrying text, and the ban on near-duplicate status hues are all
   stricter than T3's. Do not loosen them to accommodate a copied class string —
   every recommendation above is written in Crucible's own tokens for that reason.
7. **`data-density` driving row metrics from one attribute**
   (`styles/refine-touch.css`). T3 hard-codes `h-9` and `h-[4.875rem]`. If
   Crucible adds a card variant, express it as `--cru-row-card` and let the
   density attribute scale it, rather than as a Tailwind arbitrary height.

### 11.17 Ordered work list

**P1 — the row cannot answer "does this need me, and what is it doing"**
1. `STATUS_PRESENTATION` table + six-value `SessionStatus` (`lib/session-status.ts`).
2. Card / slim two-variant `SessionRow`, with the absolute-to-static hover swap.
3. `WorkingDuration` self-ticking span.
4. `shouldRecedeSessionRow`.
5. Confirmation on delete (settings-gated).
6. Search field in a new rail header.

**P2 — the rail cannot be navigated or managed**
7. Project scope combobox; flatten the tree while scoped.
8. Settled section + settle/un-settle hover action.
9. `lastVisitedAt` unread model + `Mark unread`.
10. Full context-menu table; rename via double-click with the trailing-click guard.
11. `Cmd+1..9` + prev/next + the held-modifier hint overlay.
12. Rich hover card replacing `title=`.

**P3 — polish**
13. Three empty states.
14. Sound / desktop notification / favicon badge.
15. `gap-px` between rows; reduce indent; `content-visibility: auto`.
16. Pin sessions (the menu item exists for projects; sessions have none).
17. Raise the sessions panel minimum width to about 200px.

---

## 12. Five lines worth pinning to the wall

From the source comments, because each one is a design rule Crucible can apply
without writing any T3 code:

> Status now lives in the row content; surface is reserved for interaction
> (hover, multi-select, route). — `Sidebar.tsx:1398`

> Background work always recedes when it is not selected: an unread parent
> completion must not pull a still-working thread back into the foreground.
> — `Sidebar.tsx:1318`

> No shimmer: a label that animates forever is noise in a sidebar full of them
> (and repaints every vsync on high-refresh displays). — `Sidebar.tsx:1338`

> The visible state owns this slot's width: status at rest, actions on hover.
> Keeping the hidden state out of flow lets the project label reclaim space
> without either state overlapping it. — `Sidebar.tsx:1778`

> Density comes from users (or the auto rules) actually parking work, not from
> the sidebar second-guessing what still matters. — `Sidebar.tsx:4622`

---

## 13. Proposed structure: inbox + tree

The direction: the sessions pane becomes two stacked parts. On top an **Inbox**
listing the last ~5 sessions by recent activity, flat, no project grouping.
Below it the **sessions tree**, all other sessions grouped by project. A
detected project with zero sessions shows no expand chevron and is hidden
entirely until it has at least one session.

### 13.1 Status check — most of this already landed

I re-read the files after the direction arrived. Four of the five clauses are
already in `master` as of this reading. Recording them precisely, because the
remaining work is narrow and the rest should not be rebuilt.

| Clause | State | Evidence |
|---|---|---|
| Inbox on top, flat, no grouping | **done** | `SessionsPanel.tsx:145-164` renders `<TreeSection label="Inbox">` above `<SessionTree>` |
| Last ~5 by recent activity | **done** | `lib/session-inbox.ts`: `INBOX_SIZE = 5`, `inboxSessions` sorts `byRecency` then `.slice(0, 5)` |
| Tree holds everything else, grouped by project | **done** | `SessionsPanel.tsx:161` passes `shownAbove={inboxIds()}` |
| No chevron on a project with nothing to unfold | **done** | `SessionTree.tsx` `groupHeader`: `<Show when={g.rows.length} fallback={<span class={`${treeChevron} inline-block`} aria-hidden="true" />}>` |
| Zero-session project hidden entirely | **done** | `SessionTree.tsx:324`: `return all.filter((g) => g.sessions.length > 0);` |
| Inbox row shows more than a tree row | **NOT done** on desktop | `SessionsPanel.tsx:119-131` `row()` uses the same `SessionRow` with no project and no `showKiln` |

The mobile shell already made the row distinction the desktop has not
(`components/mobile/SessionsTab.tsx:149`):

```tsx
<For each={inbox()}>{(s) => <Row session={s} showProject />}</For>
```

...against the project list at line 167, which passes
`showProject={chosen() === ALL_PROJECTS}`. So on a phone an inbox row names its
project and on the desktop it does not. That is the single largest remaining
gap and it is also a correctness issue, not only a density one: **leaving the
tree is exactly the moment a session loses its project context**, so the inbox
row is the one row that must carry it.

One bug while I was in there. The mobile project chip uses a **branch** glyph
for a **project** name (`SessionsTab.tsx:97-107`):

```tsx
<span class="... border border-hairline text-floor text-muted-dark"
  title={`project · ${name()}`}>
  <GitBranch class="w-2.5 h-2.5 shrink-0" />
```

`GitBranch` already means "branch" on the desktop row three lines above it
(`SessionTree.tsx:71-79`, same chip shape, same classes). Two meanings on one
glyph in one app. It should be `FolderGit2`, which is what the project group
header already uses.

### 13.2 What data exists today

From `lib/types.ts:57-90` and the stores, this is the complete set a row can draw
on. Anything not in this table needs a daemon change first.

| Fact | Source | Notes |
|---|---|---|
| Title | `Session.title`, via `sessionDisplayTitle` | Falls back to `Untitled · Sep 15 14:32` |
| Last activity | `Session.last_activity ?? started_at` | ISO; `touchedAt()` parses NaN-safe to 0 |
| Project | `sessionWorkspace(s)` -> path -> `Project` | Longest-prefix match in `SessionTree.groupFor` |
| Branch | `listWorkspaceTargets(repoRoot)` | Fetched per repo root in `SessionsPanel.loadBranches`, refreshed on window focus |
| Kilns | `Session.kilns` (names) | `sessionDefaultKiln` returns `kilns[0]` or null |
| Waiting on you | `attentionStore.get(id)?.pendingInteraction` | **Global** — daemon aggregate polled every 10s, so it works with no tab open |
| Streaming | `attentionStore.get(id)?.isStreaming` | **Local only** — reported by mounted `ChatProvider`s |
| Lifecycle | `Session.state`: active / paused / compacting / ended | Only `InboxPanel` uses it today |
| Model | `Session.agent_model` | Unused on the rail |
| Mode | `Session.agent_mode` | Unused on the rail |
| Event count | `Session.event_count` | Unused on the rail |
| Archived | `Session.archived` | |
| Type | `Session.session_type` — `plugin` = a reflection pass | |
| Review gate | `reviewStore.session(id).gate` | Per-session, fetched for the ACTIVE session only |

Two limits matter for the design below:

1. **There is no "unread" and no "turn started at".** `hasUnseenCompletion` and
   `WorkingDuration` from section 11 both need client state that does not exist
   yet. Everything in this section is designed to work without them, and to
   accept them later without restructuring.
2. **`working` is unreliable across clients.** `lib/session-status.ts:20-30`
   states it: a session running under another client reads as `idle`. So the
   inbox must not be a *status* filter — a status filter over a status that can
   be wrong is worse than recency. This is an independent argument for the
   direction's recency rule.

### 13.3 How T3 decides what sits at the top

Three mechanisms, and only the first is a good fit for Crucible.

**(a) Unsent drafts float above everything.** `SidebarDraftBlock`
(`Sidebar.tsx:807-930`) renders above the pinned block, then a 1px divider:

```tsx
<li aria-hidden data-testid="sidebar-draft-divider"
  className="mx-2.5 my-1.5 h-px list-none bg-sidebar-border/60" />
```

The principle is "work you started and did not finish outranks work the machine
is doing". Crucible has the same shape available: `CenterComposer` drafts.

**(b) Pinned threads, user-chosen, above active.** Manual, persistent, never
automatic.

**(c) Active order is explicitly NOT recency.** This is the one that cuts
against the direction, so it deserves the exact quote
(`apps/mobile/src/features/threads/threadListV2.ts:158-160`):

```
/** The active order shared by web and native: new/reopened rows, then the
    saved arrangement. Activity does not move a thread. */
```

T3 refuses to let a list reorder under the pointer. A row you are aiming at
must still be there when the click lands.

**This is a real tension with "last ~5 by recent activity", and it is worth
naming rather than papering over.** Crucible's `inboxSessions` re-sorts on every
`sessions()` update. An agent finishing a turn in session B while the pointer is
over session A's archive button moves A down one slot and the click lands on the
wrong session. With a 5-row list and ~10s attention polling plus SSE-driven
activity, this is not theoretical.

I do not recommend abandoning recency — "where was I" is the Inbox's whole job
and the file comment already argues this well
(`lib/session-inbox.ts:13-24`). I recommend bounding the churn. See 13.5.

### 13.4 What counts as an inbox entry

Keep the current membership predicate. It is right, and the reasoning already in
`lib/session-inbox.ts` is the reasoning I would write:

```ts
export function inboxSessions(sessions: readonly Session[]): Session[] {
  return sessions
    .filter((s) => !s.archived && s.session_type !== 'plugin')
    .sort(byRecency)
    .slice(0, INBOX_SIZE);
}
```

**Recommendation on the cutoff: plain `INBOX_SIZE = 5`, no time window.**

The direction offered "active or touched in last N hours, max 5" as an
alternative. Against it:

- That is the rule this file *used to have* and it was removed for a stated
  reason: *"it was empty on a quiet morning and it dropped the session the user
  was in a minute ago as soon as the agent went idle."* Re-adding a staleness
  bound re-creates both failures.
- A fixed-5 list has a fixed height. The rail's top block never jumps between 0
  and 5 rows as time passes with no user action. A time-windowed list changes
  size while nobody is looking at it, which is the worst kind of layout change.
- "Active" as a membership term would mean `sessionStatus(s) !== 'idle'`, and
  13.2 established that `working` is wrong for any session without an open tab.
  Membership would then depend on which tabs happen to be open.
- The cost of no window is a stale row at position 5 on a quiet Monday. That row
  is still the right answer to "where was I" — it is literally where you were.

Two additions to the predicate, both small, both taken from T3's "the open thread
never hides" rule:

```ts
export function inboxSessions(
  sessions: readonly Session[],
  opts: { currentSessionId?: string | null; attentionIds?: ReadonlySet<string> } = {},
): Session[] {
  const eligible = sessions
    .filter((s) => !s.archived && s.session_type !== 'plugin')
    .sort(byRecency);

  // Anything blocked on a human is in the Inbox regardless of rank. This is a
  // GLOBAL signal (the daemon aggregate), so unlike `working` it is trustworthy
  // for a session with no open tab.
  const waiting = eligible.filter((s) => opts.attentionIds?.has(s.id));
  const rest = eligible.filter((s) => !opts.attentionIds?.has(s.id));

  const picked = [...waiting, ...rest].slice(0, INBOX_SIZE);

  // The session you have open never falls off the Inbox into a collapsed tree
  // group. Same exception T3 makes for its route thread in a collapsed shelf.
  const current = opts.currentSessionId
    ? eligible.find((s) => s.id === opts.currentSessionId)
    : undefined;
  if (current && !picked.some((s) => s.id === current.id)) {
    return [...picked.slice(0, INBOX_SIZE - 1), current];
  }
  return picked;
}
```

The `waiting`-first clause is the one place I would let status beat recency, and
only because `pendingInteraction` comes from the daemon aggregate rather than
from a mounted tab. It also makes `TreeSection`'s existing `urgent` flag
truthful: `waitingCount()` can no longer be non-zero for a session the Inbox is
not showing.

### 13.5 Ordering rule

**Order by recency, but freeze the order while the user is interacting.**

Recency inside the Inbox is correct — it is the section's meaning. The problem
is only *when* the reorder is applied. One signal, one guard:

```ts
// SessionsPanel.tsx
const [interacting, setInteracting] = createSignal(false);
// Frozen order survives while the pointer is in the panel or focus is inside
// it. Membership changes still apply (a new session appears); only the
// arrangement of rows already on screen is held.
const inboxOrder = createMemo<string[]>((prev) => {
  const next = inboxSessions(activeList(), { ... }).map((s) => s.id);
  if (!interacting() || prev === undefined) return next;
  const held = prev.filter((id) => next.includes(id));
  return [...held, ...next.filter((id) => !held.includes(id))];
});
```

wired with `onPointerEnter` / `onPointerLeave` and `onFocusIn` / `onFocusOut` on
the Inbox container. This is the smallest change that makes a click land where
it was aimed, and it costs one signal and one memo.

The alternative — copying T3 wholesale and giving sessions a stable
`activeOrderKey` — is a daemon change and a much bigger idea (it implies
drag-to-reorder, pinning, and a "settled" lifecycle). Section 11.7 already
proposes that as P2. The freeze-while-interacting guard is the P1 version of the
same protection and does not block the larger design later.

Between the two parts, ordering is already consistent: the tree orders groups by
`lastActivity` descending (`SessionTree.live()`) and rows inside a group by
`byRecency`. So recency governs both halves, which is right — the Inbox is a
*cut* of one ordered list, not a different list.

### 13.6 Does an inbox session also appear in the tree?

**No. One session, one row.** Keep the current `shownAbove` behaviour.

This is already implemented and it is the right call, so the argument here is to
record why, so it is not reverted:

- A duplicated row makes the two counts disagree with the rail: the Inbox says
  5, the project header says 3, and there are 6 rows on screen for 5 sessions.
- Selection has to highlight both copies or the user sees a selected row and an
  identical unselected one.
- Every per-row action (archive, delete) appears twice for one object.

The one cost is that a project's own list is incomplete while its newest session
sits in the Inbox. The tree already handles this correctly, and the comment at
`SessionTree.tsx:172-178` states it:

```
 * Ids the Inbox above already lists. The tree does not repeat them: a row
 * drawn twice on one rail is one row too many. Their project still shows,
 * so New Session and the context menu stay reachable there — without a
 * chevron when nothing is left under it to unfold.
```

That is precisely the right compromise: the **group** counts every session
(`g.sessions`), the **rows** count only what is not shown above (`g.rows`), the
chevron and the count read from `g.rows`, and the visibility filter reads from
`g.sessions`. Four decisions, three different fields, all correct.

One refinement. A project header whose sessions are *all* in the Inbox currently
shows a folder, a name, no chevron and no count — which reads as an empty
project even though it is the busiest one. Give it a muted marker:

```tsx
<Show when={g.rows.length} fallback={
  <Show when={g.sessions.length}>
    <span class="text-muted-dark font-normal tabular-nums opacity-50"
      title={`${g.sessions.length} in Inbox`}>{g.sessions.length}</span>
  </Show>
}>
  <span class="text-muted-dark font-normal tabular-nums">{g.rows.length}</span>
</Show>
```

### 13.7 How a session moves between the two

Four transitions, all automatic, none animated today:

| Event | Effect |
|---|---|
| New session created | Enters Inbox at rank 0; the 5th falls into its project group |
| Message sent / agent event | `last_activity` advances; session rises within the Inbox, or re-enters from the tree |
| Another session gets newer activity | Session drops a rank, then falls out at rank 6 |
| Archive | Leaves both, joins the Archived fold |

The fall-out is the one the user can be surprised by: a row they were looking at
vanishes from the top block and reappears 200px down inside a project group that
may be collapsed. Two mitigations, both cheap:

1. The `currentSessionId` clause in 13.4 — the open session never falls out.
2. **Auto-expand the group holding the current session.** This gap exists today
   independently of the Inbox: `SessionTree` persists `collapsed` in
   localStorage and has no exception for the selected row, so selecting a
   session from the command palette can leave it invisible inside a collapsed
   group, or inside the collapsed "Other projects" fold. T3 makes exactly this
   exception twice (`Sidebar.tsx:2662-2680` and `2738-2751`). Fix:

```ts
// SessionTree.tsx — a group holding the current session is never collapsed.
const isCollapsed = (g: SessionGroup) =>
  collapsed().has(g.key) &&
  !g.rows.some((s) => s.id === props.currentSessionId);
```

and use `isCollapsed(g)` at both `<Show when={!collapsed().has(g.key) && ...}>`
call sites, plus force `idleOpen` true when the current session is in `offScope`.

### 13.8 Headers

Today both sections use the same `TreeSection`:

```tsx
class="w-full flex items-center gap-1 px-2 pt-3 pb-1 text-floor font-semibold uppercase tracking-wide text-muted-dark hover:text-shell-body"
```

Three problems, all noted in section 11.7 and all sharpened by the two-part
structure:

- `font-semibold uppercase tracking-wide` at 11px is **louder than the 13px
  session titles it heads**. The heading outshouts the content.
- The count sits hard right in the same weight as the label, so the eye reads
  "INBOX ... 5" as two competing items rather than one heading.
- `<Show when={props.count > 0}>` hides the Inbox header entirely at zero. On a
  fresh install the rail's top block silently does not exist, and the user
  never learns there is an Inbox.

**Proposed: label, hairline rule, count, chevron — T3's shape in Crucible's
tokens.** Same component, same props, new class strings:

```tsx
export const TreeSection: Component<{...}> = (props) => (
  <Show when={props.count > 0 || props.persistent}>
    <button
      type="button"
      data-testid={props.testid}
      aria-expanded={props.open}
      onClick={props.onToggle}
      class="mx-0.5 mt-2 flex h-7 w-full cursor-pointer items-center gap-2 px-2 text-left
             text-floor font-medium text-muted-dark hover:text-shell-body"
    >
      <span class="shrink-0">{props.label}</span>
      <span aria-hidden class="h-px min-w-2 flex-1 bg-hairline" />
      <Show when={!props.open || props.urgent}>
        <span class="shrink-0 tabular-nums"
          classList={{ 'text-attention': props.urgent === true }}>{props.count}</span>
      </Show>
      <ChevronDown class={`w-3 h-3 shrink-0 transition-transform ${props.open ? 'rotate-180' : ''}`} />
    </button>
    <Show when={props.open}>{props.children}</Show>
  </Show>
);
```

Four changes:

- `font-medium`, no uppercase, no tracking. Quieter than the rows, as a heading
  in a dense list should be.
- Hairline rule eats the slack, so the header reads as a divider.
- Count only when **collapsed** (T3's rule — expanded, the rows say how many) or
  when **urgent** (a waiting count must show even while open, because it is the
  reason to look).
- `persistent` prop so the Inbox keeps its header at zero with an empty body,
  while Reflections and Archived keep vanishing. The Inbox is structure; the
  other two are contents.

The project group header needs no change. It is already quieter than the Inbox
header (`treeGroupRow` = `text-muted text-xs font-medium`), which after this
change makes the two headers finally consistent in weight — one at `text-floor`
for a section, one at `text-xs` for a group.

Vertical rhythm for the whole rail:

```
[header: search + scope + new]        32px   (section 11.5, not yet built)
Inbox ──────────────── 5  v            28px   mt-2
  inbox row                            40px   card-lite, see 13.9
  inbox row                            40px
  inbox row                            40px
Sessions ─────────────── v             28px   mt-2   (new label for the tree)
  > crucible            3              28px   sticky
    session row                        28px
    session row                        28px
  > docs                1              28px   sticky
    session row                        28px
Other projects ──────── 4  v           28px   mt-2   collapsed
Reflections ─────────── 2  v           28px   mt-2   collapsed
Archived ────────────── 9  v           28px   mt-2   collapsed
```

The tree currently has **no header at all** — it starts with a bare project row.
Give it one (`Sessions`, or `Projects`), non-collapsible, so the two parts read
as siblings rather than as "a section, then some loose rows".

### 13.9 Row anatomy: inbox row vs. tree row

The direction is right that the inbox row can show more. The reason is
structural: **the tree row inherits its project from the header above it; the
inbox row has no header, so it must carry its own.** That is the whole
difference, and it should be the only difference — same heights within reason,
same tints, same hover mechanics, so the rail reads as one list with two
sections rather than two widgets.

**Tree row — unchanged, 28px.** `--cru-row-sm`, indented under its project.

```
|  [.] Fix the transcript wikilink resolution      [docs] [Y main]  3d |
   ^   ^                                            ^      ^        ^
   dot title (flex-1, truncate)                    kiln   branch   age
   (kiln only when it differs from the group's dominant kiln)
```

**Inbox row — two lines, 40px.** Not the 74px card from section 11.3: the Inbox
is 5 rows of a rail that also has to show a tree, and 5 × 74px = 370px is the
whole panel. Two lines at 40px is the compromise that fits the project name and
the status word without pushing the tree off screen.

```
+----------------------------------------------------+
| [.] crucible                      (o) Waiting   2m  |  16px line
|     Fix the transcript wikilink resolution          |  18px line
+----------------------------------------------------+
  ^   ^                              ^          ^
  dot project (truncate)             status     age
```

```tsx
/**
 * An Inbox row. Two lines because it left the tree: the project name is the
 * context the group header would have given it, and nothing else on the rail
 * can supply it. Everything else is the tree row's vocabulary at the same
 * sizes, so the two sections read as one list.
 */
export const InboxSessionRow: Component<{
  session: Session;
  selected: boolean;
  projectName: string | null;
  branch: string | null;
  onSelect: () => void;
  onArchive: () => void;
}> = (props) => {
  const status = () => sessionStatus(props.session);
  const age = () => terseAge(props.session.last_activity ?? props.session.started_at);
  return (
    <div
      role="button" tabindex="0"
      data-testid={`session-item-${props.session.id}`}
      data-session-id={props.session.id}
      onClick={props.onSelect}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); props.onSelect(); }
      }}
      class={`group relative flex h-10 w-full cursor-pointer flex-col justify-center gap-px
              rounded px-2.5 transition-colors ${
        props.selected ? 'bg-primary/10 text-shell-ink' : 'hover:bg-hover-wash text-shell-body'
      }`}
    >
      {/* line 1 — project, then the status/age swap slot */}
      <div class="flex h-4 min-w-0 items-center gap-1.5">
        <SessionStatusDot status={status()} />
        <span class="min-w-0 flex-1 truncate text-floor text-muted">
          {props.projectName ?? 'Session folder'}
        </span>

        {/* status word at rest, hidden under the hover actions */}
        <span class="pointer-events-none flex shrink-0 items-center gap-1 text-floor
                     tabular-nums text-muted-dark transition-opacity
                     group-hover:absolute group-hover:right-2.5 group-hover:opacity-0
                     group-focus-within:opacity-0">
          <Show when={STATUS_WORD[status()]}>
            {(word) => <span class={STATUS_WORD_CLASS[status()]}>{word()}</span>}
          </Show>
          <span>{age() ?? ''}</span>
        </span>

        {/* actions — out of flow at rest, in flow on hover, so nothing shifts */}
        <span class="pointer-events-none absolute inset-y-0 right-1 flex items-center gap-0.5
                     opacity-0 transition-opacity
                     group-hover:pointer-events-auto group-hover:opacity-100
                     group-focus-within:pointer-events-auto group-focus-within:opacity-100
                     [@media(hover:none)]:pointer-events-auto [@media(hover:none)]:opacity-100">
          <button type="button" aria-label={`Archive ${sessionDisplayTitle(props.session)}`}
            title="Archive session"
            class="rounded p-1 text-muted-dark transition-colors hover:bg-hover-wash hover:text-shell-ink"
            onClick={(e) => { e.stopPropagation(); props.onArchive(); }}>
            <Archive size={14} />
          </button>
        </span>
      </div>

      {/* line 2 — the title, at the tree row's size and weight */}
      <span class="min-w-0 truncate text-reading">{sessionDisplayTitle(props.session)}</span>
    </div>
  );
};
```

with

```ts
// Words, not just a dot — the Inbox is where a glance has to answer
// "does this need me". Only two states have a word: `idle` falls through to
// the timestamp, which is what "where was I" actually wants to know.
const STATUS_WORD: Record<SessionStatus, string | null> = {
  waiting: 'Waiting',
  working: 'Working',
  idle: null,
};
const STATUS_WORD_CLASS: Record<SessionStatus, string> = {
  waiting: 'text-attention font-medium',
  working: 'text-ok font-medium',
  idle: '',
};
```

Four decisions inside that, each with a reason:

- **No kiln, no branch on the inbox row.** Two lines already carry project +
  status + age + title. Adding a 70px kiln chip and an 80px branch chip to a
  250px rail would truncate the title to nothing, which is the failure mode
  section 11.3 identified on the tree row. The branch belongs on the tree row,
  where the project is implicit and there is width to spare; it belongs in the
  hover card everywhere.
- **The dot stays.** It is on line 1 beside the project, doing what
  `SessionStatusDot`'s doc comment says it does, and the word beside it is
  redundancy rather than replacement — which is the point for a colour-blind
  reader at 7px.
- **`idle` has no word.** A word on every row is no signal at all. Falling
  through to the age is what the Inbox is for.
- **Same hover-swap mechanic as the tree row**, which already has it
  (`group-hover:invisible` on the age). Upgrade both to the
  `absolute` ↔ in-flow swap from section 11.3 at the same time, so the age
  column stops reserving `w-8` forever.

Indent: the inbox row takes `px-2.5`, **not** the tree row's `pl-6`. It has no
parent, so it must not be indented as though it had one. That is 14px of title
width recovered on every inbox row, and it makes the two sections visually
distinguishable without any other device.

### 13.10 The empty-project rule, restated

Already implemented; stating the full rule in one place because it is spread
across three expressions in two files.

A project is **hidden entirely** when it has zero sessions:

```ts
// SessionTree.tsx — allGroups()
return all.filter((g) => g.sessions.length > 0);
```

The doc comment gives the reason and it is worth keeping verbatim:

```
 * A project with no session is not drawn. It used to sit behind a counted
 * "No sessions" fold, but a detected directory is not a place the user works
 * yet, and a registry of twenty of them is mostly noise. It appears the
 * moment a session starts in it.
```

A project **shows its header but no chevron and no count** when it has sessions
but all of them are in the Inbox:

```tsx
<Show when={g.rows.length}
  fallback={<span class={`${treeChevron} inline-block`} aria-hidden="true" />}>
  <ChevronRight data-testid="session-group-chevron"
    class={`${treeChevron} ${collapsed().has(g.key) ? '' : 'rotate-90'}`} />
</Show>
```

The fallback is a same-width invisible spacer, so the folder icons stay in one
column whether or not a chevron is drawn. That detail is easy to lose in a
rewrite and it is the difference between a tidy column and a ragged one.

The header still renders in that case, deliberately, because **New Session and
the context menu live on the project row** — a project you cannot start work in
is a filing cabinet. Add the muted "n in Inbox" count from 13.6 so the row is
not mistaken for an empty project.

Consequence worth stating: with 5 inbox slots and a single-project setup, the
tree can legitimately render one header and zero rows. That is correct, not a
bug, and the muted count is what makes it legible.

### 13.11 Concrete code changes

Ordered by dependency. Everything is client-side; nothing here needs a daemon
change.

**1. `lib/session-inbox.ts` — membership**
- Extend `inboxSessions(sessions, opts)` with `currentSessionId` and
  `attentionIds` as in 13.4.
- Keep `INBOX_SIZE = 5` and the `byRecency` sort. Add no time window.
- Tests: waiting session outranks a newer idle one; current session is retained
  at rank 6+; a plugin session is never a member; archived is never a member.

**2. `lib/session-status.ts` — presentation**
- Add `STATUS_WORD` and `STATUS_WORD_CLASS` beside the existing `STATUS_RANK`.
  Keep them in this file, not in a component: `SessionsPanel`, `SessionsTab` and
  `InboxPanel` all need them, and `InboxPanel`'s `LIVE_LABEL` is already a third
  copy of this vocabulary that should collapse into it.

**3. `components/tree/TreeSection.tsx` — header restyle**
- New class strings from 13.8; add the `persistent` prop.
- Count renders only when collapsed or urgent.
- Tests: header present at count 0 when `persistent`; count hidden when open and
  not urgent; `aria-expanded` unchanged.

**4. `components/SessionInboxRow.tsx` — new file**
- `InboxSessionRow` from 13.9. Keep it beside `SessionTree.tsx`'s `SessionRow`
  rather than adding a `variant` prop: the two share tints and text sizes, not
  layout, and a `variant` would fork every line of the markup anyway.
- Tests: project name rendered; falls back to `Session folder` with no
  workspace; status word absent when idle; archive button does not select the
  session; keyboard Enter/Space selects.

**5. `components/SessionsPanel.tsx` — wiring**
- Pass `currentSessionId` and `attentionIds` into `inboxSessions`.
- Add `inboxOrder` freeze-while-interacting from 13.5, with
  `onPointerEnter`/`onPointerLeave`/`onFocusIn`/`onFocusOut` on the Inbox
  container.
- Render `InboxSessionRow` for inbox entries; keep `SessionRow` for
  Reflections and Archived.
- Give the Inbox `persistent`; add a non-collapsible `Sessions` label above the
  tree.
- Resolve `projectName` per inbox session. The lookup already exists in
  `SessionTree.groupFor` (longest-prefix); **extract it** to
  `lib/session-projects.ts` as `resolveProjectForWorkspace(projects, workspace)`
  and have both call it, or the two halves of the rail will disagree about which
  project a subdirectory session belongs to.

**6. `components/SessionTree.tsx` — three small fixes**
- Extract `groupFor`'s longest-prefix logic per above.
- `isCollapsed(g)` so a group holding the current session cannot be folded
  shut; force `idleOpen` when the current session is in `offScope`.
- Muted `g.sessions.length` when `g.rows.length === 0`.
- Row containers get `gap-px` (section 11.15).

**7. `components/mobile/SessionsTab.tsx` — parity + the glyph bug**
- `GitBranch` -> `FolderGit2` on the project chip.
- Adopt `STATUS_WORD` so the phone and the desktop say the same word.
- The phone's 44px rows already have the height for two lines; the same
  `InboxSessionRow` shape applies with `h-11` and `--cru-row-touch`.

**8. `components/InboxPanel.tsx` — de-duplicate the vocabulary**
- `LIVE_LABEL` and `statusLabel` are a second status vocabulary over the same
  three states. Fold them into `lib/session-status.ts`. The file comment already
  admits the problem: *"one session wore two vocabularies depending on the
  panel"* — it was fixed once for the dot and not for the words.

**Tests to update**: `components/__tests__/SessionsPanel.test.tsx` (header
markup, inbox row shape), `components/__tests__/SessionTree.test.tsx`
(`shownAbove` count fallback, collapsed-group exception),
`lib/__tests__/session-inbox.test.ts` (new opts),
`components/mobile/__tests__/SessionsTab.test.tsx` (glyph, status word).

**What NOT to do in this pass**: do not add pinning, drag-to-reorder, a settled
lifecycle, or `activeOrderKey`. Those are section 11's P2 and they change the
data model. The structure above is compatible with all of them — a `Pinned`
section slots above the Inbox and a `Settled` shelf slots below the tree with
`mt-auto` — but landing them together would make this change unreviewable.
