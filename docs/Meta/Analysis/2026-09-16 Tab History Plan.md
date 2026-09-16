---
title: Tab History Plan — 2026-09-16
description: The task-by-task implementation plan for back and forward navigation inside a web note tab, with a history tree popover
tags: [meta, ux, web, plan, windowing]
status: draft
updated: 2026-09-16
---

# Tab History Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** A web note tab keeps a history tree, a link click shows its target in the same tab, and a bar with back and forward buttons moves through that tree.

**Architecture:** The windowing core gets a typed `Tab.history` tree, pure tree functions and four store actions. The saved layout goes to v11. The app wraps the moves in `lib/tab-host.ts` behind an app modal for unsaved changes, and the file viewer, the canvas viewer and the phone shell use that host.

**Tech Stack:** SolidJS, `solid-js/store`, Ark UI (`Menu`, `Dialog`), CodeMirror 6, Vitest with jsdom and `@solidjs/testing-library`, `fast-check`, Playwright, bun.

---

The design is [[2026-09-16 Tab History Design]]. This plan follows it. The section "Where the code differs from the design" at the end lists each place where the code forced a change.

## Rules for every task

- Run every command from the worktree root, `/home/moot/crucible/.claude/worktrees/tab-history`. The `just web-test` recipe goes into `crates/crucible-web/web` itself, so test paths start at `src/` or `e2e/`.
- In the plan, `web/` means `crates/crucible-web/web/`.
- The commit steps stage web files with `git -C crates/crucible-web/web add src/…`. The docs lint (`just lint docs`) checks that each `crates/…` path in a note exists, and most of these files do not exist yet.
- Write the test first. Run it. Read the failure. Then write the code.
- **Break every new gate once.** Each task names the break. Make the break, run the test, read the failure, then restore the code.
- Never accept a snapshot or a screenshot in bulk. Open each changed image. Check the layout, the Unicode and the colours.
- Never call `process.env.X = …` or `vi.stubEnv` in a test. No task in this plan needs an environment value.
- The core (`web/src/windowing/`) must not read `metadata.filePath` or import app modules. `windowing/__tests__/boundary.test.ts` checks the imports. A reviewer checks the metadata rule, because no test can.
- Write each commit message in ASD-STE100. End each message with a blank line and the `Co-Authored-By` line that the task shows.

## Task order

The order follows the suggested order, with two changes:

- Task 6 (the tab host) comes before Task 7 (tab ids and `closeTabsUnder`). The new `closeTabsUnder` calls `host.markHistory`, so the host must exist first.
- Task 6a is new. It adds the Pin tab and Unpin tab controls (user decision), before Task 7 gives the flag its meaning.
- Task 8 is new. A history move keeps the tab id, but `Pane.tsx`, `FloatingWindow.tsx` and `ContentSurface.tsx` mount a panel again only when the tab id or the content type changes. `reactiveMetadataProps` (`web/src/lib/panel-props.ts:36`) also fixes its key set at mount. Thus a move must mount the panel again.

---

### Task 0: Prepare the worktree

The worktree has no `node_modules`. The tests cannot run without them.

**Step 1: Install the web packages**

```bash
cd crates/crucible-web/web && bun install
```

Expected: `bun install` ends with a line such as `N packages installed`, and a `node_modules` folder exists in `crates/crucible-web/web`.

**Step 2: Run the web unit suite once, as a baseline**

```bash
just web-test unit
```

Expected: every test passes. If a test fails here, stop. Report the failure before you change code.

No commit.

---

### Task 1: The nav tree types and pure functions

**Files:**
- Modify: `web/src/windowing/model/types.ts:3-11` (the `Tab` interface) and add the three new types after it
- Create: `web/src/windowing/model/nav-tree.ts`
- Test: `web/src/windowing/__tests__/nav-tree.test.ts`

**Step 1: Write the failing test**

Create `web/src/windowing/__tests__/nav-tree.test.ts`:

```ts
import { describe, it, expect } from 'vitest';
import {
  NAV_TREE_CAP,
  backTree,
  canGoBack,
  canGoForward,
  capTree,
  childrenOf,
  entryOf,
  forwardTree,
  goToTree,
  markNodes,
  navigateTree,
  pathTo,
  readNavTree,
  seedTree,
} from '@/windowing/model/nav-tree';
import type { NavEntry, NavNode, NavTree } from '@/windowing/model/types';

const entry = (title: string): NavEntry => ({ title, contentType: 'alpha', metadata: { key: title } });

const node = (id: string, parent: string | null, lastVisit = 1): NavNode => ({
  id,
  parent,
  lastVisit,
  title: id.toUpperCase(),
  contentType: 'alpha',
});

/** A → B → C, then back to B and on to D. C is the branch that the user left. */
function forked(): NavTree {
  let t = seedTree(entry('A'), 1, 'a');
  t = navigateTree(t, entry('B'), 2, 'b');
  t = navigateTree(t, entry('C'), 3, 'c');
  t = backTree(t, 4)!;
  t = navigateTree(t, entry('D'), 5, 'd');
  return t;
}

describe('the nav tree', () => {
  it('adds a child below the current node and moves to it', () => {
    const t = navigateTree(seedTree(entry('A'), 1, 'a'), entry('B'), 2, 'b');
    expect(t.current).toBe('b');
    expect(t.nodes.b).toEqual({
      title: 'B',
      contentType: 'alpha',
      metadata: { key: 'B' },
      id: 'b',
      parent: 'a',
      lastVisit: 2,
    });
  });

  it('goes back to the parent, and stops at the root', () => {
    const up = backTree(forked(), 6)!;
    expect(up.current).toBe('b');
    expect(up.nodes.b!.lastVisit).toBe(6);
    expect(backTree(seedTree(entry('A'), 1, 'a'), 2)).toBeNull();
  });

  it('goes forward to the child with the latest visit, and keeps the branch that the user left', () => {
    const t = backTree(forked(), 6)!;
    expect(childrenOf(t, 'b')).toEqual(['d', 'c']);
    const fwd = forwardTree(t, 7)!;
    expect(fwd.current).toBe('d');
    expect(fwd.nodes.c).toBeDefined();
    expect(forwardTree(fwd, 8)).toBeNull();
  });

  it('goes to any node, and refuses an unknown id', () => {
    const t = goToTree(forked(), 'c', 9)!;
    expect(t.current).toBe('c');
    expect(t.nodes.c!.lastVisit).toBe(9);
    expect(goToTree(t, 'nope', 10)).toBeNull();
  });

  it('tells whether back and forward have a target', () => {
    const t = forked();
    expect(canGoBack(t)).toBe(true);
    expect(canGoForward(t)).toBe(false);
    expect(canGoBack(seedTree(entry('A'), 1))).toBe(false);
    expect(canGoForward(backTree(t, 6)!)).toBe(true);
  });

  it('gives the path from the root to a node', () => {
    expect(pathTo(forked(), 'd')).toEqual(['a', 'b', 'd']);
  });

  it('does not change the tree that it receives', () => {
    const t = forked();
    const before = JSON.stringify(t);
    navigateTree(t, entry('E'), 6, 'e');
    backTree(t, 6);
    forwardTree(t, 6);
    goToTree(t, 'c', 6);
    markNodes(t, ['c'], { missing: true });
    capTree(t, 1);
    expect(JSON.stringify(t)).toBe(before);
  });

  it('copies the entry of a tab', () => {
    expect(entryOf({ title: 'T', contentType: 'alpha', metadata: { k: 1 } })).toEqual({
      title: 'T',
      contentType: 'alpha',
      metadata: { k: 1 },
    });
    expect(entryOf({ title: 'T', contentType: 'alpha' })).toEqual({ title: 'T', contentType: 'alpha' });
  });

  it('merges a mark into the metadata of the named nodes only', () => {
    const t = markNodes(forked(), ['c', 'nope'], { missing: true });
    expect(t.nodes.c!.metadata).toEqual({ key: 'C', missing: true });
    expect(t.nodes.d!.metadata).toEqual({ key: 'D' });
  });
});

describe('the cap', () => {
  it('removes the leaf with the oldest visit first', () => {
    // a, b, c, d: c is the only leaf off the path.
    expect(Object.keys(capTree(forked(), 3).nodes).sort()).toEqual(['a', 'b', 'd']);
  });

  it('keeps the current node and each ancestor, also when they are the oldest', () => {
    const tree: NavTree = {
      current: 'c',
      nodes: { a: node('a', null, 1), b: node('b', 'a', 2), c: node('c', 'b', 3), s: node('s', 'a', 10) },
    };
    expect(Object.keys(capTree(tree, 3).nodes).sort()).toEqual(['a', 'b', 'c']);
  });

  it('removes the root of a single line that grows past the cap', () => {
    let t = seedTree(entry('n0'), 0, 'n0');
    for (let i = 1; i <= NAV_TREE_CAP; i += 1) t = navigateTree(t, entry(`n${i}`), i, `n${i}`);
    expect(Object.keys(t.nodes)).toHaveLength(NAV_TREE_CAP);
    expect(t.nodes.n0).toBeUndefined();
    expect(t.nodes.n1!.parent).toBeNull();
    expect(t.current).toBe(`n${NAV_TREE_CAP}`);
    expect(pathTo(t, t.current)).toHaveLength(NAV_TREE_CAP);
  });

  it('applies the cap on each navigate', () => {
    let t = seedTree(entry('root'), 0, 'root');
    for (let i = 1; i <= NAV_TREE_CAP + 20; i += 1) {
      t = navigateTree(t, entry(`n${i}`), i, `n${i}`);
      t = backTree(t, i)!;
    }
    expect(Object.keys(t.nodes)).toHaveLength(NAV_TREE_CAP);
    expect(t.current).toBe('root');
    // The oldest leaves went first.
    expect(t.nodes.n1).toBeUndefined();
    expect(t.nodes[`n${NAV_TREE_CAP + 20}`]).toBeDefined();
  });
});

describe('readNavTree', () => {
  it('returns a sound tree', () => {
    const t = forked();
    expect(readNavTree(JSON.parse(JSON.stringify(t)))).toEqual(t);
  });

  it.each([
    ['a value that is not an object', 'tree'],
    ['a missing current', { current: 'gone', nodes: { a: node('a', null) } }],
    ['a missing parent', { current: 'a', nodes: { a: node('a', null), b: node('b', 'gone') } }],
    ['a cycle', { current: 'a', nodes: { a: node('a', null), b: node('b', 'c'), c: node('c', 'b') } }],
    ['two roots', { current: 'a', nodes: { a: node('a', null), b: node('b', null) } }],
    ['no root', { current: 'a', nodes: { a: node('a', 'a') } }],
    ['a key that is not the node id', { current: 'a', nodes: { a: node('x', null) } }],
    ['a node with no title', { current: 'a', nodes: { a: { ...node('a', null), title: 3 } } }],
    ['a node with bad metadata', { current: 'a', nodes: { a: { ...node('a', null), metadata: [] } } }],
    ['a node with no visit time', { current: 'a', nodes: { a: { ...node('a', null), lastVisit: 'x' } } }],
  ])('refuses %s', (_name, raw) => {
    expect(readNavTree(raw)).toBeNull();
  });

  it('applies the cap to a stored tree that is too large', () => {
    const nodes: Record<string, NavNode> = { n0: node('n0', null, 0) };
    for (let i = 1; i <= NAV_TREE_CAP + 5; i += 1) nodes[`n${i}`] = node(`n${i}`, 'n0', i);
    const t = readNavTree({ current: 'n0', nodes })!;
    expect(Object.keys(t.nodes)).toHaveLength(NAV_TREE_CAP);
  });
});
```

**Step 2: Run the test and see it fail**

```bash
just web-test unit src/windowing/__tests__/nav-tree.test.ts
```

Expected: FAIL. Vitest reports `Failed to resolve import "@/windowing/model/nav-tree"`.

**Step 3: Add the types**

In `web/src/windowing/model/types.ts`, replace lines 3-11 with:

```ts
export interface Tab<C extends string = string> {
  id: string;
  title: string;
  icon?: Component<{ class?: string }>;
  contentType: C;
  isModified?: boolean;
  isPinned?: boolean;
  metadata?: Record<string, unknown>;
  /** Where the tab went before. Absent means that the tab never moved. */
  history?: NavTree<C>;
}

/** What a tab shows: the part of a tab that one navigation changes. */
export interface NavEntry<C extends string = string> {
  title: string;
  contentType: C;
  metadata?: Record<string, unknown>;
}

/**
 * One place in the history of a tab.
 *
 * The core never reads `metadata`. An app keeps its own keys there, for
 * example a file path or, later, a URL.
 */
export interface NavNode<C extends string = string> extends NavEntry<C> {
  id: string;
  parent: string | null;
  /** The time of the last move to this node, in milliseconds. */
  lastVisit: number;
}

/** The history of a tab: one root, and the node that the tab shows. */
export interface NavTree<C extends string = string> {
  nodes: Record<string, NavNode<C>>;
  current: string;
}
```

**Step 4: Write the pure functions**

Create `web/src/windowing/model/nav-tree.ts`:

```ts
import type { NavEntry, NavNode, NavTree, Tab } from './types';
import { generateId } from './tree';

/** The largest tree that a tab keeps. */
export const NAV_TREE_CAP = 100;

/** The entry that a tab shows now. */
export function entryOf<C extends string>(
  tab: Pick<Tab<C>, 'title' | 'contentType' | 'metadata'>,
): NavEntry<C> {
  return {
    title: tab.title,
    contentType: tab.contentType,
    ...(tab.metadata ? { metadata: { ...tab.metadata } } : {}),
  };
}

/** A tree with one node, which holds `entry`. */
export function seedTree<C extends string>(
  entry: NavEntry<C>,
  now: number,
  id: string = generateId(),
): NavTree<C> {
  return { nodes: { [id]: { ...entry, id, parent: null, lastVisit: now } }, current: id };
}

/** The ids of the children of `id`, the latest visit first. */
export function childrenOf<C extends string>(tree: NavTree<C>, id: string): string[] {
  return Object.values(tree.nodes)
    .filter((n) => n.parent === id)
    .sort((a, b) => b.lastVisit - a.lastVisit)
    .map((n) => n.id);
}

/** The ids from the root to `id`, the root first. */
export function pathTo<C extends string>(tree: NavTree<C>, id: string): string[] {
  const out: string[] = [];
  const size = Object.keys(tree.nodes).length;
  let at: string | null = id;
  // The size limit stops a loop in a tree that the reader did not check.
  while (at !== null && tree.nodes[at] && out.length <= size) {
    out.unshift(at);
    at = tree.nodes[at]!.parent;
  }
  return out;
}

function visit<C extends string>(tree: NavTree<C>, id: string, now: number): NavTree<C> {
  return { nodes: { ...tree.nodes, [id]: { ...tree.nodes[id]!, lastVisit: now } }, current: id };
}

/** Add `entry` as a child of the current node, and move to it. */
export function navigateTree<C extends string>(
  tree: NavTree<C>,
  entry: NavEntry<C>,
  now: number,
  id: string = generateId(),
): NavTree<C> {
  const added: NavNode<C> = { ...entry, id, parent: tree.current, lastVisit: now };
  return capTree({ nodes: { ...tree.nodes, [id]: added }, current: id }, NAV_TREE_CAP);
}

/** True when the current node has a parent. */
export function canGoBack<C extends string>(tree: NavTree<C>): boolean {
  return (tree.nodes[tree.current]?.parent ?? null) !== null;
}

/** True when the current node has a child. */
export function canGoForward<C extends string>(tree: NavTree<C>): boolean {
  return childrenOf(tree, tree.current).length > 0;
}

/** Move to the parent. Null at the root. */
export function backTree<C extends string>(tree: NavTree<C>, now: number): NavTree<C> | null {
  const parent = tree.nodes[tree.current]?.parent ?? null;
  return parent === null ? null : visit(tree, parent, now);
}

/** Move to the child with the latest visit. Null when the current node has no child. */
export function forwardTree<C extends string>(tree: NavTree<C>, now: number): NavTree<C> | null {
  const [latest] = childrenOf(tree, tree.current);
  return latest === undefined ? null : visit(tree, latest, now);
}

/** Move to any node. Null for an unknown id. */
export function goToTree<C extends string>(tree: NavTree<C>, id: string, now: number): NavTree<C> | null {
  return tree.nodes[id] ? visit(tree, id, now) : null;
}

/** Merge `mark` into the metadata of each node in `ids`. The core does not read the result. */
export function markNodes<C extends string>(
  tree: NavTree<C>,
  ids: readonly string[],
  mark: Record<string, unknown>,
): NavTree<C> {
  const nodes = { ...tree.nodes };
  for (const id of ids) {
    const found = nodes[id];
    if (found) nodes[id] = { ...found, metadata: { ...found.metadata, ...mark } };
  }
  return { nodes, current: tree.current };
}

/**
 * Remove nodes until the tree holds `cap` nodes or fewer.
 *
 * The leaf with the oldest visit goes first. The current node and its
 * ancestors stay. When no other leaf is left, the tree is one line from the
 * root to the current node. Then the root goes, and its child becomes the root.
 */
export function capTree<C extends string>(tree: NavTree<C>, cap: number): NavTree<C> {
  const nodes = { ...tree.nodes };
  let size = Object.keys(nodes).length;
  while (size > cap) {
    const kept = pathTo({ nodes, current: tree.current }, tree.current);
    const keptSet = new Set(kept);
    const parents = new Set(Object.values(nodes).map((n) => n.parent));
    const leaves = Object.values(nodes).filter((n) => !parents.has(n.id) && !keptSet.has(n.id));
    if (leaves.length > 0) {
      const oldest = leaves.reduce((a, b) => (b.lastVisit < a.lastVisit ? b : a));
      delete nodes[oldest.id];
    } else {
      const [root, child] = kept;
      if (root === undefined || child === undefined) break;
      delete nodes[root];
      nodes[child] = { ...nodes[child]!, parent: null };
    }
    size -= 1;
  }
  return { nodes, current: tree.current };
}

const isRecord = (v: unknown): v is Record<string, unknown> =>
  typeof v === 'object' && v !== null && !Array.isArray(v);

/**
 * The stored tree when it is sound, or null.
 *
 * Sound means: `current` names a node, each key is the id of its node, each
 * parent names a node, exactly one node is a root, and no parent chain makes
 * a loop. A sound tree that is too large gets the cap.
 */
export function readNavTree<C extends string>(raw: unknown): NavTree<C> | null {
  if (!isRecord(raw) || !isRecord(raw.nodes) || typeof raw.current !== 'string') return null;
  const nodes = raw.nodes;
  if (!isRecord(nodes[raw.current])) return null;
  let roots = 0;
  for (const [key, value] of Object.entries(nodes)) {
    if (!isRecord(value) || value.id !== key) return null;
    if (typeof value.title !== 'string' || typeof value.contentType !== 'string') return null;
    if (typeof value.lastVisit !== 'number' || !Number.isFinite(value.lastVisit)) return null;
    if (value.metadata !== undefined && !isRecord(value.metadata)) return null;
    if (value.parent === null) roots += 1;
    else if (typeof value.parent !== 'string' || !isRecord(nodes[value.parent])) return null;
  }
  if (roots !== 1) return null;
  const size = Object.keys(nodes).length;
  for (const key of Object.keys(nodes)) {
    let at: string | null = key;
    for (let steps = 0; at !== null; steps += 1) {
      // A chain longer than the tree goes around a loop.
      if (steps > size) return null;
      at = (nodes[at] as { parent: string | null }).parent;
    }
  }
  return capTree(raw as unknown as NavTree<C>, NAV_TREE_CAP);
}
```

**Step 5: Run the test and see it pass**

```bash
just web-test unit src/windowing/__tests__/nav-tree.test.ts src/windowing/__tests__/boundary.test.ts
```

Expected: PASS. Both files pass. The boundary test passes, because `nav-tree.ts` imports only from `./types` and `./tree`.

**Step 6: Break each new gate, then restore it**

1. In `capTree`, change `!keptSet.has(n.id)` to `true`. Run the test. Expected: `keeps the current node and each ancestor` fails. Restore the code.
2. In `readNavTree`, delete the loop check (the `for (const key of Object.keys(nodes))` block). Run the test. Expected: `refuses a cycle` fails. Restore the code.
3. In `readNavTree`, change `if (roots !== 1)` to `if (roots < 1)`. Run the test. Expected: `refuses two roots` fails. Restore the code.
4. Run the test again. Expected: PASS.

**Step 7: Commit**

```bash
git -C crates/crucible-web/web add src/windowing/model/types.ts \
  src/windowing/model/nav-tree.ts \
  src/windowing/__tests__/nav-tree.test.ts
git commit -m "feat(web): add a navigation tree to the windowing core

The core gets NavTree, NavNode and NavEntry, and pure functions that
move through a tree, mark nodes, apply the cap of 100 nodes and read a
stored tree. The cap keeps the current node and its ancestors.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 2: The history actions of the core store

**Files:**
- Modify: `web/src/windowing/store/tabActions.ts:1-27` (imports), `:56-79` (the `TabActions` interface), `:256-265` (after `updateTab`), `:419-430` (the returned object)
- Test: `web/src/windowing/__tests__/windowStore.history.test.ts`

The core actions take `groupId` and `tabId`, as `updateTab` and `removeTab` do. The design wrote `navigate(tabId, entry)`. The app host in Task 6 finds the group, so callers still name only the tab.

The core does not ask about unsaved changes. The app gate in Task 6 does that. So `WindowPolicy` does not change.

**Step 1: Write the failing test**

Create `web/src/windowing/__tests__/windowStore.history.test.ts`:

```ts
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { configureWindowing, windowStore, windowActions } from '@/windowing/store';
import { stubPolicy } from './stubPolicy';

/** The centre group of the stub seed. It holds one tab, `t1`, titled "One". */
function centre(): string {
  const layout = windowStore.layout;
  if (layout.type !== 'pane' || !layout.tabGroupId) throw new Error('the seed has no centre pane');
  return layout.tabGroupId;
}
const tab = () => windowStore.tabGroups[centre()]!.tabs.find((t) => t.id === 't1')!;
const rootId = () => Object.values(tab().history!.nodes).find((n) => n.parent === null)!.id;

let onActive: ReturnType<typeof vi.fn>;

beforeEach(() => {
  onActive = vi.fn();
  configureWindowing(stubPolicy({ onActiveTabChange: onActive }));
});

describe('the history actions', () => {
  it('navigate keeps the tab id, shows the entry, and seeds the tree from the old content', () => {
    windowActions.navigate(centre(), 't1', { title: 'Two', contentType: 'beta', metadata: { k: 2 } });
    const t = tab();
    expect(t.id).toBe('t1');
    expect(t).toMatchObject({ title: 'Two', contentType: 'beta', metadata: { k: 2 } });
    const tree = t.history!;
    expect(Object.keys(tree.nodes)).toHaveLength(2);
    expect(tree.nodes[rootId()]).toMatchObject({ title: 'One', contentType: 'alpha', parent: null });
    expect(tree.nodes[tree.current]!.parent).toBe(rootId());
  });

  it('back and forward show the fields of the node that they reach', () => {
    windowActions.navigate(centre(), 't1', { title: 'Two', contentType: 'beta' });
    expect(windowActions.back(centre(), 't1')).toBe(true);
    expect(tab()).toMatchObject({ title: 'One', contentType: 'alpha' });
    expect(windowActions.back(centre(), 't1')).toBe(false);
    expect(windowActions.forward(centre(), 't1')).toBe(true);
    expect(tab()).toMatchObject({ title: 'Two', contentType: 'beta' });
    expect(windowActions.forward(centre(), 't1')).toBe(false);
  });

  it('goTo reaches a node by id, and refuses an unknown id', () => {
    windowActions.navigate(centre(), 't1', { title: 'Two', contentType: 'beta' });
    expect(windowActions.goTo(centre(), 't1', rootId())).toBe(true);
    expect(tab().title).toBe('One');
    expect(windowActions.goTo(centre(), 't1', 'nope')).toBe(false);
    expect(tab().title).toBe('One');
  });

  it('tells the policy about the active tab after a move', () => {
    onActive.mockClear();
    windowActions.navigate(centre(), 't1', { title: 'Two', contentType: 'beta' });
    expect(onActive).toHaveBeenLastCalledWith(expect.objectContaining({ id: 't1', title: 'Two' }));
  });

  it('asks the policy for the icon of the new content type', () => {
    const Icon = () => null;
    configureWindowing(stubPolicy({ iconFor: (type) => (type === 'beta' ? Icon : undefined) }));
    windowActions.navigate(centre(), 't1', { title: 'Two', contentType: 'beta' });
    expect(tab().icon).toBe(Icon);
  });

  it('markHistory merges a mark into an old node', () => {
    windowActions.navigate(centre(), 't1', { title: 'Two', contentType: 'beta' });
    windowActions.markHistory(centre(), 't1', [rootId()], { missing: true });
    expect(tab().history!.nodes[rootId()]!.metadata).toEqual({ missing: true });
    expect(tab().title).toBe('Two');
  });

  it('does nothing for an unknown tab', () => {
    windowActions.navigate(centre(), 'nope', { title: 'X', contentType: 'beta' });
    expect(windowActions.back(centre(), 'nope')).toBe(false);
    windowActions.markHistory(centre(), 'nope', ['x'], { missing: true });
    expect(tab().history).toBeUndefined();
  });
});
```

**Step 2: Run the test and see it fail**

```bash
just web-test unit src/windowing/__tests__/windowStore.history.test.ts
```

Expected: FAIL. Each test reports `TypeError: windowActions.navigate is not a function` (or the same for `back` and `markHistory`).

**Step 3: Write the actions**

In `web/src/windowing/store/tabActions.ts`:

1. Change the type import at lines 2-10 to add `NavEntry` and `NavTree`:

```ts
import type {
  EdgePanelPosition,
  LayoutNode,
  NavEntry,
  NavTree,
  PaneDropPosition,
  SplitDirection,
  Tab,
  TabGroup,
  WindowState,
} from '../model/types';
```

2. After the import of `'../model/tree'` (line 26), add:

```ts
import {
  backTree,
  entryOf,
  forwardTree,
  goToTree,
  markNodes,
  navigateTree,
  seedTree,
} from '../model/nav-tree';
```

3. In the `TabActions` interface, after the `updateTab` member (line 68), add:

```ts
  /** Add `entry` below the current node of the tab, and show it. The tab keeps its id. */
  navigate(groupId: string, tabId: string, entry: NavEntry<C>): void;
  /** Show the parent node. False at the root or for an unknown tab. */
  back(groupId: string, tabId: string): boolean;
  /** Show the child with the latest visit. False when there is no child. */
  forward(groupId: string, tabId: string): boolean;
  /** Show any node of the tree. False for an unknown node. */
  goTo(groupId: string, tabId: string, nodeId: string): boolean;
  /** Merge `mark` into the metadata of the named nodes. The tab fields do not change. */
  markHistory(groupId: string, tabId: string, nodeIds: readonly string[], mark: Record<string, unknown>): void;
```

4. After the `updateTab` function (after line 265), add:

```ts
  const findTab = (groupId: string, tabId: string): Tab<C> | undefined =>
    store.tabGroups[groupId]?.tabs.find((t) => t.id === tabId);

  // A tab that never moved has no tree. Its first move starts one from what
  // the tab shows now.
  const historyOf = (tab: Tab<C>, now: number): NavTree<C> =>
    tab.history ?? seedTree(entryOf(tab), now);

  /** Write `tree` to the tab, and give the tab the fields of the current node. */
  const showNode = (groupId: string, tabId: string, tree: NavTree<C>) => {
    const node = tree.nodes[tree.current]!;
    updateTab(groupId, tabId, {
      history: tree,
      title: node.title,
      contentType: node.contentType,
      metadata: node.metadata ? { ...node.metadata } : undefined,
      icon: policy().iconFor(node.contentType),
    });
    if (store.tabGroups[groupId]?.activeTabId === tabId) {
      policy().onActiveTabChange(findTab(groupId, tabId));
    }
  };

  const moveWith = (
    groupId: string,
    tabId: string,
    move: (tree: NavTree<C>, now: number) => NavTree<C> | null,
  ): boolean => {
    const tab = findTab(groupId, tabId);
    if (!tab) return false;
    const now = Date.now();
    const next = move(historyOf(tab, now), now);
    if (!next) return false;
    showNode(groupId, tabId, next);
    return true;
  };

  const navigate = (groupId: string, tabId: string, entry: NavEntry<C>) => {
    moveWith(groupId, tabId, (tree, now) => navigateTree(tree, entry, now));
  };
  const back = (groupId: string, tabId: string) => moveWith(groupId, tabId, backTree);
  const forward = (groupId: string, tabId: string) => moveWith(groupId, tabId, forwardTree);
  const goTo = (groupId: string, tabId: string, nodeId: string) =>
    moveWith(groupId, tabId, (tree, now) => goToTree(tree, nodeId, now));

  const markHistory = (
    groupId: string,
    tabId: string,
    nodeIds: readonly string[],
    mark: Record<string, unknown>,
  ) => {
    const tree = findTab(groupId, tabId)?.history;
    if (tree) updateTab(groupId, tabId, { history: markNodes(tree, nodeIds, mark) });
  };
