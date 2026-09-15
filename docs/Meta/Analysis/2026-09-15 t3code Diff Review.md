---
title: t3code Diff Review — 2026-09-15
description: The changes list, the diff body and the review chrome of T3 Code, compared with the crucible-web Changes panel, DiffViewer and review ledger, and ranked P1 to P3
tags: [meta, ux, web, design, reference, review]
status: draft
updated: 2026-09-15
---

# t3code Diff Review — 2026-09-15

Source: [T3 Code](https://github.com/pingdotgg/t3code) at commit `3efdcc52`, read on 2026-09-15.
The comparison target is `crates/crucible-web/web` at `7d96a3a81` on master.
Part of [[2026-09-15 t3code Design Reference]].


Reference tree: `t3code (checkout 3efdcc52)`
Comparison tree: `crates/crucible-web/web/src`

All paths below are relative to one of those two roots. T3 Code paths start with `apps/`; Crucible paths start with `components/`, `lib/` or `index.css`.

---

## 0. The one-paragraph summary

T3 Code does not build a diff renderer. It buys one (`@pierre/diffs`, a web component with its own shadow DOM and virtualizer) and a tree (`@pierre/trees`), then spends its whole design budget on **theming the renderer with the app's own tokens** and on **the chrome around it**: scope selection, file tree, collapse state, comment annotation, and one very carefully-built loading skeleton. The diff body itself — colors, gutters, word highlights, sticky headers, expand-context — is configured through roughly 250 lines of `unsafeCSS` injected into the viewer's shadow root. On mobile they went the opposite direction and wrote a **native canvas** (Swift `draw(_:)` / Kotlin `onDraw`) because no view hierarchy survives a 100k-row unified diff.

Crucible today hand-rolls the diff renderer (`DiffViewer.tsx`, 256 lines, `diffLines` from the `diff` package, per-line Shiki calls), and spends its design budget on **review semantics** — a hunk ledger with accept/reject/re-applied/external/superseded states that T3 Code has no equivalent of. The two codebases are almost exactly complementary: T3 Code has the better *rendering* and the better *shell placement*; Crucible has the better *review model*.

---

## 1. Where changes surface — every entry point

T3 Code surfaces file changes in **five** distinct places. There is no badge count anywhere for changes.

### 1.1 The Diff surface in the right panel

`apps/web/src/components/RightPanelTabs.tsx` treats "Diff" as one surface among eight (Browser, Terminal, Files, Diff, Pull request, Linked pull requests, Agents, Device). It is a *tab in a tab strip*, not a fixed rail. From `RightPanelTabs.tsx:363`:

```tsx
    {
      label: "Diff",
      icon: FileDiff,
      shortcut: "D",
      available: props.diffAvailable,
      disabledReason: SURFACE_UNAVAILABLE_HINTS.diff,
      onClick: props.onAddDiff,
      badgeCount: 0,
    },
```

`badgeCount: 0` is hard-coded for every surface except Agents (`badgeCount: props.liveAgentCount`). **No change count ever badges the Diff tab.** The badge treatment exists and is deliberately not used here:

```tsx
        {action.badgeCount > 0 ? (
          <span
            aria-hidden
            className="absolute -top-1.5 -right-2 flex h-3.5 min-w-3.5 items-center justify-center rounded-full bg-info px-1 text-[9px] font-semibold tabular-nums text-white"
          >
            {action.badgeCount}
          </span>
        ) : null}
```

When the right panel is empty, a keyboard-first launcher lists the surfaces with single-letter shortcuts. `D` opens the diff from anywhere outside a typing context — a `keydown` listener in the **capture phase** on `window`, so app-level handlers cannot swallow it first (`RightPanelTabs.tsx:417-432`). Highlight only appears on hover or arrow use (`const [highlight, setHighlight] = useState(-1);` with `// -1 means no highlight`).

Tab title and icon (`RightPanelTabs.tsx:614`, `:691`):

```tsx
    case "diff":
      return "Diff";
...
    case "diff":
      return <FileDiff className="size-3 shrink-0" />;
```

### 1.2 The `ChangedFilesCard` in the chat timeline — one per completed turn

`apps/web/src/components/chat/ChangedFilesTree.tsx`. Mounted from `MessagesTimeline.tsx:2079` as `AssistantChangedFilesSection`, per assistant turn. This is the primary in-conversation surface for file changes, and it is a **tree of paths with per-node +/- stats**, not a diff.

```tsx
    <div
      className="@container/changed-files mt-4 rounded-lg bg-secondary dark:bg-input/20"
      data-changed-files-state="tree"
    >
      <div
        data-changed-files-header=""
        className="sticky top-2 z-10 flex items-center justify-between gap-2 rounded-t-lg bg-secondary px-3 py-2 dark:bg-[color-mix(in_srgb,var(--input)_20%,var(--background))]"
      >
        <div className="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1 text-xs font-medium text-foreground">
          <span>
            {files.length} changed file{files.length === 1 ? "" : "s"}
          </span>
          {hasNonZeroStat(summaryStat) && (
            <DiffStatLabel
              additions={summaryStat.additions}
              deletions={summaryStat.deletions}
              layout="inline"
              className="text-xs leading-4"
            />
          )}
        </div>
```

Three things to notice:

- **`sticky top-2 z-10`** on the header. Inside a long transcript, the "N changed files · +x -y" bar pins while you scroll its file list.
- **`@container/changed-files`** — the "Open diff" button's *label* appears only past 24rem: `<span className="hidden @[24rem]/changed-files:inline">Open diff</span>`. Below that it is icon-only. Container query, not viewport query.
- The card is one flat `rounded-lg bg-secondary` surface with **no border**; the dark-mode background is a `color-mix` of the input token over the background, so it reads as a recessed well rather than a raised card.

### 1.3 The pull-request Code tab

`apps/web/src/components/pullRequest/PullRequestCodeTab.tsx` (1459 lines) — the same viewer, the same file tree, the same toolbar order, hosted against a host-backed paged diff instead of a local git diff. Its own comment explains the parity rule: *"The same controls the thread diff panel carries, in the same order, minus the ignore-whitespace toggle: that is `git diff -w` on the server, and no host's pull request diff API offers it."*

### 1.4 A user's review comment, echoed back in the transcript

`MessagesTimeline.tsx:3434` `UserMessageReviewCommentCard`. When you comment on a diff line and send the turn, the transcript shows the comment **with its own miniature diff** — a single `FileDiff` element from `@pierre/diffs/react`:

```tsx
      {renderablePatch?.kind === "files" && (
        <DiffWorkerPoolProvider>
          {renderablePatch.files.map((fileDiff) => (
            <FileDiff
              key={resolveFileDiffPath(fileDiff)}
              fileDiff={fileDiff}
              options={{
                collapsed: false,
                diffStyle: "unified",
                theme: resolveDiffThemeName(ctx.resolvedTheme),
                preferredHighlighter: PREFERRED_HIGHLIGHTER,
              }}
            />
          ))}
        </DiffWorkerPoolProvider>
      )}
```

Wrapper chrome: `"space-y-2 rounded-lg border border-border/70 bg-background/70 p-3"`, with the path on one line and `{comment.sectionTitle} · {comment.rangeLabel}` beneath it in `text-secondary-label text-[11px]`.

### 1.5 The git actions control in the chat header

`apps/web/src/components/GitActionsControl.tsx` (2016 lines), mounted at `apps/web/src/components/chat/ChatHeader.tsx:433`. This is a **split button** whose single visible label changes with repository state: `Commit` / `Commit & push` / `Commit, push & create PR` / `Publish repository` / `Pull` / `Push` / `View PR`. Notably: **it renders no dirty count and no ahead/behind readout.** Those counts exist (`aheadCount`, `behindCount`, `aheadOfDefaultCount`) but are used only as predicates to pick which verb the button shows.

### What is *not* an entry point

There is **no inline diff on a tool call**. See §6. This is the single biggest structural difference from Crucible.

---

## 2. The changed-files list

T3 Code has **two** changed-files lists with different anatomy, because they answer different questions.

### 2.1 `DiffFileTree` — the tree beside the diff

`apps/web/src/components/diffs/DiffFileTree.tsx` (236 lines) + `apps/web/src/components/diffs/diffFileTree.logic.ts` (121 lines).

The rows are **not hand-drawn**. `@pierre/trees` owns row rendering, icons, indentation and the git-status tint. T3 Code supplies four things: paths, a git status per path, a sort comparator, and a stylesheet.

```tsx
  const { model } = useFileTree({
    density: "compact",
    flattenEmptyDirectories: true,
    initialExpansion: "open",
    icons: T3_PIERRE_ICONS,
    onSelectionChange: (selectedPaths) => {
      if (syncingSelectionRef.current) return;
      const path = selectedPaths.at(-1)?.replace(/\/$/, "");
      if (path && filePathsRef.current.has(path)) onSelectFileRef.current(path);
    },
    paths: [],
    search: false,
    sort: ordering.sort,
    unsafeCSS: PIERRE_TREE_UNSAFE_CSS,
  });
```

Four design decisions worth stealing outright:

**(a) Every directory starts open.** The header comment states the reason:

```tsx
/**
 * A directory tree of the files in a diff. Every directory starts open: a diff is a short list
 * compared to a workspace, and the reader came for the files, not the folders.
 */
```

**(b) `flattenEmptyDirectories: true`** — `src/components/chat/` collapses to one row, not three.

**(c) Sort order is diff order, not alphabetical.** A folder takes the position of its first file:

```ts
/** A folder takes the position of its first file in the diff. */
export function diffFileTreePositions(paths: ReadonlyArray<string>): ReadonlyMap<string, number> {
  const positions = new Map<string, number>();
  paths.forEach((path, index) => {
    positions.set(path, index);
    let directory = "";
    for (const segment of path.split("/").slice(0, -1)) {
      directory += `${segment}/`;
      if (!positions.has(directory)) positions.set(directory, index);
    }
  });
  return positions;
}
```

**(d) A refreshed diff does not rebuild the tree.** `buildDiffFileTreeUpdates` computes the add/remove batch so the reader's open/closed folders survive an agent edit landing mid-read:

```ts
/**
 * The adds and removes that turn one set of file paths into another, so a diff that changes
 * under the reader (a new slice, a refresh after an agent edit) keeps the directories they
 * have already opened or closed instead of rebuilding the tree from scratch.
 *
 * Directories are removed only once no file needs them; a directory that gains its first file
 * is added before that file.
 */
```
Removals are deepest-first, additions shallowest-first. When the *order* of siblings changes (which mutations cannot express), it rebuilds but carries collapsed directories forward explicitly (`DiffFileTree.tsx:119-128`).

Status mapping (`diffFileTree.logic.ts:12`):

```ts
function toGitStatus(file: FileDiffMetadata): GitStatus {
  switch (file.type) {
    case "new":       return "added";
    case "deleted":   return "deleted";
    case "rename-pure":
    case "rename-changed": return "renamed";
    case "change":    return "modified";
  }
}
```

Tree header, exact class string (`DiffFileTree.tsx:169`):

```tsx
        className="flex h-10 min-h-10 shrink-0 items-center gap-1 border-b border-border/60 bg-background px-2 text-xs text-muted-foreground in-data-[preview-panel-mode=inline]:mb-3 in-data-[preview-panel-mode=inline]:h-7 in-data-[preview-panel-mode=inline]:min-h-7 in-data-[preview-panel-mode=inline]:border-b-transparent"
        data-surface-subheader
```

`h-10 min-h-10` is the app's standard subheader height, and it shrinks to `h-7` with a transparent border when the panel is inline (a `in-data-[…]` variant reading a data attribute on an ancestor). Content: the word `Files` in `px-1 font-medium text-foreground`, then `<span className="ml-auto tabular-nums">{entries.length}</span>`, then an expand/collapse-all icon button shown only when `directoryPaths.length > 0`.

The aside that hosts it (`DiffPanel.tsx:1041`):

```tsx
<aside className="flex w-[min(16rem,40%)] min-w-40 shrink-0 border-l border-border/60">
```

`w-[min(16rem,40%)] min-w-40` — 256px, but never more than 40% of the panel, never less than 160px. The PR tab uses `w-[min(20rem,40%)] min-w-48`.

The tree is **off by default** and its state persists: `const DIFF_FILE_TREE_STORAGE_KEY = "t3code.diffFileTreeOpen";`, `useLocalStorage(DIFF_FILE_TREE_STORAGE_KEY, false, Schema.Boolean)`.

### 2.2 `ChangedFilesTree` — the tree in the chat transcript

`apps/web/src/components/chat/ChangedFilesTree.tsx`. Hand-drawn, not Pierre. This one shows +/- stats **per directory as well as per file** — the tree in the diff panel does not.

Row anatomy — directory:

```tsx
          <button
            type="button"
            data-scroll-anchor-ignore
            aria-expanded={isExpanded}
            className="group flex w-full items-center gap-2 rounded-md py-1.5 pr-2 text-left transition-colors hover:bg-accent/60 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-1 focus-visible:ring-offset-background"
            style={{ paddingLeft: `${leftPadding}px` }}
            onClick={() => toggleDirectory(node.path)}
          >
            <ChevronRightIcon
              aria-hidden="true"
              className={cn(
                "size-3.5 shrink-0 text-muted-foreground/70 transition-transform group-hover:text-foreground/80",
                isExpanded && "rotate-90",
              )}
            />
            {isExpanded ? (
              <FolderIcon className="size-3.5 shrink-0 text-muted-foreground/75" />
            ) : (
              <FolderClosedIcon className="size-3.5 shrink-0 text-muted-foreground/75" />
            )}
            <span className="truncate font-mono text-[11px] text-muted-foreground/90 group-hover:text-foreground/90">
              {node.name}
            </span>
            {hasNonZeroStat(node.stat) && (
              <span className="ml-auto shrink-0 font-mono text-[10px] tabular-nums">
                <DiffStatLabel additions={node.stat.additions} deletions={node.stat.deletions} />
              </span>
            )}
          </button>
```

Row anatomy — file:

```tsx
      <button
        key={`file:${node.path}`}
        type="button"
        className="group flex w-full items-center gap-2 rounded-md py-1.5 pr-2 text-left transition-colors hover:bg-accent/60 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-1 focus-visible:ring-offset-background"
        style={{ paddingLeft: `${leftPadding}px` }}
        onClick={() => onOpenTurnDiff(turnId, node.path)}
      >
        {hasDirectoryNodes || depth > 0 ? (
          <span aria-hidden="true" className="size-3.5 shrink-0" />
        ) : null}
        <PierreEntryIcon
          pathValue={node.path}
          kind="file"
          theme={resolvedTheme}
          className="size-3.5 text-muted-foreground/70"
        />
        <span className="truncate font-mono text-xs text-foreground/85 group-hover:text-foreground">
          {node.name}
        </span>
        {node.stat && (
          <span className="ml-auto shrink-0 font-mono text-[10px] tabular-nums">
            <DiffStatLabel additions={node.stat.additions} deletions={node.stat.deletions} />
          </span>
        )}
      </button>
```

Indentation is `const leftPadding = 8 + depth * 14;` — inline style, not a class, because it is arbitrary depth. A file at depth 0 gets a `size-3.5` spacer only if the tree has directories anywhere, so a flat list of files does not carry a phantom chevron column.

Directory compaction (`apps/web/src/lib/turnDiffTree.ts:56`) — a chain of single-child directories collapses into one row named `a/b/c`:

```ts
  while (compactedNode.children.length === 1 && compactedNode.children[0]?.kind === "directory") {
    const onlyChild = compactedNode.children[0];
    compactedNode = {
      kind: "directory",
      name: `${compactedNode.name}/${onlyChild.name}`,
      path: onlyChild.path,
      stat: onlyChild.stat,
      children: onlyChild.children,
    };
  }
```

Sorting: directories before files, each group by name with `{ numeric: true, sensitivity: "base" }`.

### 2.3 `DiffStatLabel` — the +/- widget, and it has two layouts

`apps/web/src/components/chat/DiffStatLabel.tsx`. This is the single smartest small component in the reference, and it is 55 lines.

```tsx
export const DiffStatLabel = memo(function DiffStatLabel(props: {
  additions: number;
  deletions: number;
  className?: string;
  showParentheses?: boolean;
  layout?: "aligned" | "inline";
}) {
  const { additions, deletions, className, showParentheses = false, layout = "aligned" } = props;
  return (
    <>
      {showParentheses && <span className="text-muted-foreground/70">(</span>}
      <span
        role="group"
        aria-label={`${additions} additions, ${deletions} deletions`}
        className={cn(
          layout === "inline"
            ? "inline-flex items-center gap-1 tabular-nums align-middle"
            : "inline-grid grid-cols-[4ch_4ch] gap-2 text-right tabular-nums align-middle",
          className,
        )}
      >
        <span aria-hidden="true" className="font-mono text-diff-addition">
          +{formatCompactDiffCount(additions)}
        </span>
        <span aria-hidden="true" className="font-mono text-diff-deletion">
          -{formatCompactDiffCount(deletions)}
        </span>
      </span>
      {showParentheses && <span className="text-muted-foreground/70">)</span>}
    </>
  );
});
```

Three ideas:

1. **`layout="aligned"` is an `inline-grid grid-cols-[4ch_4ch]`.** In a list of rows, every `+N` starts at the same x and every `-N` starts at the same x, regardless of digit count. This is why the tree's right column reads as a column and not as ragged text. `layout="inline"` is the flex version for a header, where there is only one.
2. **Counts are abbreviated, never truncated.** `formatCompactDiffCount` gives `999`, `1.2k`, `12k`, `1.2m`, `1.2b` — so `4ch` is genuinely enough forever.
3. **`role="group"` with one `aria-label` and both spans `aria-hidden`.** A screen reader says "42 additions, 7 deletions", not "plus four two dash seven".

Semantic tokens: `text-diff-addition` and `text-diff-deletion` — dedicated diff tokens, distinct from success/error.

---

## 3. The diff view itself

### 3.1 The library

`@pierre/diffs` (catalog-pinned) and `@pierre/trees@1.0.0-beta.4`, from `apps/web/package.json`. `@pierre/diffs/react` exports `CodeView` (multi-file, virtualized) and `FileDiff` (single file, unvirtualized). The parser is `parsePatchFiles` from `@pierre/diffs/utils/parsePatchFiles`. Syntax highlighting is Shiki-compatible and runs in a **worker pool** — `DiffWorkerPoolProvider` wraps every viewer.

T3 Code owns exactly one adapter around the raw viewer, and enforces it with a lint rule. From `apps/web/src/components/diffs/StyledDiffCodeView.tsx:1`:

```tsx
/* oxlint-disable eslint/no-restricted-imports -- This is the single styled adapter around Pierre's raw viewer. */
import {
  CodeView,
  type CodeViewHandle,
  ...
} from "@pierre/diffs/react";
/* oxlint-enable eslint/no-restricted-imports */
```

`@pierre/diffs/react` is in `no-restricted-imports` everywhere else. **Every diff in the app goes through one styled component.** That is the single most important structural decision in this slice.

### 3.2 Unified vs split

A persisted client setting, toggled by a segmented control (`DiffPanel.tsx:809`):

```tsx
        <ToggleGroup
          aria-label="Diff layout"
          className="shrink-0"
          variant="segmented"
          value={[diffLayout]}
          onValueChange={(value) => {
            const next = value[0];
            if (next === "stacked" || next === "split") {
              updateClientSettings({ diffLayout: next });
            }
          }}
        >
          <Toggle aria-label="Stacked diff view" value="stacked">
            <Rows3Icon className="size-3.5" />
          </Toggle>
          <Toggle aria-label="Split diff view" value="split">
            <Columns2Icon className="size-3.5" />
          </Toggle>
        </ToggleGroup>
```

Note the vocabulary: the *setting* is `stacked | split`, the *viewer option* is `unified | split`. The mapping happens at the call site: `diffStyle: diffLayout === "split" ? "split" : "unified"`.

### 3.3 Full viewer options

`DiffPanel.tsx:1028`:

```tsx
                    options={{
                      diffStyle: diffLayout === "split" ? "split" : "unified",
                      lineDiffType: "none",
                      overflow: wordWrap ? "wrap" : "scroll",
                      theme: resolveDiffThemeName(resolvedTheme),
                      preferredHighlighter: PREFERRED_HIGHLIGHTER,
                      themeType: resolvedTheme as DiffThemeType,
                      stickyHeaders: true,
                      ...(currentLoadDiffFiles ? { loadDiffFiles } : {}),
                    }}
```

- `lineDiffType: "none"` — **word-level highlighting inside a changed line is turned OFF** in the web diff panel. This is a deliberate choice; the mobile native surface turns it on with heavy gating (see §10).
- `stickyHeaders: true` — file headers pin.
- `loadDiffFiles` is the context-expansion loader, supplied only when the selection can serve full file contents.
- Themes are named `pierre-light` / `pierre-dark` (`lib/diffRendering.ts:4`).

### 3.4 The theme bridge — how app tokens reach the shadow DOM

`apps/web/src/lib/diffRendering.ts:258`, `DIFF_SURFACE_THEME_UNSAFE_CSS`. This is the whole colour system for the diff body, and it is worth reading in full:

```css
[data-diffs-header],
[data-diff],
[data-file],
[data-error-wrapper],
[data-virtualizer-buffer] {
  --diffs-header-font-family: var(--font-sans) !important;
  --diffs-font-family: var(--font-mono) !important;
  --diffs-bg: var(--code-background) !important;
  --diffs-light-bg: var(--code-background) !important;
  --diffs-dark-bg: var(--code-background) !important;
  --diffs-token-light-bg: transparent;
  --diffs-token-dark-bg: transparent;

  /* Gutter, context, and row tints all derive from the code surface the diff
     body sits on — mixing from the canvas leaves the gutter looking unthemed
     when a palette separates the two. */
  --diffs-bg-context-override: color-mix(in srgb, var(--code-background) 97%, var(--code-foreground));
  --diffs-bg-hover-override: color-mix(in srgb, var(--code-background) 94%, var(--code-foreground));
  --diffs-bg-separator-override: color-mix(
    in srgb,
    var(--code-background) 95%,
    var(--code-foreground)
  );
  --diffs-bg-buffer-override: color-mix(in srgb, var(--code-background) 90%, var(--code-foreground));

  --diffs-bg-addition-override: light-dark(
    color-mix(in srgb, var(--code-background) 50%, var(--diff-addition)),
    color-mix(in srgb, var(--code-background) 70%, var(--diff-addition))
  );
  --diffs-bg-addition-number-override: light-dark(
    color-mix(in srgb, var(--code-background) 35%, var(--diff-addition)),
    color-mix(in srgb, var(--code-background) 60%, var(--diff-addition))
  );
  --diffs-bg-addition-hover-override: color-mix(in srgb, var(--code-background) 85%, var(--diff-addition));
  --diffs-bg-addition-emphasis-override: color-mix(
    in srgb,
    var(--code-background) 80%,
    var(--diff-addition)
  );

  --diffs-bg-deletion-override: light-dark(
    color-mix(in srgb, var(--code-background) 50%, var(--diff-deletion)),
    color-mix(in srgb, var(--code-background) 70%, var(--diff-deletion))
  );
  --diffs-bg-deletion-number-override: light-dark(
    color-mix(in srgb, var(--code-background) 35%, var(--diff-deletion)),
    color-mix(in srgb, var(--code-background) 60%, var(--diff-deletion))
  );
  --diffs-bg-deletion-hover-override: color-mix(
    in srgb,
    var(--code-background) 85%,
    var(--diff-deletion)
  );
  --diffs-bg-deletion-emphasis-override: color-mix(
    in srgb,
    var(--code-background) 80%,
    var(--diff-deletion)
  );

  background-color: var(--diffs-bg) !important;
  color: var(--code-foreground) !important;
}
```

**The design system in that block, stated plainly:**

| surface | recipe |
|---|---|
| diff background | `var(--code-background)` verbatim |
| context row | 97% background + 3% foreground |
| hovered row | 94% background + 6% foreground |
| hunk separator | 95% background + 5% foreground |
| virtualizer buffer | 90% background + 10% foreground |
| **added line, light** | 50% background + 50% `--diff-addition` |
| **added line, dark** | 70% background + 30% `--diff-addition` |
| **added line number, light** | 35% background + 65% `--diff-addition` |
| **added line number, dark** | 60% background + 40% `--diff-addition` |
| added row hover | 85% background + 15% `--diff-addition` |
| added word emphasis | 80% background + 20% `--diff-addition` |
| deletion | identical ladder against `--diff-deletion` |

Five ideas here are transferable to any codebase:

1. **One source colour per change kind**, then every tint is a `color-mix` off it. There are no separate "addition background" and "addition gutter background" hexes to keep in sync.
2. **`light-dark()` with different mix ratios per theme.** Light gets 50% saturation, dark gets 30%. A dark diff needs a *fainter* tint to read at the same perceptual strength. This is the detail almost every hand-rolled diff gets wrong.
3. **The gutter is always more saturated than the body** (35/65 vs 50/50, 60/40 vs 70/30). The number column carries the colour; the code carries the text. Legibility of the code is preserved because the tint behind it stays light.
4. **Mix from `--code-background`, not from the page background.** The comment says exactly why: *"mixing from the canvas leaves the gutter looking unthemed when a palette separates the two."*
5. **Neutrals are also mixes**, not separate greys: a context row is literally 3% ink over the code surface.

Additionally, `StyledDiffCodeView` sets `[--code-background:var(--background)]` as a class on the host, so by default the code surface *is* the app background — no editor-coloured well. The comment in the mobile adapter states the same intent explicitly: *"so code views blend with the rest of the app instead of using a distinct code-editor background."*

### 3.5 The file header

`StyledDiffCodeView.tsx:79`:

```css
[data-diffs-header] {
  position: sticky !important;
  top: 0;
  z-index: 4;
  background-color: var(--code-background) !important;
  border-bottom-color: transparent !important;
  align-items: center !important;
  font-family: var(--font-sans) !important;
  font-size: 12px !important;
  line-height: 1 !important;
  min-height: 32px !important;
  padding-block: 6px !important;
  padding-inline: 8px 12px !important;
}
```

**The header is sans-serif, the body is mono.** 12px, 32px tall, no bottom border. The header hover state is an *inset* edge cue rather than a full-width band, and the reason is written down:

```css
[data-diffs-header]:hover {
  /* A native scrollbar gutter cannot be painted by descendants. Use an inset edge cue instead
     of a full-width band that would look accidentally clipped at the gutter. */
  background-color: var(--code-background) !important;
  box-shadow: inset 3px 0 color-mix(in srgb, var(--code-foreground) 24%, transparent);
}
```

Counts in the header are mono and tabular, 11px:

```css
[data-diffs-header] [data-additions-count],
[data-diffs-header] [data-deletions-count] {
  font-family: var(--font-mono) !important;
  font-size: 11px !important;
  font-variant-numeric: tabular-nums;
  line-height: 1 !important;
}
```

The filename is a click target with a *revealed* underline:

```css
[data-title] {
  cursor: pointer;
  transition:
    color 120ms ease,
    text-decoration-color 120ms ease;
  text-decoration: underline;
  text-decoration-color: transparent;
  text-underline-offset: 2px;
  font-family: var(--font-sans) !important;
}

[data-title]:hover {
  color: color-mix(in srgb, var(--code-foreground) 84%, var(--primary)) !important;
  text-decoration-color: currentColor;
}
```

The underline is always *present* and only its colour animates — so the text never reflows when it appears. `text-underline-offset: 2px`. Hover colour is 16% primary mixed into the foreground, not the primary itself.

Header composition is a three-part contract from the host (`DiffPanel.tsx:989`): `renderHeaderPrefix` gives the collapse chevron, the viewer gives the icon/path/counts, `renderHeaderFilenameSuffix` gives the copy-path button. The chevron is **tinted by change type** (`lib/diffRendering.ts:237`):

```ts
export function getDiffCollapseIconClassName(fileDiff: FileDiffMetadata): string {
  switch (fileDiff.type) {
    case "new":      return "text-[var(--diffs-addition-base)]";
    case "deleted":  return "text-[var(--diffs-deletion-base)]";
    case "change":
    case "rename-pure":
    case "rename-changed":
      return "text-[var(--diffs-modified-base)]";
    default:         return "text-muted-foreground/80";
  }
}
```

A green chevron means new file, red means deleted, blue means modified. That is the entire status-letter system — no `A`/`M`/`D` badges anywhere.

### 3.6 The hunk separator and the expand-context affordance

This is the most carefully-designed 90 lines in the file. `StyledDiffCodeView.tsx:101-196`.

The separator is 24px tall, and the "N unmodified lines" text sits between two hairlines that grow to fill:

```css
:is([data-separator="line-info"], [data-separator="line-info-basic"]) {
  height: 24px !important;
  margin-block: 0 !important;
  background-color: var(--code-background) !important;
}

:is(…) [data-separator-content] {
  gap: 8px;
  padding-inline: 0 !important;
  background-color: transparent !important;
  color: color-mix(in srgb, var(--code-foreground) 52%, var(--code-background)) !important;
  font-family: var(--font-sans) !important;
  font-size: 11px !important;
  text-decoration: none !important;
}

:is(…) [data-unmodified-lines]::before,
:is(…) [data-unmodified-lines]::after {
  width: auto;
  height: 1px;
  flex: 1 1 auto;
  content: "";
  background-color: color-mix(in srgb, var(--code-background) 92%, var(--code-foreground));
}
```

So the separator is `———— 47 unmodified lines ————` in 11px sans at 52% ink, with 1px rules at 8% ink. Not a coloured `@@ -1,7 +1,9 @@` band.

The expand *button* is visually hidden but keyboard-reachable, and the whole row becomes the click target:

```css
/* Visually hidden rather than display: none so the expand action stays keyboard-reachable. */
:is(…) [data-expand-button] {
  position: absolute !important;
  width: 1px !important;
  height: 1px !important;
  margin: -1px !important;
  padding: 0 !important;
  overflow: hidden !important;
  clip-path: inset(50%) !important;
  border: 0 !important;
  white-space: nowrap !important;
}

:is(…):has([data-expand-button]) [data-separator-content] {
  cursor: pointer;
}
```

And hovering lifts both the text and the rules, together:

```css
:is(…):has([data-expand-button]):is(:hover, :focus-within) [data-separator-content] {
  color: color-mix(in srgb, var(--code-foreground) 76%, var(--code-background)) !important;
}

:is(…):has([data-expand-button]):is(:hover, :focus-within) [data-unmodified-lines]::before,
:is(…):has([data-expand-button]):is(:hover, :focus-within) [data-unmodified-lines]::after {
  background-color: color-mix(in srgb, var(--code-background) 84%, var(--code-foreground));
}
```

52% → 76% ink on the text; 8% → 16% ink on the rules. `:focus-within` is in both selectors, so keyboard focus looks identical to hover.

### 3.7 Line selection

Selected lines get a tint derived from `--diffs-modified-base`, and — in the "bars" indicator mode — a 4px bar (`StyledDiffCodeView.tsx:16-71`):

```css
:is([data-line], [data-line-annotation], [data-merge-conflict], [data-merge-conflict-actions], [data-no-newline])[data-selected-line] {
  --diffs-line-bg: light-dark(
    color-mix(in lab, var(--code-background) 88%, color-mix(in srgb, var(--code-background) 50%, var(--diffs-modified-base))),
    color-mix(in lab, var(--code-background) 80%, color-mix(in srgb, var(--code-background) 70%, var(--diffs-modified-base)))
  ) !important;
}
```

Note `in lab` for the outer mix and `in srgb` for the inner: the inner mix is building a *colour*, the outer is doing a *perceptual blend*. The gutter cells get a slightly different ratio (91%/85% instead of 88%/80%), keeping the gutter marginally stronger, exactly as the add/delete ladder does.

The selection bar:

```css
[data-indicators="bars"] :is([data-column-number], [data-gutter-buffer="annotation"])[data-selected-line]::before {
  position: absolute !important;
  inset-block: 0 !important;
  inset-inline-start: 0 !important;
  display: block !important;
  width: 4px !important;
  min-width: 4px !important;
  max-width: 4px !important;
  height: auto !important;
  padding: 0 !important;
  content: "" !important;
  background-color: var(--diffs-modified-base) !important;
  background-image: none !important;
}
```

### 3.8 Virtualizer metrics — and the bug comments that earned them

`StyledDiffCodeView.tsx:305`:

```tsx
          itemMetrics: {
            diffHeaderHeight: 32,
            hunkSeparatorHeight: 24,
            // Pierre uses its general file spacing as a fallback in expanded-file layout paths.
            // Keep it zero alongside the explicit paddingTop or expanding the first file can
            // reintroduce the library's default 8px gap above its header.
            spacing: 0,
            paddingTop: 0,
            // Unlike the gap above, the 8px under a file's last line is painted
            // unconditionally by Pierre's stylesheet (`--diffs-gap-fallback`), so the metric has
            // to count it: at zero every expanded file's virtual height ran 8px short of its
            // rendered height, and the end of the list sat past the reachable scroll range —
            // one clipped file row per expanded file above it.
            paddingBottom: 8,
          },
          layout: { paddingTop: 0, paddingBottom: 0, gap: 0 },
```

The general lesson: in a virtualized diff, **the metric you declare and the pixel you paint must be the same number**, and a mismatch compounds once per expanded file.

---

## 4. Review actions

### 4.1 In the thread diff panel — comments only

The thread diff panel has exactly **one** review verb: comment on a line or range, which appends to the composer draft. There is no accept, reject, revert, or stage. `apps/web/src/components/diffs/AnnotatableCodeView.tsx` is the whole mechanism.

The gutter-utility click begins a draft:

```tsx
      options={{
        ...options,
        enableGutterUtility: !hasOpenComment,
        enableLineSelection: !hasOpenComment,
        onGutterUtilityClick: beginComment,
      }}
```

Note the interlock: **while a comment draft is open, line selection and the gutter utility are both disabled.** One draft at a time, enforced by turning off the affordance rather than by ignoring the event.

The annotation renders either as a stack of saved comments or a single editor:

```tsx
      renderAnnotation={(annotation) => {
        const hasDraft = annotation.metadata.entries.some((entry) => entry.kind === "draft");
        return (
          <div
            className={hasDraft ? "py-1" : "divide-y divide-border/30 border-y border-border/30"}
          >
```

A saved comment (`DiffCommentAnnotation.tsx:60`):

```tsx
      <div
        data-diff-comment-annotation
        className="group/comment flex min-w-0 items-start gap-2.5 border-s-2 border-primary/55 bg-primary/[0.045] px-3 py-2.5 font-sans text-foreground"
        contentEditable={false}
        onPointerDown={(event) => event.stopPropagation()}
      >
        <MessageCircle className="mt-0.5 size-3.5 shrink-0 text-primary/70" aria-hidden="true" />
        <p className="min-w-0 flex-1 whitespace-pre-wrap text-[13px] leading-5">{displayedText}</p>
        {onDelete ? (
          <Button
            className="-my-1 -mr-1 shrink-0 text-muted-foreground opacity-0 transition-opacity group-hover/comment:opacity-100 focus-visible:opacity-100 max-sm:opacity-100"
            variant="ghost"
            size="icon-xs"
            aria-label="Delete comment"
            onClick={onDelete}
          >
            <Trash2 className="size-3" />
          </Button>
        ) : null}
      </div>
```

Details worth copying:

- **`border-s-2 border-primary/55` with `bg-primary/[0.045]`.** A 2px logical-start border at 55% primary over a 4.5% primary wash. Loud enough to find, quiet enough to read past.
- **`contentEditable={false}` and `onPointerDown` stop-propagation.** The card sits inside the code surface; without both, dragging on it starts a line selection.
- **`font-sans`** inside a mono surface. Prose is prose.
- **The delete button is `opacity-0` until `group-hover/comment` or `focus-visible` — and always visible under `max-sm:`.** Hover reveal with a touch escape hatch.
- Named group (`group/comment`) so the hover does not fire from an ancestor group.

The composer:

```tsx
      <Textarea
        ref={textareaRef}
        autoFocus={focusOnMount}
        unstyled
        className="relative inline-flex w-full rounded-md border border-border/50 bg-background/20 font-sans text-foreground transition-colors focus-within:border-border/70 [&_[data-slot=textarea]]:min-h-12 [&_[data-slot=textarea]]:cursor-text [&_[data-slot=textarea]]:caret-foreground [&_[data-slot=textarea]]:px-2.5 [&_[data-slot=textarea]]:py-1.5 [&_[data-slot=textarea]]:font-sans [&_[data-slot=textarea]]:text-xs [&_[data-slot=textarea]]:leading-5 max-sm:[&_[data-slot=textarea]]:min-h-12"
        size="sm"
      />
      <div className="mt-1.5 flex items-center gap-1">
        <span className="mr-auto text-[10px] text-muted-foreground/70">Cmd/Ctrl Enter to send</span>
        <Button className="text-muted-foreground hover:text-foreground" variant="ghost" size="xs" onClick={onCancel}>Cancel</Button>
        <Button size="xs" disabled={pending || !trimmedText} onClick={() => onComment(trimmedText)}>{submitLabel}</Button>
      </div>
```

The shortcut hint is **rendered in the UI**, at 10px and 70% muted. Focus lands at the end of existing text, not at the start:

```tsx
        onFocus={(event) => {
          const end = event.currentTarget.value.length;
          event.currentTarget.setSelectionRange(end, end);
        }}
```

The shortcut itself, `apps/web/src/components/diffs/commentSubmitShortcut.ts` (16 lines, with a test file beside it):

```ts
return !pending && (event.metaKey || event.ctrlKey) && event.key === "Enter" && value.trim().length > 0;
```

### 4.2 In the pull-request tab — real review verdicts

`PullRequestReviewBar.tsx`:

```ts
{ value: "comment", label: "Comment", sent: "Review submitted", icon: <MessageSquareIcon className="size-3" /> },
{ value: "approve", label: "Approve", sent: "Pull request approved", icon: <CheckIcon className="size-3" /> },
{ value: "request-changes", label: "Request changes", sent: "Changes requested", icon: <XCircleIcon className="size-3" /> },
```

Pending comments **do not leave the browser until the verdict is sent**. The bar reads `"No line comments yet"` or `N comments pending` with a `Discard` ghost button. The verdict row is right-aligned (`"mt-2 flex flex-wrap justify-end gap-2"`); `comment` is `variant="outline"`, approve and request-changes are `variant="default"`.

Two robustness rules in the submit handler, both worth stealing:

- On failure the draft survives: *"whatever went wrong, retyping the review is not the answer."*
- On success it removes **only the exact comment ids submitted** and clears the summary **only if it still equals the submitted text** — so a comment added while the request was in flight is not silently dropped.

The review bar lives in a floating overlay, hidden entirely when the host offers no verdicts:

```tsx
<div className="pointer-events-none absolute inset-x-0 bottom-0 z-10">
```

open: `cn("surface-glass pointer-events-auto absolute inset-x-3 rounded-xl border border-border/60 shadow-lg", canComment ? "bottom-16" : "bottom-3")`

closed: a FAB `cn("pointer-events-auto absolute bottom-3 rounded-full shadow-lg", canComment ? "right-16" : "right-4")` with a count pill `"flex size-4 items-center justify-center rounded-full bg-accent text-[10px] tabular-nums text-accent-foreground"`.

The offered verbs are the **intersection of host capability and viewer permission**:

```ts
return {
  inlineComment: hostReview.inlineComment && viewer.comment,
  reply: hostReview.reply && viewer.comment,
  resolve: hostReview.resolve && viewer.resolve,
  verdicts: hostReview.verdicts.filter((verdict) => viewer.verdicts.includes(verdict)),
};
```

### 4.3 Open in editor, copy path

Clicking the **filename** opens the file in the user's preferred external editor. Clicking anywhere else on the header **toggles collapse**. The disambiguation is a capture-phase listener walking the composed path through the shadow root (`DiffPanel.tsx:946`):

```tsx
                  onClickCapture={(event) => {
                    const composedPath = event.nativeEvent.composedPath?.() ?? [];
                    for (const node of composedPath) {
                      if (!(node instanceof HTMLElement)) continue;
                      // Header controls keep their own actions. In particular, the chevron must
                      // not also trigger the row handler or the two toggles cancel each other.
                      if (node instanceof HTMLButtonElement || node instanceof HTMLAnchorElement) {
                        return;
                      }
                    }
                    const title = composedPath.find(
                      (node): node is HTMLElement =>
                        node instanceof HTMLElement && node.hasAttribute("data-title"),
                    );
                    const filePath = title?.textContent?.trim();
                    // The filename remains the explicit "open in editor" affordance.
                    if (filePath) {
                      openDiffFile(filePath);
                      return;
                    }
                    const header = composedPath.find(
                      (node): node is HTMLElement =>
                        node instanceof HTMLElement && node.hasAttribute("data-diffs-header"),
                    );
                    const headerFilePath = header?.querySelector("[data-title]")?.textContent?.trim();
                    if (!headerFilePath) return;
                    const file = codeViewFiles.find((candidate) => candidate.filePath === headerFilePath);
                    if (file) toggleDiffFileCollapsed(file.fileKey);
                  }}
```

Copy path is an icon-micro ghost button in the header suffix (`DiffFilePathCopyButton.tsx`), which flips to a green check and raises an anchored toast:

```tsx
        {isCopied ? <CheckIcon className="size-3 text-success" /> : <CopyIcon className="size-3" />}
```

### 4.4 Keyboard navigation between files and hunks

**There is none.** Neither `DiffPanel.tsx` nor `PullRequestCodeTab.tsx` binds any key for next-file / next-hunk. The only keys in the review surface are Cmd/Ctrl+Enter to submit a comment and Escape to abandon one. Navigation is: click a file in the tree, expand it if collapsed, then `viewer.scrollTo({ type: "item", id: fileKey, align: "start" })`.

This is a genuine gap in the reference and should not be copied.

### 4.5 Commit / stage / PR

Separate from the diff panel entirely, in `GitActionsControl.tsx` in the chat header. The commit dialog is where per-file staging lives: an exclusion-set selection (`excludedFiles`) over a `<ScrollArea className="h-44 rounded-lg bg-card ring-1 ring-black/5 dark:bg-white/[0.025] dark:ring-white/5">`, each row `"flex w-full items-center gap-2 rounded-md px-2 py-1 font-mono hover:bg-accent/50"` with a checkbox, an open-in-editor button, and `+insertions` / `-deletions` in `text-diff-addition` / `text-diff-deletion`. Footer: `Cancel`, `Commit on new branch`, `Commit`.

Two constants:

```ts
const COMMIT_DIALOG_TITLE = "Commit changes";
const COMMIT_DIALOG_DESCRIPTION =
  "Review and confirm your commit. Leave the message blank to auto-generate one.";
```

Committing onto the default branch raises a guard dialog offering three ways out: `Abort`, continue anyway, or `Check out feature branch & continue`.

---

## 5. The summary header

`apps/web/src/components/DiffPanelShell.tsx` owns the frame; `DiffPanel.tsx:555-892` owns the content.

The shell is 98 lines and does three things: pick a width, pick a header height, and decide whether the header is an Electron drag region.

```tsx
      className={cn(
        "flex h-full min-w-0 flex-col bg-background",
        props.mode === "inline"
          ? "w-[42vw] min-w-[360px] max-w-[560px] shrink-0 border-l border-border"
          : "w-full",
      )}
```

`42vw`, clamped to `[360px, 560px]`. Four modes: `inline | sheet | sidebar | embedded`.

```tsx
function getDiffPanelHeaderRowClassName(mode: DiffPanelMode) {
  const shouldUseDragRegion = isElectron && mode !== "sheet" && mode !== "embedded";
  return cn(
    "flex items-center justify-between gap-2",
    mode === "embedded" ? "px-2" : "px-4",
    shouldUseDragRegion
      ? "drag-region h-[var(--workspace-topbar-height)] border-b border-border wco:h-[env(titlebar-area-height)] wco:pr-[calc(100vw-env(titlebar-area-width)-env(titlebar-area-x)+1em)]"
      : "flex h-10 min-h-10 shrink-0 items-center border-b border-border/60 bg-background in-data-[preview-panel-mode=inline]:mb-3 in-data-[preview-panel-mode=inline]:h-7 in-data-[preview-panel-mode=inline]:min-h-7 in-data-[preview-panel-mode=inline]:border-b-transparent",
  );
}
```

The `wco:` variant handles Window Controls Overlay: the header reserves the width the OS window buttons occupy, computed from `env(titlebar-area-*)`.

### Header content, left to right

**Left group** (`flex min-w-0 flex-1 items-center gap-3 [-webkit-app-region:no-drag]`):

1. A **scope dropdown** whose trigger is a filled chip, not a bordered select:

```tsx
          <DropdownMenuTrigger
            className="inline-flex h-6 max-w-full items-center gap-1 rounded-md bg-accent px-2 text-xs font-medium text-accent-foreground outline-none transition-colors hover:bg-accent/80 focus-visible:ring-2 focus-visible:ring-ring"
            aria-label={`Diff scope: ${selectedScopeLabel}`}
          >
            <span className="truncate">{selectedScopeLabel}</span>
            <ChevronDownIcon className="size-3.5 shrink-0 opacity-70" />
          </DropdownMenuTrigger>
```

Its items are `Working tree`, `Branch changes`, `Latest turn`, and a `Turn` submenu listing every completed turn with a right-aligned timestamp. The selected item is marked with `"bg-foreground/[0.08]"`, an 8% ink wash, no checkmark.

The label logic is worth copying (`DiffPanel.tsx:216`): the latest turn reads **"Latest turn"**, not "Turn 12". A moving target gets a moving name.

2. A **branch comparison line**, only in branch scope:

```tsx
          <div
            className="flex min-w-0 max-w-full items-center gap-2 overflow-hidden text-xs text-muted-foreground"
            aria-label={`Comparing ${selectedGitSource.headRef ?? "HEAD"} against ${selectedGitSource.baseRef}`}
          >
```

head then base, with an `ArrowRightIcon className="size-3.5 shrink-0 opacity-70"` between. The head is a truncating span with a tooltip carrying the full pair; the base is a **combobox trigger**, the only editable half.

The base-ref picker popup is a grid with a column header row:

```tsx
                <div className="grid shrink-0 grid-cols-[1rem_minmax(0,1fr)] items-center gap-2 border-b border-border/70 ps-3 pe-6.5 pt-2 pb-1.5 font-medium text-[10px] text-muted-foreground uppercase tracking-wide">
                  <span aria-hidden="true" />
                  <div className="grid min-w-0 grid-cols-[minmax(0,1fr)_2rem] items-center">
                    <span>Branch</span>
                    <span className="text-right">Remote</span>
                  </div>
                </div>
```

A ref that exists both locally and remotely gets an inline `Switch` in the Remote column (`className="[--thumb-size:--spacing(3)]"`); a remote-only ref gets a `CheckIcon` with a "Remote only" tooltip. The first item is always `Automatic`.

**Right group** (`flex shrink-0 items-center gap-1 [-webkit-app-region:no-drag]`), in order:

| control | condition | icon / component |
|---|---|---|
| totals | `codeViewFiles.length > 0` | `<DiffStatLabel className="mr-1 text-[11px]" layout="inline" />` |
| refresh | git scopes only | `<RefreshIcon className="size-3.5" refreshing={...} />`, `size="icon-sm" variant="ghost"` |
| collapse/expand all | files present | `ChevronsUpDownIcon` / `ChevronsDownUpIcon` |
| layout | always | segmented `ToggleGroup`, `Rows3Icon` / `Columns2Icon` |
| word wrap | always | ghost `Toggle`, `TextWrapIcon` |
| ignore whitespace | always | ghost `Toggle`, `PilcrowIcon` |
| file tree | files present | ghost `Toggle`, `FolderTreeIcon` |

Every one of the seven is wrapped in a `Tooltip` with `side="top"` and a label that **states the resulting state, not the current one**: `{wordWrap ? "Disable line wrapping" : "Enable line wrapping"}`. All icons are `size-3.5` (14px).

There is **no commit affordance in the diff header**. Commit lives in the chat header.

---

## 6. Inline diffs in chat, and the fact that T3 Code has none

This is the finding most relevant to Crucible.

A tool call in the T3 Code timeline is a **one-line row** that expands to a plain `<pre>`. `MessagesTimeline.tsx:3888`:

```ts
const toolCallExpandedBodyClassName =
  "max-h-64 cursor-text overflow-auto whitespace-pre-wrap break-words font-mono text-secondary-label text-[length:var(--font-size-code,0.6875rem)] leading-relaxed select-text";
```

11px mono, `max-h-64` (256px), scrollable, selectable. The expanded body is assembled by `buildToolCallExpandedBody` (`MessagesTimeline.tsx:3840`), which concatenates de-duplicated blocks: the MCP payload as JSON, the raw command, the detail, and **the list of changed file paths**, as text. Not a diff.

```ts
  const blocks: string[] = [];
  const seen = new Set<string>([visibleLabel.trim()]);
  const addBlock = (value: string | null | undefined) => {
    const text = value?.trim();
    if (!text || seen.has(text)) return;
    seen.add(text);
    blocks.push(text);
  };
```

The de-duplication is the design: a tool row already shows its primary label, so the expanded body never repeats it. The collapsed row shows a `firstLine` preview:

```tsx
      {!open && firstLine ? (
        <p className="truncate text-xs text-muted-foreground">{firstLine}</p>
      ) : null}
      {open ? (
        <div
          className="mt-1 cursor-default rounded-md bg-muted/40 px-3 py-2"
          onClick={stopRowToggle}
          onPointerDown={stopRowToggle}
        >
          <pre className={toolCallExpandedBodyClassName}>{body}</pre>
        </div>
      ) : null}
```

The row itself:

```tsx
      className={cn(
        "flex flex-col rounded-md px-1 py-0.5 transition-colors",
        canExpand &&
          "cursor-pointer hover:bg-accent/20 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring/70",
      )}
```

Note `focus-visible:ring-inset`: a dense row cannot afford an outset ring.

**So where does the diff go?** Up one level, to the turn. The `ChangedFilesCard` (section 1.2) appears once per assistant turn with the *union* of that turn's file changes, and its "Open diff" button opens the right-panel Diff surface scoped to that turn, revealing the file you picked:

```tsx
onOpenTurnDiff(turnId, files[0]?.path)
```

which lands in `diffPanelStore.selectTurn(ref, turnId, filePath)`.

**The argument for this design.** An agent turn that rewrites one file with six `Edit` calls produces six inline diffs in Crucible's model, of which five are obsolete by the time the user reads them. One per-turn tree plus a real diff panel shows the *net* change once. T3 Code's server-side `getReviewDiffPreview` computes the composed diff; the transcript never diffs anything.

**The argument against.** You lose the ability to see *what one call proposed*, which is exactly what a permission prompt needs, and what Crucible's superseded/re-applied semantics depend on. Crucible should not adopt this wholesale.

### The one place a diff does render inline: a user's own review comment

Covered in section 1.4. It uses `FileDiff` (unvirtualized) with `collapsed: false, diffStyle: "unified"`, no word diff, wrapped in `"space-y-2 rounded-lg border border-border/70 bg-background/70 p-3"`.

---

## 7. Empty, loading, large, binary and renamed states

### 7.1 Loading, a real skeleton, not a spinner

`DiffPanelShell.tsx:47-98`. `DiffPanelLoadingState` draws **the shape of a diff**: one file header, one hunk separator, three code lines, then two more collapsed file headers.

```tsx
function DiffFileHeaderSkeleton({ titleClassName }: { titleClassName: string }) {
  return (
    <div className="flex h-8 items-center gap-2 px-2 pr-3">
      <div className="flex size-5 shrink-0 items-center justify-center">
        <Skeleton className="size-2.5 rounded-[2px]" />
      </div>
      <Skeleton className="size-5 shrink-0 rounded-md" />
      <Skeleton className={cn("h-3 rounded-full", titleClassName)} />
      <div className="ml-auto flex shrink-0 items-center gap-2">
        <Skeleton className="h-3 w-5 rounded-full" />
        <Skeleton className="h-3 w-5 rounded-full" />
      </div>
    </div>
  );
}

function DiffCodeLineSkeleton({ contentClassName }: { contentClassName: string }) {
  return (
    <div className="flex items-center gap-3">
      <Skeleton className="h-2.5 w-5 shrink-0 rounded-full" />
      <Skeleton className={cn("h-2.5 rounded-full", contentClassName)} />
    </div>
  );
}

export function DiffPanelLoadingState(props: { label: string }) {
  return (
    <div
      className="min-h-0 flex-1 overflow-hidden bg-background"
      role="status"
      aria-live="polite"
      aria-label={props.label}
    >
      <DiffFileHeaderSkeleton titleClassName="w-1/2 max-w-64" />
      <div className="flex h-6 items-center gap-2 px-2 pr-3">
        <div className="h-px flex-1 bg-border/40" />
        <Skeleton className="h-2.5 w-24 rounded-full" />
        <div className="h-px flex-1 bg-border/40" />
      </div>
      <div className="space-y-2 px-3 py-2">
        <DiffCodeLineSkeleton contentClassName="w-2/3" />
        <DiffCodeLineSkeleton contentClassName="w-4/5" />
        <DiffCodeLineSkeleton contentClassName="w-3/5" />
      </div>
      <DiffFileHeaderSkeleton titleClassName="w-2/5 max-w-52" />
      <DiffFileHeaderSkeleton titleClassName="w-3/5 max-w-72" />
      <span className="sr-only">{props.label}</span>
    </div>
  );
}
```

Points of craft:

- The heights are the *real* heights: `h-8` header (close to the live 32px), `h-6` separator (the live 24px plus border), even the two 1px rules flanking the separator label at `bg-border/40`.
- Title widths are **varied and fractional** (`w-1/2 max-w-64`, `w-2/5 max-w-52`, `w-3/5 max-w-72`) so the placeholder does not read as a stack of identical bars.
- Line widths vary too: `w-2/3`, `w-4/5`, `w-3/5`.
- `role="status" aria-live="polite"` plus a `sr-only` label, and the label is contextual:

```tsx
                  label={
                    selectedTurn
                      ? "Loading checkpoint diff..."
                      : selectedGitScope === "unstaged"
                        ? "Loading working tree diff..."
                        : "Loading branch diff..."
                  }
```

### 7.2 Empty states, five distinct sentences

`DiffPanel.tsx:896-941`. Every one is `"flex flex-1 items-center justify-center px-5 text-center text-xs text-muted-foreground/70"`: centered, 12px, 70% muted.

| condition | text |
|---|---|
| no thread | `Select a thread to inspect turn diffs.` |
| not a git repo | `Turn diffs are unavailable because this project is not a git repository.` |
| turn scope, no turns | `No completed turns yet.` |
| patch resolved but empty | `No net changes in this selection.` |
| patch absent | `No patch available for this selection.` |

The distinction between the last two matters: `hasNoNetChanges` means the daemon answered with an empty patch (edits cancelled out); `No patch available` means it answered with nothing.

### 7.3 Truncation

Server-side caps are exact constants in `apps/server/src/vcs/GitVcsDriverCore.ts:47-60`:

```ts
const DEFAULT_MAX_OUTPUT_BYTES = 1_000_000;
const REVIEW_DIFF_PATCH_MAX_OUTPUT_BYTES = 120_000;
const REVIEW_UNTRACKED_DIFF_MAX_OUTPUT_BYTES = 80_000;
const REVIEW_DIFF_FILE_MAX_OUTPUT_BYTES = 1024 * 1024;
const WORKSPACE_FILES_MAX_OUTPUT_BYTES = 120_000;
```

Over-cap patches get `truncated: true` on the source; over-cap file expansion is a hard error.

The client draws a strip above the diff:

```tsx
            {isSelectedPatchTruncated && (
              <p className="shrink-0 border-b border-border/70 bg-muted/40 px-3 py-1.5 text-[11px] text-muted-foreground">
                This diff was truncated because it exceeded the preview limit. The changes shown are
                incomplete.
              </p>
            )}
```

The mobile client has a stronger version: a `border-warning-border bg-warning` strip labelled **"Partial diff"**.

### 7.4 Unparseable patches, a raw fallback, not an error

`lib/diffRendering.ts:109`:

```ts
    if (files.length > 0) {
      return { kind: "files", files };
    }
    return {
      kind: "raw",
      text: normalizedPatch,
      reason: "Unsupported diff format. Showing raw patch.",
    };
  } catch {
    return {
      kind: "raw",
      text: normalizedPatch,
      reason: "Failed to parse patch. Showing raw patch.",
    };
  }
```

and it renders:

```tsx
              <div className="min-h-0 flex-1 overflow-auto p-2">
                <div className="space-y-2">
                  <p className="text-[11px] text-muted-foreground/75">{renderablePatch.reason}</p>
                  <pre
                    className={cn(
                      "max-h-[72vh] rounded-md border border-border/70 bg-background/70 p-3 font-mono text-[11px] leading-relaxed text-muted-foreground/90",
                      wordWrap ? "overflow-auto whitespace-pre-wrap wrap-break-word" : "overflow-auto",
                    )}
                  >
                    {renderablePatch.text}
                  </pre>
                </div>
              </div>
```

**A patch that cannot be parsed still shows its bytes**, and the wrap toggle still applies to it.

### 7.5 Binary

Detected server-side by a NUL scan, in two places, in `GitVcsDriverCore.ts`: `readReviewFileAtRevision` refuses when `result.stdout` contains a NUL byte, with the message `Cannot expand binary file '<path>'.`, and `readWorkingTreeReviewFile` refuses when the read bytes contain a zero, with the same message shape.

Binary files still appear in the diff (git emits a `Binary files ... differ` stanza the parser handles); only *context expansion* refuses. Mobile has a richer client-side policy, a 30-extension non-text set and an explicit suppressed state (`apps/mobile/src/features/review/reviewModel.ts`):

```ts
const LARGE_DIFF_LINE_THRESHOLD = 400;
const LARGE_DIFF_CHARACTER_THRESHOLD = 24_000;
const NON_TEXT_FILE_EXTENSIONS = new Set([
  "png","jpg","jpeg","gif","webp","bmp","ico","icns","avif","heic","tif","tiff",
  "mp3","wav","flac","ogg","m4a","aac","mp4","mov","avi","mkv","webm",
  "pdf","zip","gz","tgz","bz2","7z","rar",
  "woff","woff2","ttf","otf","eot","wasm","exe","dll","so","dylib",
]);
```

yielding `{kind:"suppressed", reason:"non-text", title:"Non-text file", message:"Diff preview is not available for this file format.", actionLabel:null}` or `{reason:"large", title:"Large diff", message:"Large diffs are not rendered by default.", actionLabel:"Load diff"}`. **A large diff gets an explicit "Load diff" button; a binary one gets no action at all.**

### 7.6 Renames

The parser distinguishes `rename-pure` (moved, unchanged) from `rename-changed`. Both tint the chevron `--diffs-modified-base`. The identity key joins the previous path and the current path with a NUL separator so a rename does not read as delete plus add across a refresh (`lib/diffRendering.ts:166`, `buildFileDiffIdentityKey`).

Context expansion skips the old side entirely for a pure rename (`lib/diffFileContents.ts`):

```ts
if (fileDiff.type === "rename-pure") {
  return { oldFile: null, newFile };
}
```

On mobile a pure rename with no rows gets a notice row: `"This file was renamed without modifications."`

**Known inconsistency in the reference:** the working-tree diff passes `--find-renames`, the branch-range diff does not. The same rename shows as a rename in one scope and as add plus delete in the other.

---

## 8. Density and typography

| property | value | source |
|---|---|---|
| diff body font | `var(--font-mono)` | `--diffs-font-family` override |
| file header font | `var(--font-sans)` | `--diffs-header-font-family` override |
| file header size | 12px, `line-height: 1` | `[data-diffs-header]` |
| file header height | `min-height: 32px`, `padding-block: 6px` | same |
| file header padding | `padding-inline: 8px 12px` (asymmetric) | same |
| additions/deletions count | 11px mono, `tabular-nums` | `[data-additions-count]` |
| hunk separator height | 24px | `[data-separator="line-info"]` |
| hunk separator text | 11px sans, 52% ink | `[data-separator-content]` |
| separator padding | `padding-inline: 8px 12px` | `[data-separator-wrapper]` |
| diff stat label | `grid-cols-[4ch_4ch] gap-2` | `DiffStatLabel` aligned layout |
| tree header | `h-10 min-h-10`, `text-xs`, `px-2` | `DiffFileTree` |
| tree row density | `density: "compact"` | `useFileTree` |
| transcript tree row | `py-1.5 pr-2`, indent `8 + depth*14` px | `ChangedFilesTree` |
| transcript file name | `font-mono text-xs` | same |
| transcript dir name | `font-mono text-[11px]` | same |
| transcript stats | `font-mono text-[10px] tabular-nums` | same |
| tool-call expanded body | 11px mono via `--font-size-code`, `max-h-64` | `toolCallExpandedBodyClassName` |
| all toolbar icons | `size-3.5` (14px) | `DiffPanel` |
| panel header | `h-10 min-h-10`, `px-4` | `DiffPanelShell` |
| inline panel width | `42vw`, clamp `[360px, 560px]` | `DiffPanelShell` |

`tabular-nums` appears on the tree file count, the scope-menu timestamps, the header counts, every stat label, the PR commit oid, and the agent badge. It is applied as a policy, not case by case.

**Tab width is never set** in the web diff; Pierre's default stands. The React Native mobile path expands tabs to 4 spaces and converts leading spaces to non-breaking spaces, in `renderVisibleWhitespace` (`apps/mobile/src/features/review/reviewDiffRendering.tsx`): a regex replaces each tab with four spaces, then a second regex replaces every leading space with a non-breaking space so React Native does not collapse the indentation.

Mobile code surface constants (`apps/mobile/src/lib/typography.ts`): `MOBILE_CODE_SURFACE = { rowHeight: 22, gutterWidth: 46, codePadding: 7, textVerticalInset: 2, fontSize: 12, lineNumberFontSize: 11 }`, with `rowHeight = max(14, round(22 * fontSize/12))` so the line height tracks a user-set code font size (range 8 to 18, step 1).

---

## 9. Motion

T3 Code's diff motion budget is **one transition, 200ms, one-way**, and the reason is documented in the stylesheet (`StyledDiffCodeView.tsx:239`):

```css
/* Expanding a file mounts its body all at once; easing it in matches the 200ms the app's
   collapsibles take. Appearance only - the viewer owns geometry, so height cannot animate.
   Departing content cuts, the same one-way rule the pull request chrome fold follows. */
[data-diff],
[data-file] {
  transition: opacity 200ms ease-out;
}

@starting-style {
  [data-diff],
  [data-file] {
    opacity: 0;
  }
}

@media (prefers-reduced-motion: reduce) {
  [data-diff],
  [data-file] {
    transition: none;
  }
}
```

Four things:

- **`@starting-style`**, not a keyframe and not a JS-toggled class. The element fades in from `opacity: 0` on first paint with no state to manage.
- **Entry animates, exit cuts.** Stated as a house rule.
- **Height is never animated.** A virtualizer owns geometry, so the only honest property is opacity.
- `prefers-reduced-motion` is handled in the same block.

The other motions in the slice:

| what | duration / curve |
|---|---|
| filename hover colour and underline | `120ms ease` on `color` and `text-decoration-color` |
| every toolbar/row hover | `transition-colors` (Tailwind default 150ms) |
| tree directory chevron | `transition-transform` plus `rotate-90` |
| comment delete button reveal | `transition-opacity` |
| base-ref combobox border | `transition-colors focus-within:border-ring` |
| PR provider card | `transition-[background-color,border-color,box-shadow]` |
| refresh icon | a dedicated `RefreshIcon` component with a `refreshing` prop |
| a live diff update while the agent works | **nothing.** Items re-render only when their `version` hash changed |

That last row is a deliberate choice. Live updates are made *invisible* by making them cheap: every item carries a content hash, and only changed items repaint. The item version is an `fnv1a32` over the file's content version, its collapsed flag, and every annotation entry's id, range label and text.

`buildFileDiffContentVersion` (`lib/diffRendering.ts:187`) hashes **only the content** of a file diff, never the enclosing patch:

```ts
/**
 * Content-only version for CodeView reconciliation. Pierre's cache key includes
 * the whole patch, so using it here would repaint every file when one changes.
 */
```

The file tree has the matching rule: a path-list change is applied as a batch of adds and removes, so the tree does not flash when the agent saves a file (section 2.1d).

Collapse state also survives a refresh, because it is keyed by scope rather than by index:

```ts
  const collapseScopeKey = routeThreadRef
    ? `${routeThreadRef.environmentId}:${routeThreadRef.threadId}:${reviewSectionId}`
    : null;
```

and the reveal mechanism is a **monotonic counter**, so asking for the same file twice still scrolls (`diffPanelStore.ts`):

```ts
  revealRequestId: previous?.kind === "turn" ? previous.revealRequestId + 1 : 1,
```

with `useCodeViewFileReveal` waiting for a mounted viewer before applying, and applying each request exactly once:

```ts
// Wait for a mounted viewer and expanded rows, then apply each tree click once.
// Keep scope stable until the diff or external file selection changes.
export function useCodeViewFileReveal<TScope>(viewer: FileRevealHandle | null, scope: TScope) {
  const [request, setRequest] = useState<{ fileKey: string; scope: TScope } | null>(null);
  const handledRequest = useRef<typeof request>(null);

  useEffect(() => {
    if (request === null || handledRequest.current === request) return;
    if (request.scope !== scope) { handledRequest.current = request; return; }
    if (!viewer?.getInstance()) return;
    viewer.scrollTo({ type: "item", id: request.fileKey, align: "start" });
    handledRequest.current = request;
  }, [request, scope, viewer]);

  return useCallback((fileKey: string) => setRequest({ fileKey, scope }), [scope]);
}
```

---

## 10. The mobile native review diff module, and what is worth learning

`apps/mobile/modules/t3-review-diff/` is an Expo module named `T3ReviewDiffSurface` with an iOS `UIView` (2582 lines of Swift) and an Android `View` (1429 lines of Kotlin plus 315 lines of canvas drawing). It exists because **no view hierarchy survives a real unified diff.**

### 10.1 Why a canvas

Three reasons are in the source, and a fourth is structural.

**(a) Prop diffing is itself a frame cost.** The three large payloads are async view *functions*, not props (`ios/T3ReviewDiffModule.swift`):

```swift
      // Large, frequently changing JSON values cannot be regular Fabric props. Expo's
      // prop adapter compares strings on the main thread before invoking a setter, which
      // makes a syntax-token patch capable of blocking a frame by itself.
      AsyncFunction("setRowsJson") { (view: T3ReviewDiffView, rowsJson: String) in
        view.setRowsJson(rowsJson)
      }
      AsyncFunction("setTokensJson") { ... }
      AsyncFunction("setTokensPatchJson") { ... }
```

**(b) One view, no recycling.** Row geometry is a prefix-sum array of floats. Visible rows are found by binary search in `draw(_:)` with an overscan of `max(rowHeight, fileHeaderHeight) * 4`.

**(c) Two independent scroll axes per file.** Each file keeps its own horizontal offset (`horizontalOffsetsByFileId`), and the file-header path scrolls separately (`headerPathOffsetsByFileId`). Not expressible as nested scroll views without gesture arbitration.

**(d) The scroll view is a lie.** `contentView` is kept exactly viewport-sized and re-anchored each frame, so one viewport-sized backing layer exists regardless of a 100k-row diff:

```swift
    contentView.frame = CGRect(
      x: 0,
      y: scrollView.contentOffset.y,
      width: max(width, 1),
      height: max(bounds.height, 1)
    )
    contentView.viewportWidth = bounds.width
    contentView.verticalOffset = scrollView.contentOffset.y
```

### 10.2 Ideas that transfer to a web review pane

**Per-file measured content width.** Horizontal scrolling never exceeds the file's own longest line:

```swift
    let characterWidth = monospaceCharacterWidth(font: codeFont)
    codeCharacterWidth = characterWidth
    contentWidthsByFileId = maxColumnCountsByFileId.mapValues { maxColumnCount in
      let measuredWidth = ceil(CGFloat(maxColumnCount) * characterWidth) + style.codePadding * 2
      return max(0, min(style.contentWidth, measuredWidth))
    }
```

with the character width measured once by sampling 64 capital M glyphs.

**Collapse preserves the tapped header's screen position.** When exactly one file's collapse state changed, iOS captures that header's screen Y before relayout and restores it after:

```swift
    if changedFileIds.count == 1, let changedFileId = changedFileIds.first {
      scrollAnchor = contentView.scrollAnchor(forFileId: changedFileId)
    } else { scrollAnchor = nil }
    ...
      let targetOffset = headerOffset - scrollAnchor.screenY
      scrollView.setContentOffset(CGPoint(x: 0, y: min(max(targetOffset, 0), maxOffset)), animated: false)
```

Collapsing a file above you does not teleport the viewport. Android lacks this and is visibly worse for it.

**Deletions get striped change bars, additions get solid ones.** A colour-blind-safe second channel at zero cost:

```swift
  private func drawDeleteStripes(rect: CGRect, context: CGContext) {
    theme.deleteBar.setFill()
    var y = rect.minY
    while y < rect.maxY {
      context.fill(CGRect(x: rect.minX, y: y, width: rect.width, height: 1))
      y += 2
    }
  }
```

**Word-diff highlighting is gated to prevent a solid bar** (`apps/mobile/src/features/review/nativeReviewDiffAdapter.ts`):

```ts
const NATIVE_REVIEW_MAX_WORD_DIFF_RANGE_COUNT = 4;
const NATIVE_REVIEW_MAX_WORD_DIFF_COVERAGE = 0.45;
```

More than 4 ranges on a line, or more than 45% of the line's non-whitespace highlighted, and the whole set is dropped, so a near-total rewrite shows a plain add/delete row. Ranges are also trimmed past leading and trailing whitespace first.

**The word-diff algorithm** (`reviewWordDiffs.ts`) is `diffWordsWithSpace` from the `diff` package, with two heuristics on top. A single unchanged character is absorbed into a preceding changed span:

```ts
  if (
    isNeutral === lastSpanIsNeutral ||
    (isNeutral && operation.value.length === 1 && !lastSpanIsNeutral)
  ) {
    lastSpan[1] += operation.value;
    return;
  }
```

so `foo.bar` becoming `foo.baz` highlights `.baz` as one block rather than `baz` with a gap. Then `mergeNearbyRanges` closes gaps of one character or less.

**Syntax highlighting is stitched, not run over the whole diff.** `nativeReviewDiffHighlighter.ts` decides whether two rows may be tokenized as one grammar context:

```ts
  if (previous.row.change === "delete" || next.row.change === "delete") {
    return (
      previous.row.change !== "add" &&
      next.row.change !== "add" &&
      hasConsecutiveLineNumbers(previous.row.oldLineNumber, next.row.oldLineNumber)
    );
  }
  if (previous.row.change === "add" || next.row.change === "add") {
    return hasConsecutiveLineNumbers(previous.row.newLineNumber, next.row.newLineNumber);
  }
```

An addition block and the deletion block it replaces are tokenized as **two independent documents**, which is the only correct answer. `hasOnlyCommentRowsBetween` keeps an interleaved comment card from breaking a run. The same split-document trick appears in `shikiReviewHighlighter.ts` as `highlightReviewSelectedLines`.

**The tokenizer yields.** Segments of at most 8000 characters, `await waitForNextFrame()` between them, lines over 1000 characters emitted as one plain token with an explicit reason:

```ts
        // Skipping this line leaves its ending grammar state unknown. Resume
        // from a fresh state rather than leaving the rest of the segment plain:
        // highlighted rows are cached for the sheet's lifetime, so a plain tail
        // would stick, and which rows it covered would depend on where the
        // first visible window happened to start.
```

**Highlighting follows the viewport, in patches.** Assume rows 0 to 80, re-request when the window moves by 20 rows or more in total, push each result as an independent token patch, abort the previous request:

```ts
    const movedRows =
      Math.abs(nextRange.firstRowIndex - previousRange.firstRowIndex) +
      Math.abs(nextRange.lastRowIndex - previousRange.lastRowIndex);
    visibleRangeRef.current = nextRange;
    if (movedRows >= 20) setVisibleHighlightRequest((request) => request + 1);
```

with `NATIVE_REVIEW_DIFF_VISIBLE_OVERSCAN_ROWS = 160` and `NATIVE_REVIEW_DIFF_VISIBLE_MAX_ROWS = 360`.

**Stale results are refused twice**, by a generation counter plus a reset-key check:

```swift
          guard contentResetKey == self.contentResetKey else { return }
          // A highlighter request from the previous file can finish after the view has
          // already reset. Never let that stale patch roll the native token state back.
          if let resetKey = patch.resetKey, resetKey != self.tokensResetKey { return }
```

**Prewarming is one nearby section per idle period.** `useReviewDiffPrewarming.ts` uses `requestIdleCallback` with a 2000ms timeout and a 100ms `setTimeout` fallback, expands outward by distance from the selected section, and refuses any section that would push the cache past its budget:

```ts
export const MAX_CACHED_REVIEW_DIFFS = 8;
// This bounds source string length, not the parsed or native heap size.
export const MAX_CACHED_REVIEW_SOURCE_CHARACTERS = 4 * 1024 * 1024;
```

A true LRU: a hit re-inserts the key; eviction walks insertion order until both budgets fit.

**The mobile colour palette** (`nativeReviewDiffAdapter.ts`), as concrete numbers:

| | light | dark |
|---|---|---|
| addBackground | `#e5f8f5` | `#0d2f28` |
| deleteBackground | `#ffe6e7` | `#391415` |
| addBar | `#00cab1` | `#00cab1` |
| deleteBar | `#ff2e3f` | `#ff2e3f` |
| addText | `#199F43` | `#5ECC71` |
| deleteText | `#D52C36` | `#FF6762` |

**The bars are the same colour in both themes; the backgrounds and text are not.** A 4px bar is a shape, not text, so it does not owe contrast to the surface.

Selection is a 22% tint of the primary over the row plus a 95% primary change bar. Word highlights are 28% of the bar colour at corner radius 3. `headerBackground` equals `background`, so the file header gets no distinct fill, only a device-pixel hairline.

**Motion is deliberately absent.** Collapse, expand, viewed-toggle and comment-collapse are all instant (`rebuildRowLayout` plus `setNeedsDisplay`). The only softening is the scroll anchor. `scrollToFile` uses the system curve on iOS and a 250ms `OverScroller` on Android. Visible-file events are suppressed during programmatic scrolls specifically so the file navigator does not flash.

**Comments attach by long-press then tap.** Long press a line sets an anchor and the action bar reads "Select range end"; tapping a second line in the same file completes the range and the bar reads "Comment on <range>"; tapping with no anchor makes a single-line comment and opens the composer immediately. The action bar is a floating pill row at `left/right: 18`, `bottom: max(bottomInset, 10) + 18`, with a 48pt `rounded-full bg-primary` primary button and a 48 by 48 clear button.

Range labels: `+42`, `-42` or `42` for one line; `+42 to +48` when the range is homogeneous; `5 lines` when line numbers are missing.

---

# Part II. Comparison with Crucible, with recommendations

## A. What Crucible has that T3 Code does not

State this first, because the recommendations below must not cost any of it.

1. **A hunk ledger with real review states.** `unreviewed | accepted | rejected`, persisted in the daemon, with `reject` actually reverting bytes on disk. T3 Code has no accept/reject at all outside a GitHub pull request.
2. **`re-applied` detection.** `ChangesPanel.tsx:155`. A hunk the user rejected and the agent applied again is flagged, with the reasoning written down: *"without it a change the user already rejected, and the agent applied again, is indistinguishable from first-time work, and gets accepted out of fatigue."* Nothing in T3 Code does this.
3. **`external` hunks.** Changes with no tool call, the user's own editor or a formatter, are shown for context and are **structurally unrejectable**. `decidable()` excludes them from every bulk action.
4. **`superseded` tool cards.** `ToolCard.tsx:207`. A call whose entire output was overwritten says so.
5. **Inline review in the real buffer.** `components/editor/review-decorations.ts` puts the composed diff into the actual CodeMirror document as line decorations plus a gutter chip that jumps the transcript to the tool call. T3 Code's review is always a separate viewer. This is a genuinely better idea than anything in the reference, and the header comment says why: *"real highlighting, real folding, real go-to-definition, and your spatial memory of the file survives."*
6. **A write gate.** `SessionStatusChips` shows `waiting on review (N)` when a tool is held pending review.
7. **Degraded-root handling.** A root whose history cannot be read contributes zero hunks while holding every write, and the panel says exactly that instead of "No changes yet."
8. **Conflicts as a first-class disposition**, surfaced in the same panel.

None of this should be traded away.

## B. Difference by difference, with recommendations

### P1-1. The diff renderer is hand-rolled, per-line, unvirtualized, and capped at 320px

**Crucible today.** `components/DiffViewer.tsx`. `analyzeDiff` runs `diffLines` from the `diff` package and produces a flat `DiffLine[]`. Each line is rendered as four spans, and syntax highlighting calls Shiki **once per line**:

```tsx
  const tokensForLine = (line: string): ThemedToken[] | null => {
    const lang = effectiveLanguage();
    if (lang === 'text' || lang === 'plaintext') return null;
    if (!(SHIKI_LANGS as readonly string[]).includes(lang)) return null;
    const h = highlighter();
    if (!h) return null;
    try {
      const result = h.codeToTokens(line, {
        lang: lang as BundledLanguage,
        theme: SHIKI_THEMES[theme()],
      });
      return result.tokens[0] ?? [];
    } catch {
      return null;
    }
  };
```

Three consequences:

- **Highlighting is wrong**, not just slow. Tokenizing each line independently loses grammar state, so a multi-line string, a template literal, a JSDoc block or a JSX child region colours incorrectly from the second line on. T3 Code's mobile highlighter documents the correct fix (split-document plus `canShareGrammarContext`), and the web path gets it free from Pierre's worker pool.
- **No virtualization.** Every line of every hunk of every file is in the DOM.
- The body is `max-h-80` (320px) with its own scrollbar, a diff inside a scroll box inside a transcript:

```tsx
      <div class="font-mono text-xs overflow-x-auto max-h-80 overflow-y-auto">
```

**Recommendation.** Do not adopt `@pierre/diffs`; it is React-coupled and Crucible is Solid. Do adopt the shape:

1. **Create one `components/diffs/StyledDiffSurface.tsx`** that is the only place a diff is rendered. Move `DiffViewer`'s internals behind it. Add it to a restricted-import list so no other component builds a diff.
2. **Tokenize per hunk, not per line.** Concatenate a hunk's context plus addition lines into one document and its context plus deletion lines into another, call `codeToTokens` twice, then distribute tokens back to rows. That is the mobile reference's `highlightReviewSelectedLines`, and it is roughly 60 lines. It fixes correctness and cuts Shiki calls by two orders of magnitude.
3. **Virtualize.** Crucible already has `components/windowing/`. A composed diff with 40 files is not renderable otherwise.
4. Keep `max-h-80` only for the transcript inline case; the Changes panel body should fill its pane.

### P1-2. There are no diff colour tokens; `ok` and `error` are doing double duty

**Crucible today.** `DiffViewer.tsx:94`:

```ts
const lineStyles = {
  add: 'bg-ok/15 text-ok',
  remove: 'bg-error/15 text-error',
  context: 'bg-surface-base text-muted',
};

const gutterStyles = {
  add: 'text-ok/60',
  remove: 'text-error/60',
  context: 'text-muted-dark',
};
```

`index.css` has no diff token at all. A grep for `diff-addition` or `--color-diff` across `crates/crucible-web/web/src` returns nothing. So an added line uses `--cru-color-ok: #7bc47f`, whose documented meaning in the same file is:

```
     *   attention - something is waiting on YOU
     *   ok        - the machine is busy, and it is fine
     *   primary   - this is the one you are on (selection and binding)
```

Three problems:

- **`ok` means "busy and fine" in the status system and "added line" in the diff.** The stylesheet's own rule, *"One colour, one meaning, and no near-duplicates"*, is violated by the diff.
- **`text-ok` on `bg-ok/15` is coloured text on a tinted ground.** A whole added line is rendered in green. T3 Code never does this: the *background* carries the change, the *text* carries syntax highlighting. When Crucible's Shiki tokens do arrive they override `text-ok` via an inline `style={{ color: tok.color }}`, so the line's colour depends on whether highlighting loaded, which is a visible flash on every diff.
- **The same 15% and 60% ratios in both themes.** T3 Code uses `light-dark()` with 50% and 30% precisely because a dark diff needs a fainter tint.

**Recommendation, exact tokens to add to `index.css`.** Follow the existing two-layer pattern.

In `:root` (dark):

```css
    /* Diff. Distinct from ok/error on purpose. `ok` means "the machine is busy
     * and it is fine"; an added line means neither. A diff is also the one place
     * in the app where a WASH carries the meaning and the TEXT stays
     * syntax-coloured, so these are mix bases, not text colours. */
    --cru-color-diff-add: #4ec9a0;
    --cru-color-diff-del: #e06c75;
    --cru-color-diff-mod: #6ba6e8;
```

In `:root[data-theme='light']`:

```css
    --cru-color-diff-add: #1f9d6b;
    --cru-color-diff-del: #c4483f;
    --cru-color-diff-mod: #2f6fb5;
```

In the `@theme` block beside `--color-ok`:

```css
  --color-diff-add: var(--cru-color-diff-add);
  --color-diff-del: var(--cru-color-diff-del);
  --color-diff-mod: var(--cru-color-diff-mod);
```

Then define the surface recipe once, copying T3 Code's ladder against Crucible's surfaces:

```css
/* The diff surface. One base colour per change kind; every tint is a mix off
   it, so there is no second hex to keep in step. The gutter is always more
   saturated than the body: the number column carries the colour, the code
   carries the text. Light and dark take DIFFERENT ratios - a dark diff needs a
   fainter tint to read at the same strength. */
.cru-diff {
  --diff-surface: var(--color-surface-base);
  --diff-ink: var(--color-shell-body);

  --diff-row-context: color-mix(in srgb, var(--diff-surface) 97%, var(--diff-ink));
  --diff-row-hover:   color-mix(in srgb, var(--diff-surface) 94%, var(--diff-ink));
  --diff-row-sep:     color-mix(in srgb, var(--diff-surface) 95%, var(--diff-ink));

  --diff-row-add: light-dark(
    color-mix(in srgb, var(--diff-surface) 50%, var(--color-diff-add)),
    color-mix(in srgb, var(--diff-surface) 70%, var(--color-diff-add))
  );
  --diff-num-add: light-dark(
    color-mix(in srgb, var(--diff-surface) 35%, var(--color-diff-add)),
    color-mix(in srgb, var(--diff-surface) 60%, var(--color-diff-add))
  );
  --diff-word-add: color-mix(in srgb, var(--diff-surface) 80%, var(--color-diff-add));

  --diff-row-del: light-dark(
    color-mix(in srgb, var(--diff-surface) 50%, var(--color-diff-del)),
    color-mix(in srgb, var(--diff-surface) 70%, var(--color-diff-del))
  );
  --diff-num-del: light-dark(
    color-mix(in srgb, var(--diff-surface) 35%, var(--color-diff-del)),
    color-mix(in srgb, var(--diff-surface) 60%, var(--color-diff-del))
  );
  --diff-word-del: color-mix(in srgb, var(--diff-surface) 80%, var(--color-diff-del));
}
```

And replace `DiffViewer`'s maps:

```ts
const lineStyles = {
  add: 'bg-[var(--diff-row-add)]',
  remove: 'bg-[var(--diff-row-del)]',
  context: 'bg-[var(--diff-row-context)]',
};

const gutterStyles = {
  add: 'bg-[var(--diff-num-add)] text-shell-body',
  remove: 'bg-[var(--diff-num-del)] text-shell-body',
  context: 'text-muted-dark',
};
```

**Note the deletion of `text-ok` and `text-error` from the row.** Line text goes back to inheriting, so a highlighted line and an unhighlighted line have the same colour and there is no flash.

`review-decorations.ts` should follow suit. Its current `'color-mix(in srgb, var(--color-ok) 10%, transparent)'` for an accepted hunk is the same token collision inside the editor. Change the wash to `var(--color-diff-add)` while keeping `--color-ok` for the *chip*, which is genuinely a status.

### P1-3. The tool card renders a diff per call; the turn has no summary

**Crucible today.** `ToolCard.tsx:394` renders `DiffViewer` or `MultiEditDiff` for every Edit/Write/MultiEdit call, each with its own bordered card, its own header, and its own 320px scroll box. Six edits to one file produce six diffs of which five are stale.

There is **no per-turn changed-files summary anywhere**. The only file lists in the app are the Changes panel (session or turn scope, flat root then file then hunk) and the file tree.

**T3 Code.** Zero inline diffs on tool calls; one `ChangedFilesCard` per turn with a compacted tree, per-directory stats, a sticky header, and an "Open diff" button.

**Recommendation: keep the inline diff, add the turn card.** Crucible's inline diff earns its place because of the review semantics attached to it (`liveHunks()`, accept/reject chips, `superseded`). But it should be *collapsed by default* and joined by a turn-level summary.

1. **Add `components/chat/TurnChangedFiles.tsx`**, rendered once at the end of each assistant turn, using the `ChangedFilesTree` anatomy. Crucible already has the data: `reviewStore.session(id).hunks` grouped by root and path, and `groupByFile` already exists at `ChangesPanel.tsx:49`. Extract `groupByFile` and `groupByRoot` into `lib/review-tree.ts` and add the directory compaction from `lib/turnDiffTree.ts:56` (single-child chains collapse to `a/b/c`).

   Header, adapting T3 Code's classes to Crucible's tokens:

```tsx
<div class="@container/changed-files mt-3 rounded-card bg-surface-elevated" data-changed-files>
  <div class="sticky top-2 z-10 flex items-center justify-between gap-2 rounded-t-card bg-surface-elevated px-3 py-2">
    <div class="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1 text-xs font-medium text-shell-ink">
      <span>{files.length} changed file{files.length === 1 ? '' : 's'}</span>
      <DiffStat additions={stat.additions} deletions={stat.deletions} layout="inline" />
    </div>
  </div>
```

   `sticky top-2 z-10` is the detail that makes it work in a long transcript.

2. **Collapse the inline diff by default** when the turn card is present. Keep the accept/reject chips visible on the collapsed card; they are the reason the card exists.

3. **Drop the per-edit 320px scroll box in `MultiEditDiff`.** Six nested scroll regions in one card is a trap; the card should scroll as a unit.

### P2-1. There is no `DiffStatLabel`, and +/- counts are ragged

**Crucible today.** Three places write the pair by hand.

`DiffViewer.tsx:227`:

```tsx
          <div class="flex items-center gap-2 ml-auto">
            <span class="text-ok font-mono">+{stats().additions}</span>
            <span class="text-error font-mono">-{stats().deletions}</span>
          </div>
```

`MultiEditDiff.tsx:41` repeats the same three lines.

`ChangesPanel.tsx:580` shows a hunk *count*, not a stat:

```tsx
                          <span class="shrink-0 text-floor text-muted-dark">
                            {file.hunks.length}
                          </span>
```

No `tabular-nums`, no fixed column, no abbreviation, no `aria-label`. In a list, `+7 -2` and `+1284 -931` do not line up.

**Recommendation.** Port `DiffStatLabel` essentially verbatim as `components/DiffStat.tsx`:

```tsx
const DiffStat: Component<{
  additions: number;
  deletions: number;
  class?: string;
  layout?: 'aligned' | 'inline';
}> = (props) => (
  <span
    role="group"
    aria-label={`${props.additions} additions, ${props.deletions} deletions`}
    class={`${
      props.layout === 'inline'
        ? 'inline-flex items-center gap-1 tabular-nums align-middle'
        : 'inline-grid grid-cols-[4ch_4ch] gap-2 text-right tabular-nums align-middle'
    } ${props.class ?? ''}`}
  >
    <span aria-hidden="true" class="font-mono text-diff-add">+{compact(props.additions)}</span>
    <span aria-hidden="true" class="font-mono text-diff-del">-{compact(props.deletions)}</span>
  </span>
);
```

with `compact()` copied from `formatCompactDiffCount`. Use `layout="aligned"` in every list (the Changes panel file rows, the turn card tree) and `layout="inline"` in every header. This one component fixes the alignment of four surfaces.

While there: **the Changes panel file row currently shows a hunk count where a stat belongs.** `{file.hunks.length}` tells the reader nothing about the size of the change. Show `+N -M` in the aligned layout and put the hunk count in the `title`.

### P2-2. The Changes panel header stacks three rows of controls

**Crucible today.** `ChangesPanel.tsx:364-444`. `PanelHeader title="Changes"` plus **two** additional rows.

Row 1 (`mt-1.5`): `{unreviewed()} unreviewed . {state().hunks.length} total`, an `unreviewed only` checkbox with a raw `<input type="checkbox">`, and a refresh button.

Row 2 (`mt-1`): a Session/Turn segmented group, `Accept all`, `Reject all`.

So the panel spends roughly 72px of vertical space on chrome before the first hunk, and it uses a **native checkbox**, the only one in this slice, against a codebase that has `components/ui/`.

**T3 Code.** One 40px row: scope chip, branch line, then seven icon toggles right-aligned, every one 14px with a tooltip.

**Recommendation.** Collapse to one row plus one optional row.

- Turn the scope control into T3 Code's **filled chip dropdown**, which also gives room to grow (Crucible will want `Working tree` and `Since commit` eventually):

```tsx
<button class="inline-flex h-6 max-w-full items-center gap-1 rounded-control bg-hover-wash px-2 text-xs font-medium text-shell-ink outline-none transition-colors hover:bg-hover-wash/80 focus-visible:ring-2 focus-visible:ring-ring">
  <span class="truncate">{scopeLabel()}</span>
  <ChevronDown class="w-3.5 h-3.5 shrink-0 opacity-70" />
</button>
```

  Mark the selected menu item with `bg-shell-ink/[0.08]`, an 8% wash, no checkmark. This is T3 Code's `bg-foreground/[0.08]`.
- Replace the checkbox with a **ghost icon toggle** carrying a tooltip, beside refresh. `unreviewed only` becomes a filter icon that is pressed or not, matching every other control.
- Move `Accept all` and `Reject all` **to the right end of the same row**; they are the only text buttons and the row has space once the checkbox label is gone.
- Put `{unreviewed} unreviewed . {total} total` where T3 Code puts its stat: right-aligned in the header, `tabular-nums`, `text-floor`.
- Adopt `h-10 min-h-10 shrink-0 px-4 border-b border-hairline` as the panel-header height contract so Changes, Activity and Backlinks line up. Crucible's `PanelHeader` should own this.

### P2-3. Hunk state is a text badge; there is no status colour in the row

**Crucible today.** `ChangesPanel.tsx:80`:

```ts
const STATE_CLASS: Record<string, string> = {
  unreviewed: 'border-attention/50 bg-attention/10 text-attention',
  accepted: 'border-ok/50 bg-ok/10 text-ok',
  rejected: 'border-hairline bg-surface-elevated text-muted-dark',
};
```

rendered as a pill reading the literal word `unreviewed`, `accepted` or `rejected`, beside up to two more pills (`re-applied`, `external`) and a comma-joined list of tool call labels. A row can carry four text chips before the action buttons.

**T3 Code.** Status is *colour on the chevron*, never a letter or a word:

```ts
    case "new":      return "text-[var(--diffs-addition-base)]";
    case "deleted":  return "text-[var(--diffs-deletion-base)]";
    case "change":
    case "rename-pure":
    case "rename-changed":
      return "text-[var(--diffs-modified-base)]";
```

**Recommendation.** Crucible's states are richer and some genuinely need words. Split them:

- **`unreviewed` and `accepted` become chevron colour**, exactly as T3 Code does it: `text-attention` when unreviewed, `text-ok` when accepted, `text-muted-dark` when rejected. Drop the pill. The count in the header already says how many are owed, and the panel is a queue: "unreviewed" is the default, and a badge for the default state is noise on every row.
- **`re-applied` and `external` keep their pills.** They are exceptional, they are rare, and both carry a `title` explaining themselves. Exceptional states earn words; default states do not.
- The tool-call label list should truncate to the first label plus `+N` rather than `join(', ')`, which currently can push the action buttons off a narrow panel.

This takes the common row from four chips to zero.

### P2-4. No hunk separator treatment, no expand-context

**Crucible today.** `DiffViewer.tsx:244`:

```tsx
              <button
                onClick={() => toggleSection((section as CollapsedSection).startIndex)}
                class="w-full px-3 py-1 text-center text-xs text-muted-dark bg-surface-elevated hover:bg-hover-wash hover:text-shell-body transition-colors border-y border-hairline cursor-pointer"
              >
                ... {section.lines.length} lines unchanged ...
              </button>
```

A full-width tinted bar with `border-y`, centred dots, 12px. `CONTEXT_LINES = 3`. Expanding reveals the *whole* collapsed run at once; there is no "expand 20 more" step, and there is no way to expand past what the patch carries because Crucible never fetches full file contents.

**T3 Code.** A 24px transparent row: a rule, the label "47 unmodified lines", another rule, in 11px sans at 52% ink with hairlines at 8% ink, both lifting to 76% and 16% on hover or focus. The button is visually hidden but keyboard-reachable; the whole row is the target.

**Recommendation.** Port the separator treatment directly. In Crucible's tokens:

```tsx
<button
  type="button"
  class="group flex h-6 w-full items-center gap-2 px-2 text-left text-floor text-muted-dark transition-colors hover:text-shell-body focus-visible:text-shell-body focus-visible:outline-none"
  onClick={() => toggleSection(startIndex)}
>
  <span class="h-px flex-1 bg-[color-mix(in_srgb,var(--diff-surface)_92%,var(--diff-ink))] transition-colors group-hover:bg-[color-mix(in_srgb,var(--diff-surface)_84%,var(--diff-ink))]" />
  <span class="shrink-0">{count} unmodified lines</span>
  <span class="h-px flex-1 bg-[color-mix(in_srgb,var(--diff-surface)_92%,var(--diff-ink))] transition-colors group-hover:bg-[color-mix(in_srgb,var(--diff-surface)_84%,var(--diff-ink))]" />
</button>
```

Height 24px, no borders, no fill. The visual weight moves from "a bar between two blocks" to "a rule through a label", which is what it means.

Second, **add context expansion**. Crucible's daemon already serves file contents (`getFileContent` is used by `ToolCard.openInEditor`). The T3 Code contract is worth copying exactly:

```ts
ReviewDiffFileContentsInput = { cwd, sourceKind, changeType, baseRef, headRef, oldPath, newPath }
ReviewDiffFileContentsResult = { oldContents, newContents }
```

with a 1 MiB cap, a NUL-byte binary refusal, a symlink-escape check on both root and target, and single-flight dedupe keyed on the whole request identity. Without it, a hunk's three context lines are all a reviewer ever gets.

### P2-5. No word-level highlighting in `DiffViewer`

**Crucible today.** `analyzeDiff` produces whole-line add and remove records only. A one-character change paints the entire line green and the entire line red. `HunkMergeView` *does* get word-level highlighting, but only because CodeMirror's `unifiedMergeView({ highlightChanges: true, allowInlineDiffs: true })` supplies it, so the same hunk looks different in the merge view and in the tool card.

**T3 Code.** Web turns it off (`lineDiffType: "none"`); mobile turns it on with the gating in section 10.

**Recommendation, P2, and do it with the gating.** Use `diffWordsWithSpace` from the `diff` package (already a dependency) on paired delete and add lines, then apply both heuristics from the reference:

```ts
const MAX_WORD_DIFF_RANGES = 4;
const MAX_WORD_DIFF_COVERAGE = 0.45;
```

Drop the whole range set if there are more than four ranges, or if highlighted non-whitespace exceeds 45% of the line's non-whitespace. Absorb single unchanged characters into a preceding changed span, and merge ranges separated by one character or less. Paint at `--diff-word-add` and `--diff-word-del` (the 20% mix) with `border-radius: 3px`.

Without the gating, word highlighting makes a rewritten line *worse* than a plain one, a mottled bar. The gating is the feature.

### P3-1. No file tree beside the diff

**Crucible today.** The Changes panel lists root, file, hunks in one flat scroll. A session touching 30 files across 3 roots is a 30-row list with no structure and no way to jump.

**T3 Code.** A toggleable tree in a right aside, persisted in localStorage, off by default, with all folders open and single-child chains flattened.

**Recommendation.** Crucible already has `components/tree/`. Add a tree behind a `FolderTree` ghost toggle in the panel header, `w-[min(14rem,40%)] min-w-36 shrink-0 border-l border-hairline`, persisted under `crucible.changesTreeOpen`. Reuse the compaction and first-file-position ordering from section 2. Off by default: the Changes panel is narrower than T3 Code's diff panel, and a tree in a 320px rail is not a win until the file count is high.

### P3-2. Loading is a spinner; empty states are one-size

**Crucible today.** `FileViewerPanel.tsx:403`:

```tsx
        <div class="absolute inset-0 flex items-center justify-center bg-surface-base/80 z-10">
          <div class="flex items-center gap-3">
            <div class="w-5 h-5 border-2 border-hairline border-t-shell-body rounded-full animate-spin" />
            <span class="text-muted text-sm">Loading file...</span>
          </div>
        </div>
```

The Changes panel has no loading state at all; `state().loading` only spins the refresh icon. Empty states are good prose (`"No changes in this {scope} yet."`, `"Nothing left to review."`, the degraded-root block) but they are `p-3 text-xs text-muted-dark` top-left, not centred.

**Recommendation.**

1. **Port `DiffPanelLoadingState` as a Changes-panel skeleton.** Crucible already has the geometry: a file row is `py-1.5`, a hunk row is `py-1`. Draw two file rows and three hunk rows in `Skeleton` with varied widths (`w-1/2 max-w-64`, `w-2/5 max-w-52`, `w-2/3`, `w-4/5`, `w-3/5`), wrapped in `role="status" aria-live="polite"` with a contextual `sr-only` label (`Loading session changes...` or `Loading turn changes...`).
2. **Centre the empty states**: `flex flex-1 items-center justify-center px-5 text-center text-xs text-muted-dark`. Crucible's *sentences* are better than the reference's; only the placement is worse.
3. Keep `changes-empty`, `changes-all-reviewed` and `changes-degraded` as three distinct states. That distinction is already correct and is exactly the `hasNoNetChanges` versus `No patch available` care T3 Code shows.

### P3-3. No motion discipline on the diff

**Crucible today.** Expanding a hunk (`<Show when={open()}>` in `HunkRow`) mounts a whole CodeMirror instance with no transition. Expanding a tool card mounts a diff with no transition. The chevron rotates (`transition-transform`), and that is the entire motion budget.

**T3 Code.** One rule: `transition: opacity 200ms ease-out` with `@starting-style`, entry only, height never, `prefers-reduced-motion` handled inline.

**Recommendation.** Add to `styles/refine-states.css`:

```css
/* A diff body mounts all at once; easing it in matches the app's collapsibles.
   Appearance only - a virtualizer owns geometry, so height cannot animate.
   Departing content cuts: one-way, like every other fold in the shell. */
[data-diff-body] {
  transition: opacity 200ms ease-out;
}

@starting-style {
  [data-diff-body] { opacity: 0; }
}

@media (prefers-reduced-motion: reduce) {
  [data-diff-body] { transition: none; }
}
```

and put `data-diff-body` on `DiffViewer`'s body and on `HunkMergeView`'s host. `@starting-style` means no signal, no class toggle, no cleanup.

### P3-4. No keyboard navigation between hunks

Neither codebase has it. T3 Code's absence is a gap, not a design. Crucible's Changes panel is a *queue*, which makes the gap sharper: `j` and `k` to move between unreviewed hunks and `a` and `r` to decide, with the confirm still gating `r`, would make the panel usable without a mouse. `lib/keyboard-shortcuts.ts` already exists. Recommend building it; do not look to the reference for a pattern.

### P3-5. Copy path, and the filename click target

**Crucible today.** `ChangesPanel.tsx:568`. Clicking a file row opens it in the editor and reveals the first hunk. There is no copy-path anywhere in the review surface. `FileViewerPanel` has `Copy File Path` but only in a right-click menu.

**T3 Code.** An `icon-micro` ghost copy button in every file header, flipping to a green check with an anchored toast, plus the filename-versus-header click split of section 4.3.

**Recommendation.** Add a copy-path icon button to the Changes panel file row, revealed on hover the way T3 Code reveals its comment delete button:

```tsx
class="shrink-0 rounded p-1 text-muted-dark opacity-0 transition-opacity group-hover/file:opacity-100 focus-visible:opacity-100 max-sm:opacity-100 hover:text-shell-ink hover:bg-hover-wash"
```

The `max-sm:opacity-100` escape hatch matters: Crucible ships a compact shell and hover does not exist there. Crucible's `hit()` helper already handles the touch target; combine them.

### P3-6. `ChangesPanel` has no per-file stats and no directory grouping

Covered by P2-1 and P3-1 in their mechanisms. Worth stating as its own difference because the *information* is missing, not just the presentation: a reviewer scanning the panel cannot tell a 3-line change from a 300-line one without expanding it.

---

## C. Proposed layout for Crucible's Changes pane

Two states: the rail (current placement, `right` region, registered at `lib/register-panels.tsx:53`) and a wide or centre variant for when the panel is popped out.

### C.1 The rail, about 320 to 400px, one header row, tree off

```
+--------------------------------------------------------------+
| Changes                                               [h-10] |  PanelHeader, px-4,
|                                                              |  border-b border-hairline
+--------------------------------------------------------------+
| +----------+                    7 owed . 24 total  R  F  T   |  h-10 control row
| | Session v|                                                 |  scope chip = bg-hover-wash
| +----------+                              Accept all  Reject |  R refresh F filter T tree
+--------------------------------------------------------------+
| ! CONFLICTS                                                  |  bg-attention/10, text-floor
|   notes/daily/2026-09-15.md              2        [ Open ]   |  uppercase tracking-wider
+--------------------------------------------------------------+
| CRUCIBLE                                                     |  root band: text-floor
+--------------------------------------------------------------+  uppercase, bg-surface-base
| v  crates/crucible-daemon/src/                    +84   -12  |  NEW: directory row,
| v    agent_manager/                               +61   -12  |  compacted single-child
|        scope.rs                                   +48    -9  |  chains, aligned 4ch/4ch
|  >  *  L142-168   Edit           (v) (u) (c)                 |  * = chevron tinted by
|  >  *  L204-211   Edit           (v) (u) (c)                 |  state, NOT a text pill
|  v  o  L233       Write   re-applied  (v) (u) (c)            |  exceptional states keep
|     +----------------------------------------------------+   |  their word
|     |  231   fn main() {                                 |   |
|     |  232 -     let x = 1;                              |   |  merge view / diff body,
|     |  233 +     let x = compute();                      |   |  data-diff-body, fades in
|     |  234   }                                           |   |  200ms, gutter tinted
|     |            [ Accept ]  [ Reject ]                  |   |  stronger than body
|     +----------------------------------------------------+   |
|        messaging.rs                               +13    -3  |
|  >  *  L88-94    Edit            (v) (u) (c)                 |
| v  crates/crucible-web/web/src/components/                   |
|        ChangesPanel.tsx     external              +2     -0  |
|  >  .  L18       external  (v)         (c)                   |  no reject for external
+--------------------------------------------------------------+
| COMMENTS                                                     |
|   scope.rs:142 . you                                  (v)    |
|   Use the existing helper instead of a new one.              |
+--------------------------------------------------------------+
```

Legend for the state dot, which replaces the `unreviewed` / `accepted` pill:

```
*   text-attention   unreviewed
o   text-ok          accepted
.   text-muted-dark  external or rejected
```

Action glyphs in the sketch: `(v)` accept (Check), `(u)` reject (Undo2), `(c)` comment (MessageCircle).

### C.2 Popped out, 720px and wider, tree on, diff fills

```
+----------------------------------------------------------------------------------------+
| +----------+  main -> session_base          7 owed . 24 total  +312 -148                |
| | Session v|                                  R  F  [S|P]  W  X  T   Accept    Reject   |
| +----------+                                                                            |
+----------------------------------------------------------------+-----------------------+
|                                                                | Files              12 |
|  v  * crates/crucible-daemon/src/agent_manager/scope.rs  +48 -9| -------------------   |
|  --------------- 47 unmodified lines ---------------           | v crates/             |
|   139   140    fn admit(&self) -> Result<()> {                 |   v crucible-daemon/  |
|   140   141        let scope = self.scope();                   |     v src/agent_.../  |
|   141      -       if scope.is_empty() {                       |         scope.rs   *  |
|        142 +       if scope.is_empty() && !self.trusted {      |         messaging  *  |
|   142   143            return Err(Denied);                     |   v crucible-web/.../ |
|   143   144        }                                           |         ChangesP...  .|
|  --------------- 12 unmodified lines ---------------           |                       |
|                                                                |                       |
|  >  * crates/crucible-daemon/src/agent_manager/messaging.rs    |                       |
|  >  . crates/crucible-web/web/src/components/ChangesPanel.tsx  |                       |
+----------------------------------------------------------------+-----------------------+
```

Toolbar glyphs, right to left, matching T3 Code's order exactly: `T` file tree, `X` ignore whitespace, `W` word wrap, `[S|P]` stacked/split segmented group, `F` unreviewed-only filter, `R` refresh. All `w-3.5 h-3.5`, all ghost toggles, all tooltipped with the *resulting* state.

### C.3 The turn card in the transcript

```
    +----------------------------------------------------------+
    | 4 changed files            +112  -31             [ Diff ] |  sticky top-2 z-10
    +----------------------------------------------------------+  rounded-card
    | v crates/crucible-daemon/src/agent_manager/   +84   -12   |  bg-surface-elevated
    |     scope.rs                                  +48    -9   |  no border
    |     messaging.rs                              +36    -3   |
    | v crates/crucible-web/web/src/components/     +28   -19   |  indent 8 + depth*14 px
    |     ChangesPanel.tsx                          +28   -19   |  file: font-mono text-xs
    +----------------------------------------------------------+  dir:  font-mono text-[11px]
                                                                   stat: aligned 4ch/4ch
```

---

## D. Priority summary

| # | Change | Why | Effort |
|---|---|---|---|
| **P1-1** | One diff surface; per-hunk (not per-line) Shiki; virtualize | Highlighting is currently *incorrect* past line 1 of any multi-line construct, and the DOM is unbounded | L |
| **P1-2** | `--color-diff-add/del/mod` tokens plus the `color-mix` ladder; stop using `ok` and `error`; stop colouring line text | One token, one meaning; light and dark need different ratios; removes a highlight-load flash | M |
| **P1-3** | Per-turn changed-files card; collapse inline tool diffs by default | Six stale diffs per turn today; no net view anywhere | M |
| **P2-1** | `DiffStat` with `grid-cols-[4ch_4ch]`, `tabular-nums`, compact counts, one `aria-label` | Four surfaces currently ragged; the Changes panel shows hunk counts where stats belong | S |
| **P2-2** | One-row panel header: scope chip, icon toggles, right-aligned counts | 72px of chrome, a raw `<input type=checkbox>`, three rows | S |
| **P2-3** | Chevron colour for `unreviewed` and `accepted`; keep words only for `re-applied` and `external` | Four chips per row on the common case | S |
| **P2-4** | Rule-through-label hunk separator; add a context-expansion RPC | A reviewer gets 3 lines of context and no way to get more | M |
| **P2-5** | Word-level diff **with** the 4-range / 45%-coverage gating | Whole-line paint for one-character changes; ungated it is worse | M |
| **P3-1** | Toggleable file tree, persisted, folders open, chains flattened | 30 flat rows at 3 roots | M |
| **P3-2** | Diff-shaped skeleton; centre the empty states | Spinner over a diff; good prose, wrong placement | S |
| **P3-3** | `opacity 200ms ease-out` plus `@starting-style`, entry only, reduced-motion | Everything mounts hard | S |
| **P3-4** | `j`/`k`/`a`/`r` hunk navigation | Neither codebase has it; a queue needs it more than a viewer does | M |
| **P3-5** | Hover-revealed copy-path with a `max-sm` escape hatch | Absent from the review surface | S |

## E. Things in the reference NOT to copy

- **No badge count on the diff surface.** Crucible's `{unreviewed} unreviewed` count is load-bearing; it is what the write gate holds on. Keep it, and consider badging the panel tab with it, which T3 Code explicitly declines to do.
- **No keyboard navigation between files or hunks.** A gap, not a design.
- **No inline diff on a tool call.** Crucible's version carries accept/reject and `superseded`; T3 Code's absence is only defensible because it has no review model.
- **`--find-renames` on one scope and not the other.** A real inconsistency: the same rename reads as a rename in the working tree and as add plus delete against the branch.
- **`truncated` without a cause.** A file-listing cap and a patch cap are indistinguishable to the client.
- **Both diff sources computed and sent on every refresh** even though the panel renders one. Up to 240 KB per refresh on a dirty tree.

---

## F. File index

T3 Code, web:

- `apps/web/src/components/diffs/StyledDiffCodeView.tsx` - the single adapter, and 250 lines of shadow-DOM CSS
- `apps/web/src/components/diffs/AnnotatableCodeView.tsx` - comment annotations over the viewer
- `apps/web/src/components/diffs/DiffCommentAnnotation.tsx` - the comment card and composer
- `apps/web/src/components/diffs/DiffFileTree.tsx` - the tree beside the diff
- `apps/web/src/components/diffs/diffFileTree.logic.ts` - ordering and incremental tree updates
- `apps/web/src/components/diffs/useCodeViewFileReveal.ts` - reveal-once scroll requests
- `apps/web/src/components/diffs/commentSubmitShortcut.ts` - the Cmd/Ctrl+Enter predicate
- `apps/web/src/components/DiffPanel.tsx` - the panel, scope selection, toolbar
- `apps/web/src/components/DiffPanelShell.tsx` - frame, header sizing, loading skeleton
- `apps/web/src/components/DiffFilePathCopyButton.tsx`
- `apps/web/src/components/RightPanelTabs.tsx` - surface registry, launcher, badges
- `apps/web/src/components/chat/ChangedFilesTree.tsx` - the per-turn card and tree
- `apps/web/src/components/chat/DiffStatLabel.tsx` - the +/- widget
- `apps/web/src/components/chat/MessagesTimeline.tsx` - tool rows, review-comment cards
- `apps/web/src/lib/diffRendering.ts` - parsing, hashing, theme bridge
- `apps/web/src/lib/turnDiffTree.ts` - tree build, compaction, stats
- `apps/web/src/lib/diffFileContents.ts` - context-expansion loader
- `apps/web/src/diffPanelStore.ts` - the selection model
- `apps/web/src/components/GitActionsControl.tsx` - commit, push, PR
- `apps/web/src/components/pullRequest/PullRequestCodeTab.tsx`, `PullRequestReviewBar.tsx`, `PullRequestReviewAnnotation.tsx`

T3 Code, server and mobile:

- `apps/server/src/review/ReviewService.ts` - admission shell, two RPCs
- `apps/server/src/vcs/GitVcsDriverCore.ts` - the real diff engine, all size constants
- `apps/mobile/modules/t3-review-diff/ios/T3ReviewDiffView.swift` - the native canvas
- `apps/mobile/modules/t3-review-diff/android/.../ReviewDiffCanvasDrawing.kt`
- `apps/mobile/src/features/review/reviewWordDiffs.ts` - the word-diff algorithm
- `apps/mobile/src/features/review/nativeReviewDiffAdapter.ts` - colours, style, gating
- `apps/mobile/src/features/diffs/nativeReviewDiffHighlighter.ts` - grammar-context stitching
- `apps/mobile/src/features/review/useReviewDiffPrewarming.ts` - the idle prewarm

Crucible:

- `components/ChangesPanel.tsx` (663) - the review queue
- `components/DiffViewer.tsx` (256) - the renderer
- `components/MultiEditDiff.tsx` (66)
- `components/HunkMergeView.tsx` (108) - CodeMirror unified merge view per hunk
- `components/ToolCard.tsx` (511) - inline diffs, per-hunk accept/reject chips
- `components/FileViewerPanel.tsx` (573) - inline review in the real buffer
- `components/editor/review-decorations.ts` (220) - line decorations and gutter chips
- `components/ConflictsPanel.tsx` (99), `components/ConflictView.tsx` (249)
- `lib/diff-stats.ts` - `analyzeDiff`
- `index.css` (1737) - the token system; no diff tokens today
- `lib/register-panels.tsx:53` - where Changes is registered
