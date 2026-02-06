import { describe, it, expect, beforeEach, vi } from 'vitest';
import {
  floatPanel,
  dockPanel,
  isFloating,
  getOriginalZone,
  getFloatingPanelIds,
  resetFloatManager,
} from '../float-manager';
import type { Zone } from '../panel-registry';
import type { DockviewInstance } from '../solid-dockview';

function createMockPanel(id: string, title?: string) {
  return { id, title: title ?? id, group: { id: `group-${id}` } };
}

function createMockContainer(): HTMLElement {
  return { getBoundingClientRect: () => ({ width: 800, height: 600 }) } as unknown as HTMLElement;
}

function createMockInstance(viewId: string, panels: ReturnType<typeof createMockPanel>[] = []) {
  const panelMap = new Map(panels.map(p => [p.id, p]));
  const addedPanels: any[] = [];

  const api = {
    id: viewId,
    get panels() { return [...panelMap.values()]; },
    getPanel: (id: string) => panelMap.get(id),
    removePanel: (panel: any) => { panelMap.delete(panel.id); },
    addPanel: (opts: any) => {
      const panel = createMockPanel(opts.id, opts.title);
      panelMap.set(opts.id, panel);
      addedPanels.push(opts);
      return panel;
    },
  };

  return {
    instance: { api, component: {} as any, dispose: vi.fn() } as unknown as DockviewInstance,
    addedPanels,
    panelMap,
  };
}

describe('float-manager', () => {
  let leftMock: ReturnType<typeof createMockInstance>;
  let centerMock: ReturnType<typeof createMockInstance>;
  let rightMock: ReturnType<typeof createMockInstance>;
  let instances: Map<Zone, DockviewInstance>;

  beforeEach(() => {
    resetFloatManager();
    leftMock = createMockInstance('dv-left', [
      createMockPanel('sessions', 'Sessions'),
      createMockPanel('files', 'Files'),
    ]);
    centerMock = createMockInstance('dv-center', [
      createMockPanel('chat', 'Chat'),
    ]);
    rightMock = createMockInstance('dv-right', [
      createMockPanel('editor', 'Editor'),
    ]);
    instances = new Map<Zone, DockviewInstance>([
      ['left', leftMock.instance],
      ['center', centerMock.instance],
      ['right', rightMock.instance],
    ]);
  });

  describe('floatPanel', () => {
    it('removes panel from source zone and adds floating panel to center', () => {
      const result = floatPanel('sessions', 'left', centerMock.instance.api, instances);

      expect(result).toBe(true);
      expect(leftMock.panelMap.has('sessions')).toBe(false);
      expect(centerMock.addedPanels).toHaveLength(1);
      expect(centerMock.addedPanels[0]).toMatchObject({
        id: 'sessions',
        component: 'sessions',
        title: 'Sessions',
      });
      expect(centerMock.addedPanels[0].floating).toBeDefined();
      expect(centerMock.addedPanels[0].floating.width).toBe(400);
      expect(centerMock.addedPanels[0].floating.height).toBe(300);
    });

    it('centers the floating panel within the container', () => {
      const container = createMockContainer();
      floatPanel('sessions', 'left', centerMock.instance.api, instances, container);

      const floating = centerMock.addedPanels[0].floating;
      expect(floating.x).toBe(200);
      expect(floating.y).toBe(150);
    });

    it('returns false for already-floating panel', () => {
      floatPanel('sessions', 'left', centerMock.instance.api, instances);
      const result = floatPanel('sessions', 'left', centerMock.instance.api, instances);

      expect(result).toBe(false);
      expect(centerMock.addedPanels).toHaveLength(1);
    });

    it('returns false when source zone does not exist', () => {
      const result = floatPanel('sessions', 'bottom', centerMock.instance.api, instances);
      expect(result).toBe(false);
    });

    it('returns false when panel does not exist in source', () => {
      const result = floatPanel('nonexistent', 'left', centerMock.instance.api, instances);
      expect(result).toBe(false);
    });
  });

  describe('dockPanel', () => {
    it('returns floating panel to its original zone', () => {
      floatPanel('sessions', 'left', centerMock.instance.api, instances);
      const result = dockPanel('sessions', centerMock.instance.api, instances);

      expect(result).toBe(true);
      expect(leftMock.panelMap.has('sessions')).toBe(true);
      expect(isFloating('sessions')).toBe(false);
    });

    it('allows docking to a different target zone', () => {
      floatPanel('sessions', 'left', centerMock.instance.api, instances);
      const result = dockPanel('sessions', centerMock.instance.api, instances, 'right');

      expect(result).toBe(true);
      expect(rightMock.panelMap.has('sessions')).toBe(true);
      expect(leftMock.panelMap.has('sessions')).toBe(false);
    });

    it('returns false when panel is not floating and no target zone given', () => {
      const result = dockPanel('chat', centerMock.instance.api, instances);
      expect(result).toBe(false);
    });

    it('returns false when panel not found in center', () => {
      floatPanel('sessions', 'left', centerMock.instance.api, instances);
      centerMock.panelMap.delete('sessions');
      const result = dockPanel('sessions', centerMock.instance.api, instances);
      expect(result).toBe(false);
    });
  });

  describe('tracking', () => {
    it('tracks original zone correctly', () => {
      floatPanel('sessions', 'left', centerMock.instance.api, instances);
      floatPanel('editor', 'right', centerMock.instance.api, instances);

      expect(getOriginalZone('sessions')).toBe('left');
      expect(getOriginalZone('editor')).toBe('right');
      expect(getOriginalZone('chat')).toBeUndefined();
    });

    it('reports floating state correctly', () => {
      expect(isFloating('sessions')).toBe(false);

      floatPanel('sessions', 'left', centerMock.instance.api, instances);
      expect(isFloating('sessions')).toBe(true);

      dockPanel('sessions', centerMock.instance.api, instances);
      expect(isFloating('sessions')).toBe(false);
    });

    it('lists all floating panel IDs', () => {
      floatPanel('sessions', 'left', centerMock.instance.api, instances);
      floatPanel('editor', 'right', centerMock.instance.api, instances);

      const ids = getFloatingPanelIds();
      expect(ids).toContain('sessions');
      expect(ids).toContain('editor');
      expect(ids).toHaveLength(2);
    });

    it('clears tracking on resetFloatManager', () => {
      floatPanel('sessions', 'left', centerMock.instance.api, instances);
      resetFloatManager();

      expect(isFloating('sessions')).toBe(false);
      expect(getFloatingPanelIds()).toHaveLength(0);
    });
  });
});