```

5. In the returned object (lines 419-430), add `navigate, back, forward, goTo, markHistory,` after `updateTab,`.

**Step 4: Run the test and see it pass**

```bash
just web-test unit src/windowing/__tests__/
```

Expected: PASS. All the core tests pass, the boundary test also.

**Step 5: Break the gate, then restore it**

1. In `showNode`, delete the line `contentType: node.contentType,`. Run `just web-test unit src/windowing/__tests__/windowStore.history.test.ts`. Expected: `navigate keeps the tab id…` and `back and forward…` fail. Restore the line.
2. Delete the `if (store.tabGroups[groupId]?.activeTabId === tabId)` block. Run the test. Expected: `tells the policy about the active tab` fails. Restore the block.

**Step 6: Commit**

```bash
git -C crates/crucible-web/web add src/windowing/store/tabActions.ts \
  src/windowing/__tests__/windowStore.history.test.ts
git commit -m "feat(web): move a tab through its history in the core store

The store gets navigate, back, forward, goTo and markHistory. A move
keeps the tab id and gives the tab the title, the content type, the
metadata and the icon of the node that it reaches.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 3: The saved layout goes to v11

**Files:**
- Modify: `web/src/windowing/model/serializer.ts:1-12` (imports), `:22` (`LAYOUT_VERSION`), `:24-31` (`SerializedTab`), `:66-73` (add `SerializedLayoutV10`), `:75-79` (`StoredLayout`), `:145-157` (`migrateV9toV10`), after `:157` (add `migrateV10toV11`), `:159-167` (guards), `:180-216` (the reader)
- Modify: `web/src/stores/__tests__/layoutMigrations.property.test.ts:23-30` (`arbTab`)
- Test: `web/src/windowing/__tests__/serializer.test.ts` (append)

**Step 1: Write the failing test**

Append to `web/src/windowing/__tests__/serializer.test.ts`. First, extend the type import at the top of the file:

```ts
import type {
  LayoutCodecHooks,
  RestoredLayout,
  SerializedLayout,
  SerializedLayoutV10,
  SerializedLayoutV9,
} from '@/windowing/model/serializer';
import { navigateTree, seedTree } from '@/windowing/model/nav-tree';
```

Then append:

```ts
/** A stored v10 layout: modes on the rails, and no tab history. */
function v10Fixture(): SerializedLayoutV10<Kind> {
  return {
    version: 10,
    layout: { id: 'p', type: 'pane', tabGroupId: 'g' },
    tabGroups: {
      g: {
        id: 'g',
        tabs: [{ id: 't', title: 'T', contentType: 'beta', metadata: { path: '/n.md' } }],
        activeTabId: 't',
      },
    },
    edgePanels: {
      left: { id: 'l', layout: { id: 'lp', type: 'pane', tabGroupId: null }, mode: 'docked' },
      right: { id: 'r', layout: { id: 'rp', type: 'pane', tabGroupId: null }, mode: 'strip' },
    },
    floatingWindows: [],
  };
}

/** The v10 fixture at v11, with `history` as the stored tree of its one tab. */
function v11Fixture(history: unknown): SerializedLayout<Kind> {
  const v10 = v10Fixture();
  const tab = { ...v10.tabGroups.g!.tabs[0]!, history };
  return {
    ...v10,
    version: 11,
    tabGroups: { g: { ...v10.tabGroups.g!, tabs: [tab as never] } },
  };
}

const onlyTab = (out: RestoredLayout<Kind>) => out.tabGroups.g!.tabs[0]!;

describe('v10 → v11: tab history', () => {
  it('gives each tab a tree with one node that holds its content', () => {
    const tree = onlyTab(deserializeLayout(v10Fixture(), stubHooks())).history!;
    const nodes = Object.values(tree.nodes);
    expect(nodes).toHaveLength(1);
    expect(nodes[0]).toMatchObject({
      id: tree.current,
      parent: null,
      title: 'T',
      contentType: 'beta',
      metadata: { path: '/n.md' },
    });
  });

  it('carries a v9 payload through both steps', () => {
    const tab = deserializeLayout(v9Fixture(), stubHooks()).tabGroups.g!.tabs[0]!;
    expect(Object.keys(tab.history!.nodes)).toHaveLength(1);
  });

  it('round-trips a tree', () => {
    let tree = seedTree<Kind>({ title: 'T', contentType: 'beta' }, 1, 'a');
    tree = navigateTree(tree, { title: 'U', contentType: 'alpha', metadata: { path: '/u.md' } }, 2, 'b');
    const out = deserializeLayout(v11Fixture(tree), stubHooks());
    expect(onlyTab(out).history).toEqual(tree);
    expect(serializeLayout(out).tabGroups.g!.tabs[0]!.history).toEqual(tree);
  });

  it('leaves an absent tree absent in a current layout', () => {
    const payload = v11Fixture(undefined);
    delete (payload.tabGroups.g!.tabs[0] as { history?: unknown }).history;
    expect('history' in onlyTab(deserializeLayout(payload, stubHooks()))).toBe(false);
  });

  const node = (id: string, parent: string | null) => ({
    id,
    parent,
    lastVisit: 1,
    title: id,
    contentType: 'alpha',
  });

  it.each([
    ['a cycle', { current: 'a', nodes: { a: node('a', null), b: node('b', 'c'), c: node('c', 'b') } }],
    ['a missing parent', { current: 'a', nodes: { a: node('a', null), b: node('b', 'gone') } }],
    ['a missing current', { current: 'gone', nodes: { a: node('a', null) } }],
    ['a value that is not a tree', 'tree'],
  ])('repairs %s to one node that holds the content of the tab', (_name, history) => {
    const tab = onlyTab(deserializeLayout(v11Fixture(history), stubHooks()));
    expect(tab).toMatchObject({ title: 'T', contentType: 'beta', metadata: { path: '/n.md' } });
    const nodes = Object.values(tab.history!.nodes);
    expect(nodes).toHaveLength(1);
    expect(nodes[0]).toMatchObject({ parent: null, title: 'T', contentType: 'beta', metadata: { path: '/n.md' } });
  });

  it('writes v11, and writes the tree without an icon', () => {
    const state = defaultLayoutFixture();
    const tab = state.tabGroups['centre-group']!.tabs[0]!;
    tab.history = seedTree({ title: 'One', contentType: 'alpha' }, 1, 'n');
    const saved = serializeLayout(state);
    expect(saved.version).toBe(11);
    const written = saved.tabGroups['centre-group']!.tabs[0]!;
    expect(written.history).toEqual(tab.history);
    expect('icon' in written).toBe(false);
  });
});
```

**Step 2: Run the test and see it fail**

```bash
just web-test unit src/windowing/__tests__/serializer.test.ts
```

Expected: FAIL. The type import fails at typecheck only, so vitest runs. `gives each tab a tree` fails with `Cannot read properties of undefined (reading 'nodes')`. The repair cases fail because `history` stays as the stored value. `writes v11` fails with `expected 10 to be 11`.

**Step 3: Write the code**

In `web/src/windowing/model/serializer.ts`:

1. Add to the imports at the top:

```ts
import { entryOf, readNavTree, seedTree } from './nav-tree';
```

and add `NavTree` to the type import from `./types`.

2. Replace line 22 and its comment (lines 14-22) with:

```ts
/**
 * The saved layout format.
 *
 * The core owns the CURRENT format and the steps into it from v9. The
 * history before v9 belongs to the app, because each of those steps names
 * content the app had at the time: a navigator, a bottom terminal, a settings
 * page. The app gives that history to the reader as `upgradeLegacy`.
 */
export const LAYOUT_VERSION = 11;
```

3. In `SerializedTab` (lines 24-31), add after `metadata?`:

```ts
  /** The history of the tab. Absent means that the tab never moved. */
  history?: NavTree<C>;
```

4. After `SerializedLayout` (line 55), add:

```ts
/** The format before v11: the same shape, and no tab history. */
export interface SerializedLayoutV10<C extends string = string> {
  version: 10;
  layout: LayoutNode;
  tabGroups: Record<string, SerializedTabGroup<C>>;
  edgePanels: Record<EdgePanelPosition, SerializedEdgePanel>;
  floatingWindows: FloatingWindow[];
}
```

5. Change `StoredLayout` (lines 75-79) to:

```ts
export type StoredLayout<C extends string = string> =
  | SerializedLayout<C>
  | SerializedLayoutV10<C>
  | SerializedLayoutV9<C>
  | { version: number };
```

6. Change `migrateV9toV10` (lines 145-157) to return `SerializedLayoutV10<C>`, with `version: 10` in place of `version: LAYOUT_VERSION`:

```ts
function migrateV9toV10<C extends string>(v9: SerializedLayoutV9<C>): SerializedLayoutV10<C> {
  // A partial payload reaches this step too, so an absent rail stays absent.
  // The rebuild below gives it a panel.
  const edgePanels = {} as Record<EdgePanelPosition, SerializedEdgePanel>;
  for (const [pos, panel] of Object.entries(v9.edgePanels ?? {})) {
    const { isCollapsed, ...rest } = panel;
    edgePanels[pos as EdgePanelPosition] = {
      ...rest,
      mode: isCollapsed ? 'strip' : 'docked',
    };
  }
  return { ...v9, version: 10, edgePanels };
}

// v11 gives each tab a history. A tab starts with one node, which holds what
// the tab shows now. The visit time is 0, because the payload holds no time.
function migrateV10toV11<C extends string>(v10: SerializedLayoutV10<C>): SerializedLayout<C> {
  const tabGroups: Record<string, SerializedTabGroup<C>> = {};
  for (const [id, group] of Object.entries(v10.tabGroups ?? {})) {
    tabGroups[id] = {
      ...group,
      tabs: group.tabs.map((t) => ({ ...t, history: seedTree(entryOf(t), 0) })),
    };
  }
  return { ...v10, version: LAYOUT_VERSION, tabGroups };
}
```

7. After `isLegacyV9` (line 162), add:

```ts
/** A stored layout at v10, the last version before tab history. */
export function isLegacyV10<C extends string>(s: { version: number }): s is SerializedLayoutV10<C> {
  return s.version === 10;
}
```

8. Before `deserializeLayout`, add:

```ts
/**
 * A stored tab, with its history checked.
 *
 * A bad tree does not fail the read. The tab keeps what it shows, as the only
 * node of a new tree. An absent tree stays absent, so a tab that never moved
 * reads back as it was written.
 */
function readTab<C extends string>(stored: SerializedTab<C>): Tab<C> {
  const { history, ...rest } = stored;
  if (history === undefined) return { ...rest };
  return { ...rest, history: readNavTree<C>(history) ?? seedTree(entryOf(rest), 0) };
}
```

9. In the reader doc comment (lines 180-186), change "the step to v10" to "the steps to v10 and v11". Change the dispatch (lines 204-207) to:

```ts
  let current: SerializedLayout<C>;
  if (isLegacyV9<C>(stored)) current = migrateV10toV11(migrateV9toV10(stored));
  else if (isLegacyV10<C>(stored)) current = migrateV10toV11(stored);
  else if (isCurrentLayout<C>(stored)) current = stored;
  else throw unsupported(stored.version);
```

10. In the group copy (line 213), change `tabs: group.tabs.map((t) => ({ ...t })),` to `tabs: group.tabs.map(readTab),`.

**Step 4: Give the property test a history**

In `web/src/stores/__tests__/layoutMigrations.property.test.ts`, add to the imports:

```ts
import { backTree, navigateTree, seedTree } from '@/windowing/model/nav-tree';
```

Before `arbTab` (line 23), add:

```ts
// A small tree: a line of navigations, then some moves back. `history` is
// the LAST key of the tab, because the reader puts it last.
const arbNavTree = fc.tuple(fc.integer({ min: 0, max: 5 }), fc.integer({ min: 0, max: 5 })).map(([ahead, back]) => {
  let t = seedTree<TabContentType>({ title: 'n0', contentType: 'file' }, 0, 'n0');
  for (let i = 1; i <= ahead; i += 1) t = navigateTree(t, { title: `n${i}`, contentType: 'file', metadata: { i } }, i, `n${i}`);
  for (let i = 0; i < back; i += 1) t = backTree(t, ahead + i + 1) ?? t;
  return t;
});
```

In `arbTab`, add as the last field:

```ts
  history: fc.option(arbNavTree, { freq: 2, nil: undefined }),
```

`fc.record` writes a key with the value `undefined` when the option gives `nil`. `JSON.stringify` drops such a key, so the round-trip check still holds. Check that `TabContentType` is in the imports of the file. Add it if it is absent.

**Step 5: Run the tests and see them pass**

```bash
just web-test unit src/windowing/__tests__/ src/stores/__tests__/layoutMigrations.test.ts src/stores/__tests__/layoutMigrations.property.test.ts
```

Expected: PASS. The existing test `prunes a current (v10) layout on every restore` still passes, because the v10 payload now goes through the v11 step.

**Step 6: Typecheck**

```bash
cd crates/crucible-web/web && bun run typecheck
```

Expected: no error.

**Step 7: Break the gate, then restore it**

1. In `readTab`, change the last line to `return { ...rest, history };`. Run `just web-test unit src/windowing/__tests__/serializer.test.ts`. Expected: the four `repairs …` cases fail. Restore the line.
2. In the dispatch, change `migrateV10toV11(stored)` to `stored as never`. Run the test. Expected: `gives each tab a tree` fails. Restore the line.

**Step 8: Commit**

```bash
git -C crates/crucible-web/web add src/windowing/model/serializer.ts \
  src/windowing/__tests__/serializer.test.ts \
  src/stores/__tests__/layoutMigrations.property.test.ts
git commit -m "feat(web): write the tab history to the saved layout (v11)

The step from v10 gives each tab a tree with one node. The reader
repairs a bad tree: the tab keeps its content as the only node.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 4: The lanes of the history graph

**Files:**
- Create: `web/src/windowing/model/nav-layout.ts`
- Test: `web/src/windowing/__tests__/nav-layout.test.ts`

`layoutNavTree` goes in the core. It reads only the tree shape, never `metadata`.

**Step 1: Write the failing test**

Create `web/src/windowing/__tests__/nav-layout.test.ts`:

```ts
import { describe, it, expect } from 'vitest';
import { layoutNavTree, type NavRow } from '@/windowing/model/nav-layout';
import { backTree, goToTree, navigateTree, seedTree } from '@/windowing/model/nav-tree';
import type { NavEntry, NavTree } from '@/windowing/model/types';

const entry = (title: string): NavEntry => ({ title, contentType: 'alpha' });
const lanes = (rows: NavRow[]) => rows.map((r) => [r.id, r.lane]);

function line(): NavTree {
  let t = seedTree(entry('A'), 1, 'a');
  t = navigateTree(t, entry('B'), 2, 'b');
  return navigateTree(t, entry('C'), 3, 'c');
}

/** a → b → c, then back to b and on to d. The user left c. */
function fork(): NavTree {
  let t = seedTree(entry('A'), 1, 'a');
  t = navigateTree(t, entry('B'), 2, 'b');
  t = navigateTree(t, entry('C'), 3, 'c');
  t = backTree(t, 4)!;
  return navigateTree(t, entry('D'), 5, 'd');
}

/** a → b → c is the path. b → e is a branch, and e forks again into f and g. */
function deepFork(): NavTree {
  let t = seedTree(entry('A'), 1, 'a');
  t = navigateTree(t, entry('B'), 2, 'b');
  t = navigateTree(t, entry('E'), 3, 'e');
  t = navigateTree(t, entry('F'), 4, 'f');
  t = backTree(t, 5)!;
  t = navigateTree(t, entry('G'), 6, 'g');
  t = goToTree(t, 'b', 7)!;
  return navigateTree(t, entry('C'), 8, 'c');
}

describe('layoutNavTree', () => {
  it('draws a line in lane 0', () => {
    const rows = layoutNavTree(line());
    expect(lanes(rows)).toEqual([['a', 0], ['b', 0], ['c', 0]]);
    expect(rows.map((r) => r.up)).toEqual([false, true, true]);
    expect(rows.map((r) => r.down)).toEqual([true, true, false]);
    expect(rows.map((r) => r.current)).toEqual([false, false, true]);
    expect(rows.every((r) => r.onPath && r.join === null && r.pass.length === 0)).toBe(true);
  });

  it('draws a fork: the left branch gets lane 1, and lane 0 runs past its row', () => {
    const rows = layoutNavTree(fork());
    expect(lanes(rows)).toEqual([['a', 0], ['b', 0], ['c', 1], ['d', 0]]);
    const c = rows[2]!;
    expect(c).toMatchObject({ up: false, join: 0, down: false, onPath: false, pass: [0] });
    expect(rows[3]).toMatchObject({ up: true, join: null, current: true, pass: [] });
  });

  it('draws a deep fork with one lane for each branch', () => {
    const rows = layoutNavTree(deepFork());
    expect(lanes(rows)).toEqual([['a', 0], ['b', 0], ['e', 1], ['f', 2], ['g', 1], ['c', 0]]);
    const byId = Object.fromEntries(rows.map((r) => [r.id, r]));
    expect(byId.e).toMatchObject({ join: 0, up: false, down: true, pass: [0] });
    expect(byId.f).toMatchObject({ join: 1, up: false, down: false, pass: [0, 1] });
    expect(byId.g).toMatchObject({ join: null, up: true, pass: [0] });
    expect(byId.c).toMatchObject({ up: true, current: true, pass: [] });
  });

  it('draws the children of the current node in new lanes below it', () => {
    // b is current. Its children d (visit 5) and c (visit 3) each open a lane.
    // The line from b to c runs in lane 0 through the row of d.
    const rows = layoutNavTree(goToTree(fork(), 'b', 9)!);
    expect(lanes(rows)).toEqual([['a', 0], ['b', 0], ['d', 1], ['c', 2]]);
    expect(rows[2]).toMatchObject({ join: 0, pass: [0] });
    expect(rows[3]).toMatchObject({ join: 0, pass: [] });
  });

  it('marks only the path from the root to the current node', () => {
    const rows = layoutNavTree(deepFork());
    expect(rows.filter((r) => r.onPath).map((r) => r.id)).toEqual(['a', 'b', 'c']);
  });
});
```

**Step 2: Run the test and see it fail**

```bash
just web-test unit src/windowing/__tests__/nav-layout.test.ts
```

Expected: FAIL with `Failed to resolve import "@/windowing/model/nav-layout"`.

**Step 3: Write the layout**

Create `web/src/windowing/model/nav-layout.ts`:

```ts
import type { NavTree } from './types';
import { childrenOf, pathTo } from './nav-tree';

/** One row of the drawn tree. Rows go from top to bottom, and lanes from left to right. */
export interface NavRow {
  id: string;
  lane: number;
  /** True when a line comes down from the parent in the same lane. */
  up: boolean;
  /** The lane of the parent when that lane is another lane, else null. */
  join: number | null;
  /** True when a line goes down from the node to a child. */
  down: boolean;
  /** The lanes whose line crosses this row without a node, in order. */
  pass: number[];
  current: boolean;
  /** True for the current node and its ancestors. */
  onPath: boolean;
}

/**
 * Put each node of the tree on a row and a lane, as a git graph does.
 *
 * Lane 0 holds the path from the root to the current node. At each node, the
 * child that continues the lane comes last. Thus the branches come first, and
 * the lane runs down past them. On the path, the next node of the path
 * continues the lane. Off the path, the child with the latest visit continues
 * it. Each other child opens a new lane. A lane never holds two branches, so a
 * straight line in a lane never crosses a node.
 */
export function layoutNavTree(tree: NavTree): NavRow[] {
  const path = pathTo(tree, tree.current);
  const onPath = new Set(path);
  const rows: NavRow[] = [];
  const rowOf = new Map<string, number>();
  const laneOf = new Map<string, number>();
  let lastLane = 0;

  const place = (id: string, lane: number): void => {
    const parent = tree.nodes[id]!.parent;
    const parentLane = parent === null ? null : laneOf.get(parent)!;
    const kids = childrenOf(tree, id);
    laneOf.set(id, lane);
    rowOf.set(id, rows.length);
    rows.push({
      id,
      lane,
      up: parentLane === lane,
      join: parentLane !== null && parentLane !== lane ? parentLane : null,
      down: kids.length > 0,
      pass: [],
      current: id === tree.current,
      onPath: onPath.has(id),
    });
    const next = onPath.has(id) ? path[path.indexOf(id) + 1] : kids[0];
    for (const kid of kids) {
      if (kid !== next) place(kid, (lastLane += 1));
    }
    if (next !== undefined) place(next, lane);
  };

  const root = path[0];
  if (root === undefined) return rows;
  place(root, 0);

  // The line of a parent runs in its lane through each row between the
  // parent and the child.
  for (const row of rows) {
    const parent = tree.nodes[row.id]!.parent;
    if (parent === null) continue;
    const lane = laneOf.get(parent)!;
    for (let r = rowOf.get(parent)! + 1; r < rowOf.get(row.id)!; r += 1) {
      const pass = rows[r]!.pass;
      if (!pass.includes(lane)) pass.push(lane);
    }
  }
  for (const row of rows) row.pass.sort((a, b) => a - b);
  return rows;
}
```

**Step 4: Run the test and see it pass**

```bash
just web-test unit src/windowing/__tests__/nav-layout.test.ts src/windowing/__tests__/boundary.test.ts
```

Expected: PASS.

**Step 5: Break the gate, then restore it**

1. Move the line `if (next !== undefined) place(next, lane);` above the `for (const kid of kids)` loop. Run the test. Expected: the fork cases fail, because the row order changes. Restore the order.
2. Delete the `if (!pass.includes(lane))` guard, so that `pass.push(lane)` always runs. Run the test. Expected: `draws a deep fork` fails with a repeated lane in `pass`. Restore the guard.

**Step 6: Commit**

```bash
git -C crates/crucible-web/web add src/windowing/model/nav-layout.ts \
  src/windowing/__tests__/nav-layout.test.ts
git commit -m "feat(web): calculate the lanes of the tab history graph

layoutNavTree puts each node on a row and a lane. Lane 0 holds the
path to the current node, and each branch that the user left gets its
own lane.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 5: The app modal for unsaved changes

**Files:**
- Create: `web/src/lib/confirm-discard.ts`
- Create: `web/src/components/ConfirmDiscardHost.tsx`
- Modify: `web/src/App.tsx:27` (import) and `:337` (mount the host beside `<NotificationToast />`)
- Modify: `web/src/contexts/EditorContext.tsx:1-25` (import), `:109-131` (`evictFile`), `:134-151` (`closeFile`), `:305-309` (`reloadFile`)
- Test: `web/src/components/__tests__/ConfirmDiscardHost.test.tsx`
- Test: `web/src/contexts/__tests__/EditorContext.test.tsx:1-85` (mock), `:161-208`, `:799`, `:819-823`

The project has no Ark `Dialog` use yet. `ExportDialog.tsx` and `PluginCommandDialog.tsx` draw their own overlay. This task uses Ark `Dialog` from `@ark-ui/solid` (5.35.0), because it gives the focus trap, Escape and the outside click.

The promise API lives in `lib/`, and the component lives in `components/`. Thus the editor context and the tab host can import the API without a component.

**Step 1: Write the failing test for the host**

Create `web/src/components/__tests__/ConfirmDiscardHost.test.tsx`:

```tsx
import { describe, it, expect, afterEach, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@solidjs/testing-library';
import { ConfirmDiscardHost } from '@/components/ConfirmDiscardHost';
import { confirmDiscard } from '@/lib/confirm-discard';

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe('confirmDiscard', () => {
  // This case must run first: no host is mounted yet.
  it('answers false when no host is mounted, so no edit is lost', async () => {
    vi.spyOn(console, 'warn').mockImplementation(() => {});
    await expect(confirmDiscard('a.md')).resolves.toBe(false);
  });

  it('shows the file name, and answers true when the user discards', async () => {
    render(() => <ConfirmDiscardHost />);
    const answer = confirmDiscard('a.md');
    expect(await screen.findByText(/a\.md has changes that are not saved/)).toBeTruthy();
    fireEvent.click(screen.getByTestId('confirm-discard-discard'));
    await expect(answer).resolves.toBe(true);
    await waitFor(() => expect(screen.queryByTestId('confirm-discard')).toBeNull());
  });

  it('answers false when the user keeps the changes', async () => {
    render(() => <ConfirmDiscardHost />);
    const answer = confirmDiscard('a.md');
    fireEvent.click(await screen.findByTestId('confirm-discard-keep'));
    await expect(answer).resolves.toBe(false);
  });

  it('answers false on Escape', async () => {
    render(() => <ConfirmDiscardHost />);
    const answer = confirmDiscard('a.md');
    const dialog = await screen.findByTestId('confirm-discard');
    fireEvent.keyDown(dialog, { key: 'Escape' });
    await expect(answer).resolves.toBe(false);
  });

  it('answers false to a second question while the first is open', async () => {
    render(() => <ConfirmDiscardHost />);
    const first = confirmDiscard('a.md');
    await expect(confirmDiscard('b.md')).resolves.toBe(false);
    expect(await screen.findByText(/a\.md/)).toBeTruthy();
    fireEvent.click(screen.getByTestId('confirm-discard-discard'));
    await expect(first).resolves.toBe(true);
  });

  it('answers false when the host unmounts with a question open', async () => {
    const { unmount } = render(() => <ConfirmDiscardHost />);
    const answer = confirmDiscard('a.md');
    await screen.findByTestId('confirm-discard');
    unmount();
    await expect(answer).resolves.toBe(false);
  });

  it('never calls the browser prompt', async () => {
    const native = vi.spyOn(window, 'confirm');
    render(() => <ConfirmDiscardHost />);
    const answer = confirmDiscard('a.md');
    fireEvent.click(await screen.findByTestId('confirm-discard-keep'));
    await answer;
    expect(native).not.toHaveBeenCalled();
  });
});
```

**Step 2: Run the test and see it fail**

```bash
just web-test unit src/components/__tests__/ConfirmDiscardHost.test.tsx
```

Expected: FAIL with `Failed to resolve import "@/components/ConfirmDiscardHost"`.

**Step 3: Write the promise API**

Create `web/src/lib/confirm-discard.ts`:

```ts
import { createSignal } from 'solid-js';

/** One open question to the user. */
export interface DiscardRequest {
  filename: string;
  answer: (discard: boolean) => void;
}

const [request, setRequest] = createSignal<DiscardRequest | null>(null);
let hosts = 0;

/** The open question. The one host reads it. */
export const discardRequest = request;

/**
 * Register the host that draws the question. Returns the release.
 *
 * A release answers an open question with false, so a promise never waits
 * on a modal that is gone.
 */
export function attachDiscardHost(): () => void {
  hosts += 1;
  return () => {
    hosts -= 1;
    request()?.answer(false);
  };
}

/**
 * Ask the user to discard the unsaved changes to `filename`.
 *
 * The answer is true only when the user confirms. An app modal asks, not
 * `window.confirm`: an installed PWA can suppress a browser prompt, and a
 * suppressed prompt looks like a broken button. With no host, the answer is
 * false, so no edit is lost. A second question while one is open also gets
 * false, and the first question keeps the modal.
 */
export function confirmDiscard(filename: string): Promise<boolean> {
  if (hosts === 0) {
    console.warn('confirmDiscard: no host is mounted, so the changes stay');
    return Promise.resolve(false);
  }
  if (request()) return Promise.resolve(false);
  return new Promise((resolve) => {
    setRequest({
      filename,
      answer: (discard) => {
        setRequest(null);
        resolve(discard);
      },
    });
  });
}
```

