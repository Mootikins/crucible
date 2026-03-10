import { describe, it, expect } from 'vitest';
import * as fc from 'fast-check';
import { serializeLayout, deserializeLayout } from '../layout-serializer';
import type {
  WindowManagerState,
  TabGroup,
  Tab,
  EdgePanel,
  LayoutNode,
  PaneNode,
} from '@/types/windowTypes';

// Arbitraries for building random WindowManagerState

const arbTabContentType = fc.constantFrom('file', 'document', 'tool', 'terminal', 'preview', 'settings');

const arbTab = fc.record({
  id: fc.uuid(),
  title: fc.string({ minLength: 1, maxLength: 30 }),
  contentType: arbTabContentType,
  isModified: fc.option(fc.boolean(), { freq: 1 }),
  isPinned: fc.option(fc.boolean(), { freq: 1 }),
  metadata: fc.option(fc.record({}), { freq: 1 }),
});

const arbTabGroup = (groupId: string): fc.Arbitrary<TabGroup> =>
  fc.record({
    id: fc.constant(groupId),
    tabs: fc.array(arbTab, { minLength: 1, maxLength: 3 }),
    activeTabId: fc.option(fc.uuid(), { freq: 2 }),
  }).map((group) => {
    // Ensure activeTabId is either null or one of the tab IDs
    if (group.activeTabId && group.tabs.length > 0) {
      const tabIds = group.tabs.map((t) => t.id);
      return {
        ...group,
        activeTabId: fc.sample(fc.constantFrom(...tabIds), 1)[0],
      };
    }
    return { ...group, activeTabId: null };
  });

const arbEdgePanel = (tabGroupId: string): fc.Arbitrary<EdgePanel> =>
  fc.record({
    id: fc.uuid(),
    tabGroupId: fc.constant(tabGroupId),
    isCollapsed: fc.boolean(),
    width: fc.option(fc.integer({ min: 100, max: 500 }), { freq: 2 }),
    height: fc.option(fc.integer({ min: 100, max: 500 }), { freq: 2 }),
  });

const arbPaneNode = (tabGroupId: string): fc.Arbitrary<PaneNode> =>
  fc.record({
    id: fc.uuid(),
    type: fc.constant('pane' as const),
    tabGroupId: fc.constant(tabGroupId),
  });

const arbLayoutNode = (tabGroupId: string): fc.Arbitrary<LayoutNode> =>
  arbPaneNode(tabGroupId);

const arbWindowManagerState = fc
  .tuple(
    fc.array(fc.uuid(), { minLength: 1, maxLength: 4 }),
    fc.uuid(),
    fc.uuid(),
    fc.uuid(),
  )
  .chain(([centerGroupIds, leftGroupId, rightGroupId, bottomGroupId]) => {
    const allGroupIds = [...centerGroupIds, leftGroupId, rightGroupId, bottomGroupId];
    const centerGroupId = centerGroupIds[0] || leftGroupId;

    return fc
      .record({
        tabGroups: fc.constant(
          Object.fromEntries(
            allGroupIds.map((id) => [id, fc.sample(arbTabGroup(id), 1)[0]]),
          ),
        ),
        layout: fc.constant(fc.sample(arbLayoutNode(centerGroupId), 1)[0]),
        edgePanels: fc.constant({
          left: fc.sample(arbEdgePanel(leftGroupId), 1)[0],
          right: fc.sample(arbEdgePanel(rightGroupId), 1)[0],
          bottom: fc.sample(arbEdgePanel(bottomGroupId), 1)[0],
        }),
        floatingWindows: fc.array(
          fc.record({
            id: fc.uuid(),
            tabGroupId: fc.constantFrom(...allGroupIds),
            x: fc.integer({ min: 0, max: 1000 }),
            y: fc.integer({ min: 0, max: 1000 }),
            width: fc.integer({ min: 200, max: 800 }),
            height: fc.integer({ min: 200, max: 800 }),
            isMinimized: fc.boolean(),
            isMaximized: fc.boolean(),
            zIndex: fc.integer({ min: 1, max: 100 }),
            title: fc.option(fc.string({ maxLength: 50 }), { freq: 2 }),
          }),
          { maxLength: 3 },
        ),
        activePaneId: fc.option(fc.uuid(), { freq: 2 }),
        focusedRegion: fc.constantFrom('center', 'left', 'right', 'bottom'),
        dragState: fc.constant(null),
        flyoutState: fc.constant(null),
        nextZIndex: fc.integer({ min: 1, max: 1000 }),
      })
      .map((state) => state as WindowManagerState);
  });

describe('layout-serializer property tests', () => {
  it('round-trip: deserializeLayout(serializeLayout(state)) deep-equals state', () => {
    fc.assert(
      fc.property(arbWindowManagerState, (state) => {
        const serialized = serializeLayout(state);
        const deserialized = deserializeLayout(serialized);

        // Compare only the fields that deserializeLayout returns
        const stateSubset = {
          layout: state.layout,
          tabGroups: state.tabGroups,
          edgePanels: state.edgePanels,
          floatingWindows: state.floatingWindows,
        };

        expect(JSON.stringify(deserialized)).toBe(JSON.stringify(stateSubset));
      }),
      { numRuns: 100 },
    );
  });

  it('idempotency: serializing twice gives same result', () => {
    fc.assert(
      fc.property(arbWindowManagerState, (state) => {
        const serialized1 = serializeLayout(state);
        const deserialized1 = deserializeLayout(serialized1);
        const serialized2 = serializeLayout({
          layout: deserialized1.layout,
          tabGroups: deserialized1.tabGroups,
          edgePanels: deserialized1.edgePanels,
          floatingWindows: deserialized1.floatingWindows,
          activePaneId: null,
          focusedRegion: 'center',
          dragState: null,
          flyoutState: null,
          nextZIndex: 1,
        });

        // Serializing the deserialized state should give the same result
        expect(JSON.stringify(serialized1)).toBe(JSON.stringify(serialized2));
      }),
      { numRuns: 100 },
    );
  });

  it('large states (20+ tab groups) serialize without error', () => {
    fc.assert(
      fc.property(
        fc.tuple(
          fc.array(fc.uuid(), { minLength: 20, maxLength: 25 }),
          fc.uuid(),
          fc.uuid(),
          fc.uuid(),
        ),
        ([centerGroupIds, leftGroupId, rightGroupId, bottomGroupId]) => {
          const allGroupIds = [...centerGroupIds, leftGroupId, rightGroupId, bottomGroupId];
          const centerGroupId = centerGroupIds[0] || leftGroupId;

          const state: WindowManagerState = {
            tabGroups: Object.fromEntries(
              allGroupIds.map((id) => [id, fc.sample(arbTabGroup(id), 1)[0]]),
            ),
            layout: fc.sample(arbLayoutNode(centerGroupId), 1)[0],
            edgePanels: {
              left: fc.sample(arbEdgePanel(leftGroupId), 1)[0],
              right: fc.sample(arbEdgePanel(rightGroupId), 1)[0],
              bottom: fc.sample(arbEdgePanel(bottomGroupId), 1)[0],
            },
            floatingWindows: [],
            activePaneId: null,
            focusedRegion: 'center',
            dragState: null,
            flyoutState: null,
            nextZIndex: 1,
          };

          // Should not throw
          const serialized = serializeLayout(state);
          const deserialized = deserializeLayout(serialized);

          expect(deserialized.tabGroups).toBeDefined();
          expect(Object.keys(deserialized.tabGroups).length).toBeGreaterThanOrEqual(20);
        },
      ),
      { numRuns: 50 },
    );
  });
});
