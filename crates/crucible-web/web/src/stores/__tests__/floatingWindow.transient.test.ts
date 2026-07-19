import { describe, it, expect, beforeEach } from 'vitest';
import { produce } from 'solid-js/store';
import { windowStore, windowActions, setStore } from '@/stores/windowStore';
import { createInitialState } from '@/stores/windowStoreInternals';

beforeEach(() => {
  const fresh = createInitialState();
  setStore(
    produce((s) => {
      s.layout = fresh.layout;
      s.tabGroups = fresh.tabGroups;
      s.edgePanels = fresh.edgePanels;
      s.floatingWindows = [];
      s.activePaneId = fresh.activePaneId;
      s.nextZIndex = 100;
    }),
  );
});

function spawnTransient(): string {
  const groupId = windowActions.createTabGroup();
  windowActions.addTab(groupId, {
    id: 'tab-hoverfile-/k/x.md',
    title: 'X',
    contentType: 'file',
    metadata: { filePath: '/k/x.md' },
  });
  return windowActions.createFloatingWindow(groupId, 50, 60, 460, 320, {
    transient: true,
    showTabBar: false,
    title: 'X',
  });
}

describe('transient (hover) floating windows', () => {
  it('pinFloatingWindow promotes a popover to a normal window', () => {
    const id = spawnTransient();
    expect(windowStore.floatingWindows[0].transient).toBe(true);
    windowActions.pinFloatingWindow(id);
    expect(windowStore.floatingWindows[0].transient).toBe(false);
  });

  it('exportLayout excludes transient windows AND their tab groups', () => {
    spawnTransient();
    const exported = windowActions.exportLayout();
    const s = JSON.stringify(exported);
    expect(s).not.toContain('tab-hoverfile-');
    expect(s).not.toContain('"transient":true');
    // Pinned windows persist.
    windowActions.pinFloatingWindow(windowStore.floatingWindows[0].id);
    const exported2 = JSON.stringify(windowActions.exportLayout());
    expect(exported2).toContain('tab-hoverfile-');
  });

  it('closing the last tab auto-closes the floating window (no zombie shells)', () => {
    const id = spawnTransient();
    const groupId = windowStore.floatingWindows[0].tabGroupId;
    windowActions.removeTab(groupId, 'tab-hoverfile-/k/x.md');
    expect(windowStore.floatingWindows.find((w) => w.id === id)).toBeUndefined();
    expect(windowStore.tabGroups[groupId]).toBeUndefined();
  });

  it('dragging the last tab out auto-closes the floating window', () => {
    const id = spawnTransient();
    const groupId = windowStore.floatingWindows[0].tabGroupId;
    const centerGroup = Object.keys(windowStore.tabGroups).find((g) => g !== groupId)!;
    windowActions.moveTab(groupId, centerGroup, 'tab-hoverfile-/k/x.md');
    expect(windowStore.floatingWindows.find((w) => w.id === id)).toBeUndefined();
    expect(windowStore.tabGroups[groupId]).toBeUndefined();
    expect(
      windowStore.tabGroups[centerGroup].tabs.some((t) => t.id === 'tab-hoverfile-/k/x.md'),
    ).toBe(true);
  });

  it('maximize stores restore bounds; restore reapplies them', () => {
    const id = spawnTransient();
    windowActions.maximizeFloatingWindow(id);
    const w = () => windowStore.floatingWindows[0];
    expect(w().isMaximized).toBe(true);
    expect(w().restoreBounds).toEqual({ x: 50, y: 60, width: 460, height: 320 });
    windowActions.restoreFloatingWindow(id);
    expect(w().isMaximized).toBe(false);
    expect(w().x).toBe(50);
    expect(w().y).toBe(60);
    expect(w().width).toBe(460);
    expect(w().height).toBe(320);
  });
});