**Step 4: Write the host**

Create `web/src/components/ConfirmDiscardHost.tsx`:

```tsx
import { Component, onCleanup } from 'solid-js';
import { Portal } from 'solid-js/web';
import { Dialog } from '@ark-ui/solid';
import { attachDiscardHost, discardRequest } from '@/lib/confirm-discard';
import { btnNeutral, btnPrimary } from '@/lib/button-style';

/**
 * The one modal that asks to discard unsaved changes.
 *
 * Mount it once, at the app root, beside the toasts, so that both shells
 * have it. It uses the app theme and the app keys. Escape and a click
 * outside keep the changes. The first focus goes to Keep, the safe answer.
 */
export const ConfirmDiscardHost: Component = () => {
  onCleanup(attachDiscardHost());
  const answer = (discard: boolean) => discardRequest()?.answer(discard);
  return (
    <Dialog.Root
      open={discardRequest() !== null}
      onOpenChange={(details) => {
        if (!details.open) answer(false);
      }}
      role="alertdialog"
      lazyMount
      unmountOnExit
    >
      <Portal>
        <Dialog.Backdrop class="fixed inset-0 z-[110] bg-black/65" />
        <Dialog.Positioner class="fixed inset-0 z-[120] flex items-center justify-center p-4">
          <Dialog.Content
            data-testid="confirm-discard"
            class="w-[min(420px,92vw)] rounded-xl border border-hairline bg-surface-overlay p-4 shadow-2xl focus-ring"
          >
            <Dialog.Title class="text-sm font-medium text-shell-ink">Discard unsaved changes?</Dialog.Title>
            <Dialog.Description class="mt-1 text-reading text-muted-dark">
              {discardRequest()?.filename} has changes that are not saved.
            </Dialog.Description>
            <div class="mt-4 flex justify-end gap-2">
              <button
                type="button"
                data-testid="confirm-discard-keep"
                class={`h-8 ${btnNeutral} focus-ring`}
                onClick={() => answer(false)}
              >
                Keep
              </button>
              <button
                type="button"
                data-testid="confirm-discard-discard"
                class={`h-8 ${btnPrimary} focus-ring`}
                onClick={() => answer(true)}
              >
                Discard
              </button>
            </div>
          </Dialog.Content>
        </Dialog.Positioner>
      </Portal>
    </Dialog.Root>
  );
};
```

**Step 5: Run the host test and see it pass**

```bash
just web-test unit src/components/__tests__/ConfirmDiscardHost.test.tsx
```

Expected: PASS. If the Escape case fails in jsdom, find the target that the zag dismiss layer listens on (`node_modules/@zag-js/dismissable`). Send the key to that target. Do not delete the case.

**Step 6: Write the failing editor tests**

In `web/src/contexts/__tests__/EditorContext.test.tsx`, add after the last `vi.mock` block (after line 85):

```ts
// The editor asks through the app modal. The test answers for the user.
const confirmDiscard = vi.hoisted(() => vi.fn(async (_filename: string) => false));
vi.mock('@/lib/confirm-discard', () => ({ confirmDiscard }));
```

Add `confirmDiscard.mockReset(); confirmDiscard.mockResolvedValue(false);` to the `beforeEach` of the file.

Then change the four close-guard tests (lines 161-208) and the two reload tests (lines 799 and 819):

- In `closing a dirty file asks for confirmation and keeps it open on cancel`, replace the `window.confirm` spy line with `const native = vi.spyOn(window, 'confirm');`. Replace the three lines after `editor.closeFile(path);` with:

```ts
    await waitFor(() => expect(confirmDiscard).toHaveBeenCalledWith('dirty.md'));
    await Promise.resolve();
    expect(editor.openFiles().length).toBe(1);
    expect(native).not.toHaveBeenCalled();
```

- In `closing a dirty file discards when the user confirms`, replace the spy line with `confirmDiscard.mockResolvedValueOnce(true);`. Replace the last assertion with `await waitFor(() => expect(editor.openFiles().length).toBe(0));`.
- In `closing a clean file never prompts` and `force-close skips the prompt`, delete the spy line. Replace `expect(confirm).not.toHaveBeenCalled();` with `expect(confirmDiscard).not.toHaveBeenCalled();`.
- In `reload takes the disk text and the new base`, replace the spy line (line 799) with `confirmDiscard.mockResolvedValueOnce(true);`.
- In `reload asks before it discards unsaved edits`, replace the spy line (line 819) with `const native = vi.spyOn(window, 'confirm');`. Replace `expect(confirm).toHaveBeenCalledOnce();` with `expect(confirmDiscard).toHaveBeenCalledOnce();` and `expect(native).not.toHaveBeenCalled();`.

Add one new case after `closing a dirty file discards when the user confirms`:

```ts
  it('keeps a buffer that a panel took again while the modal was open', async () => {
    const path = `${KILN}/notes/dirty.md`;
    let answer!: (discard: boolean) => void;
    confirmDiscard.mockImplementationOnce(() => new Promise((resolve) => (answer = resolve)));
    const editor = await openDirtyFile(path);

    editor.closeFile(path);
    await waitFor(() => expect(confirmDiscard).toHaveBeenCalled());
    await editor.openFile(path);
    answer(true);
    await Promise.resolve();
    await Promise.resolve();

    expect(editor.openFiles().length).toBe(1);
    expect(editor.openFiles()[0]!.dirty).toBe(true);
  });
```

**Step 7: Run the editor test and see it fail**

```bash
just web-test unit src/contexts/__tests__/EditorContext.test.tsx
```

Expected: FAIL. `closing a dirty file asks for confirmation…` reports that `confirmDiscard` was not called, because the context still calls `window.confirm`. The new case fails, because the buffer goes.

**Step 8: Change the editor context**

In `web/src/contexts/EditorContext.tsx`:

1. Add the import after line 25:

```ts
import { confirmDiscard } from '@/lib/confirm-discard';
```

2. Replace `evictFile` (lines 109-131) with:

```ts
  const evictFile = async (path: string, force?: boolean) => {
    const file = openFilesStore.find((f) => f.path === path);
    if (!file) return;

    // Data-loss guard (bug 6): closing a dirty file must not silently discard
    // edits. `force` skips the prompt for callers whose close was already
    // confirmed upstream (e.g. a window tab close vetted by confirmTabClose).
    if (file.dirty && !force) {
      const filename = path.split('/').pop() ?? path;
      if (!(await confirmDiscard(filename))) return;
      // A panel took the file again while the modal was open. That panel
      // shows the buffer now, so the buffer stays.
      if ((openCounts.get(path) ?? 0) > 0) return;
    }

    // The modal can take time, so read the index after it.
    const idx = openFilesStore.findIndex((f) => f.path === path);
    if (idx === -1) return;

    setOpenFiles(produce((files) => files.splice(idx, 1)));

    if (activeFile() === path) {
      const remaining = openFilesStore.filter((f) => f.path !== path);
      if (remaining.length > 0) {
        const newIdx = Math.min(idx, remaining.length - 1);
        setActiveFileSignal(remaining[newIdx].path);
      } else {
        setActiveFileSignal(null);
      }
    }
  };
```

3. In `closeFile`, change `evictFile(path, opts?.force);` to `void evictFile(path, opts?.force);`.

4. In `reloadFile` (lines 305-309), replace the `window.confirm` line with:

```ts
      if (!(await confirmDiscard(filename))) return;
```

**Step 9: Mount the host**

In `web/src/App.tsx`, add after line 27:

```ts
import { ConfirmDiscardHost } from '@/components/ConfirmDiscardHost';
```

Add after `<NotificationToast />` (line 337):

```tsx
            <ConfirmDiscardHost />
```

**Step 10: Run the tests and see them pass**

```bash
just web-test unit src/contexts/ src/components/__tests__/ConfirmDiscardHost.test.tsx src/components/__tests__/FileViewerPanel.refcount.test.tsx
```

Expected: PASS.

**Step 11: Check that no discard prompt uses the browser**

```bash
grep -rn 'window.confirm(`Discard' crates/crucible-web/web/src
```

Expected: one hit only, in `web/src/windowing/model/tab-guards.ts`. The design keeps the tab close prompt outside its scope.

**Step 12: Break the gates, then restore them**

1. In `confirmDiscard`, change `if (hosts === 0)` to `if (hosts < 0)`. Run the host test. Expected: the first case times out, because no host answers. Restore the line.
2. In `evictFile`, delete the `openCounts` check after the modal. Run the editor test. Expected: `keeps a buffer that a panel took again…` fails. Restore the check.

**Step 13: Commit**

```bash
git -C crates/crucible-web/web add src/lib/confirm-discard.ts \
  src/components/ConfirmDiscardHost.tsx \
  src/components/__tests__/ConfirmDiscardHost.test.tsx \
  src/App.tsx \
  src/contexts/EditorContext.tsx \
  src/contexts/__tests__/EditorContext.test.tsx
git commit -m "feat(web): ask to discard unsaved changes in an app modal

confirmDiscard opens one app-level modal and answers a promise. The
editor context uses it for its two discard prompts. These paths no
longer use the browser prompt, because an installed PWA can suppress it.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 6: History moves on the tab host, for the desktop and the phone

**Files:**
- Modify: `web/src/lib/tab-host.ts:1-9` (imports), `:33-43` (`TabHost`), `:55-113` (`windowTabHost`), `:145-159` (`stackTabHost`)
- Modify: `web/src/stores/tabStackStore.ts:1-3` (imports), `:26-32` (`StoredTab`), `:37-64` (`loadCompactTabs`), `:75-91` (`persist`), `:99-163` (`tabStackActions`)
- Test: `web/src/lib/__tests__/tab-history-host.test.ts`
- Test: `web/src/stores/__tests__/tabStackStore.test.ts` (append)

The unsaved-changes gate lives here, in the app. The design lists `navigate`, `back` and `goTo`. This plan also gates `forward`, because `forward` also leaves the buffer.

A move that stays on the same file does not ask. The buffer stays open across the remount (Task 8), so no edit goes.

The phone store already has `back()`, which walks the visit order of the tabs. Thus the new store actions are `navigate`, `historyBack`, `historyForward`, `historyGoTo` and `markHistory`. The two meanings of "back" do not share one name.

**Step 1: Write the failing host test**

Create `web/src/lib/__tests__/tab-history-host.test.ts`:

```ts
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { produce } from 'solid-js/store';

const device = vi.hoisted(() => ({ compact: false }));
vi.mock('@/stores/deviceStore', () => ({ isCompact: () => device.compact }));
const confirmDiscard = vi.hoisted(() => vi.fn(async (_filename: string) => false));
vi.mock('@/lib/confirm-discard', () => ({ confirmDiscard }));
vi.mock('@/lib/recent-files', () => ({ recordRecentFile: vi.fn(), recentFiles: () => [] }));

import { tabHost } from '@/lib/tab-host';
import { tabStackActions } from '@/stores/tabStackStore';
import { setStore } from '@/stores/windowStore';
import type { Tab } from '@/types/windowTypes';

const note = (id: string, path: string): Tab => ({
  id,
  title: path.split('/').pop()!,
  contentType: 'file',
  metadata: { filePath: path },
});
const entry = (path: string) => ({
  title: path.split('/').pop()!,
  contentType: 'file' as const,
  metadata: { filePath: path },
});
const shown = (id: string) => tabHost().list().find((t) => t.id === id)!;
const pathOf = (id: string) => shown(id).metadata?.filePath;

function desktopWith(tabs: Tab[], activeId: string) {
  setStore(
    produce((s) => {
      s.layout = { id: 'pane-editor', type: 'pane', tabGroupId: 'g-editor' };
      s.tabGroups = { 'g-editor': { id: 'g-editor', tabs, activeTabId: activeId } };
      s.floatingWindows = [];
      s.activePaneId = 'pane-editor';
      s.focusedRegion = 'center';
    }),
  );
}

describe.each([
  ['desktop', false],
  ['phone', true],
])('history moves on the %s host', (_shell, compact) => {
  beforeEach(() => {
    device.compact = compact;
    localStorage.clear();
    tabStackActions.reset();
    confirmDiscard.mockReset();
    confirmDiscard.mockResolvedValue(false);
    // Two tabs show a.md. t2 comes first in the list, and t1 is active, so
    // the `find` case proves the active-first rule.
    const t1 = note('t1', '/k/a.md');
    const t2 = note('t2', '/k/a.md');
    if (compact) {
      tabStackActions.open(t2);
      tabStackActions.open(t1);
    } else {
      desktopWith([t2, t1], 't1');
    }
  });

  it('navigates in place, and walks back and forward', async () => {
    const host = tabHost();
    expect(await host.navigate('t1', entry('/k/b.md'))).toBe(true);
    expect(shown('t1').id).toBe('t1');
    expect(pathOf('t1')).toBe('/k/b.md');
    expect(await host.back('t1')).toBe(true);
    expect(pathOf('t1')).toBe('/k/a.md');
    expect(await host.back('t1')).toBe(false);
    expect(await host.forward('t1')).toBe(true);
    expect(pathOf('t1')).toBe('/k/b.md');
    expect(await host.forward('t1')).toBe(false);
  });

  it('goes to a node by id', async () => {
    const host = tabHost();
    await host.navigate('t1', entry('/k/b.md'));
    const root = Object.values(shown('t1').history!.nodes).find((n) => n.parent === null)!;
    expect(await host.goTo('t1', root.id)).toBe(true);
    expect(pathOf('t1')).toBe('/k/a.md');
    expect(await host.goTo('t1', 'nope')).toBe(false);
  });

  it('asks before it leaves unsaved changes, and a cancel keeps the tab and the tree', async () => {
    const host = tabHost();
    await host.navigate('t1', entry('/k/b.md'));
    host.update('t1', { isModified: true });
    const before = JSON.stringify(shown('t1').history);

    expect(await host.back('t1')).toBe(false);
    expect(await host.navigate('t1', entry('/k/c.md'))).toBe(false);
    expect(confirmDiscard).toHaveBeenCalledWith('b.md');
    expect(pathOf('t1')).toBe('/k/b.md');
    expect(JSON.stringify(shown('t1').history)).toBe(before);
  });

  it('moves on when the user discards', async () => {
    const host = tabHost();
    await host.navigate('t1', entry('/k/b.md'));
    host.update('t1', { isModified: true });
    confirmDiscard.mockResolvedValueOnce(true);
    expect(await host.back('t1')).toBe(true);
    expect(pathOf('t1')).toBe('/k/a.md');
  });

  it('does not ask for a move that stays on the same file', async () => {
    const host = tabHost();
    host.update('t1', { isModified: true });
    const sameFile = { ...entry('/k/a.md'), metadata: { filePath: '/k/a.md', scrollToLine: 4 } };
    expect(await host.navigate('t1', sameFile)).toBe(true);
    expect(confirmDiscard).not.toHaveBeenCalled();
  });

  it('does not ask for a move with no target', async () => {
    const host = tabHost();
    host.update('t1', { isModified: true });
    expect(await host.back('t1')).toBe(false);
    expect(confirmDiscard).not.toHaveBeenCalled();
  });

  it('finds the active tab first when two tabs show one note', () => {
    expect(tabHost().find((t) => t.metadata?.filePath === '/k/a.md')?.id).toBe('t1');
  });

  it('marks old nodes of a tab', async () => {
    const host = tabHost();
    await host.navigate('t1', entry('/k/b.md'));
    const root = Object.values(shown('t1').history!.nodes).find((n) => n.parent === null)!;
    host.markHistory('t1', [root.id], { missing: true });
    expect(shown('t1').history!.nodes[root.id]!.metadata).toMatchObject({ missing: true });
  });

  it('names the active note tab as the editor tab', () => {
    expect(tabHost().editorTab()?.id).toBe('t1');
  });

  it('still moves a pinned tab back and forward', async () => {
    const host = tabHost();
    await host.navigate('t1', entry('/k/b.md'));
    host.update('t1', { isPinned: true });
    expect(await host.back('t1')).toBe(true);
    expect(pathOf('t1')).toBe('/k/a.md');
    expect(await host.forward('t1')).toBe(true);
    expect(pathOf('t1')).toBe('/k/b.md');
  });
});
```

**Step 2: Run the test and see it fail**

```bash
just web-test unit src/lib/__tests__/tab-history-host.test.ts
```

Expected: FAIL. Most cases report `host.navigate is not a function`. The `find` case reports `expected 't2' to be 't1'`.

**Step 3: Give the phone store the history actions**

In `web/src/stores/tabStackStore.ts`:

1. Change the imports (lines 1-3) to:

```ts
import { createStore } from 'solid-js/store';
import type { Tab, TabContentType } from '@/types/windowTypes';
import type { NavEntry, NavTree } from '@/windowing/model/types';
import { iconForContentType } from '@/lib/tab-icons';
import {
  backTree,
  entryOf,
  forwardTree,
  goToTree,
  markNodes,
  navigateTree,
  readNavTree,
  seedTree,
} from '@/windowing/model/nav-tree';
```

2. In `StoredTab` (lines 26-32), add `history?: unknown;`. The value is not trusted until `readNavTree` checks it.

3. In `loadCompactTabs`, replace the `tabs.push({...})` call (lines 48-55) with:

```ts
      const base: Tab = {
        id: stored.id,
        title: stored.title ?? stored.id,
        contentType: stored.contentType,
        icon,
        metadata: stored.metadata,
        isPinned: stored.isPinned,
      };
      // A bad tree does not drop the tab. The tab keeps what it shows, as
      // the only node. The desktop layout reader has the same rule.
      tabs.push(
        stored.history === undefined
          ? base
          : { ...base, history: readNavTree<TabContentType>(stored.history) ?? seedTree(entryOf(base), 0) },
      );
```

4. In `persist`, add `history: t.history,` to the stored object, after `isPinned`.

5. Before `export { tabStack };` (line 99), add:

```ts
/** The kind of a history move. The phone's back button listens for it. */
export type HistoryMoveKind = 'navigate' | 'back' | 'forward' | 'goTo';
type HistoryListener = (tabId: string, kind: HistoryMoveKind) => void;
const historyListeners = new Set<HistoryListener>();

function moveHistory(
  id: string,
  kind: HistoryMoveKind,
  move: (tree: NavTree<TabContentType>, now: number) => NavTree<TabContentType> | null,
): boolean {
  const at = tabStack.tabs.findIndex((t) => t.id === id);
  if (at === -1) return false;
  const tab = tabStack.tabs[at]!;
  const now = Date.now();
  const next = move(tab.history ?? seedTree(entryOf(tab), now), now);
  if (!next) return false;
  const node = next.nodes[next.current]!;
  setTabStack('tabs', at, {
    history: next,
    title: node.title,
    contentType: node.contentType,
    metadata: node.metadata ? { ...node.metadata } : undefined,
    icon: iconForContentType(node.contentType),
  });
  persist();
  for (const listener of historyListeners) listener(id, kind);
  return true;
}
```

6. Add to `tabStackActions`, before `reset()`:

```ts
  /** Show `entry` in the tab, below its current node. The tab keeps its id. */
  navigate(id: string, entry: NavEntry<TabContentType>): boolean {
    return moveHistory(id, 'navigate', (tree, now) => navigateTree(tree, entry, now));
  },

  /** Show the parent node of the tab. `back()` is different: it walks the tabs. */
  historyBack(id: string): boolean {
    return moveHistory(id, 'back', backTree);
  },

  historyForward(id: string): boolean {
    return moveHistory(id, 'forward', forwardTree);
  },

  historyGoTo(id: string, nodeId: string): boolean {
    return moveHistory(id, 'goTo', (tree, now) => goToTree(tree, nodeId, now));
  },

  markHistory(id: string, nodeIds: readonly string[], mark: Record<string, unknown>): void {
    const at = tabStack.tabs.findIndex((t) => t.id === id);
    const tree = at === -1 ? undefined : tabStack.tabs[at]!.history;
    if (!tree) return;
    setTabStack('tabs', at, 'history', markNodes(tree, nodeIds, mark));
    persist();
  },

  /** Listen for each history move. Returns the release. */
  onHistoryMove(listener: HistoryListener): () => void {
    historyListeners.add(listener);
    return () => historyListeners.delete(listener);
  },
```

**Step 4: Give the host the history moves**

In `web/src/lib/tab-host.ts`:

1. Add to the imports:

```ts
import { confirmDiscard } from '@/lib/confirm-discard';
import { backTree, entryOf, forwardTree, goToTree, navigateTree, seedTree } from '@/windowing/model/nav-tree';
import type { NavEntry, NavTree } from '@/windowing/model/types';
```

Change the `Tab` type import (line 9) to `import type { Tab, TabContentType } from '@/types/windowTypes';`.

2. Add to `TabHost`, after `remove`:

```ts
  /** The tab that a link opens into: the active note tab of the editor, or null. */
  editorTab(): Tab | null;
  /** Show `entry` in the tab, below its current node. False when nothing moved. */
  navigate(tabId: string, entry: NavEntry<TabContentType>): Promise<boolean>;
  /** Show the parent node. False when nothing moved. */
  back(tabId: string): Promise<boolean>;
  /** Show the child with the latest visit. False when nothing moved. */
  forward(tabId: string): Promise<boolean>;
  /** Show any node. False when nothing moved. */
  goTo(tabId: string, nodeId: string): Promise<boolean>;
  /** Merge `mark` into the metadata of old nodes. */
  markHistory(tabId: string, nodeIds: readonly string[], mark: Record<string, unknown>): void;
```

3. After the `TabHost` interface, add:

```ts
/** The content types whose tab takes a link. */
const NOTE_CONTENT: ReadonlySet<string> = new Set(['file', 'canvas']);

type Move = (tree: NavTree<TabContentType>, now: number) => NavTree<TabContentType> | null;

/** The entry that `move` reaches from the tab, or null when it reaches none. */
function reach(tab: Tab, move: Move): NavEntry<TabContentType> | null {
  const next = move(tab.history ?? seedTree(entryOf(tab), 0), 0);
  return next ? next.nodes[next.current]! : null;
}

/**
 * True when the move may go on.
 *
 * A tab with unsaved changes asks first. A move that stays on the same file
 * does not ask: the buffer stays open across the remount, so no edit goes.
 */
async function mayLeave(tab: Tab, target: NavEntry<TabContentType>): Promise<boolean> {
  if (!tab.isModified) return true;
  const here = tab.metadata?.filePath;
  if (typeof here === 'string' && target.metadata?.filePath === here) return true;
  return confirmDiscard(tab.title);
}

/** Run a history move on a host, behind the unsaved-changes gate. */
async function guardedMove(host: TabHost, tabId: string, move: Move, apply: () => boolean): Promise<boolean> {
  const tab = host.list().find((t) => t.id === tabId);
  if (!tab) return false;
  const target = reach(tab, move);
  if (!target) return false;
  if (!(await mayLeave(tab, target))) return false;
  return apply();
}

const navigateMove = (entry: NavEntry<TabContentType>): Move => (tree, now) => navigateTree(tree, entry, now);
const goToMove = (nodeId: string): Move => (tree, now) => goToTree(tree, nodeId, now);

/** The active tab of the editor group, whatever it shows. */
function editorActiveTab(): Tab | null {
  const groupId = editorGroup();
  const group = groupId ? windowStore.tabGroups[groupId] : null;
  return group?.tabs.find((t) => t.id === group.activeTabId) ?? null;
}
```

4. In `windowTabHost`, replace `find` (lines 60-62) with:

```ts
  find(pred) {
    // Two tabs can show one note. The tab that the user is in comes first.
    const first = editorActiveTab();
    if (first && pred(first)) return first;
    return windowTabHost.list().find(pred) ?? null;
  },
```

Add after `remove` (line 112):

```ts
  editorTab() {
    const tab = editorActiveTab();
    return tab && NOTE_CONTENT.has(tab.contentType) ? tab : null;
  },

  navigate: (tabId, entry) =>
    guardedMove(windowTabHost, tabId, navigateMove(entry), () => {
      const groupId = groupOf(tabId);
      if (!groupId) return false;
      windowActions.navigate(groupId, tabId, entry);
      return true;
    }),

  back: (tabId) =>
    guardedMove(windowTabHost, tabId, backTree, () => {
      const groupId = groupOf(tabId);
      return groupId !== null && windowActions.back(groupId, tabId);
    }),

  forward: (tabId) =>
    guardedMove(windowTabHost, tabId, forwardTree, () => {
      const groupId = groupOf(tabId);
      return groupId !== null && windowActions.forward(groupId, tabId);
    }),

  goTo: (tabId, nodeId) =>
    guardedMove(windowTabHost, tabId, goToMove(nodeId), () => {
      const groupId = groupOf(tabId);
      return groupId !== null && windowActions.goTo(groupId, tabId, nodeId);
    }),

  markHistory(tabId, nodeIds, mark) {
    const groupId = groupOf(tabId);
    if (groupId) windowActions.markHistory(groupId, tabId, nodeIds, mark);
  },
```

5. Replace `stackTabHost` (lines 147-159) with:

```ts
const stackTabHost: TabHost = {
  list: () => tabStack.tabs,
  // The active tab comes first, as on the desktop.
  find: (pred) => {
    const active = tabStackActions.activeTab();
    if (active && pred(active)) return active;
    return tabStack.tabs.find(pred) ?? null;
  },
  activeTab: () => tabStackActions.activeTab(),
  // Placement is a desktop word. A phone has one surface, so it is ignored.
  open: (tab) => {
    tabStackActions.open(tab);
    return true;
  },
  activate: (tabId) => tabStackActions.activate(tabId),
  update: (tabId, patch) => tabStackActions.update(tabId, patch),
  remove: (tabId) => tabStackActions.remove(tabId),
  editorTab: () => {
    const tab = tabStackActions.activeTab();
    return tab && NOTE_CONTENT.has(tab.contentType) ? tab : null;
  },
  navigate: (tabId, entry) =>
    guardedMove(stackTabHost, tabId, navigateMove(entry), () => tabStackActions.navigate(tabId, entry)),
  back: (tabId) => guardedMove(stackTabHost, tabId, backTree, () => tabStackActions.historyBack(tabId)),
  forward: (tabId) => guardedMove(stackTabHost, tabId, forwardTree, () => tabStackActions.historyForward(tabId)),
  goTo: (tabId, nodeId) =>
    guardedMove(stackTabHost, tabId, goToMove(nodeId), () => tabStackActions.historyGoTo(tabId, nodeId)),
  markHistory: (tabId, nodeIds, mark) => tabStackActions.markHistory(tabId, nodeIds, mark),
};
```

**Step 5: Add the phone store tests**

Append to `web/src/stores/__tests__/tabStackStore.test.ts`:

```ts
describe('tab history on the phone', () => {
  it('keeps a tree across a reload, and repairs a bad one', () => {
    tabStackActions.open(tab('a'));
    tabStackActions.navigate('a', { title: 'b', contentType: 'file', metadata: { filePath: '/kiln/b.md' } });
    expect(Object.keys(loadCompactTabs().tabs[0]!.history!.nodes)).toHaveLength(2);

    localStorage.setItem(
      COMPACT_TABS_KEY,
      JSON.stringify({
        tabs: [
          {
            id: 'a',
            title: 'a',
            contentType: 'file',
            metadata: { filePath: '/kiln/a.md' },
            history: { current: 'x', nodes: {} },
          },
        ],
        activeTabId: 'a',
      }),
    );
    const nodes = Object.values(loadCompactTabs().tabs[0]!.history!.nodes);
    expect(nodes).toHaveLength(1);
    expect(nodes[0]).toMatchObject({ title: 'a', metadata: { filePath: '/kiln/a.md' } });
  });

  it('tells a listener about each move, with its kind', () => {
    const heard: string[] = [];
    const stop = tabStackActions.onHistoryMove((id, kind) => heard.push(`${id}:${kind}`));
    tabStackActions.open(tab('a'));
    tabStackActions.navigate('a', { title: 'b', contentType: 'file' });
    tabStackActions.historyBack('a');
    tabStackActions.historyBack('a');
    tabStackActions.historyForward('a');
    stop();
    tabStackActions.historyBack('a');
    expect(heard).toEqual(['a:navigate', 'a:back', 'a:forward']);
  });

  it('keeps the visit-order back apart from the tab history', () => {
    tabStackActions.open(tab('a'));
    tabStackActions.open(tab('b'));
    tabStackActions.navigate('b', { title: 'c', contentType: 'file' });
    expect(tabStackActions.back()).toBe(true);
    expect(tabStackActions.activeTab()?.id).toBe('a');
    expect(tabStack.tabs.find((t) => t.id === 'b')!.title).toBe('c');
  });
});
```

