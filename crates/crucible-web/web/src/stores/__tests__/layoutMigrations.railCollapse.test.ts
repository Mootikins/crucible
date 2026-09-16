import { describe, it, expect } from 'vitest';
import { serializeLayout, deserializeLayout as readLayout } from '@/windowing/model/serializer';
import type { StoredLayout } from '@/windowing/model/serializer';
import { appLayoutHooks } from '../layoutMigrations';
import type { LayoutNode, PaneNode, TabContentType } from '@/types/windowTypes';

/** A stored layout at any version, as these tests write one. */
type SerializedLayout = StoredLayout<TabContentType> & Record<string, any>;

/** The core reader with the app history and prune. */
const deserializeLayout = (json: unknown) =>
  readLayout(json as StoredLayout<TabContentType>, appLayoutHooks);

// v7 → v8 adds `PaneNode.collapsed`: a rail pane collapses to its tab strip on
// its own. The migration gives a stored layout the same terminal default a
// fresh one gets from `defaultLayout`.

const panes = (node: LayoutNode): PaneNode[] =>
  node.type === 'pane' ? [node] : [...panes(node.first), ...panes(node.second)];

const group = (id: string, contentType: string, tabId: string) => ({
  id,
  tabs: [{ id: tabId, title: tabId, contentType: contentType as never }],
  activeTabId: tabId,
});

const railSplit = (firstGroup: string, secondGroup: string): LayoutNode => ({
  id: 'right-split',
  type: 'split',
  direction: 'vertical',
  splitRatio: 0.65,
  first: { id: 'right-pane', type: 'pane', tabGroupId: firstGroup },
  second: { id: 'right-term-pane', type: 'pane', tabGroupId: secondGroup },
});

const v7 = (over: Partial<SerializedLayout> = {}): SerializedLayout => ({
  version: 7,
  layout: { id: 'centre', type: 'pane', tabGroupId: 'centre-group' },
  tabGroups: {
    'centre-group': group('centre-group', 'file', 'file-tab'),
    'left-group': group('left-group', 'sessions', 'sessions-tab'),
    'files-group': group('files-group', 'files', 'files-tab'),
    'term-group': group('term-group', 'terminal', 'terminal-tab'),
  },
  edgePanels: {
    left: {
      id: 'left-panel',
      layout: { id: 'left-pane', type: 'pane', tabGroupId: 'left-group' },
      isCollapsed: false,
      width: 280,
    },
    right: {
      id: 'right-panel',
      layout: railSplit('files-group', 'term-group'),
      isCollapsed: true,
      width: 340,
    },
  },
  floatingWindows: [],
  ...over,
});

describe('migrateV7toV8 — the terminal ships collapsed', () => {
  it('collapses a terminal-only rail pane and leaves the tree pane open', () => {
    const right = deserializeLayout(v7()).edgePanels.right;
    const [tree, terminal] = panes(right.layout);
    expect(tree.collapsed).not.toBe(true);
    expect(terminal.collapsed).toBe(true);
  });

  // Keyed on what the pane HOLDS, not on the id migrateV6toV7 minted: a user
  // who dragged the shell into the other rail still gets the new default.
  it('collapses a terminal pane in either rail', () => {
    const layout = v7();
    layout.edgePanels.left.layout = {
      id: 'left-split',
      type: 'split',
      direction: 'vertical',
      splitRatio: 0.5,
      first: { id: 'left-pane', type: 'pane', tabGroupId: 'left-group' },
      second: { id: 'left-term', type: 'pane', tabGroupId: 'term-group' },
    };
    const left = deserializeLayout(layout).edgePanels.left;
    expect(panes(left.layout).find((p) => p.id === 'left-term')?.collapsed).toBe(true);
  });

  // The invariant the action enforces at runtime holds at rest too: a rail
  // with nothing expanded is a rail that reads as broken.
  it('leaves a rail whose ONLY pane is a terminal expanded', () => {
    const layout = v7();
    layout.edgePanels.right.layout = {
      id: 'right-pane',
      type: 'pane',
      tabGroupId: 'term-group',
    };
    const right = deserializeLayout(layout).edgePanels.right;
    expect(panes(right.layout)[0].collapsed).not.toBe(true);
  });

  it('leaves a rail with no terminal untouched', () => {
    const layout = v7();
    layout.tabGroups['term-group'] = group('term-group', 'backlinks', 'backlinks-tab');
    const right = deserializeLayout(layout).edgePanels.right;
    for (const pane of panes(right.layout)) {
      expect(pane.collapsed).not.toBe(true);
    }
  });

  // A pane holding a terminal ALONGSIDE something else is a tab strip the
  // user built; collapsing it would hide their other tabs.
  it('leaves a pane that holds more than terminals expanded', () => {
    const layout = v7();
    layout.tabGroups['term-group'].tabs.push({
      id: 'notes-tab',
      title: 'Notes',
      contentType: 'file' as never,
    });
    const right = deserializeLayout(layout).edgePanels.right;
    expect(panes(right.layout)[1].collapsed).not.toBe(true);
  });

  // The rewrite is what makes the migration run ONCE per stored layout: the
  // next load reads the current version and skips it.
  it('rewrites the stored version to the current one', () => {
    const restored = deserializeLayout(v7());
    expect(serializeLayout(restored).version).toBe(10);
  });
});

describe('v8 round trip', () => {
  // Runs ONCE per stored layout: a terminal the user expanded after the
  // migration must not be re-collapsed on the next load.
  it('keeps a pane the user expanded after the migration', () => {
    const once = deserializeLayout(v7());
    const expanded = serializeLayout(once);
    const termPane = panes(expanded.edgePanels.right.layout).find(
      (p) => p.id === 'right-term-pane',
    )!;
    termPane.collapsed = false;

    const twice = deserializeLayout(expanded);
    expect(
      panes(twice.edgePanels.right.layout).find((p) => p.id === 'right-term-pane')
        ?.collapsed,
    ).toBe(false);
  });
});
