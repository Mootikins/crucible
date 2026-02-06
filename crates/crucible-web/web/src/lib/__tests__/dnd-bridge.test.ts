import { describe, it, expect, beforeEach, vi } from 'vitest';
import { setupCrossZoneDnD } from '../dnd-bridge';
import type { Zone } from '../panel-registry';
import type { DockviewInstance } from '../solid-dockview';

type EventHandler = (event: any) => void;

function createMockPanel(id: string, title?: string) {
  return { id, title: title ?? id, group: { id: `group-${id}` } };
}

function createMockInstance(viewId: string, panels: ReturnType<typeof createMockPanel>[] = []) {
  const panelMap = new Map(panels.map(p => [p.id, p]));
  const unhandledDragHandlers: EventHandler[] = [];
  const didDropHandlers: EventHandler[] = [];
  const removedPanels: string[] = [];
  const addedPanels: any[] = [];

  const api = {
    id: viewId,
    get panels() { return [...panelMap.values()]; },
    get totalPanels() { return panelMap.size; },
    getPanel: (id: string) => panelMap.get(id),
    removePanel: (panel: any) => {
      panelMap.delete(panel.id);
      removedPanels.push(panel.id);
    },
    addPanel: (opts: any) => {
      const panel = createMockPanel(opts.id, opts.title);
      panelMap.set(opts.id, panel);
      addedPanels.push(opts);
      return panel;
    },
    onUnhandledDragOverEvent: (handler: EventHandler) => {
      unhandledDragHandlers.push(handler);
      return { dispose: () => { const i = unhandledDragHandlers.indexOf(handler); if (i >= 0) unhandledDragHandlers.splice(i, 1); } };
    },
    onDidDrop: (handler: EventHandler) => {
      didDropHandlers.push(handler);
      return { dispose: () => { const i = didDropHandlers.indexOf(handler); if (i >= 0) didDropHandlers.splice(i, 1); } };
    },
  };

  return {
    instance: { api, component: {} as any, dispose: vi.fn() } as unknown as DockviewInstance,
    fireUnhandledDragOver: (event: any) => unhandledDragHandlers.forEach(h => h(event)),
    fireDidDrop: (event: any) => didDropHandlers.forEach(h => h(event)),
    removedPanels,
    addedPanels,
    panelMap,
  };
}

function createDragOverEvent(sourceViewId: string, panelId: string | null = null) {
  let accepted = false;
  return {
    event: {
      nativeEvent: new Event('dragover'),
      getData: () => ({ viewId: sourceViewId, groupId: 'g1', panelId }),
      accept: () => { accepted = true; },
      get isAccepted() { return accepted; },
    },
    get accepted() { return accepted; },
  };
}

function createDropEvent(
  sourceViewId: string,
  panelId: string | null,
  group?: any,
  position: string = 'center',
) {
  return {
    nativeEvent: new Event('drop'),
    getData: () => ({ viewId: sourceViewId, groupId: 'g1', panelId }),
    group,
    position,
  };
}

describe('setupCrossZoneDnD', () => {
  let leftMock: ReturnType<typeof createMockInstance>;
  let centerMock: ReturnType<typeof createMockInstance>;
  let instances: Map<Zone, DockviewInstance>;
  let cleanup: () => void;

  beforeEach(() => {
    leftMock = createMockInstance('dv-left', [
      createMockPanel('sessions', 'Sessions'),
      createMockPanel('files', 'Files'),
    ]);
    centerMock = createMockInstance('dv-center', [
      createMockPanel('chat', 'Chat'),
    ]);
    instances = new Map<Zone, DockviewInstance>([
      ['left', leftMock.instance],
      ['center', centerMock.instance],
    ]);
    cleanup = setupCrossZoneDnD(instances);
  });

  it('accepts drag-over from a different dockview instance', () => {
    const dragOver = createDragOverEvent('dv-left');
    centerMock.fireUnhandledDragOver(dragOver.event);
    expect(dragOver.accepted).toBe(true);
  });

  it('rejects drag-over from the same dockview instance', () => {
    const dragOver = createDragOverEvent('dv-center');
    centerMock.fireUnhandledDragOver(dragOver.event);
    expect(dragOver.accepted).toBe(false);
  });

  it('moves panel from left to center on cross-zone drop', () => {
    const dropEvent = createDropEvent('dv-left', 'sessions');
    centerMock.fireDidDrop(dropEvent);

    expect(leftMock.removedPanels).toContain('sessions');
    expect(centerMock.addedPanels).toHaveLength(1);
    expect(centerMock.addedPanels[0].id).toBe('sessions');
    expect(centerMock.addedPanels[0].title).toBe('Sessions');
    expect(centerMock.panelMap.has('sessions')).toBe(true);
  });

  it('handles drag to empty zone (target has no panels)', () => {
    const emptyMock = createMockInstance('dv-right', []);
    instances.set('right', emptyMock.instance);
    cleanup();
    cleanup = setupCrossZoneDnD(instances);

    const dropEvent = createDropEvent('dv-left', 'files');
    emptyMock.fireDidDrop(dropEvent);

    expect(leftMock.removedPanels).toContain('files');
    expect(emptyMock.panelMap.has('files')).toBe(true);
  });

  it('handles dragging last panel out (source becomes empty)', () => {
    const singleMock = createMockInstance('dv-right', [
      createMockPanel('editor', 'Editor'),
    ]);
    instances.set('right', singleMock.instance);
    cleanup();
    cleanup = setupCrossZoneDnD(instances);

    const dropEvent = createDropEvent('dv-right', 'editor');
    centerMock.fireDidDrop(dropEvent);

    expect(singleMock.removedPanels).toContain('editor');
    expect(singleMock.panelMap.size).toBe(0);
    expect(centerMock.panelMap.has('editor')).toBe(true);
  });

  it('does nothing on cancel (no drop event fired = panel stays in source)', () => {
    const { event } = createDragOverEvent('dv-left', 'sessions');
    centerMock.fireUnhandledDragOver(event);

    expect(leftMock.removedPanels).toHaveLength(0);
    expect(leftMock.panelMap.has('sessions')).toBe(true);
    expect(leftMock.panelMap.has('files')).toBe(true);
  });

  it('ignores drop events from the same instance', () => {
    const dropEvent = createDropEvent('dv-center', 'chat');
    centerMock.fireDidDrop(dropEvent);

    expect(centerMock.removedPanels).toHaveLength(0);
    expect(centerMock.addedPanels).toHaveLength(0);
  });

  it('ignores group-level drags (no panelId)', () => {
    const dropEvent = createDropEvent('dv-left', null);
    centerMock.fireDidDrop(dropEvent);

    expect(leftMock.removedPanels).toHaveLength(0);
    expect(centerMock.addedPanels).toHaveLength(0);
  });

  it('cleanup removes all event listeners', () => {
    cleanup();

    const dropEvent = createDropEvent('dv-left', 'sessions');
    centerMock.fireDidDrop(dropEvent);

    expect(leftMock.removedPanels).toHaveLength(0);
    expect(centerMock.addedPanels).toHaveLength(0);
  });
});