**Step 6: Run the tests and see them pass**

```bash
just web-test unit src/lib/__tests__/tab-history-host.test.ts src/stores/__tests__/tabStackStore.test.ts src/lib/__tests__/tab-host.test.ts src/lib/__tests__/tab-host-routing.test.ts
```

Expected: PASS. `tab-host-routing.test.ts` still passes here, because `openFileInEditor` does not use the new moves until Task 7. The desktop `editorTab()` reads `editorGroupId()` (`web/src/lib/panel-actions.ts:84-104`). If the desktop cases fail because that function picks no group for the `desktopWith` state, read that function. Then change the state in `desktopWith`, not the function.

**Step 7: Break the gates, then restore them**

1. In `mayLeave`, add `return true;` as the first line. Run the host test. Expected: `asks before it leaves unsaved changes…` fails on both shells. Restore the code.
2. In `windowTabHost.find`, delete the two `editorActiveTab` lines. Run the host test. Expected: `finds the active tab first…` fails on the desktop. Restore the lines. Do the same in `stackTabHost.find`. Expected: the phone case fails. Restore the lines.
3. In `loadCompactTabs`, replace `readNavTree<TabContentType>(stored.history) ?? seedTree(entryOf(base), 0)` with `stored.history as never`. Run the store test. Expected: `keeps a tree across a reload, and repairs a bad one` fails. Restore the code.

**Step 8: Commit**

```bash
git -C crates/crucible-web/web add src/lib/tab-host.ts \
  src/stores/tabStackStore.ts \
  src/lib/__tests__/tab-history-host.test.ts \
  src/stores/__tests__/tabStackStore.test.ts
git commit -m "feat(web): move tabs through their history on both shells

The tab host gets navigate, back, forward, goTo and markHistory. A move
away from unsaved changes asks through confirmDiscard first. find and
editorTab give the active tab first, because two tabs can show one note.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 6a: Pin and unpin a tab

**Files:**
- Modify: `web/src/windowing/store/tabActions.ts` (the `TabActions` interface, a new `setPinned`, the returned object)
- Modify: `web/src/lib/tab-host.ts` (`TabHost.setPinned`, `windowTabHost`, `stackTabHost`)
- Modify: `web/src/components/windowing/TabBar.tsx:14` (import), `:128-210` (`TabItem`, the pin mark), `:425-481` (`TabContextMenu`)
- Modify: `web/src/components/mobile/TabOverview.tsx:15-20` (props), `:80-105` (the card)
- Modify: `web/src/components/mobile/MobileShell.tsx:230-238` (the overview props)
- Test: `web/src/windowing/__tests__/windowStore.history.test.ts` (append)
- Test: `web/src/lib/__tests__/tab-history-host.test.ts` (append)
- Test: `web/src/components/windowing/__tests__/TabBar.pin.test.tsx` (create)
- Test: `web/src/components/mobile/__tests__/TabOverview.test.tsx` (append)

`Tab.isPinned` exists in the core type, in the saved layout and in the phone store, but no control sets it. This task adds the control, before Task 7 gives the flag its meaning.

- The desktop: the tab context menu gets **Pin tab** or **Unpin tab**, and a pinned tab shows a pin mark after its title. The close button does not change: a pinned tab still closes.
- The phone: the tab overview is the phone's tab menu, so each card gets a pin toggle. `tabStackStore` already stores and persists `isPinned` (`tabStackStore.ts:31`, `:54`, `:82`), so the phone needs no store change. The phone has no tab strip, so the pin mark goes on the card.
- The core action only writes the flag. The core does not read it; the app rule in Task 7 reads it.

**Step 1: Write the failing tests**

Append to the `describe` in `web/src/windowing/__tests__/windowStore.history.test.ts`:

```ts
  it('setPinned writes the flag, and unpinning removes it', () => {
    windowActions.setPinned(centre(), 't1', true);
    expect(tab().isPinned).toBe(true);
    windowActions.setPinned(centre(), 't1', false);
    expect(tab().isPinned).toBeUndefined();
  });
```

Append to the `describe.each` in `web/src/lib/__tests__/tab-history-host.test.ts`:

```ts
  it('pins and unpins a tab', () => {
    tabHost().setPinned('t1', true);
    expect(shown('t1').isPinned).toBe(true);
    tabHost().setPinned('t1', false);
    expect(shown('t1').isPinned).toBeFalsy();
  });
```

Create `web/src/components/windowing/__tests__/TabBar.pin.test.tsx`:

```tsx
import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@solidjs/testing-library';
import { produce } from 'solid-js/store';
import { DragDropProvider } from '@thisbeyond/solid-dnd';
import { TabBar } from '../TabBar';
import { windowStore, windowActions, setStore } from '@/stores/windowStore';
import { findFirstPane } from '@/windowing/model/tree';
import { defaultLayout } from '@/stores/defaultLayout';

let paneId: string;
let groupId: string;

beforeEach(() => {
  const fresh = defaultLayout();
  setStore(
    produce((s) => {
      s.layout = fresh.layout;
      s.tabGroups = fresh.tabGroups;
      s.edgePanels = fresh.edgePanels;
      s.floatingWindows = [];
      s.activePaneId = fresh.activePaneId;
      s.focusedRegion = 'center';
    }),
  );
  const pane = findFirstPane(windowStore.layout)!;
  paneId = pane.id;
  groupId = pane.tabGroupId!;
  windowActions.addTab(groupId, { id: 'tab-a', title: 'A.md', contentType: 'file' });
});
afterEach(cleanup);

const bar = () =>
  render(() => (
    <DragDropProvider>
      <TabBar groupId={groupId} paneId={paneId} />
    </DragDropProvider>
  ));
const tabEl = (container: HTMLElement) => container.querySelector<HTMLElement>('[data-tab-id="tab-a"]')!;
const pinned = () => windowStore.tabGroups[groupId]!.tabs[0]!.isPinned;

describe('TabBar — pin a tab', () => {
  it('pins a tab from its context menu, and shows the pin mark', async () => {
    const { container } = bar();
    expect(tabEl(container).querySelector('[data-testid="tab-pinned"]')).toBeNull();

    fireEvent.contextMenu(tabEl(container));
    fireEvent.click(await screen.findByText('Pin tab'));

    await waitFor(() => expect(pinned()).toBe(true));
    expect(tabEl(container).querySelector('[data-testid="tab-pinned"]')).not.toBeNull();
  });

  it('offers Unpin tab on a pinned tab, and unpins it', async () => {
    windowActions.setPinned(groupId, 'tab-a', true);
    const { container } = bar();

    fireEvent.contextMenu(tabEl(container));
    expect(screen.queryByText('Pin tab')).toBeNull();
    fireEvent.click(await screen.findByText('Unpin tab'));

    await waitFor(() => expect(pinned()).toBeUndefined());
    expect(tabEl(container).querySelector('[data-testid="tab-pinned"]')).toBeNull();
  });

  it('keeps the close button on a pinned tab', () => {
    windowActions.setPinned(groupId, 'tab-a', true);
    const { container } = bar();
    expect(tabEl(container).querySelector('button[aria-label="Close tab"]')).not.toBeNull();
  });
});
```

Append to `web/src/components/mobile/__tests__/TabOverview.test.tsx`:

```tsx
  it('pins a tab from its card without picking it', () => {
    const onPick = vi.fn();
    const onTogglePin = vi.fn();
    render(() => (
      <TabOverview tabs={tabs} activeId="a" onPick={onPick} onClose={() => {}} onTogglePin={onTogglePin} />
    ));
    const pin = screen.getByRole('button', { name: 'Pin Note A' });
    expect(pin.getAttribute('aria-pressed')).toBe('false');
    fireEvent.click(pin);
    expect(onTogglePin).toHaveBeenCalledWith('a');
    expect(onPick).not.toHaveBeenCalled();
  });

  it('marks a pinned tab, and offers to unpin it', () => {
    const pinnedTabs: Tab[] = [{ ...tabs[0]!, isPinned: true }];
    render(() => (
      <TabOverview tabs={pinnedTabs} activeId="a" onPick={() => {}} onClose={() => {}} onTogglePin={() => {}} />
    ));
    const unpin = screen.getByRole('button', { name: 'Unpin Note A' });
    expect(unpin.getAttribute('aria-pressed')).toBe('true');
    expect(screen.getByTestId('tab-card-pinned')).toBeTruthy();
    // The mark does not change the name of the pick button.
    expect(screen.getByRole('button', { name: 'Note A' })).toBeTruthy();
  });
```

**Step 2: Run the tests and see them fail**

```bash
just web-test unit src/windowing/__tests__/windowStore.history.test.ts src/lib/__tests__/tab-history-host.test.ts src/components/windowing/__tests__/TabBar.pin.test.tsx src/components/mobile/__tests__/TabOverview.test.tsx
```

Expected: FAIL. `setPinned` is not a function. The menu has no `Pin tab` row. The overview has no pin button.

**Step 3: Write the core action**

In `web/src/windowing/store/tabActions.ts`, add to the `TabActions` interface, after `markHistory`:

```ts
  /** Pin or unpin the tab. The core stores the flag; the app decides what it means. */
  setPinned(groupId: string, tabId: string, pinned: boolean): void;
```

Add after `markHistory`:

```ts
  // An unpinned tab carries no flag, so the saved layout stays as it was.
  const setPinned = (groupId: string, tabId: string, pinned: boolean) => {
    updateTab(groupId, tabId, { isPinned: pinned ? true : undefined });
  };
```

Add `setPinned,` to the returned object.

**Step 4: Give the host the action**

In `web/src/lib/tab-host.ts`, add to `TabHost`:

```ts
  /** Pin or unpin a tab. An open never moves a pinned tab (see `lib/file-actions.ts`). */
  setPinned(tabId: string, pinned: boolean): void;
```

Add to `windowTabHost`:

```ts
  setPinned(tabId, pinned) {
    const groupId = groupOf(tabId);
    if (groupId) windowActions.setPinned(groupId, tabId, pinned);
  },
```

Add to `stackTabHost`:

```ts
  // The phone store already persists the flag with the tab.
  setPinned: (tabId, pinned) => tabStackActions.update(tabId, { isPinned: pinned ? true : undefined }),
```

**Step 5: Add the desktop control**

In `web/src/components/windowing/TabBar.tsx`:

1. Change the import at line 12 to `import { ChevronDown, Pin } from '@/lib/icons';`, and the import at line 14 to `import { menuContent, menuItem, menuSeparator } from '@/components/ui/menu-style';`.
2. In `TabItem`, after the title `span` (line 175), add:

```tsx
      {/* A pinned tab keeps its note: an open goes to a new tab. */}
      <Show when={props.tab.isPinned}>
        <Pin
          data-testid="tab-pinned"
          aria-label="Pinned"
          class="w-3 h-3 flex-shrink-0 text-muted-dark"
        />
      </Show>
```

   Check the icon component: if it does not pass `data-testid` and `aria-label` to the `svg`, wrap it in `<span data-testid="tab-pinned" aria-label="Pinned" class="flex-shrink-0">`.
3. In `TabContextMenu` (lines 425-481), change the doc comment to add: "Pin tab / Unpin tab sits above the close rows." Replace the `Menu.Root` line and the start of the content with:

```tsx
  type TabMenuAction = TabCloseMode | 'pin' | 'unpin';
  const onSelect = (value: TabMenuAction) => {
    if (value === 'pin' || value === 'unpin') {
      windowActions.setPinned(props.groupId(), props.tab.id, value === 'pin');
      return;
    }
    closeWith(value);
  };
  return (
    <Menu.Root onSelect={(d) => onSelect(d.value as TabMenuAction)}>
```

   and, as the first rows of `Menu.Content`:

```tsx
          <Menu.Item value={props.tab.isPinned ? 'unpin' : 'pin'} class={menuItem}>
            {props.tab.isPinned ? 'Unpin tab' : 'Pin tab'}
          </Menu.Item>
          <Menu.Separator class={menuSeparator} />
```

**Step 6: Add the phone control**

In `web/src/components/mobile/TabOverview.tsx`:

1. Change the import at line 3 to `import { Pin, X } from '@/lib/icons';`.
2. Add to the props (line 19):

```ts
  /** Pin or unpin the tab. The overview is the phone's tab menu. */
  onTogglePin: (tabId: string) => void;
```

3. In the pick button, after the modified dot (line 97), add:

```tsx
                  <Show when={tab.isPinned}>
                    {/* Hidden from the name: the toggle beside the card says "Unpin". */}
                    <Pin data-testid="tab-card-pinned" aria-hidden="true" class="w-3.5 h-3.5 shrink-0 text-muted-dark" />
                  </Show>
```

4. Before the close button (line 99), add:

```tsx
                <button
                  type="button"
                  aria-label={`${tab.isPinned ? 'Unpin' : 'Pin'} ${tab.title}`}
                  aria-pressed={tab.isPinned ? 'true' : 'false'}
                  class={`w-11 shrink-0 flex items-center justify-center rounded focus-ring ${
                    tab.isPinned ? 'text-shell-ink' : 'text-muted-dark hover:text-shell-ink'
                  }`}
                  onClick={() => props.onTogglePin(tab.id)}
                >
                  <Pin class="w-4 h-4" />
                </button>
```

5. In each existing overview test that renders `TabOverview`, add `onTogglePin={() => {}}`. TypeScript flags each one that lacks it.

In `web/src/components/mobile/MobileShell.tsx`, add to the `TabOverview` props (after line 237):

```tsx
              onTogglePin={(id) => {
                const tab = tabStack.tabs.find((t) => t.id === id);
                if (tab) tabHost().setPinned(id, !tab.isPinned);
              }}
```

`tabHost` is imported in Task 12. If Task 12 is not done yet, add `import { tabHost } from '@/lib/tab-host';` here.

**Step 7: Run the tests and see them pass**

```bash
just web-test unit src/windowing/ src/lib/ src/components/windowing/ src/components/mobile/
cd crates/crucible-web/web && bun run typecheck
```

Expected: PASS, and no type error.

**Step 8: Break the gates, then restore them**

1. In `TabContextMenu`, change `value === 'pin'` in the `setPinned` call to `true`. Run `TabBar.pin.test.tsx`. Expected: `offers Unpin tab on a pinned tab…` fails. Restore the code.
2. Delete the `<Show when={props.tab.isPinned}>` block in `TabItem`. Run the same test. Expected: `pins a tab… and shows the pin mark` fails. Restore the block.
3. In `stackTabHost.setPinned`, change the body to `{}`. Run `tab-history-host.test.ts`. Expected: `pins and unpins a tab` fails on the phone. Restore the body.

**Step 9: Look at the controls**

Run `just web`. Right-click a note tab. Pin it. Check that the pin mark sits after the title, that the title still fades before the trailing slot, and that the mark reads in the light theme and the dark theme. Narrow the window to a phone width. Open the tab overview. Check that the pin toggle is a 44 px target and that the card title still fits. Stop the server.

**Step 10: Commit**

```bash
git -C crates/crucible-web/web add src/windowing/store/tabActions.ts \
  src/lib/tab-host.ts \
  src/components/windowing/TabBar.tsx \
  src/components/mobile/TabOverview.tsx \
  src/components/mobile/MobileShell.tsx \
  src/windowing/__tests__/windowStore.history.test.ts \
  src/lib/__tests__/tab-history-host.test.ts \
  src/components/windowing/__tests__/TabBar.pin.test.tsx \
  src/components/mobile/__tests__/TabOverview.test.tsx
git commit -m "feat(web): pin and unpin a tab

The tab context menu gets Pin tab and Unpin tab, and a pinned tab shows
a pin mark. On a phone, each card of the tab overview has a pin toggle.
The core stores the flag and does not read it.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 7: Opaque tab ids, in-place opens and deleted files

**Files:**
- Modify: `web/src/lib/file-actions.ts` (the whole file, 141 lines)
- Test: `web/src/lib/__tests__/file-actions.test.ts:1-5` (imports), `:78-163` (the `findTabByFilePath` and `openFileInEditor` blocks), `:165-227` (the placement block)
- Test: `web/src/lib/__tests__/tab-host-routing.test.ts:1-69`

A new file tab gets the id `tab-file-<generateId()>`. The `tab-file-` prefix stays, because the Playwright specs select tabs with `[data-tab-id^="tab-file-"]`. No code reads the id. An old id such as `tab-file-/k/a.md` stays valid.

`openFileInEditor` has one behavior for every caller (user decision): it shows the file in the active editor tab and adds a history node. Only `newTab`, a pinned target or a missing editor tab opens a new tab. The callers:

| Caller | Line | Gesture | Test that covers the caller |
|---|---|---|---|
| `lib/note-actions.ts` `openNoteInEditor` | 94 | link click (chat, reading view, canvas card, backlinks) | `note-actions.test.ts` (Task 9) |
| `components/FilesPanel.tsx` `onOpenLeaf` | 344 | file tree click | `FilesPanel.test.tsx` (this task, Step 1b) |
| `components/FilesPanel.tsx` new note | 433 | New Note in the tree menu | `FilesPanel.test.tsx` (Step 1b) |
| `components/FilesPanel.tsx` `open` action | 442 | Open in the tree menu | `FilesPanel.test.tsx` (Step 1b) |
| `components/SearchPanel.tsx` hit rows | 383, 390, 397 | search result click | `SearchPanel.test.tsx:83` (Step 1b) |
| `components/CommandPalette.tsx` note item | 124 | palette note pick | `CommandPalette.test.tsx` (Step 1b) |
| `components/graph/GraphPanel.tsx` | 514 | graph node click | no test file exists; the call has the plain two-argument form, and `file-actions.test.ts` covers that form |
| `components/blocks/GraphBlock.tsx` | 116 | graph block node click | `GraphBlock.test.tsx:219` (unchanged) |
| `components/ChangesPanel.tsx` | 572 | changed file click | `ChangesPanel.test.tsx:406` (unchanged) |
| `components/canvas/CanvasPanel.tsx` `openFile` | 1040 | canvas file node | `file-actions.test.ts` (the plain form) |
| `App.tsx` `crucible:open-file` | 308 | product event | `e2e/tab-history.spec.ts` (Task 13) |
| `openFileWithDiff` (from `ToolCard.tsx:251`) | 57-60 | review a proposed edit | `file-actions.test.ts` (the plain form) |
| `openFileAtLine` (from `settings/AppConfigSettings.tsx:438`) | 74 | jump to a locked setting | `file-actions.test.ts`, `tab-host-routing.test.ts` |

No caller passes `newTab` except the link paths of Task 9. The caller tests prove that each caller uses the plain form, and `file-actions.test.ts` proves what the plain form does. `openFileInGroup` (a file that the user drops on a pane, `Pane.tsx:94`, `EdgePanel.tsx:361`) is a placement, not an open: it keeps its rule (below).

`openFileAtLine` adds a node, because the panel reads `scrollToLine` only when it mounts.

**Pinned tabs (user decision).** An open whose target tab is pinned opens a new tab. A pinned tab that already shows the file gets the focus. Back, forward and the popover still move a pinned tab, because those moves do not go through `openEntry`. Task 6a adds the controls that set the flag.

`openFileInGroup` (a file that the user drops on a pane) keeps its rule: it focuses a tab that shows the file, else it adds a tab to the target group.

**Step 1: Write the failing tests**

In `web/src/lib/__tests__/file-actions.test.ts`, change the imports (lines 1-5) to:

```ts
import { describe, it, expect, beforeEach } from 'vitest';
import { produce } from 'solid-js/store';
import { windowStore, setStore } from '@/stores/windowStore';
import { closeTabsUnder, findTabByFilePath, openFileAtLine, openFileInEditor } from '../file-actions';
import type { Tab, EdgeMode, EdgePanelPosition, TabGroup, LayoutNode } from '@/types/windowTypes';

/** An in-place open waits on the unsaved-changes gate, which is a promise. */
const settle = () => new Promise((resolve) => setTimeout(resolve, 0));
const centre = () => windowStore.tabGroups['center-group']!;
```

Replace the `describe('findTabByFilePath'…)` block and the `describe('openFileInEditor'…)` block (lines 78-163) with:

```ts
describe('findTabByFilePath', () => {
  beforeEach(() => {
    setupDefaultState();
  });

  it('returns null when no tab shows the path', () => {
    setupDefaultState([makeTab('tab-file-a', 'a.md', 'file', { filePath: '/docs/a.md' })]);
    expect(findTabByFilePath('/docs/not-found.md')).toBeNull();
  });

  it('finds a tab by metadata.filePath', () => {
    setupDefaultState([
      makeTab('tab-file-a', 'a.md', 'file', { filePath: '/docs/a.md' }),
      makeTab('tab-file-b', 'b.md', 'file', { filePath: '/docs/b.md' }),
    ]);
    expect(findTabByFilePath('/docs/b.md')).toMatchObject({ groupId: 'center-group', tab: { id: 'tab-file-b' } });
  });

  it('prefers the active editor tab when two tabs show one note', () => {
    setupDefaultState();
    setStore(
      'tabGroups',
      'center-group',
      makeTabGroup(
        'center-group',
        [
          makeTab('tab-x', 'a.md', 'file', { filePath: '/docs/a.md' }),
          makeTab('tab-y', 'a.md', 'file', { filePath: '/docs/a.md' }),
        ],
        'tab-y',
      ),
    );
    expect(findTabByFilePath('/docs/a.md')!.tab.id).toBe('tab-y');
  });
});

describe('openFileInEditor', () => {
  beforeEach(() => {
    setupDefaultState();
  });

  it('opens a new tab with an opaque id when no editor tab exists', () => {
    openFileInEditor('/docs/readme.md', 'readme.md');
    expect(centre().tabs).toHaveLength(1);
    const tab = centre().tabs[0]!;
    expect(tab.id).toMatch(/^tab-file-/);
    expect(tab.id).not.toContain('/docs/readme.md');
    expect(tab).toMatchObject({ title: 'readme.md', contentType: 'file', metadata: { filePath: '/docs/readme.md' } });
  });

  it('shows the next file in the same tab, and keeps the tab id', async () => {
    openFileInEditor('/docs/a.md', 'a.md');
    const id = centre().tabs[0]!.id;
    openFileInEditor('/docs/b.md', 'b.md');
    await settle();
    expect(centre().tabs).toHaveLength(1);
    expect(centre().tabs[0]).toMatchObject({ id, title: 'b.md', metadata: { filePath: '/docs/b.md' } });
    expect(Object.keys(centre().tabs[0]!.history!.nodes)).toHaveLength(2);
  });

  it('opens a new tab when the caller asks for one', async () => {
    openFileInEditor('/docs/a.md', 'a.md');
    openFileInEditor('/docs/b.md', 'b.md', { newTab: true });
    await settle();
    expect(centre().tabs.map((t) => t.metadata?.filePath)).toEqual(['/docs/a.md', '/docs/b.md']);
    expect(centre().activeTabId).toBe(centre().tabs[1]!.id);
  });

  it('adds no node when the active tab already shows the file', async () => {
    openFileInEditor('/docs/a.md', 'a.md');
    openFileInEditor('/docs/a.md', 'a.md');
    await settle();
    expect(centre().tabs).toHaveLength(1);
    expect(centre().tabs[0]!.history).toBeUndefined();
  });

  it('shows the file in the named tab', async () => {
    openFileInEditor('/docs/a.md', 'a.md');
    openFileInEditor('/docs/b.md', 'b.md', { newTab: true });
    const first = centre().tabs[0]!.id;
    openFileInEditor('/docs/c.md', 'c.md', { inTab: first });
    await settle();
    expect(centre().tabs.map((t) => t.metadata?.filePath)).toEqual(['/docs/c.md', '/docs/b.md']);
  });

  it('opens a file at a line as a new node of the editor tab', async () => {
    openFileInEditor('/docs/a.md', 'a.md');
    openFileAtLine('/docs/a.md', 12);
    await settle();
    expect(centre().tabs).toHaveLength(1);
    expect(centre().tabs[0]!.metadata).toEqual({ filePath: '/docs/a.md', scrollToLine: 12 });
  });

  it('opens a new tab when the active editor tab is pinned, and keeps the pinned tab', async () => {
    openFileInEditor('/docs/a.md', 'a.md');
    const pinned = centre().tabs[0]!.id;
    setStore('tabGroups', 'center-group', 'tabs', 0, 'isPinned', true);
    openFileInEditor('/docs/b.md', 'b.md');
    await settle();
    expect(centre().tabs.map((t) => t.metadata?.filePath)).toEqual(['/docs/a.md', '/docs/b.md']);
    expect(centre().tabs[0]!.id).toBe(pinned);
    expect(centre().tabs[0]!.history).toBeUndefined();
    expect(centre().activeTabId).toBe(centre().tabs[1]!.id);
  });

  it('opens a new tab for a link in a pinned tab', async () => {
    openFileInEditor('/docs/a.md', 'a.md');
    const pinned = centre().tabs[0]!.id;
    setStore('tabGroups', 'center-group', 'tabs', 0, 'isPinned', true);
    openFileInEditor('/docs/c.md', 'c.md', { inTab: pinned });
    await settle();
    expect(centre().tabs).toHaveLength(2);
    expect(centre().tabs[0]!.metadata?.filePath).toBe('/docs/a.md');
  });

  it('focuses a pinned tab that already shows the file', async () => {
    openFileInEditor('/docs/a.md', 'a.md');
    setStore('tabGroups', 'center-group', 'tabs', 0, 'isPinned', true);
    openFileInEditor('/docs/a.md', 'a.md');
    await settle();
    expect(centre().tabs).toHaveLength(1);
  });
});

describe('closeTabsUnder', () => {
  beforeEach(() => {
    setupDefaultState();
  });

  it('closes a tab that shows a deleted file, and marks old nodes in the other tabs', async () => {
    openFileInEditor('/docs/gone/x.md', 'x.md');
    openFileInEditor('/docs/keep.md', 'keep.md');
    await settle();
    openFileInEditor('/docs/gone/y.md', 'y.md', { newTab: true });
    const keeper = centre().tabs[0]!.id;

    closeTabsUnder('/docs/gone', true);

    expect(centre().tabs.map((t) => t.id)).toEqual([keeper]);
    const nodes = Object.values(centre().tabs[0]!.history!.nodes);
    const old = nodes.find((n) => n.metadata?.filePath === '/docs/gone/x.md')!;
    const now = nodes.find((n) => n.metadata?.filePath === '/docs/keep.md')!;
    expect(old.metadata).toMatchObject({ missing: true });
    expect(now.metadata).not.toHaveProperty('missing');
  });

  it('marks nothing for a path that only shares a prefix', async () => {
    openFileInEditor('/docs/gone-not/x.md', 'x.md');
    openFileInEditor('/docs/keep.md', 'keep.md');
    await settle();
    closeTabsUnder('/docs/gone', true);
    const nodes = Object.values(centre().tabs[0]!.history!.nodes);
    expect(nodes.some((n) => n.metadata?.missing === true)).toBe(false);
  });
});
```

In the block `openFileInEditor — beside the conversation, never on top of it`:

- Make `opens into the editor pane, not the first leaf` `async`. Add `await settle();` after the open. Replace the last line with:

```ts
    expect(windowStore.tabGroups['g-editor'].tabs.some((t) => t.metadata?.filePath === '/b.md')).toBe(true);
```

- In `opens its own pane on the files side…`, replace `g.tabs.some((t) => t.id === 'tab-file-/b.md')` with `g.tabs.some((t) => t.metadata?.filePath === '/b.md')`.

In `web/src/lib/__tests__/tab-host-routing.test.ts`:

- After line 5, add:

```ts
vi.mock('@/lib/confirm-discard', () => ({ confirmDiscard: vi.fn(async () => false) }));
const settle = () => new Promise((resolve) => setTimeout(resolve, 0));
```

- Replace lines 27-30 with:

```ts
    openFileInEditor('/kiln/a.md', 'A');
    openFileInEditor('/kiln/a.md', 'A');
    expect(tabStack.tabs).toHaveLength(1);
    expect(tabStack.tabs[0]!.metadata?.filePath).toBe('/kiln/a.md');
    expect(desktopHolds(tabStack.tabs[0]!.id)).toBe(false);
```

- Make `opens a file at a line, and scrolls one already open` `async`. Add `await settle();` after the second `openFileAtLine` call.
- Replace the test `closes the tabs under a deleted folder` (lines 64-69) with:

```ts
  it('closes a tab under a deleted folder, and marks the old node of another tab', async () => {
    openFileInEditor('/kiln/notes/x.md');
    openFileInEditor('/kiln/other.md');
    await settle();
    openFileInEditor('/kiln/notes/y.md', undefined, { newTab: true });
    closeTabsUnder('/kiln/notes', true);
    expect(tabStack.tabs).toHaveLength(1);
    const tab = tabStack.tabs[0]!;
    expect(tab.metadata?.filePath).toBe('/kiln/other.md');
    const old = Object.values(tab.history!.nodes).find((n) => n.metadata?.filePath === '/kiln/notes/x.md');
    expect(old?.metadata).toMatchObject({ missing: true });
  });
```

**Step 1b: Pin each caller to the plain form**

A caller test proves that the caller uses `openFileInEditor(path, name)` with no third argument. The behavior of that form is in `file-actions.test.ts`. These tests pass before and after this task. They are guards against a later caller that adds its own tab choice.

In `web/src/components/__tests__/FilesPanel.test.tsx`, add after the `SessionContext` mock (line 56):

```tsx
// The tree opens files through the one shared opener. The spy records the
// call; the real module still serves `closeTabsUnder`.
const openFileInEditorSpy = vi.hoisted(() => vi.fn());
vi.mock('@/lib/file-actions', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/file-actions')>()),
  openFileInEditor: (...args: unknown[]) => openFileInEditorSpy(...args),
}));
```

Add `waitFor` to the `@solidjs/testing-library` import, and append:

```tsx
describe('FilesPanel — every open uses the plain form', () => {
  it('a leaf opens its file in the current tab, with no tab choice', async () => {
    const { findByText } = render(() => <FilesPanel />);
    openLeaf(await findByText('readme.md'));
    await waitFor(() =>
      expect(openFileInEditorSpy).toHaveBeenCalledWith('/project/kiln/readme.md', 'readme.md'),
    );
    expect(openFileInEditorSpy.mock.calls[0]).toHaveLength(2);
  });

  it('Open in the row menu uses the same form', async () => {
    const { findByText } = render(() => <FilesPanel />);
    const row = await findByText('readme.md');
    fireEvent.contextMenu(row);
    fireEvent.click(await findByText('Open'));
    await waitFor(() => expect(openFileInEditorSpy).toHaveBeenCalled());
    expect(openFileInEditorSpy.mock.lastCall).toEqual(['/project/kiln/readme.md', 'readme.md']);
  });

  it('New Note opens the new file with the same form', async () => {
    vi.spyOn(window, 'prompt').mockReturnValue('Fresh');
    const { findByText } = render(() => <FilesPanel />);
    fireEvent.contextMenu(await findByText('readme.md'));
    fireEvent.click(await findByText('New Note'));
    await waitFor(() => expect(openFileInEditorSpy).toHaveBeenCalled());
    expect(openFileInEditorSpy.mock.lastCall).toEqual(['/project/kiln/Fresh.md', 'Fresh.md']);
  });
});
```

Before you run it, read how the tree calls `onOpenLeaf` (`FilesPanel.tsx:344` and the tree component that receives it). Define `openLeaf` as that gesture, for example `const openLeaf = (el: HTMLElement) => fireEvent.click(el);` or a double click. Read the menu labels in `FilesPanel.tsx` (`ContextAction`) and use the real labels in place of `Open` and `New Note`. The New Note action writes through `saveFileContent`. Add `saveFileContent: vi.fn(async () => {})` to the `@/lib/api` mock of the file.

In `web/src/components/__tests__/SearchPanel.test.tsx:83`, after `expect(openFileMock).toHaveBeenCalled();`, add:

```ts
    // One behavior for every caller: the search panel names no tab.
    expect(openFileMock.mock.calls[0]).toHaveLength(2);
```

In `web/src/components/__tests__/CommandPalette.test.tsx`, add a mock after the `@/lib/api` mock (line 28):

```tsx
const openFileInEditorSpy = vi.hoisted(() => vi.fn());
vi.mock('@/lib/file-actions', () => ({
  openFileInEditor: (...args: unknown[]) => openFileInEditorSpy(...args),
}));
```

Append to the `CommandPalette — selection` block (line 233):

```tsx
  it('a note pick opens the note in the current tab, with no tab choice', async () => {
    // Stage one note as `notes mode lists kiln notes only…` (line 109) does.
    renderNotesPalette();
    fireEvent.click(await screen.findByText(NOTE_TITLE));
    await waitFor(() => expect(openFileInEditorSpy).toHaveBeenCalled());
    expect(openFileInEditorSpy.mock.calls[0]).toHaveLength(2);
  });
```

Write `renderNotesPalette` and `NOTE_TITLE` from the setup of the test at line 109: the same API answer, `mode="notes"` and `open`. If the file already mocks `@/lib/file-actions`, add the spy to that mock instead.

`GraphBlock.test.tsx:219` and `ChangesPanel.test.tsx:406` already assert a call with exactly `(path, name)`, which `toHaveBeenCalledWith` checks with the argument count. Leave them as they are. `GraphPanel.tsx:514` and `CanvasPanel.tsx:1040` have no test that reaches the open. Their calls use the plain form in the source, and a reviewer checks that form in the diff.

**Step 2: Run the tests and see them fail**

```bash
just web-test unit src/lib/__tests__/file-actions.test.ts src/lib/__tests__/tab-host-routing.test.ts
```

Expected: FAIL. The id case reports `expected 'tab-file-/docs/readme.md' not to contain '/docs/readme.md'`. The in-place case reports two tabs. The `closeTabsUnder` cases report no mark.

**Step 3: Write the code**

Check for an import cycle first:

```bash
grep -n "file-actions" crates/crucible-web/web/src/lib/panel-actions.ts
```

Expected: no hit. If a hit exists, read `editorGroupId` through `tab-host.ts` instead.

Replace `web/src/lib/file-actions.ts` with:

```ts
import { windowActions, windowStore } from '@/stores/windowStore';
import type { Tab, TabContentType } from '@/types/windowTypes';
import type { NavEntry } from '@/windowing/model/types';
import { generateId } from '@/windowing/model/tree';

import { iconForContentType } from './tab-icons';
import { recordRecentFile } from './recent-files';
import { pendingDiffActions } from '@/stores/pendingDiffStore';
import { editorGroupId } from './panel-actions';
import { tabHost } from './tab-host';

/** How a caller wants a file opened. */
export interface OpenOptions {
  /** True opens a new tab. Ctrl+click and a middle click ask for one. */
  newTab?: boolean;
  /** The tab to show the file in. Absent means the active editor tab. */
  inTab?: string;
}

/**
 * The tab that shows a file. Two tabs can show one note, so the active tab
 * of the editor group comes first.
 */
export function findTabByFilePath(filePath: string): { groupId: string; tab: Tab } | null {
  const shows = (t: Tab) => t.metadata?.filePath === filePath;
  const editor = editorGroupId();
  const group = editor ? windowStore.tabGroups[editor] : undefined;
  const active = group?.tabs.find((t) => t.id === group.activeTabId);
  if (editor && active && shows(active)) return { groupId: editor, tab: active };
  for (const [groupId, g] of Object.entries(windowStore.tabGroups)) {
    const tab = g.tabs.find(shows);
    if (tab) return { groupId, tab };
  }
  return null;
}

/**
 * Which panel opens a path. A `.canvas` is a spatial document, not text, so it
 * routes to the canvas editor rather than the file viewer.
 */
function contentTypeForPath(filePath: string): 'file' | 'canvas' {
  return /\.canvas$/i.test(filePath) ? 'canvas' : 'file';
}

/** What a tab shows for a file. */
function fileEntry(
  filePath: string,
  fileName?: string,
  extra: Record<string, unknown> = {},
): NavEntry<TabContentType> {
  return {
    // Last-resort basename fallback: a falsy caller value would otherwise
    // mint a tab literally titled "undefined" (save prompts included).
    title: fileName || filePath.split('/').pop() || filePath,
    contentType: contentTypeForPath(filePath),
    metadata: { filePath, ...extra },
  };
}

/**
 * A new tab for a file entry.
 *
 * The id is opaque. It names the tab, not the file, because a tab keeps its
 * id while its content changes. The `tab-file-` prefix stays for the test
 * selectors. No code reads the id.
 */
function fileTab(entry: NavEntry<TabContentType>): Tab {
  return { id: `tab-file-${generateId()}`, ...entry, icon: iconForContentType(entry.contentType) };
}

function openEntry(entry: NavEntry<TabContentType>, opts: OpenOptions): void {
  const host = tabHost();
  const path = entry.metadata?.filePath as string;
  const named = opts.inTab ? (host.list().find((t) => t.id === opts.inTab) ?? null) : null;
  const candidate = opts.newTab ? null : (named ?? host.editorTab());
  if (candidate?.metadata?.filePath === path && entry.metadata?.scrollToLine === undefined) {
    host.activate(candidate.id);
    return;
  }
  // A pinned tab keeps its note, so an open that would move it opens a new
  // tab. Back and forward still move a pinned tab: they do not come here.
  const target = candidate && !candidate.isPinned ? candidate : null;
  if (target) {
    void host.navigate(target.id, entry).then((moved) => {
      if (!moved) return;
      host.activate(target.id);
      recordRecentFile(path, entry.title);
    });
    return;
  }
  const tab = fileTab(entry);
  if (host.open(tab, { placement: 'editor' })) recordRecentFile(path, tab.title);
}

/**
 * Show a file.
 *
 * A plain open shows the file in the active editor tab, as a link click does
 * in Obsidian. `newTab`, or no editor tab, opens a new tab. On the desktop
 * the editor group is the pane beside the files rail, never a chat. On a
 * phone it is the one content surface.
 */
export function openFileInEditor(filePath: string, fileName?: string, opts: OpenOptions = {}): void {
  openEntry(fileEntry(filePath, fileName), opts);
}

/**
 * Open (or focus, if already open) a file and overlay a proposed edit as an
 * inline diff in the real editor buffer: `original` is the current content,
 * `proposed` is what the agent wants. The file's editor renders the proposed
 * content diffed against the original (unified merge view), so a pending edit
 * is reviewed in place. Registering the diff before opening means an
 * already-open tab picks it up reactively too.
 */
export function openFileWithDiff(
  filePath: string,
  original: string,
  proposed: string,
  fileName?: string,
): void {
  pendingDiffActions.set(filePath, { original, proposed });
  openFileInEditor(filePath, fileName);
}

/**
 * Open a file in the editor and scroll to one line.
 *
 * The route out of a locked setting: a settings control the user's `init.lua`
 * pins names the file and the line that pins it, and this opens that line. A
 * lock with no route out is a dead end, which is why the jump is part of the
 * lock rather than an extra.
 *
 * The line goes on a new node, not on the open node: the panel reads its
 * props when it mounts, and a history move mounts it again.
 */
export function openFileAtLine(filePath: string, line: number, fileName?: string): void {
  openEntry(fileEntry(filePath, fileName, { scrollToLine: line }), {});
}

/**
 * Open a file as a tab in a SPECIFIC tab group (drag-a-file-onto-a-pane).
 * A tab that already shows the file gets the focus instead.
 */
export function openFileInGroup(
  groupId: string | null,
  filePath: string,
  fileName?: string,
): void {
  const existing = findTabByFilePath(filePath);
  if (existing) {
    windowActions.setActiveTab(existing.groupId, existing.tab.id);
    return;
  }
  if (!groupId) return;

  const newTab = fileTab(fileEntry(filePath, fileName));
  windowActions.addTab(groupId, newTab);
  recordRecentFile(filePath, newTab.title);
}

/**
 * A path went to the trash. Close each tab that shows a file under it: the
 * tab would show stale content that cannot be saved. A tab that only went
 * there before stays open. Its old nodes get a `missing` mark, so the
 * history popover draws them dim.
 */
export function closeTabsUnder(absPath: string, isDir: boolean): void {
  const prefix = `${absPath}/`;
  const gone = (fp: unknown) =>
    typeof fp === 'string' && (fp === absPath || (isDir && fp.startsWith(prefix)));
  const host = tabHost();
  for (const tab of [...host.list()]) {
    if (gone(tab.metadata?.filePath)) {
      host.remove(tab.id);
      continue;
    }
    const old = Object.values(tab.history?.nodes ?? {})
      .filter((n) => gone(n.metadata?.filePath))
      .map((n) => n.id);
    if (old.length > 0) host.markHistory(tab.id, old, { missing: true });
  }
}
```

**Step 4: Run the tests and see them pass**

```bash
just web-test unit src/lib/ src/stores/ src/windowing/ src/components/__tests__/FilesPanel.test.tsx src/components/__tests__/SearchPanel.test.tsx src/components/__tests__/CommandPalette.test.tsx
```

Expected: PASS. `tab-placement.test.ts` and `centre-placement.test.ts` still pass, because they build their own ids. If `setupDefaultState` makes `editorGroupId()` pick no group, read `web/src/lib/panel-actions.ts:84-104`. Then fix the test state, not the function.

**Step 5: Break the gates, then restore them**

1. In `findTabByFilePath`, delete the `if (editor && active && shows(active))` line. Run `file-actions.test.ts`. Expected: `prefers the active editor tab…` fails. Restore the line.
2. In `closeTabsUnder`, delete the `markHistory` line. Run both test files. Expected: the cases that expect a mark fail. Restore the line.
3. In `fileTab`, change the id to `` `tab-file-${String(entry.metadata?.filePath)}` ``. Run `file-actions.test.ts`. Expected: `opens a new tab with an opaque id…` fails. Restore the id.
4. In `openEntry`, change `candidate && !candidate.isPinned` to `candidate`. Run `file-actions.test.ts`. Expected: the two pinned cases that expect a new tab fail. Restore the code.
5. In `FilesPanel.tsx:344`, change the call to `openFileInEditor(node.absPath, node.name, { newTab: true })` for the run only. Run `FilesPanel.test.tsx`. Expected: `a leaf opens its file in the current tab…` fails. Restore the call.

**Step 6: Commit**

```bash
git -C crates/crucible-web/web add src/lib/file-actions.ts \
  src/lib/__tests__/file-actions.test.ts \
  src/lib/__tests__/tab-host-routing.test.ts \
  src/components/__tests__/FilesPanel.test.tsx \
  src/components/__tests__/SearchPanel.test.tsx \
  src/components/__tests__/CommandPalette.test.tsx
git commit -m "feat(web): show an opened file in the active editor tab

Every caller of openFileInEditor shows the file in the active editor
tab and adds a history node. newTab or a pinned tab opens a new tab. A
new tab gets an opaque id. closeTabsUnder closes only the tabs that
show a deleted file, and marks old nodes in the other tabs.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 8: Mount the panel again after a history move

**Files:**
- Modify: `web/src/components/windowing/Pane.tsx:107-131`
- Modify: `web/src/components/windowing/FloatingWindow.tsx:295-312`, and the memo block above it that declares `activeTabId` and `activeContentType`
- Modify: `web/src/components/mobile/ContentSurface.tsx:19-47`
- Modify: `web/src/components/FileViewerPanel.tsx:344-358` (the `isModified` sync)
- Test: `web/src/components/mobile/__tests__/ContentSurface.test.tsx` (append)
- Test: `web/src/components/windowing/__tests__/Pane.test.tsx` (append)

A move keeps the tab id. The three renderers mount a panel again only for a new id or a new content type. `reactiveMetadataProps` also fixes its key set at mount, so a new key such as `scrollToLine` never reaches a mounted panel. Thus the renderers also key on `history.current`. They also give the tab id to the panel as the `tabId` prop.

`FileViewerPanel` gives the dirty flag to one tab only. Two tabs can now show one buffer, so it marks each tab that shows the file.

**Step 1: Write the failing tests**

Append to the first `describe` in `web/src/components/mobile/__tests__/ContentSurface.test.tsx`:

```tsx
  it('mounts the panel again after a history move, and gives it the tab id', () => {
    let seenTab: unknown;
    resetGlobalRegistry();
    getGlobalRegistry().register(
      'file',
      'File',
      (props: { label?: string; tabId?: string }) => {
        onMount(() => {
          mounts += 1;
          seenTab = props.tabId;
        });
        return <div data-testid="counting-panel">{props.label}</div>;
      },
      'center',
    );
    const tree = (current: string) => ({
      current,
      nodes: {
        a: { id: 'a', parent: null, lastVisit: 1, title: 'A', contentType: 'file' as const, metadata: { label: 'first' } },
        b: { id: 'b', parent: 'a', lastVisit: 2, title: 'B', contentType: 'file' as const, metadata: { label: 'second', extra: 1 } },
      },
    });
    const [tab, setTab] = createSignal<Tab | null>(noteTab({ history: tree('a') }));
    render(() => <ContentSurface tab={tab} empty={<p>empty</p>} />);
    expect(mounts).toBe(1);
    setTab(noteTab({ history: tree('b'), metadata: { label: 'second', extra: 1 } }));
    expect(mounts).toBe(2);
    expect(seenTab).toBe('tab-1');
    expect(screen.getByTestId('counting-panel').textContent).toBe('second');
  });
```

Append to `web/src/components/windowing/__tests__/Pane.test.tsx`:

```tsx
describe('Pane — history moves', () => {
  it('mounts the panel again when the tab moves to another node', () => {
    let mounts = 0;
    let lastPath: unknown;
    resetGlobalRegistry();
    const Probe: Component<{ filePath?: string; tabId?: string }> = (props) => {
      mounts += 1;
      lastPath = props.filePath;
      return <div data-testid="probe">{props.tabId}</div>;
    };
    getGlobalRegistry().register('file', 'File', Probe as Component, 'center');
    windowActions.addTab(groupId, {
      id: 'tab-1',
      title: 'a.md',
      contentType: 'file',
      metadata: { filePath: '/k/a.md' },
    });

    const { getByTestId } = render(() => (
      <DragDropProvider>
        <Pane paneId={paneId} />
      </DragDropProvider>
    ));
    expect(mounts).toBe(1);
    expect(getByTestId('probe').textContent).toBe('tab-1');

    windowActions.updateTab(groupId, 'tab-1', { isModified: true });
    expect(mounts).toBe(1);

    windowActions.navigate(groupId, 'tab-1', {
      title: 'b.md',
      contentType: 'file',
      metadata: { filePath: '/k/b.md' },
    });
    expect(mounts).toBe(2);
    expect(lastPath).toBe('/k/b.md');
  });
});
```

**Step 2: Run the tests and see them fail**

```bash
just web-test unit src/components/mobile/__tests__/ContentSurface.test.tsx src/components/windowing/__tests__/Pane.test.tsx
```

Expected: FAIL. Both new cases report `expected 1 to be 2`. The Pane case also reports an empty `probe` text, because no `tabId` arrives.

**Step 3: Write the code**

In `web/src/components/windowing/Pane.tsx`, after line 115, add:

```ts
  // A history move keeps the tab id and changes the node. The panel mounts
  // again, so it reads the props of the new node from the start.
  const activeNodeId = createMemo(() => activeTab()?.history?.current ?? null);
```

In `renderContent`, add `activeNodeId();` after `const contentType = activeContentType();`. Change the `Dynamic` line to:

```tsx
      return <Dynamic component={panel.component} tabId={id} {...panelProps} />;
```

In `web/src/components/windowing/FloatingWindow.tsx`, add the same `activeNodeId` memo beside the `activeTabId` memo. In the render block (lines 295-312), add `activeNodeId();` after `const contentType = activeContentType();`, and give `tabId={id}` to `Dynamic`.

In `web/src/components/mobile/ContentSurface.tsx`, after line 34, add:

```ts
  // A history move keeps the id and changes the node. The panel mounts again.
  const nodeId = createMemo(() => props.tab()?.history?.current ?? null);
```

In the `panel` memo, add `nodeId();` after `const type = contentType();`. Change the `Dynamic` line to:

```tsx
    return <Dynamic component={component} tabId={id} {...panelProps} />;
```

In the doc comment above `ContentSurface` (lines 19-27), add a last sentence: "A history move changes the node, and that also rebuilds the panel."

In `web/src/components/FileViewerPanel.tsx`, replace the body of the `untrack` call in the `isModified` sync (lines 353-357) with:

```ts
    untrack(() => {
      const host = tabHost();
      // Two tabs can show one note. They share one buffer, so each one gets
      // the mark. A tab that already has the value gets no write.
      for (const tab of host.list()) {
        if (tab.metadata?.filePath === props.filePath && !!tab.isModified !== isModified) {
          host.update(tab.id, { isModified });
        }
      }
    });
```

In the comment above that effect (line 346), change `findTabByFilePath reads windowStore.tabGroups` to `the host list reads windowStore.tabGroups`.

**Step 4: Run the tests and see them pass**

```bash
just web-test unit src/components/
```

Expected: PASS. The old case `keeps the panel mounted when a field other than identity changes` still passes.

**Step 5: Break the gate, then restore it**

In `Pane.tsx`, delete the `activeNodeId();` call. Run the Pane test. Expected: `mounts the panel again when the tab moves…` fails. Restore the call. Do the same in `ContentSurface.tsx` with `nodeId();`, and expect the ContentSurface case to fail. Restore the call.

**Step 6: Commit**

```bash
git -C crates/crucible-web/web add src/components/windowing/Pane.tsx \
  src/components/windowing/FloatingWindow.tsx \
  src/components/mobile/ContentSurface.tsx \
  src/components/FileViewerPanel.tsx \
  src/components/mobile/__tests__/ContentSurface.test.tsx \
  src/components/windowing/__tests__/Pane.test.tsx
git commit -m "feat(web): mount a panel again after a history move

The pane, the floating window and the phone surface also key the panel
on the current history node, and give it the tab id. The file viewer
marks each tab that shows its buffer as modified.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 9: Link clicks

**Files:**
- Modify: `web/src/lib/note-actions.ts:8-11` (imports), `:85-102` (`openNoteInEditor`)
- Modify: `web/src/lib/markdown-click.ts` (the whole file, 61 lines)
- Modify: `web/src/components/Message.tsx:141`, `web/src/components/AssistantTurn.tsx:113`, `web/src/components/editor/MarkdownPreview.tsx:167-170` (add `onAuxClick`)
- Modify: `web/src/components/editor/wikilink-extension.ts:120-157`
- Modify: `web/src/components/editor/CodeMirrorEditor.tsx:92-93`, `:246-247`
- Modify: `web/src/components/editor/EditorWithPreview.tsx:29`
- Modify: `web/src/components/FileViewerPanel.tsx:39-54` (props), `:486` (the editor area `div`), `:522-527` (`onFollowLink`)
- Modify: `web/src/components/canvas/CanvasCard.tsx:101`
- Test: `web/src/lib/__tests__/markdown-click.test.ts` (create)
- Test: `web/src/components/editor/__tests__/wikilink-extension.test.ts:109-146`
- Test: `web/src/lib/__tests__/note-actions.test.ts:188`, `:219` (and one new case)

The gestures:

| Where | Plain click | Ctrl/Cmd+click | Middle click | Mod-Enter |
|---|---|---|---|---|
| Rendered markdown (chat, reading view, canvas card preview) | same tab | new tab | new tab | — |
| CodeMirror editor (source, live) | same tab | new tab | new tab | same tab |

The editor follows Obsidian (user decision). A plain click on a wikilink follows it, so a mouse click cannot put the cursor on a link. The keyboard can: the arrow keys move into the link, and Mod-Enter follows from there. This replaces the old WS-209 rule, in which a plain click only moved the cursor and Ctrl+click followed.

**How a drag still selects text.** The link handler takes the `mousedown` from CodeMirror, so the cursor does not move and a live-preview link does not open to its source under the pointer. The handler then watches the pointer:

- The button comes up within 4 px of the press: the press is a click, and the link is followed.
- The pointer moves 4 px or more with the button down: the press is a drag. The handler sets the selection from the press point to the pointer on each move (`view.posAtCoords`, `userEvent: 'select.pointer'`), as the CodeMirror drag does. On release it follows nothing.
- Shift+click and Alt+click go to CodeMirror unchanged, so Shift+click still extends a selection and Alt still makes a rectangular selection.
- A drag that starts outside a link and crosses a link is a CodeMirror drag. The handler never sees it.

A double click on a link does not select the word, because the first press follows the link. That is the cost that the user accepts.

"Same tab" is the tab that holds the link when a viewer declares it with `data-nav-tab`. The file viewer declares its tab. The chat and the canvas declare none, so the active editor tab takes the click. A hover popover declares none either.

A middle click sends `auxclick`, not `click`. The three markdown surfaces add `onAuxClick`.

**Step 1: Write the failing tests**

Create `web/src/lib/__tests__/markdown-click.test.ts`:

```ts
import { describe, it, expect, vi, beforeEach } from 'vitest';

const openNoteInEditor = vi.hoisted(() => vi.fn(async () => {}));
vi.mock('@/lib/note-actions', () => ({
  openNoteInEditor,
  kilnForElement: (el: Element | null | undefined) =>
    el?.closest?.('[data-kiln]')?.getAttribute('data-kiln') || undefined,
}));

import { makeMarkdownClickHandler } from '@/lib/markdown-click';

/** Rendered markdown with one link, under the given attributes. */
function mount(attrs: string, link = '<a class="wikilink" href="#" data-note="Other">Other</a>'): HTMLAnchorElement {
  document.body.innerHTML = `<div ${attrs}><p>${link}</p></div>`;
  const root = document.body.firstElementChild as HTMLElement;
  const handler = makeMarkdownClickHandler();
  root.addEventListener('click', handler);
  root.addEventListener('auxclick', handler);
  return root.querySelector('a')!;
}

const click = (el: Element, init: MouseEventInit = {}, type = 'click') =>
  el.dispatchEvent(new MouseEvent(type, { bubbles: true, cancelable: true, ...init }));

beforeEach(() => {
  openNoteInEditor.mockClear();
});

describe('link clicks in rendered markdown', () => {
  it('a plain click shows the note in the tab that holds the link', () => {
    click(mount('data-kiln="/k" data-nav-tab="t9"'));
    expect(openNoteInEditor).toHaveBeenCalledWith('Other', '/k', { newTab: false, inTab: 't9' });
  });

  it.each([['ctrlKey'], ['metaKey']])('a click with %s opens a new tab', (key) => {
    click(mount('data-kiln="/k" data-nav-tab="t9"'), { [key]: true });
    expect(openNoteInEditor).toHaveBeenCalledWith('Other', '/k', expect.objectContaining({ newTab: true }));
  });

  it('a middle click opens a new tab', () => {
    click(mount('data-kiln="/k"'), { button: 1 }, 'auxclick');
    expect(openNoteInEditor).toHaveBeenCalledWith('Other', '/k', expect.objectContaining({ newTab: true }));
  });

  it('a right click does nothing here, because the context menu owns it', () => {
    click(mount('data-kiln="/k"'), { button: 2 }, 'auxclick');
    expect(openNoteInEditor).not.toHaveBeenCalled();
  });

  it('a click in the chat names no tab, so the active editor tab takes it', () => {
    click(mount('data-kiln="/k"'));
    expect(openNoteInEditor).toHaveBeenCalledWith('Other', '/k', { newTab: false, inTab: undefined });
  });

  it('a relative link follows the same rule', () => {
    click(mount('data-kiln="/k" data-nav-tab="t9"', '<a href="notes/Other.md">Other</a>'), { ctrlKey: true });
    expect(openNoteInEditor).toHaveBeenCalledWith('notes/Other', '/k', { newTab: true, inTab: 't9' });
  });
});
```

In `web/src/components/editor/__tests__/wikilink-extension.test.ts`:

- In `Mod-Enter command follows the link under the cursor` (line 117), change `expect(onFollow).toHaveBeenCalledWith('My Note');` to `expect(onFollow).toHaveBeenCalledWith('My Note', { newTab: false });`.
- Replace the two mouse cases (`Ctrl+Click on a decorated link follows it` and `plain click does not follow`, lines 129-145) with:

```ts
  /** A press on the link, then a release at the given offset from the press. */
  function press(view: EditorView, init: MouseEventInit = {}, moveBy = 0) {
    const link = view.dom.querySelector('.cm-wikilink')!;
    const down = new MouseEvent('mousedown', { bubbles: true, cancelable: true, button: 0, clientX: 10, clientY: 10, ...init });
    link.dispatchEvent(down);
    if (moveBy > 0) {
      document.dispatchEvent(new MouseEvent('mousemove', { bubbles: true, buttons: 1, clientX: 10 + moveBy, clientY: 10 }));
    }
    document.dispatchEvent(new MouseEvent('mouseup', { bubbles: true, clientX: 10 + moveBy, clientY: 10 }));
    return down;
  }

  it('a plain click on a decorated link follows it in the same tab, and keeps the cursor', () => {
    const onFollow = vi.fn();
    const view = track(makeView('see [[My Note]]', onFollow));
    const before = view.state.selection.main.head;
    const down = press(view);
    expect(down.defaultPrevented).toBe(true);
    expect(onFollow).toHaveBeenCalledWith('My Note', { newTab: false });
    expect(view.state.selection.main.head).toBe(before);
  });

  it.each([['ctrlKey'], ['metaKey']])('a click with %s follows the link into a new tab', (key) => {
    const onFollow = vi.fn();
    const view = track(makeView('see [[My Note]]', onFollow));
    press(view, { [key]: true });
    expect(onFollow).toHaveBeenCalledWith('My Note', { newTab: true });
  });

  it('a middle click follows the link into a new tab at once', () => {
    const onFollow = vi.fn();
    const view = track(makeView('see [[My Note]]', onFollow));
    const link = view.dom.querySelector('.cm-wikilink')!;
    link.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true, button: 1 }));
    expect(onFollow).toHaveBeenCalledWith('My Note', { newTab: true });
  });

  it('a drag that starts on a link selects text and follows nothing', () => {
    const onFollow = vi.fn();
    const view = track(makeView('see [[My Note]]', onFollow));
    // The press point is position 5, and the pointer ends at position 9.
    vi.mocked(EditorView.prototype.posAtCoords).mockReturnValueOnce(5).mockReturnValue(9);
    press(view, {}, 12);
    expect(onFollow).not.toHaveBeenCalled();
    expect(view.state.selection.main).toMatchObject({ anchor: 5, head: 9 });
  });

  it('a small wobble under 4 px is still a click', () => {
    const onFollow = vi.fn();
    const view = track(makeView('see [[My Note]]', onFollow));
    press(view, {}, 3);
    expect(onFollow).toHaveBeenCalledTimes(1);
  });

  it.each([['shiftKey'], ['altKey']])('a click with %s goes to the editor and follows nothing', (key) => {
    const onFollow = vi.fn();
    const view = track(makeView('see [[My Note]]', onFollow));
    const down = press(view, { [key]: true });
    expect(down.defaultPrevented).toBe(false);
    expect(onFollow).not.toHaveBeenCalled();
  });

  it('a click outside a link goes to the editor', () => {
    const onFollow = vi.fn();
    const view = track(makeView('see [[My Note]]', onFollow));
    const down = new MouseEvent('mousedown', { bubbles: true, cancelable: true, button: 0 });
    view.contentDOM.firstElementChild!.dispatchEvent(down);
    document.dispatchEvent(new MouseEvent('mouseup', { bubbles: true }));
    expect(onFollow).not.toHaveBeenCalled();
  });

  it('stops watching the pointer after the release', () => {
    const onFollow = vi.fn();
    const view = track(makeView('see [[My Note]]', onFollow));
    press(view);
    document.dispatchEvent(new MouseEvent('mouseup', { bubbles: true }));
    expect(onFollow).toHaveBeenCalledTimes(1);
  });
```

jsdom has no layout and no `document.elementFromPoint`, so the real `posAtCoords` cannot run there. At the top of the `follow gestures` block, add:

```ts
  beforeEach(() => {
    // jsdom cannot map a point to a position. Each case says what the map answers.
    vi.spyOn(EditorView.prototype, 'posAtCoords').mockReturnValue(null);
  });
  afterEach(() => {
    vi.restoreAllMocks();
  });
```

The Playwright story in Task 13 also checks a drag selection in a real browser. In the last-but-one case, the press lands on the line element, and `closest('.cm-wikilink')` looks only at the element and its ancestors, so the press is outside the link.

In `web/src/lib/__tests__/note-actions.test.ts`:

- Change line 188 to `expect(openFileInEditorMock).toHaveBeenCalledWith('/kiln/notes/rust.md', 'Rust', { newTab: false });`.
- Change line 219 to `expect(openFileInEditorMock).toHaveBeenCalledWith('/kiln/Help/Wikilinks.md', 'Wikilinks', { newTab: false });`.
- Add a case after the one at line 188:

```ts
  it('passes the tab choice through to the opener', async () => {
    resolveNotePathMock.mockResolvedValue({
      path: 'notes/rust.md',
      absolutePath: '/kiln/notes/rust.md',
      title: 'Rust',
    });
    await openNoteInEditor('rust', '/kiln', { newTab: true });
    expect(openFileInEditorMock).toHaveBeenLastCalledWith('/kiln/notes/rust.md', 'Rust', { newTab: true });
    await openNoteInEditor('rust', '/kiln', { inTab: 't9' });
    expect(openFileInEditorMock).toHaveBeenLastCalledWith('/kiln/notes/rust.md', 'Rust', { newTab: false, inTab: 't9' });
  });
```

**Step 2: Run the tests and see them fail**

```bash
just web-test unit src/lib/__tests__/markdown-click.test.ts src/components/editor/__tests__/wikilink-extension.test.ts src/lib/__tests__/note-actions.test.ts
```

Expected: FAIL. The markdown cases report a call with two arguments. The middle-click and right-click cases fail, because the handler ignores `auxclick` buttons only by chance. The editor cases fail: a plain click does not follow, Ctrl+click follows in the same tab, and a drag is not tracked.

**Step 3: Write the code**

In `web/src/lib/note-actions.ts`, change the import at line 9 to:

```ts
import { openFileInEditor, type OpenOptions } from './file-actions';
```

Change `openNoteInEditor` (lines 85-102) to:

```ts
/**
 * Resolve a wikilink target to its kiln file and show it.
 *
 * `opts` says which tab: the active editor tab by default, a named tab, or a
 * new tab. The kiln is the one that the caller gives (for example the kiln of
 * the chat session). There is no default kiln.
 */
export async function openNoteInEditor(name: string, kiln?: string, opts: OpenOptions = {}): Promise<void> {
  try {
    const hit = await resolveTarget(name, kiln);
    if (!hit) throw new Error(`not found: ${name}`);
    openFileInEditor(hit.absPath, hit.title, { ...opts, newTab: opts.newTab ?? false });
  } catch (err) {
    const message =
      err instanceof Error && /not found|404/i.test(err.message)
        ? `Note not found: ${name}`
        : `Failed to open note: ${name}`;
    notificationActions.addNotification('warning', message);
  }
}
```

Replace `web/src/lib/markdown-click.ts` with:

```ts
import { kilnForElement, openNoteInEditor } from '@/lib/note-actions';

/** True when a click asks for a new tab: Ctrl, Cmd or the middle button. */
export function wantsNewTab(event: MouseEvent): boolean {
  return event.ctrlKey || event.metaKey || event.button === 1;
}

/**
 * The tab that holds the clicked content, or undefined.
 *
 * A viewer declares its tab with `data-nav-tab`, as it declares its kiln with
 * `data-kiln`. Content with no declared tab (the chat, a canvas, a hover
 * popover) sends its links to the active editor tab.
 */
function tabForElement(el: Element): string | undefined {
  return el.closest('[data-nav-tab]')?.getAttribute('data-nav-tab') || undefined;
}

/**
 * Click delegation for rendered-markdown containers. Chat transcripts and the
 * note reading view share one implementation so their link semantics can't
 * drift: `[data-copy]` buttons copy the adjacent code block, `[data-note]`
 * anchors (wikilinks) open notes, external links open a new tab, and other
 * relative hrefs are treated as kiln note references.
 *
 * A plain click shows the note in place. Ctrl, Cmd or the middle button opens
 * a new tab. Attach the handler to `click` and to `auxclick`, because a
 * middle click sends only `auxclick`.
 *
 * The kiln is read from the DOM — the nearest `data-kiln` ancestor of the
 * clicked link — rather than passed in. Hover reads it from the same element,
 * so the two cannot disagree about which kiln a link belongs to.
 */
export function makeMarkdownClickHandler(): (event: MouseEvent) => void {
  return (event: MouseEvent) => {
    // The right button also sends `auxclick`. The context menu owns it.
    if (event.type === 'auxclick' && event.button !== 1) return;
    const target = event.target as HTMLElement | null;

    const copyBtn = target?.closest?.('[data-copy]');
    if (copyBtn) {
      event.preventDefault();
      const pre = copyBtn.closest('.md-codeblock')?.querySelector('pre');
      const code = pre?.textContent ?? '';
      if (code) {
        void navigator.clipboard?.writeText(code);
        const prev = copyBtn.textContent;
        copyBtn.textContent = 'Copied';
        copyBtn.classList.add('is-copied');
        setTimeout(() => {
          copyBtn.textContent = prev;
          copyBtn.classList.remove('is-copied');
        }, 1200);
      }
      return;
    }

    const noteElement = target?.closest('[data-note]') as HTMLElement | null;
    if (noteElement) {
      event.preventDefault();
      const note = noteElement.dataset.note;
      if (note) {
        void openNoteInEditor(note, kilnForElement(noteElement), {
          newTab: wantsNewTab(event),
          inTab: tabForElement(noteElement),
        });
      }
      return;
    }

    const anchor = target?.closest('a') as HTMLAnchorElement | null;
    if (!anchor) return;
    const href = anchor.getAttribute('href') ?? '';
    if (!href || href.startsWith('#')) return;
    event.preventDefault();
    if (/^[a-z][a-z0-9+.-]*:/i.test(href)) {
      window.open(href, '_blank', 'noopener,noreferrer');
      return;
    }
    const note = decodeURIComponent(href)
      .replace(/^\.?\//, '')
      .replace(/\.md$/i, '');
    void openNoteInEditor(note, kilnForElement(anchor), {
      newTab: wantsNewTab(event),
      inTab: tabForElement(anchor),
    });
  };
}
```

Add the middle-click wiring:

- `web/src/components/Message.tsx:141`: add `onAuxClick={handleMarkdownClick}` after `onClick={handleMarkdownClick}`.
- `web/src/components/AssistantTurn.tsx:113`: add `onAuxClick={props.onMarkdownClick}` after `onClick={props.onMarkdownClick}`.
- `web/src/components/editor/MarkdownPreview.tsx:167-170`: add `onAuxClick={handleClick}` after the `onClick` block.

In `web/src/components/editor/wikilink-extension.ts`, replace lines 120-157 with:

```ts
/** How a follow asks to show the target. */
export interface FollowOptions {
  newTab: boolean;
}

export type FollowHandler = (target: string, opts: FollowOptions) => void;

/** Keymap command: follow the wikilink under the cursor, in the same tab. */
export function followWikilinkAtCursor(onFollow: FollowHandler): (view: EditorView) => boolean {
  return (view) => {
    const target = wikilinkTargetAt(view.state, view.state.selection.main.head);
    if (!target) return false;
    onFollow(target, { newTab: false });
    return true;
  };
}

/** A press that moves this far, in px, is a drag and not a click. */
const DRAG_PX = 4;

/** The link target under a mouse event, or null. */
function linkUnder(event: MouseEvent): string | null {
  const el = (event.target as Element | null)?.closest?.('.cm-wikilink');
  return el?.getAttribute('data-note') ?? null;
}

/**
 * Follow on release, or select on drag.
 *
 * CodeMirror never sees the press, so the cursor stays where it was and a
 * live-preview link does not open to its source under the pointer. When the
 * pointer moves DRAG_PX or more with the button down, the press becomes a
 * text selection from the press point, as a CodeMirror drag makes.
 */
function watchPress(view: EditorView, down: MouseEvent, follow: () => void): void {
  const doc = view.dom.ownerDocument;
  const anchor = view.posAtCoords({ x: down.clientX, y: down.clientY });
  let dragging = false;
  const move = (e: MouseEvent) => {
    const far = Math.hypot(e.clientX - down.clientX, e.clientY - down.clientY) >= DRAG_PX;
    if (!dragging && !far) return;
    dragging = true;
    const head = view.posAtCoords({ x: e.clientX, y: e.clientY });
    if (anchor !== null && head !== null) {
      view.dispatch({ selection: { anchor, head }, userEvent: 'select.pointer' });
    }
  };
  const up = () => {
    doc.removeEventListener('mousemove', move);
    doc.removeEventListener('mouseup', up);
    if (dragging) view.focus();
    else follow();
  };
  doc.addEventListener('mousemove', move);
  doc.addEventListener('mouseup', up);
}

/**
 * Full wikilink navigation bundle: decorations, styling, the mouse gestures,
 * and the Mod-Enter follow binding.
 *
 * The gestures follow Obsidian. A plain click follows the link in the same
 * tab. Ctrl/Cmd+click and a middle click open a new tab. Shift and Alt
 * clicks stay with CodeMirror, for selection. The keyboard puts the cursor on
 * a link, and Mod-Enter follows it in the same tab.
 */
export function wikilinkNavigation(onFollow: FollowHandler): Extension {
  return [
    wikilinkHighlighter,
    wikilinkTheme,
    EditorView.domEventHandlers({
      mousedown: (event, view) => {
        const target = linkUnder(event);
        if (!target) return false;
        if (event.button === 1) {
          event.preventDefault();
          onFollow(target, { newTab: true });
          return true;
        }
        if (event.button !== 0 || event.shiftKey || event.altKey) return false;
        const newTab = event.ctrlKey || event.metaKey;
        event.preventDefault();
        watchPress(view, event, () => onFollow(target, { newTab }));
        return true;
      },
    }),
    // defaultKeymap binds Mod-Enter to insertBlankLine; Prec.high makes the
    // follow command win when the cursor is inside a link. It returns false
    // outside links, so insertBlankLine still runs everywhere else.
    Prec.high(keymap.of([{ key: 'Mod-Enter', run: followWikilinkAtCursor(onFollow) }])),
  ];
}
```

In `web/src/components/editor/CodeMirrorEditor.tsx`:

- Import `type FollowOptions` from `./wikilink-extension` (line 22).
- Change the prop at lines 92-93 to:

```ts
  /** Follow a [[wikilink]] (a click, Ctrl/Cmd+click, a middle click or Mod-Enter); markdown files only. */
  onFollowLink?: (target: string, opts: FollowOptions) => void;
```

- Change line 247 to:

```ts
      extensions.push(wikilinkNavigation((target, opts) => props.onFollowLink?.(target, opts)));
```

In `web/src/components/editor/EditorWithPreview.tsx:29`, change the prop type to `onFollowLink?: (target: string, opts: FollowOptions) => void;`, and import `type FollowOptions` from `./wikilink-extension`.

In `web/src/components/FileViewerPanel.tsx`:

- Add to `FileViewerPanelProps` (lines 39-54):

```ts
  /** The tab that shows this file. The pane gives it. A hover popover has none. */
  tabId?: string;
  /** The history node points at a deleted file. Set by `closeTabsUnder`. */
  missing?: boolean;
```

- Add a helper after `owningKilnAsync`:

```ts
  /** The tab that link follows go to, or undefined for a hover popover. */
  const navTab = () => (props.background ? undefined : props.tabId);
```

- On the editor area `div` (line 486, `<div class="flex-1 overflow-hidden" ref={attachNativeMenuGuard}>`), add `data-nav-tab={navTab()}`.
- Change `onFollowLink` (lines 522-527) to:

```tsx
                  onFollowLink={(target, opts) =>
                    // The file's own kiln, or none. Falling back to the active
                    // kiln let a project file — which belongs to no kiln —
                    // follow links into whichever kiln was showing.
                    void owningKilnAsync(file().path).then((kiln) =>
                      openNoteInEditor(target, kiln, { ...opts, inTab: navTab() }),
                    )
                  }
```

In `web/src/components/canvas/CanvasCard.tsx:101`, change the line to:

```tsx
          onFollowLink={(target, opts) => void openNoteInEditor(target, props.kiln, opts)}
```

**Step 4: Run the tests and the typecheck, and see them pass**

```bash
just web-test unit src/lib/ src/components/
cd crates/crucible-web/web && bun run typecheck
```

Expected: PASS, and no type error. `EditorPanel.tsx:153` passes a handler with one parameter. TypeScript accepts it.

**Step 5: Break the gates, then restore them**

1. In `wantsNewTab`, delete `|| event.button === 1`. Run the markdown test. Expected: `a middle click opens a new tab` fails. Restore the code.
2. Delete the `auxclick` guard line in the handler. Run the markdown test. Expected: `a right click does nothing here` fails. Restore the line.
3. In `wikilinkNavigation`, change `const newTab = event.ctrlKey || event.metaKey;` to `const newTab = false;`. Run the editor test. Expected: the Ctrl and Cmd cases fail. Restore the code.
4. In `watchPress`, change `if (!dragging && !far) return;` to `return;`. Run the editor test. Expected: the drag case fails, because the press follows the link. Restore the code.
5. In `wikilinkNavigation`, delete `|| event.shiftKey || event.altKey`. Run the editor test. Expected: the Shift and Alt cases fail. Restore the code.

**Step 6: Commit**

```bash
git -C crates/crucible-web/web add src/lib/note-actions.ts \
  src/lib/markdown-click.ts \
  src/lib/__tests__/markdown-click.test.ts \
  src/lib/__tests__/note-actions.test.ts \
  src/components/Message.tsx \
  src/components/AssistantTurn.tsx \
  src/components/editor/MarkdownPreview.tsx \
  src/components/editor/wikilink-extension.ts \
  src/components/editor/__tests__/wikilink-extension.test.ts \
  src/components/editor/CodeMirrorEditor.tsx \
  src/components/editor/EditorWithPreview.tsx \
  src/components/FileViewerPanel.tsx \
  src/components/canvas/CanvasCard.tsx
git commit -m "feat(web): follow a link in the same tab, or in a new tab on request

A plain click in rendered markdown shows the note in the tab that holds
the link, or in the active editor tab. Ctrl, Cmd and the middle button
open a new tab. The editor does the same, as Obsidian does: a plain
click and Mod-Enter follow in the same tab. A drag that starts on a link
still selects text.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 10: The nav bar, the history keys and the missing state

**Files:**
- Create: `web/src/components/TabNavBar.tsx`
- Create: `web/src/lib/history-keys.ts`
- Modify: `web/src/windowing/shortcuts.ts:13-22` (`LAYOUT_SHORTCUTS`)
- Modify: `web/src/components/windowing/WindowManager.tsx:19-21` (imports), `:226-233` (`handleKeyDown`)
- Modify: `web/src/components/FileViewerPanel.tsx:305-314` (the load effect), `:360-392` (the early returns), `:395` (the main return)
- Modify: `web/src/components/canvas/CanvasPanel.tsx:106-109` (props), `:1043-1049` (the header)
- Test: `web/src/components/__tests__/TabNavBar.test.tsx` (create)
- Test: `web/src/lib/__tests__/history-keys.test.ts` (create)
- Test: `web/src/lib/__tests__/keyboard-shortcuts.test.ts` (append)
- Test: `web/src/components/__tests__/FileViewerPanel.test.tsx` (append)

The core owns the chords, because `windowing/shortcuts.ts` holds the layout chords and the design names that file. The action runs in the app, because only the app host knows the unsaved-changes gate.

CodeMirror binds Alt+Left and Alt+Right to a cursor move (`cursorSyntaxLeft`). The keyboard loop runs on `document`, after the editor. Thus a key that the editor took (`defaultPrevented`) is not a history move. The phone shell has no keyboard loop, so the keys work on the desktop only.

**Step 1: Write the failing tests**

Create `web/src/components/__tests__/TabNavBar.test.tsx`:

```tsx
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@solidjs/testing-library';
import { produce } from 'solid-js/store';

const device = vi.hoisted(() => ({ compact: false }));
vi.mock('@/stores/deviceStore', () => ({ isCompact: () => device.compact }));
vi.mock('@/lib/confirm-discard', () => ({ confirmDiscard: vi.fn(async () => false) }));

import { TabNavBar } from '@/components/TabNavBar';
import { setStore, windowActions, windowStore } from '@/stores/windowStore';

const back = () => screen.getByTestId('tab-nav-back') as HTMLButtonElement;
const forward = () => screen.getByTestId('tab-nav-forward') as HTMLButtonElement;
const shownPath = () => windowStore.tabGroups.g!.tabs[0]!.metadata?.filePath;

beforeEach(() => {
  setStore(
    produce((s) => {
      s.layout = { id: 'pane-editor', type: 'pane', tabGroupId: 'g' };
      s.tabGroups = {
        g: {
          id: 'g',
          tabs: [{ id: 't1', title: 'a.md', contentType: 'file', metadata: { filePath: '/k/a.md' } }],
          activeTabId: 't1',
        },
      };
      s.floatingWindows = [];
      s.activePaneId = 'pane-editor';
    }),
  );
});
afterEach(cleanup);

describe('TabNavBar', () => {
  it('disables both buttons for a tab that never moved, and shows the path', () => {
    render(() => <TabNavBar tabId="t1" path="notes/a.md" />);
    expect(back().disabled).toBe(true);
    expect(forward().disabled).toBe(true);
    expect(screen.getByTestId('tab-nav-path').textContent).toBe('notes/a.md');
  });

  it('enables back after a navigate, and forward after a back', async () => {
    render(() => <TabNavBar tabId="t1" path="notes/a.md" />);
    windowActions.navigate('g', 't1', { title: 'b.md', contentType: 'file', metadata: { filePath: '/k/b.md' } });
    expect(back().disabled).toBe(false);
    expect(forward().disabled).toBe(true);

    fireEvent.click(back());
    await waitFor(() => expect(shownPath()).toBe('/k/a.md'));
    expect(back().disabled).toBe(true);
    expect(forward().disabled).toBe(false);

    fireEvent.click(forward());
    await waitFor(() => expect(shownPath()).toBe('/k/b.md'));
  });

  it('names the keys in the button titles', () => {
    render(() => <TabNavBar tabId="t1" path="notes/a.md" />);
    expect(back().title).toBe('Back (Alt+ArrowLeft)');
    expect(forward().title).toBe('Forward (Alt+ArrowRight)');
  });
});
```

Create `web/src/lib/__tests__/history-keys.test.ts`:

```ts
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { produce } from 'solid-js/store';

vi.mock('@/stores/deviceStore', () => ({ isCompact: () => false }));
vi.mock('@/lib/confirm-discard', () => ({ confirmDiscard: vi.fn(async () => false) }));

import { isHistoryAction, runHistoryKey } from '@/lib/history-keys';
import { setStore, windowActions, windowStore } from '@/stores/windowStore';

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));
const shownPath = () => windowStore.tabGroups.g!.tabs[0]!.metadata?.filePath;
const key = (k: string) => new KeyboardEvent('keydown', { key: k, altKey: true, cancelable: true });

beforeEach(() => {
  setStore(
    produce((s) => {
      s.layout = { id: 'pane-editor', type: 'pane', tabGroupId: 'g' };
      s.tabGroups = {
        g: {
          id: 'g',
          tabs: [{ id: 't1', title: 'a.md', contentType: 'file', metadata: { filePath: '/k/a.md' } }],
          activeTabId: 't1',
        },
      };
      s.activePaneId = 'pane-editor';
    }),
  );
  windowActions.navigate('g', 't1', { title: 'b.md', contentType: 'file', metadata: { filePath: '/k/b.md' } });
});

describe('the history keys', () => {
  it('knows only the two history actions', () => {
    expect(isHistoryAction('historyBack')).toBe(true);
    expect(isHistoryAction('historyForward')).toBe(true);
    expect(isHistoryAction('nextTab')).toBe(false);
    expect(isHistoryAction('toString')).toBe(false);
  });

  it('moves the active tab, and stops the browser history move', async () => {
    const e = key('ArrowLeft');
    runHistoryKey('historyBack', e);
    expect(e.defaultPrevented).toBe(true);
    await settle();
    expect(shownPath()).toBe('/k/a.md');
    runHistoryKey('historyForward', key('ArrowRight'));
    await settle();
    expect(shownPath()).toBe('/k/b.md');
  });

  it('leaves a key that the editor took', async () => {
    const e = key('ArrowLeft');
    e.preventDefault();
    runHistoryKey('historyBack', e);
    await settle();
    expect(shownPath()).toBe('/k/b.md');
  });
});
```

Append to the `matchShortcut` block of `web/src/lib/__tests__/keyboard-shortcuts.test.ts`:

```ts
    it('matches Alt+ArrowLeft and Alt+ArrowRight to the tab history', () => {
      const ev = (key: string, altKey: boolean) =>
        ({ key, altKey, ctrlKey: false, shiftKey: false, metaKey: false }) as KeyboardEvent;
      expect(matchShortcut(ev('ArrowLeft', true))).toBe('historyBack');
      expect(matchShortcut(ev('ArrowRight', true))).toBe('historyForward');
      expect(matchShortcut(ev('ArrowLeft', false))).toBeNull();
    });
```

Append to `web/src/components/__tests__/FileViewerPanel.test.tsx`:

```tsx
describe('FileViewerPanel — the nav bar and the missing state', () => {
  beforeEach(() => {
    openFilesValue = [];
    openFileSpy.mockClear();
  });

  it('draws the nav bar for a tab, with the path inside the kiln', () => {
    render(() => <FileViewerPanel filePath={FILE_PATH} tabId="t1" />);
    expect(screen.getByTestId('tab-nav-bar')).toBeTruthy();
    expect(screen.getByTestId('tab-nav-path').textContent).toBe('notes/from-tui.md');
  });

  it('draws no nav bar in a hover popover or without a tab', () => {
    render(() => <FileViewerPanel filePath={FILE_PATH} tabId="t1" background />);
    expect(screen.queryByTestId('tab-nav-bar')).toBeNull();
    cleanup();
    render(() => <FileViewerPanel filePath={FILE_PATH} />);
    expect(screen.queryByTestId('tab-nav-bar')).toBeNull();
  });

  it('shows the missing state for a deleted file, and reads nothing', () => {
    render(() => <FileViewerPanel filePath={FILE_PATH} tabId="t1" missing />);
    expect(screen.getByTestId('file-missing')).toBeTruthy();
    expect(screen.getByTestId('tab-nav-bar')).toBeTruthy();
    expect(openFileSpy).not.toHaveBeenCalled();
  });
});
```

`kilnsValue` is `[{ path: '/kiln' }]` in that file, so the kiln-relative path is `notes/from-tui.md`.

**Step 2: Run the tests and see them fail**

```bash
just web-test unit src/components/__tests__/TabNavBar.test.tsx src/lib/__tests__/history-keys.test.ts src/lib/__tests__/keyboard-shortcuts.test.ts src/components/__tests__/FileViewerPanel.test.tsx
```

Expected: FAIL. The two new modules do not resolve. The chord case reports `null`. The FileViewerPanel cases find no `tab-nav-bar`.

**Step 3: Add the chords**

In `web/src/windowing/shortcuts.ts`, add to the end of `LAYOUT_SHORTCUTS` (before line 22):

```ts
  // The browser binds the same chords to its own history. The app handler
  // stops the browser only when the key reaches it.
  { key: 'ArrowLeft', modifiers: ['alt'], action: 'historyBack', description: 'Back in tab history' },
  { key: 'ArrowRight', modifiers: ['alt'], action: 'historyForward', description: 'Forward in tab history' },
```

**Step 4: Write the key handler**

Create `web/src/lib/history-keys.ts`:

```ts
import { tabHost } from './tab-host';

const HISTORY_ACTIONS = { historyBack: 'back', historyForward: 'forward' } as const;

export type HistoryAction = keyof typeof HISTORY_ACTIONS;

/** True for the two actions of the tab history. An inherited name such as `toString` is not one. */
export function isHistoryAction(action: string): action is HistoryAction {
  // `Object.hasOwn` is ES2022, and the web compiles against ES2020.
  return Object.prototype.hasOwnProperty.call(HISTORY_ACTIONS, action);
}

/**
 * Run Alt+Left or Alt+Right on the active tab.
 *
 * CodeMirror binds these keys to a cursor move, and the keyboard loop runs
 * after the editor. A key that the editor took is not a history move. Any
 * other such key stops the browser, which would leave the app.
 */
export function runHistoryKey(action: HistoryAction, e: KeyboardEvent): void {
  if (e.defaultPrevented) return;
  e.preventDefault();
  const host = tabHost();
  const tab = host.activeTab();
  if (!tab) return;
  void (HISTORY_ACTIONS[action] === 'back' ? host.back(tab.id) : host.forward(tab.id));
}
```

In `web/src/components/windowing/WindowManager.tsx`, add the import `import { isHistoryAction, runHistoryKey } from '@/lib/history-keys';`. Replace `handleKeyDown` (lines 227-233) with:

```ts
    const handleKeyDown = (e: KeyboardEvent) => {
      const action = matchShortcut(e, DEFAULT_SHORTCUTS);
      if (!action) return;
      // The history keys decide for themselves whether to stop the browser.
      if (isHistoryAction(action)) {
        runHistoryKey(action, e);
        return;
      }
      e.preventDefault();
      handleShortcutAction(action);
    };
```

**Step 5: Write the bar**

Create `web/src/components/TabNavBar.tsx`:

```tsx
import { Component, createMemo } from 'solid-js';
import { ArrowLeft, ArrowRight } from '@/lib/icons';
import { tabHost } from '@/lib/tab-host';
import { hit } from '@/lib/touch';
import { shortcutLabel } from '@/lib/keyboard-shortcuts';
import { canGoBack, canGoForward } from '@/windowing/model/nav-tree';

const navButton =
  'inline-flex items-center justify-center h-6 w-6 rounded text-muted-dark hover:text-shell-ink hover:bg-hover-wash disabled:opacity-40 disabled:hover:bg-transparent focus-ring';

/** A button title with its keys, when a binding carries the action. */
const titled = (name: string, action: string) => {
  const keys = shortcutLabel(action);
  return keys ? `${name} (${keys})` : name;
};

/**
 * The thin bar above a note: back, forward and the path.
 *
 * The path is plain text. A later web tab changes it to an editable URL
 * field. A button with no target is disabled.
 */
export const TabNavBar: Component<{ tabId: string; path: string }> = (props) => {
  const history = createMemo(() => tabHost().list().find((t) => t.id === props.tabId)?.history);
  const canBack = () => {
    const tree = history();
    return !!tree && canGoBack(tree);
  };
  const canForward = () => {
    const tree = history();
    return !!tree && canGoForward(tree);
  };

  return (
    <div
      data-testid="tab-nav-bar"
      class="flex items-center gap-1 h-7 shrink-0 px-2 border-b border-hairline text-xs text-muted-dark"
    >
      <button
        type="button"
        data-testid="tab-nav-back"
        aria-label="Back"
        title={titled('Back', 'historyBack')}
        class={`${navButton} ${hit()}`}
        disabled={!canBack()}
        onClick={() => void tabHost().back(props.tabId)}
      >
        <ArrowLeft class="w-3.5 h-3.5" />
      </button>
      <button
        type="button"
        data-testid="tab-nav-forward"
        aria-label="Forward"
        title={titled('Forward', 'historyForward')}
        class={`${navButton} ${hit()}`}
        disabled={!canForward()}
        onClick={() => void tabHost().forward(props.tabId)}
      >
        <ArrowRight class="w-3.5 h-3.5" />
      </button>
      <span data-testid="tab-nav-path" class="min-w-0 truncate" title={props.path}>
        {props.path}
      </span>
    </div>
  );
};
```

**Step 6: Put the bar and the missing state in the file viewer**

In `web/src/components/FileViewerPanel.tsx`:

- Add the import `import { TabNavBar } from './TabNavBar';`.
- After `navTab` (Task 9), add:

```tsx
  /** The path that the bar shows: inside the kiln when a kiln owns the file. */
  const displayPath = () => {
    const path = props.filePath ?? '';
    const kiln = owningKiln(path);
    return kiln && path.startsWith(`${kiln}/`) ? path.slice(kiln.length + 1) : path;
  };

  /** The bar, for a tab. A hover popover has no bar. */
  const NavBarSlot = () => (
    <Show when={navTab()}>{(tabId) => <TabNavBar tabId={tabId()} path={displayPath()} />}</Show>
  );
```

- In the load effect (line 311), change `if (path && !isImage())` to `if (path && !isImage() && !props.missing)`.
- After the `if (!props.filePath)` early return (line 367), add:

```tsx
  // The node points at a file that went to the trash. Nothing is read; the
  // bar stays, so the user can move on.
  if (props.missing) {
    return (
      <PanelShell class="overflow-hidden">
        <NavBarSlot />
        <div
          data-testid="file-missing"
          class="flex-1 flex items-center justify-center text-sm text-muted-dark"
        >
          This file no longer exists.
        </div>
      </PanelShell>
    );
  }
```

- In the image branch (line 377), add `<NavBarSlot />` as the first child of `PanelShell`.
- In the main return (line 395), add `<NavBarSlot />` as the first child of `PanelShell`, before the loading overlay.

The early returns do not react to a prop change. Task 8 mounts the panel again on each history move, so each node gets a fresh body.

**Step 7: Put the bar in the canvas viewer**

In `web/src/components/canvas/CanvasPanel.tsx`:

- Add `tabId?: string;` to `CanvasPanelProps` (line 108), with the doc comment `/** The tab that shows this canvas. The pane gives it. */`.
- Import `TabNavBar` from `@/components/TabNavBar`.
- Before the header `div` at line 1043, add:

```tsx
      <Show when={props.tabId}>
        {(tabId) => <TabNavBar tabId={tabId()} path={props.filePath ?? ''} />}
      </Show>
```

**Step 8: Run the tests and see them pass**

```bash
just web-test unit src/components/ src/lib/ src/windowing/
```

Expected: PASS.

**Step 9: Break the gates, then restore them**

1. In `runHistoryKey`, delete the `defaultPrevented` line. Run `history-keys.test.ts`. Expected: `leaves a key that the editor took` fails. Restore the line.
2. In `TabNavBar`, change `disabled={!canBack()}` to `disabled={false}`. Run `TabNavBar.test.tsx`. Expected: `disables both buttons…` fails. Restore the code.
3. In `FileViewerPanel`, delete `&& !props.missing` from the load effect. Run `FileViewerPanel.test.tsx`. Expected: `shows the missing state… and reads nothing` fails. Restore the code.

**Step 10: Look at the bar in the browser**

To see the layout, start the app:

```bash
just web
```

Open a note from the file tree. Open a second note. Check that the bar sits above the editor, that the back button is live, and that the path does not push the buttons off the row in a narrow pane. Stop the server with Ctrl+C.

**Step 11: Commit**

```bash
git -C crates/crucible-web/web add src/components/TabNavBar.tsx \
  src/lib/history-keys.ts \
  src/windowing/shortcuts.ts \
  src/components/windowing/WindowManager.tsx \
  src/components/FileViewerPanel.tsx \
  src/components/canvas/CanvasPanel.tsx \
  src/components/__tests__/TabNavBar.test.tsx \
  src/lib/__tests__/history-keys.test.ts \
  src/lib/__tests__/keyboard-shortcuts.test.ts \
  src/components/__tests__/FileViewerPanel.test.tsx
git commit -m "feat(web): add back and forward to the note and canvas viewers

A thin bar shows back, forward and the path. Alt+Left and Alt+Right
move the active tab, unless the editor took the key. A node that points
at a deleted file shows a missing state and reads nothing.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 11: The history popover

**Files:**
- Create: `web/src/components/NavTreeMenu.tsx`
- Modify: `web/src/components/TabNavBar.tsx` (wrap the two buttons)
- Test: `web/src/components/__tests__/NavTreeMenu.test.tsx` (create)
- Test: `web/src/components/__tests__/TabNavBar.test.tsx` (append)

Ark `Menu.ContextTrigger` opens on `contextmenu` and on a long press of a touch or pen pointer (700 ms in `@zag-js/menu`). `TabContextMenu` in `components/windowing/TabBar.tsx:425-470` uses the same trigger. Ark `Menu` gives the arrow keys, Enter and Escape.

A long press ends with a `click` on the button. Without a guard, that click moves the tab while the popover opens. The bar keeps the pointer type of the last `pointerdown`, and it skips the next click after a popover that a touch opened.

A disabled button may get no pointer event in some browsers. A tree with two or more nodes always has one enabled button: a current node that is not the root can go back, and a root with a child can go forward. So the popover is always reachable when it has something to show.

**Step 1: Write the failing tests**

Create `web/src/components/__tests__/NavTreeMenu.test.tsx`:

```tsx
import { describe, it, expect, vi, afterEach } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@solidjs/testing-library';
import { NavTreeMenu } from '@/components/NavTreeMenu';
import { backTree, markNodes, navigateTree, seedTree } from '@/windowing/model/nav-tree';
import type { NavTree } from '@/windowing/model/types';

afterEach(cleanup);

/** a → b → c, back to b, then d. The user left c, and c went to the trash. */
function tree(): NavTree {
  let t = seedTree({ title: 'a.md', contentType: 'file' }, 1, 'a');
  t = navigateTree(t, { title: 'b.md', contentType: 'file' }, 2, 'b');
  t = navigateTree(t, { title: 'c.md', contentType: 'file' }, 3, 'c');
  t = backTree(t, 4)!;
  t = navigateTree(t, { title: 'd.md', contentType: 'file' }, 5, 'd');
  return markNodes(t, ['c'], { missing: true });
}

function open(onGo = vi.fn()) {
  render(() => (
    <NavTreeMenu tree={tree()} onGo={onGo}>
      <button data-testid="inside">back</button>
    </NavTreeMenu>
  ));
  fireEvent.contextMenu(screen.getByTestId('inside'));
  return onGo;
}

describe('NavTreeMenu', () => {
  it('opens on a right-click with one row for each node, in graph order', async () => {
    open();
    const menu = await screen.findByTestId('tab-nav-history');
    const rows = [...menu.querySelectorAll('[data-nav-node]')].map((r) => r.getAttribute('data-nav-node'));
    expect(rows).toEqual(['a', 'b', 'c', 'd']);
  });

  it('marks the current node and dims a missing node', async () => {
    open();
    const menu = await screen.findByTestId('tab-nav-history');
    expect(menu.querySelector('[data-current]')?.getAttribute('data-nav-node')).toBe('d');
    const gone = menu.querySelector('[data-nav-node="c"]')!;
    expect(gone.hasAttribute('data-missing')).toBe(true);
    expect(gone.className).toContain('opacity-50');
  });

  it('draws one lane for the path and one for the branch', async () => {
    open();
    const menu = await screen.findByTestId('tab-nav-history');
    const svg = menu.querySelector('[data-nav-node="c"] svg')!;
    expect(svg.getAttribute('width')).toBe('24');
    expect(svg.querySelectorAll('circle')).toHaveLength(1);
  });

  it('goes to a node on a click', async () => {
    const onGo = open();
    fireEvent.click(await screen.findByText('b.md'));
    await waitFor(() => expect(onGo).toHaveBeenCalledWith('b'));
  });

  it('moves through the nodes with the arrow keys, and goes on Enter', async () => {
    const onGo = open();
    const menu = await screen.findByTestId('tab-nav-history');
    fireEvent.keyDown(menu, { key: 'ArrowDown' });
    await waitFor(() => expect(menu.querySelector('[data-highlighted]')).not.toBeNull());
    const first = menu.querySelector('[data-highlighted]')!.getAttribute('data-nav-node');
    fireEvent.keyDown(menu, { key: 'ArrowDown' });
    await waitFor(() =>
      expect(menu.querySelector('[data-highlighted]')!.getAttribute('data-nav-node')).not.toBe(first),
    );
    const second = menu.querySelector('[data-highlighted]')!.getAttribute('data-nav-node');
    fireEvent.keyDown(menu, { key: 'Enter' });
    await waitFor(() => expect(onGo).toHaveBeenCalledWith(second));
  });
});
```

Append to `web/src/components/__tests__/TabNavBar.test.tsx`:

```tsx
describe('TabNavBar — the popover', () => {
  it('opens the history on a right-click on a button, and a row moves the tab', async () => {
    render(() => <TabNavBar tabId="t1" path="notes/a.md" />);
    windowActions.navigate('g', 't1', { title: 'b.md', contentType: 'file', metadata: { filePath: '/k/b.md' } });
    fireEvent.contextMenu(back());
    fireEvent.click(await screen.findByText('a.md'));
    await waitFor(() => expect(shownPath()).toBe('/k/a.md'));
  });

  it('skips the click that ends a long press', async () => {
    render(() => <TabNavBar tabId="t1" path="notes/a.md" />);
    windowActions.navigate('g', 't1', { title: 'b.md', contentType: 'file', metadata: { filePath: '/k/b.md' } });
    fireEvent.pointerDown(back(), { pointerType: 'touch' });
    // The long press opens the menu. The test opens it the short way.
    fireEvent.contextMenu(back());
    await screen.findByTestId('tab-nav-history');
    fireEvent.click(back());
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(shownPath()).toBe('/k/b.md');
  });

  it('does not skip a click after a mouse right-click', async () => {
    render(() => <TabNavBar tabId="t1" path="notes/a.md" />);
    windowActions.navigate('g', 't1', { title: 'b.md', contentType: 'file', metadata: { filePath: '/k/b.md' } });
    fireEvent.pointerDown(back(), { pointerType: 'mouse', button: 2 });
    fireEvent.contextMenu(back());
    await screen.findByTestId('tab-nav-history');
    fireEvent.keyDown(screen.getByTestId('tab-nav-history'), { key: 'Escape' });
    fireEvent.click(back());
    await waitFor(() => expect(shownPath()).toBe('/k/a.md'));
  });
});
```

jsdom has no `PointerEvent` class in some versions. If `fireEvent.pointerDown` drops `pointerType`, add a small polyfill at the top of the test file: `class PointerEventStub extends MouseEvent { pointerType: string; constructor(type: string, init: PointerEventInit = {}) { super(type, init); this.pointerType = init.pointerType ?? 'mouse'; } }` and `globalThis.PointerEvent ??= PointerEventStub as never;`.

**Step 2: Run the tests and see them fail**

```bash
just web-test unit src/components/__tests__/NavTreeMenu.test.tsx src/components/__tests__/TabNavBar.test.tsx
```

Expected: FAIL. `NavTreeMenu` does not resolve, and the bar opens no popover.

**Step 3: Write the popover**

Create `web/src/components/NavTreeMenu.tsx`:

```tsx
import { Component, For, JSX, Show, createMemo } from 'solid-js';
import { Portal } from 'solid-js/web';
import { Menu } from '@ark-ui/solid';
import { menuContent, menuItem } from '@/components/ui/menu-style';
import { layoutNavTree, type NavRow } from '@/windowing/model/nav-layout';
import type { NavTree } from '@/windowing/model/types';

const LANE = 12;
const ROW = 24;
const DOT = 3.5;
const laneX = (lane: number) => LANE / 2 + lane * LANE;

/** The graph cell of one row: the lines through it and the dot of its node. */
const GraphCell: Component<{ row: NavRow; lanes: number }> = (props) => {
  const x = () => laneX(props.row.lane);
  const mid = ROW / 2;
  return (
    <svg width={props.lanes * LANE} height={ROW} class="shrink-0 text-muted-dark" aria-hidden="true">
      <For each={props.row.pass}>
        {(lane) => <line x1={laneX(lane)} y1={0} x2={laneX(lane)} y2={ROW} stroke="currentColor" />}
      </For>
      <Show when={props.row.up}>
        <line x1={x()} y1={0} x2={x()} y2={mid} stroke="currentColor" />
      </Show>
      <Show when={props.row.join !== null}>
        <line x1={laneX(props.row.join!)} y1={0} x2={x()} y2={mid} stroke="currentColor" />
      </Show>
      <Show when={props.row.down}>
        <line x1={x()} y1={mid} x2={x()} y2={ROW} stroke="currentColor" />
      </Show>
      <circle
        cx={x()}
        cy={mid}
        r={DOT}
        class={props.row.current ? 'fill-primary' : 'fill-current'}
      />
    </svg>
  );
};

/**
 * The history of a tab as a git-style graph, in a context menu.
 *
 * A right-click or a long press on the children opens it. Each node is one
 * row. Lane 0 is the path to the current node, and each branch that the user
 * left has its own lane. A node whose file went to the trash is dim.
 */
export const NavTreeMenu: Component<{
  tree: NavTree | undefined;
  onGo: (nodeId: string) => void;
  onOpenChange?: (open: boolean) => void;
  children: JSX.Element;
}> = (props) => {
  const rows = createMemo(() => (props.tree ? layoutNavTree(props.tree) : []));
  const lanes = createMemo(() => Math.max(1, ...rows().map((r) => r.lane + 1)));
  const nodeOf = (id: string) => props.tree?.nodes[id];
  const isMissing = (id: string) => nodeOf(id)?.metadata?.missing === true;

  return (
    <Menu.Root onSelect={(d) => props.onGo(d.value)} onOpenChange={(d) => props.onOpenChange?.(d.open)}>
      {/* asChild div: the default trigger is a BUTTON, and the children are buttons. */}
      <Menu.ContextTrigger
        asChild={(triggerProps) => (
          <div {...triggerProps({ class: 'flex items-center gap-1 shrink-0' })}>{props.children}</div>
        )}
      />
      {/* Portaled, as in TabContextMenu: an in-flow positioner adds layout to the bar. */}
      <Portal>
        <Menu.Positioner>
          <Menu.Content
            data-testid="tab-nav-history"
            class={`${menuContent} z-50 max-h-[60vh] max-w-[min(360px,90vw)] overflow-y-auto`}
          >
            <For each={rows()}>
              {(row) => (
                <Menu.Item
                  value={row.id}
                  data-nav-node={row.id}
                  data-current={row.current || undefined}
                  data-missing={isMissing(row.id) || undefined}
                  aria-current={row.current ? 'page' : undefined}
                  class={[
                    menuItem,
                    'py-0 gap-2',
                    row.current ? 'font-medium text-shell-ink' : '',
                    isMissing(row.id) ? 'opacity-50' : '',
                  ]
                    .filter(Boolean)
                    .join(' ')}
                >
                  <GraphCell row={row} lanes={lanes()} />
                  <span class="truncate">{nodeOf(row.id)?.title}</span>
                </Menu.Item>
              )}
            </For>
          </Menu.Content>
        </Menu.Positioner>
      </Portal>
    </Menu.Root>
  );
};
```

The web has no `cn` helper (`web/src/lib/cn.ts` does not exist, although `boundary.test.ts` allows the name). The class list is joined by hand.

**Step 4: Wrap the buttons in the bar**

In `web/src/components/TabNavBar.tsx`:

- Import `NavTreeMenu` from `./NavTreeMenu`.
- Add in the component body, before `return`:

```tsx
  // A long press ends with a click on the button. That click must not move
  // the tab. A mouse right-click sends no click, so only a touch or a pen
  // press arms the skip.
  let pointer = 'mouse';
  let skipClick = false;
  const onPointerDown = (e: PointerEvent) => {
    pointer = e.pointerType || 'mouse';
    skipClick = false;
  };
  const onMenuOpen = (open: boolean) => {
    if (open && pointer !== 'mouse') skipClick = true;
  };
  const press = (move: () => void) => () => {
    if (skipClick) {
      skipClick = false;
      return;
    }
    move();
  };
```

- Wrap the two buttons in `NavTreeMenu`:

```tsx
      <NavTreeMenu
        tree={history()}
        onGo={(nodeId) => void tabHost().goTo(props.tabId, nodeId)}
        onOpenChange={onMenuOpen}
      >
        {/* the back button, as before */}
        {/* the forward button, as before */}
      </NavTreeMenu>
```

- On each button, add `onPointerDown={onPointerDown}`. Change `onClick={() => void tabHost().back(props.tabId)}` to `onClick={press(() => void tabHost().back(props.tabId))}`, and do the same for forward.

**Step 5: Run the tests and see them pass**

```bash
just web-test unit src/components/
```

Expected: PASS.

**Step 6: Break the gates, then restore them**

1. In `press`, delete the `if (skipClick)` block. Run the bar test. Expected: `skips the click that ends a long press` fails. Restore the block.
2. In `onMenuOpen`, delete `&& pointer !== 'mouse'`. Run the bar test. Expected: `does not skip a click after a mouse right-click` fails. Restore the code.
3. In `NavTreeMenu`, delete the `isMissing(row.id) && 'opacity-50'` entry. Run the menu test. Expected: `marks the current node and dims a missing node` fails. Restore the entry.

**Step 7: Look at the popover in the browser**

Run `just web`. Open three notes in one tab. Go back once. Open a fourth note. Right-click the back button. Check the graph: a line in lane 0, one branch in lane 1, the current row in bold, the ember dot on the current node. Check the light theme and the dark theme. Check that a long title is cut with an ellipsis and does not widen the menu past the screen. Stop the server.

**Step 8: Commit**

```bash
git -C crates/crucible-web/web add src/components/NavTreeMenu.tsx \
  src/components/TabNavBar.tsx \
  src/components/__tests__/NavTreeMenu.test.tsx \
  src/components/__tests__/TabNavBar.test.tsx
git commit -m "feat(web): show the tab history as a graph on right-click or long press

The popover draws one row for each node, with lane 0 for the path to
the current node and a lane for each branch that the user left. A row
click goes to that node. A dim row points at a deleted file.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 12: The phone back button and the deep link

**Files:**
- Create: `web/src/components/mobile/tab-history-back.ts`
- Modify: `web/src/components/mobile/MobileShell.tsx:20-21` (imports), after `:88` (bind the back stack)
- Test: `web/src/components/mobile/__tests__/tab-history-back.test.ts` (create)

Each `navigate` on the phone adds one NavStack layer. The `onBack` of the layer moves the tab back through the gated host. If the user keeps unsaved changes, the layer comes back, because the browser already spent its entry.

A back from the bar or the popover releases the newest layer of that tab, so that one press is never spent twice. The browser forward gesture does nothing.

`navigate` writes `#note=<path>` with `history.replaceState` on the new entry. A back to an older entry shows the older hash, because each entry keeps its own URL. The app has no reader of `#note=` yet (decision log row of 2026-08-13 fixed only the format). This task writes the hash and reads nothing. See "Open questions".

**Step 1: Write the failing test**

Create `web/src/components/mobile/__tests__/tab-history-back.test.ts`:

```ts
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { createNavStack } from '@/components/mobile/NavStack';
import { bindTabHistoryBack, noteHash, type HistoryMoves } from '@/components/mobile/tab-history-back';
import type { HistoryMoveKind } from '@/stores/tabStackStore';

/** A window with a small history: entries, a cursor and each entry's URL. */
function fakeWindow() {
  const target = new EventTarget();
  const entries: { state: unknown; url: string }[] = [{ state: null, url: '/' }];
  let cursor = 0;
  const history = {
    get state() {
      return entries[cursor]!.state;
    },
    pushState: vi.fn((state: unknown) => {
      entries.splice(cursor + 1);
      entries.push({ state, url: entries[cursor]!.url });
      cursor += 1;
    }),
    replaceState: vi.fn((state: unknown, _title: string, url: string) => {
      entries[cursor] = { state, url };
    }),
    back: vi.fn(() => {
      cursor = Math.max(0, cursor - 1);
    }),
    go: vi.fn((delta: number) => {
      cursor = Math.max(0, cursor + delta);
    }),
  };
  const firePop = () =>
    target.dispatchEvent(new PopStateEvent('popstate', { state: entries[cursor]!.state as object }));
  return {
    win: Object.assign(target, { history }) as unknown as Window,
    history,
    url: () => entries[cursor]!.url,
    pressBack: () => {
      cursor = Math.max(0, cursor - 1);
      firePop();
    },
  };
}

/** The tab stack, in the hand. */
function fakeMoves() {
  const listeners = new Set<(tabId: string, kind: HistoryMoveKind) => void>();
  const paths: Record<string, string> = { t1: '/k/b.md' };
  const moves: HistoryMoves & { emit: (id: string, kind: HistoryMoveKind) => void } = {
    onHistoryMove: (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    historyBack: vi.fn(async (id: string) => {
      moves.emit(id, 'back');
      return true;
    }),
    pathOf: (id) => paths[id] ?? null,
    emit: (id, kind) => listeners.forEach((l) => l(id, kind)),
  };
  return moves;
}

let fake: ReturnType<typeof fakeWindow>;
let moves: ReturnType<typeof fakeMoves>;
const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

beforeEach(() => {
  fake = fakeWindow();
  moves = fakeMoves();
});

describe('the phone back button and the tab history', () => {
  it('adds a layer for each navigate, and the back button moves the tab back', async () => {
    const nav = createNavStack(fake.win);
    bindTabHistoryBack(nav, moves, fake.win);
    moves.emit('t1', 'navigate');
    expect(fake.history.pushState).toHaveBeenCalledTimes(1);
    fake.pressBack();
    await settle();
    expect(moves.historyBack).toHaveBeenCalledWith('t1');
  });

  it('closes a drawer before it moves back in the tab history', async () => {
    const nav = createNavStack(fake.win);
    bindTabHistoryBack(nav, moves, fake.win);
    moves.emit('t1', 'navigate');
    const closeDrawer = vi.fn();
    nav.push(closeDrawer);

    fake.pressBack();
    await settle();
    expect(closeDrawer).toHaveBeenCalledTimes(1);
    expect(moves.historyBack).not.toHaveBeenCalled();

    fake.pressBack();
    await settle();
    expect(moves.historyBack).toHaveBeenCalledWith('t1');
  });

  it('leaves the press to the browser at the root of the tree', async () => {
    const nav = createNavStack(fake.win);
    bindTabHistoryBack(nav, moves, fake.win);
    fake.pressBack();
    await settle();
    expect(moves.historyBack).not.toHaveBeenCalled();
  });

  it('gives the layer back when the user keeps unsaved changes', async () => {
    const nav = createNavStack(fake.win);
    bindTabHistoryBack(nav, moves, fake.win);
    moves.emit('t1', 'navigate');
    vi.mocked(moves.historyBack).mockResolvedValueOnce(false);
    fake.pressBack();
    await settle();
    expect(fake.history.pushState).toHaveBeenCalledTimes(2);
    fake.pressBack();
    await settle();
    expect(moves.historyBack).toHaveBeenCalledTimes(2);
  });

  it('releases the layer when the bar moves the tab back', () => {
    const nav = createNavStack(fake.win);
    bindTabHistoryBack(nav, moves, fake.win);
    moves.emit('t1', 'navigate');
    moves.emit('t1', 'back');
    expect(fake.history.back).toHaveBeenCalledTimes(1);
  });

  it('writes the note of the new node to the hash, and a back shows the old hash', () => {
    const nav = createNavStack(fake.win);
    bindTabHistoryBack(nav, moves, fake.win);
    moves.emit('t1', 'navigate');
    expect(fake.url()).toBe(noteHash('/k/b.md'));
    expect(fake.history.replaceState.mock.calls[0]![0]).toEqual(fake.history.state);
    fake.pressBack();
    expect(fake.url()).toBe('/');
  });

  it('stops listening when it is released', () => {
    const nav = createNavStack(fake.win);
    const stop = bindTabHistoryBack(nav, moves, fake.win);
    stop();
    moves.emit('t1', 'navigate');
    expect(fake.history.pushState).not.toHaveBeenCalled();
  });
});
```

**Step 2: Run the test and see it fail**

```bash
just web-test unit src/components/mobile/__tests__/tab-history-back.test.ts
```

Expected: FAIL with `Failed to resolve import "@/components/mobile/tab-history-back"`.

**Step 3: Write the binder**

Create `web/src/components/mobile/tab-history-back.ts`:

```ts
import type { NavStack } from './NavStack';
import type { HistoryMoveKind } from '@/stores/tabStackStore';

/** The part of the tab stack that the back button needs. */
export interface HistoryMoves {
  onHistoryMove(listener: (tabId: string, kind: HistoryMoveKind) => void): () => void;
  /** Move the tab back through the unsaved-changes gate. False when it did not move. */
  historyBack(tabId: string): Promise<boolean>;
  /** The file that the tab shows now, or null. */
  pathOf(tabId: string): string | null;
}

/** The deep link of a note. Decision log, 2026-08-13: deep links use the hash. */
export function noteHash(path: string): string {
  return `#note=${encodeURIComponent(path)}`;
}

/**
 * Give each navigation on the phone one layer of the back stack.
 *
 * The hardware back button then walks the tab history, after the drawers and
 * the sheets above it. A tab at the root of its tree has no layer, so the
 * browser gets the press. A back from the bar releases the newest layer of
 * that tab, so that one press is never spent twice. The browser forward
 * gesture does nothing: the bar and the popover move forward.
 */
export function bindTabHistoryBack(nav: NavStack, moves: HistoryMoves, win: Window = window): () => void {
  const layers: { tabId: string; release: () => void }[] = [];
  // The back moves that a layer started. Their own event must not release a layer.
  const fromLayer = new Set<string>();

  const addLayer = (tabId: string) => {
    const layer = {
      tabId,
      release: nav.push(() => {
        layers.splice(layers.indexOf(layer), 1);
        fromLayer.add(tabId);
        void moves.historyBack(tabId).then((moved) => {
          fromLayer.delete(tabId);
          // The browser spent its entry, but the tab stayed. The layer comes back.
          if (!moved) addLayer(tabId);
        });
      }),
    };
    layers.push(layer);
  };

  return moves.onHistoryMove((tabId, kind) => {
    if (kind === 'navigate') {
      addLayer(tabId);
      const path = moves.pathOf(tabId);
      // The new entry carries the note. Keep its state: NavStack reads the id there.
      if (path) win.history.replaceState(win.history.state, '', noteHash(path));
      return;
    }
    if (kind === 'back' && !fromLayer.has(tabId)) {
      const at = layers.map((l) => l.tabId).lastIndexOf(tabId);
      if (at !== -1) layers.splice(at, 1)[0]!.release();
    }
  });
}
```

`replaceState` with a hash-only URL keeps the path. The PWA rule of `NavStack.ts:14-17` (never strip the hash) still holds.

**Step 4: Bind it in the shell**

In `web/src/components/mobile/MobileShell.tsx`:

- Add the imports:

```ts
import { bindTabHistoryBack } from '@/components/mobile/tab-history-back';
import { tabHost } from '@/lib/tab-host';
```

- After the tab-switch effect (after line 88), add:

```ts
  // Each navigation inside a tab takes a history entry too. The back button
  // walks those after the drawers, and it asks before it drops unsaved edits.
  onMount(() => {
    const stop = bindTabHistoryBack(navStack(), {
      onHistoryMove: tabStackActions.onHistoryMove,
      historyBack: (tabId) => tabHost().back(tabId),
      pathOf: (tabId) => {
        const path = tabStack.tabs.find((t) => t.id === tabId)?.metadata?.filePath;
        return typeof path === 'string' ? path : null;
      },
    });
    onCleanup(stop);
  });
```

The existing effect keys on the active tab id only. A history move keeps the id, so that effect adds no second layer.

**Step 5: Run the tests and see them pass**

```bash
just web-test unit src/components/mobile/
```

Expected: PASS. `MobileShell.test.tsx` and `NavStack.test.ts` still pass.

**Step 6: Break the gates, then restore them**

1. In `bindTabHistoryBack`, delete `if (!moved) addLayer(tabId);`. Run the test. Expected: `gives the layer back when the user keeps unsaved changes` fails. Restore the line.
2. Delete `&& !fromLayer.has(tabId)`. Run the test. Expected: `adds a layer for each navigate…` or `closes a drawer…` fails, because the move of the layer releases a second layer. If both still pass, add a second `navigate` before the press in `adds a layer…` and assert that `history.back` was not called. Restore the code.
3. Change `win.history.state` in the `replaceState` call to `null`. Run the test. Expected: the hash case fails. Restore the code.

**Step 7: Commit**

```bash
git -C crates/crucible-web/web add src/components/mobile/tab-history-back.ts \
  src/components/mobile/MobileShell.tsx \
  src/components/mobile/__tests__/tab-history-back.test.ts
git commit -m "feat(web): walk the tab history with the phone back button

Each navigation on the phone adds a back-stack layer after the drawers.
A kept edit gives the layer back. A navigation writes the note to the
#note= hash of its own history entry.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 13: Playwright tests of the popover and the buttons

**Files:**
- Create: `web/e2e/tab-history.spec.ts` (the `ui` project; `playwright.config.ts:127-128` takes every spec outside `stories/` and `live/`)
- Modify: `web/e2e/stories/wikilink-navigation.story.spec.ts:73-166` (the new click rule of Task 9)

The `ui` tier mocks every route. These tests prove what the bar and the popover do in a real browser. They do not prove anything about the daemon, and no leg of this feature reaches the daemon.

**Step 1: Write the spec**

Create `web/e2e/tab-history.spec.ts`:

```ts
import { test, expect, type Page } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';
import { appReady } from './helpers/nav';

/**
 * Tab history in the shipped app: the buttons, the right-click popover and
 * the long-press popover. Files open through the product event, so the real
 * FileViewerPanel renders under the real EditorProvider.
 */

const KILN = '/home/user/notes';
const A = `${KILN}/a.md`;
const B = `${KILN}/b.md`;

async function openFile(page: Page, path: string): Promise<void> {
  await page.evaluate(
    (p) => window.dispatchEvent(new CustomEvent('crucible:open-file', { detail: { path: p } })),
    path,
  );
}

async function setup(page: Page): Promise<void> {
  await setupBasicMocks(page, { sessions: [] });
  await page.route('**/api/kiln/file**', (route) => {
    const path = new URL(route.request().url()).searchParams.get('path') ?? '';
    return route.fulfill({ json: { content: `# ${path.split('/').pop()}\n`, content_hash: 'h' } });
  });
  await page.addInitScript(() => {
    localStorage.setItem('crucible:settings', JSON.stringify({ editor: { vimMode: false } }));
  });
  await page.goto('/');
  await appReady(page);
}

const path = (page: Page) => page.getByTestId('tab-nav-path');

test.describe('tab history', () => {
  test('the buttons are disabled when they have no target', async ({ page }) => {
    await setup(page);
    await openFile(page, A);
    const back = page.getByTestId('tab-nav-back');
    const forward = page.getByTestId('tab-nav-forward');
    await expect(back).toBeDisabled();
    await expect(forward).toBeDisabled();

    await openFile(page, B);
    await expect(page.locator('[data-tab-id^="tab-file-"]')).toHaveCount(1);
    await expect(path(page)).toContainText('b.md');
    await expect(back).toBeEnabled();
    await expect(forward).toBeDisabled();

    await back.click();
    await expect(path(page)).toContainText('a.md');
    await expect(back).toBeDisabled();
    await expect(forward).toBeEnabled();
  });

  test('a right-click on a button opens the popover, and a row moves the tab', async ({ page }) => {
    await setup(page);
    await openFile(page, A);
    await openFile(page, B);
    await expect(path(page)).toContainText('b.md');

    await page.getByTestId('tab-nav-back').click({ button: 'right' });
    const menu = page.getByTestId('tab-nav-history');
    await expect(menu).toBeVisible();
    await expect(menu.getByRole('menuitem')).toHaveCount(2);
    await expect(menu.locator('[data-current]')).toContainText('b.md');

    await menu.getByRole('menuitem', { name: 'a.md' }).click();
    await expect(path(page)).toContainText('a.md');
    await expect(menu).toBeHidden();
  });

  test('the arrow keys and Enter move through the popover', async ({ page }) => {
    await setup(page);
    await openFile(page, A);
    await openFile(page, B);
    await expect(path(page)).toContainText('b.md');

    await page.getByTestId('tab-nav-back').click({ button: 'right' });
    await expect(page.getByTestId('tab-nav-history')).toBeVisible();
    await page.keyboard.press('ArrowDown');
    await page.keyboard.press('Enter');
    await expect(path(page)).toContainText('a.md');
  });

  test('a pinned tab keeps its note: an open goes to a new tab, and unpinning ends that', async ({ page }) => {
    await setup(page);
    await openFile(page, A);
    const tabs = page.locator('[data-tab-id^="tab-file-"]');
    await expect(tabs).toHaveCount(1);

    await tabs.first().click({ button: 'right' });
    await page.getByRole('menuitem', { name: 'Pin tab' }).click();
    await expect(tabs.first().getByTestId('tab-pinned')).toBeVisible();

    await openFile(page, B);
    await expect(tabs).toHaveCount(2);
    await expect(tabs.first()).toContainText('a.md');

    // The pinned tab kept a.md. Unpinning it lets the next open move it.
    await tabs.first().click({ button: 'right' });
    await page.getByRole('menuitem', { name: 'Unpin tab' }).click();
    await expect(tabs.first().getByTestId('tab-pinned')).toHaveCount(0);
    await tabs.first().click();
    await openFile(page, `${KILN}/c.md`);
    await expect(tabs).toHaveCount(2);
    await expect(tabs.first()).toContainText('c.md');
  });

  test('a long press on a button opens the popover, and the press does not move the tab', async ({ page }) => {
    await setup(page);
    await openFile(page, A);
    await openFile(page, B);
    const back = page.getByTestId('tab-nav-back');
    await expect(back).toBeEnabled();

    const box = (await back.boundingBox())!;
    const at = {
      clientX: box.x + box.width / 2,
      clientY: box.y + box.height / 2,
      pointerType: 'touch',
      isPrimary: true,
      button: 0,
    };
    await back.dispatchEvent('pointerdown', at);
    const menu = page.getByTestId('tab-nav-history');
    // The long press takes 700 ms in @zag-js/menu.
    await expect(menu).toBeVisible({ timeout: 5000 });
    await back.dispatchEvent('pointerup', at);
    await back.dispatchEvent('click', at);
    await expect(menu.locator('[data-current]')).toContainText('b.md');
    await expect(path(page)).toContainText('b.md');
  });
});
```

If the ArrowDown step highlights the current row first, the Enter step keeps `b.md`. Then press ArrowDown until the highlighted row reads `a.md`: read `menu.locator('[data-highlighted]')` and assert its text before Enter.

**Step 1b: Change the editor story for the new click rule**

`web/e2e/stories/wikilink-navigation.story.spec.ts` drives the editor harness (`EditorPanel`, which keeps its own tab list and ignores `newTab`). Two of its steps use the old rule, in which a plain click only moved the cursor.

- In `decorates, previews on hover, and Ctrl+Click follows` (lines 73-145), step 1 clicks the link to show its raw source (line 90). A plain click now follows the link. Put the cursor in the link with the keyboard instead:

```ts
    // A plain click follows a link now (WS-325), so the keyboard places the
    // cursor inside the link to show its raw source.
    await page.locator('.cm-content').click({ position: { x: 2, y: 2 } });
    await page.keyboard.press('Home');
    const lineText = (await page.locator('.cm-line').first().textContent()) ?? '';
    const into = lineText.indexOf('Other Note') + 1;
    for (let i = 0; i < into; i += 1) await page.keyboard.press('ArrowRight');
    await expect(page.locator('.cm-content')).toContainText('[[Other Note]]');
```

  Read the fixture `NOTE_A` first. If the link is not on the first line, press `ArrowDown` to its line before `Home`. The click at `(2, 2)` must land on text that is not a link. Check this in the fixture.

- In the same test, change the comment of step 3 (line 136) to "Ctrl/Cmd+Click follows the link into a new tab. The harness has one tab list, so the target opens there." The assertions stay.
- In `Mod-Enter follows the link under the cursor` (lines 147-166), replace the plain click (lines 155-157) with the keyboard placement above, and keep `await expect(otherTab).toHaveCount(0);`. Change the comment to "The keyboard places the cursor in the link, which opens nothing."
- Append two tests to the `describe`:

```ts
  test('a plain click on a link follows it', async ({ page }) => {
    await setupNoteResolution(page);
    const harness = await setupEditorHarness(page, [NOTE_A, OTHER]);
    await harness.open(NOTE_A);
    await expect(page.locator('.cm-editor')).toBeVisible({ timeout: 5000 });

    await page.locator('.cm-wikilink').click();
    await expect(
      page.getByTestId('editor-tab').filter({ hasText: 'Other Note.md' }),
    ).toBeVisible({ timeout: 5000 });
    await expect(page.getByText('●')).toHaveCount(0);
  });

  test('a drag that starts on a link selects text and opens nothing', async ({ page }) => {
    await setupNoteResolution(page);
    const harness = await setupEditorHarness(page, [NOTE_A, OTHER]);
    await harness.open(NOTE_A);
    await expect(page.locator('.cm-editor')).toBeVisible({ timeout: 5000 });

    const box = (await page.locator('.cm-wikilink').boundingBox())!;
    await page.mouse.move(box.x + 4, box.y + box.height / 2);
    await page.mouse.down();
    await page.mouse.move(box.x + box.width + 30, box.y + box.height / 2, { steps: 5 });
    await page.mouse.up();

    const selected = await page.evaluate(() => {
      const view = (document.querySelector('.cm-content') as HTMLElement & {
        cmView?: { view: { state: { selection: { main: { empty: boolean } } } } };
      }).cmView?.view;
      return view ? !view.state.selection.main.empty : null;
    });
    expect(selected).toBe(true);
    await expect(page.getByTestId('editor-tab').filter({ hasText: 'Other Note.md' })).toHaveCount(0);
  });
```

  `cmView` is the CodeMirror back-reference on the content element. If it is absent in this build, assert that `.cm-selectionBackground` is visible instead.

Run the story:

```bash
just web-test stories e2e/stories/wikilink-navigation.story.spec.ts
```

Expected: every test passes. The step screenshots change only where the cursor now sits. The baseline `editor-wikilink-decorated.png` is taken after the cursor leaves the link, so it must not change. If it changes, open both images and find the cause before you update it.

**Step 2: Run the spec and see it pass**

```bash
just web-test ui e2e/tab-history.spec.ts
```

Expected: `5 passed`. If a test fails, read the trace in the output folder that the recipe prints. Do not add a `waitForTimeout`. Wait on a condition.

**Step 3: Break a gate, then restore it**

In `TabNavBar.tsx`, remove the `NavTreeMenu` wrapper (keep the buttons). Run the spec. Expected: the two popover tests and the long-press test fail. Restore the wrapper. Run the spec again. Expected: `5 passed`. Then delete the `Pin tab` row in `TabContextMenu`, run the spec, and expect the pinned test to fail. Restore the row.

**Step 4: Run the whole ui tier**

The in-place rule changes what `openFileInEditor` does, so older specs can change.

```bash
just web-test ui
```

Expected: every spec passes. Pay attention to `e2e/file-tab.spec.ts`, `e2e/windowing-regression.spec.ts`, `e2e/session-file-integration.spec.ts` and `e2e/stories/wikilink-hover.story.spec.ts`. They select `[data-tab-id^="tab-file-"]`, and the prefix stays. If a spec opens two files and expects two tabs, change it to open the second with a new tab (`openFileInEditor(path, name, { newTab: true })`), and write the reason in a comment.

**Step 5: Run the story tier, and check each changed screenshot**

```bash
just web-test stories
```

Expected: every story passes. The bar adds a 28 px row above the file viewer, so a screenshot of the shipped file viewer can change. The editor harness stories (`editor-*.story.spec.ts`, `wikilink-navigation.story.spec.ts`) use `EditorPanel`, which has no bar, so their baselines must not change. For each baseline that changes, open the old and the new image. Check the layout, the Unicode and the colours. Update one baseline at a time with `just web-test stories <spec> --update-snapshots`, and only after that check. Never run `--update-snapshots` on the whole tier.

**Step 6: Commit**

```bash
git -C crates/crucible-web/web add e2e/tab-history.spec.ts \
  e2e/stories/wikilink-navigation.story.spec.ts
git commit -m "test(web): prove the tab history popover and buttons in a browser

A right-click and a long press open the popover, a row and the arrow
keys move the tab, a button with no target is disabled, and a pinned
tab sends an open to a new tab. The editor
story follows a link on a plain click, and a drag still selects text.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

If Step 4 or Step 5 changed another spec or a baseline, add those paths to the same commit.

---

### Task 14: The documents

**Files:**
- Modify: `docs/Meta/Web User Stories.md` (add WS-325 after WS-324, before line 503; add one sentence to WS-209, line 160)
- Modify: `docs/Meta/Product Decision Log.md` (add one row after line 111)
- Modify: `docs/Meta/Architecture/Mobile Shell.md` (section 6, after the bullet at line 285-286)
- Modify: `docs/Meta/Product.md:1192` (`v10` → `v11`, and one sentence)
- Modify: `docs/Meta/Analysis/2026-09-16 Tab History Design.md` and this plan (frontmatter `status`)

**Step 1: Add the story**

In `docs/Meta/Web User Stories.md`, after the WS-324 block and its `---` (before `## Infra requirements`), add:

```markdown
---

### WS-325: Move back and forward inside a note tab
**As a user**, a link that I click shows its note in the tab that I am in, and I go back and forward in that tab, as I do in Obsidian. When I need the path that I took, I open it as a graph.
**Acceptance:** a plain click on a link in the chat, in the reading view or in a canvas card shows the note in the tab that holds the link, or in the active editor tab when the content names no tab. Ctrl, Cmd or a middle click opens a new tab. The editor has the same gestures (WS-209): a plain click on a link follows it, so only the keyboard puts the cursor on a link, and Mod-Enter follows from there; a drag that starts on a link still selects text. The file tree, a search result and a palette note also show the note in the current tab. I pin a tab with Pin tab in its context menu, and unpin it with Unpin tab; a pinned tab shows a pin mark after its title and still closes. On a phone, each card of the tab overview has a pin toggle and shows the mark. A pinned tab never moves for an open: the open goes to a new tab, and Back and Forward still move the pinned tab. With no editor tab, a click opens a new tab. Two tabs can show one note. The file viewer and the canvas viewer have a thin bar with Back, Forward and the path; a button with no target is disabled. Alt+Left and Alt+Right move the active tab on the desktop, unless the editor takes the key. A right-click or a long press on a button opens the history as a graph: lane 0 is the path to the current node, each branch that I left has its own lane, the current row is bold, and a row whose file went to the trash is dim. A row click and Enter go to that node; the arrow keys move through the rows. Forward goes to the child that I visited last. A tab keeps 100 nodes; the oldest leaf goes first, and the current node and its ancestors stay. A move away from unsaved changes asks in an app modal, never in a browser prompt, and Keep leaves the tab and its history as they were. The history is part of the saved layout (v11); the reader repairs a bad tree to one node that holds the tab's content. Deleting a file closes the tabs that show it and marks it in the history of the others. On a phone, each navigation takes one back-stack entry after the drawers and sheets, the back button walks the history, a tab at its root leaves the press to the browser, the browser forward gesture does nothing, and a navigation writes `#note=<path>` to its entry.
**TUI parity:** none, and none is owed. The TUI has no note tabs.
**Tests:** W1 (`windowing/__tests__/nav-tree.test.ts`, `nav-layout.test.ts`, `windowStore.history.test.ts`, `serializer.test.ts` — `v10 → v11: tab history`; `stores/__tests__/layoutMigrations.property.test.ts`; `stores/__tests__/tabStackStore.test.ts` — `tab history on the phone`; `lib/__tests__/tab-history-host.test.ts`, `file-actions.test.ts`, `tab-host-routing.test.ts`, `markdown-click.test.ts`, `note-actions.test.ts`, `history-keys.test.ts`; `components/__tests__/ConfirmDiscardHost.test.tsx`, `TabNavBar.test.tsx`, `NavTreeMenu.test.tsx`, `FileViewerPanel.test.tsx` — `the nav bar and the missing state`; `components/editor/__tests__/wikilink-extension.test.ts`; `components/windowing/__tests__/Pane.test.tsx` — `history moves`; `components/mobile/__tests__/ContentSurface.test.tsx`, `tab-history-back.test.ts`; `contexts/__tests__/EditorContext.test.tsx`). W1 also `components/windowing/__tests__/TabBar.pin.test.tsx` and `components/mobile/__tests__/TabOverview.test.tsx` — the pin controls. W2 `e2e/tab-history.spec.ts` — disabled buttons, the right-click popover, the arrow keys, the long press, and a pinned tab that sends an open to a new tab. W2 GAP: no phone-viewport journey walks the tab history with the back button; it needs the story spec of WS-317 to open two notes. W4 none: the history lives in the layout, which the daemon stores as an opaque blob.
```

In WS-209 (line 160-162), add a sentence at the end of the **Acceptance** line: "**Changed 2026-09-16 (WS-325):** a plain click on a link now follows it, and Ctrl/Cmd+click or a middle click opens a new tab. The keyboard puts the cursor on a link. A drag that starts on a link still selects text." Also change the first sentence of the story to match, and change the **Tests** line: `wikilink-extension.test.ts` now covers the plain click, the new-tab clicks, the drag and the Shift and Alt clicks.

**Step 2: Add the decision**

In `docs/Meta/Product Decision Log.md`, after line 111, add:

```markdown
| 2026-09-16 | A link shows its note in the tab that the user is in; a tab keeps a history tree in the saved layout (v11); the rule "one tab for each note" ends | A note tab that jumped to another tab on each link click lost the reading path, and Obsidian users expect the tab to move. Two tabs can now show one note, so the openers find the active editor tab first and a tab id names the tab, not the file. The tree lives in the layout as a typed core field, not in `metadata` (the core could not check it there) and not in a store keyed by tab id (every move, split and close would have to follow it). It keeps 100 nodes and never drops the current node or its ancestors. Every opener (the file tree, search, the palette) follows the same rule. The editor takes the Obsidian gestures: a plain click follows a link, and Ctrl/Cmd or a middle click opens a new tab, which replaces the Ctrl-to-follow rule of WS-209. A pinned tab, which the tab menu or the phone overview pins, never moves for an open. A move away from unsaved edits asks in an app modal, because an installed PWA can suppress `window.confirm`. The TUI has no note tabs. See [[2026-09-16 Tab History Design]] |
```

**Step 3: Update the phone design**

In `docs/Meta/Architecture/Mobile Shell.md`, section 6, after the bullet "It must eventually leave the app" (lines 287-289), add:

```markdown
- **Inside a tab, back walks the tab history first** (decision log, 2026-09-16).
  Each navigation takes one NavStack layer, after the drawers and the sheets.
  A tab at the root of its tree has no layer, so the press goes on to the tab
  walk above, and then to the browser. A kept edit gives the layer back. A
  navigation writes `#note=<path>` to its own entry with `replaceState`. See
  `components/mobile/tab-history-back.ts`.
```

**Step 4: Update the product page**

In `docs/Meta/Product.md:1192`, change `a versioned layout (v10)` to `a versioned layout (v11)`. At the end of that paragraph, add: "Each note tab keeps a history tree in that layout: a link shows its note in place, and Back, Forward and a graph popover move through the tree (WS-325)."

**Step 5: Mark the design as the plan's source**

In `docs/Meta/Analysis/2026-09-16 Tab History Design.md`, add one line after the first paragraph of the body: "The implementation plan is [[2026-09-16 Tab History Plan]]."

**Step 6: Check the docs**

```bash
just lint docs
```

Expected: PASS. The recipe runs the `dev_kiln` and `docs_config` tests (`justfile:135-137`). The docs kiln is test input, so every wikilink must resolve, and each note must keep `title`, `description` and `tags` in its frontmatter. (`just test doc` runs the Rust doctests, not this check.)

**Step 7: Commit**

```bash
git add "docs/Meta/Web User Stories.md" \
  "docs/Meta/Product Decision Log.md" \
  "docs/Meta/Architecture/Mobile Shell.md" \
  docs/Meta/Product.md \
  "docs/Meta/Analysis/2026-09-16 Tab History Design.md"
git commit -m "docs: record tab history in the stories, the decision log and the phone design

WS-325 gives the acceptance and the tests. The decision log records the
end of the rule one tab for each note. The TUI has no note tabs.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 15: The full CI

**Step 1: Typecheck and run everything**

```bash
cd crates/crucible-web/web && bun run typecheck
just ci
```

Expected: `CI checks passed!`. `just ci` runs the lints, the Rust tiers, the doc tier, the web unit tier with coverage, the `ui` tier and the `live` tier.

**Step 2: Read each failure**

Do not call a failure unrelated or old. Find its cause. A coverage failure means that a new branch has no test: add the test. Do not lower a threshold in `vite.config.ts`.

**Step 3: Fix, then commit the fix with its test**

Use one commit for each cause, with a conventional message and the `Co-Authored-By` line. Run `just ci` again until it passes.

**Step 4: Merge**

To put the branch on its base, make a fast-forward only:

```bash
git rebase master
git switch master
git merge --ff-only feat/tab-history
```

If the merge fails, the base moved. Rebase again, then merge again. Delete the branch after the merge. Do these steps only when the user asks for the merge.

---

## Where the code differs from the design

1. **"The active editor tab" for a link inside a note.** The design sends every click to the active editor tab. A link inside a note in a floating window or a second pane would then move another tab. The plan adds `data-nav-tab`: a click shows the note in the tab that holds the link, and content with no tab (chat, canvas, hover popover) uses the active editor tab.
2. **The panel does not follow a move.** `Pane.tsx:107-131`, `FloatingWindow.tsx:295-312` and `ContentSurface.tsx:33-47` mount a panel again only for a new tab id or content type, and `reactiveMetadataProps` fixes its key set at mount (`web/src/lib/panel-props.ts:36-48`). `FileViewerPanel` also decides image, empty and loaded at mount (`FileViewerPanel.tsx:360-392`). Task 8 keys the renderers on `history.current`.
3. **The cap.** The design says that the core never removes the current node or an ancestor. A tab that navigates 101 times in a line has only the current node and its ancestors. The plan then removes the root, and its child becomes the root. This happens only when no other leaf exists.
4. **`forward` asks too.** The design gates `navigate`, `back` and `goTo`. `forward` also leaves the buffer, so the plan gates it.
5. **Store action signatures.** The design writes `navigate(tabId, entry)`. The core actions follow `updateTab` and take `(groupId, tabId, …)`. The host takes only the tab id.
6. **No Ark `Dialog` exists.** `ExportDialog.tsx` and `PluginCommandDialog.tsx` draw their own overlay. The plan uses Ark `Dialog` for the first time.
7. **No `#note=` reader exists.** Only the format is fixed (decision log, 2026-08-13; `NavStack.ts:14-17`). The plan writes the hash on the phone and reads nothing (user decision).
8. **The phone store already has `back()`.** `tabStackStore.ts:146` walks the visit order of tabs. The plan names the new actions `historyBack`, `historyForward` and `historyGoTo`.
9. **The editor click rule replaces WS-209.** The old rule (`wikilink-extension.ts:141-142`) followed a link only on Ctrl+click. The user chose the Obsidian rule, so Task 9 changes the extension and Task 13 changes `wikilink-navigation.story.spec.ts`.
10. **Nothing set `isPinned`.** The field existed (`windowing/model/types.ts:9`, `stores/tabStackStore.ts:31`) with no control. Task 6a adds the control (user decision).

## Decisions from the user (2026-09-16)

1. The file tree, search results, palette notes and every other caller of `openFileInEditor` show the note in the current tab and add a history node (Task 7).
2. The editor follows Obsidian: a plain click follows in place, Ctrl/Cmd+click and a middle click open a new tab, Mod-Enter follows in place, and a drag that starts on a link still selects text (Task 9).
3. Only the phone writes `#note=`. Nothing reads it (Task 12).
4. An open from a pinned tab goes to a new tab. Back and forward still move a pinned tab (Tasks 6 and 7). Pin tab and Unpin tab go in the tab context menu, a pinned tab shows a pin mark, and the phone pins from the tab overview (Task 6a).

## Open questions for the user

None.
