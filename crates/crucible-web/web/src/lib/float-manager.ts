import type { DockviewApi, AddPanelOptions } from 'dockview-core';
import type { Zone } from './panel-registry';
import type { DockviewInstance } from './solid-dockview';

const FLOAT_WIDTH = 400;
const FLOAT_HEIGHT = 300;

const originalZones = new Map<string, Zone>();

export function getOriginalZone(panelId: string): Zone | undefined {
  return originalZones.get(panelId);
}

export function isFloating(panelId: string): boolean {
  return originalZones.has(panelId);
}

export function getFloatingPanelIds(): string[] {
  return Array.from(originalZones.keys());
}

export function floatPanel(
  panelId: string,
  sourceZone: Zone,
  centerApi: DockviewApi,
  zoneApis: Map<Zone, DockviewInstance>,
  containerEl?: HTMLElement,
): boolean {
  if (originalZones.has(panelId)) return false;

  const sourceInstance = zoneApis.get(sourceZone);
  if (!sourceInstance) return false;

  const panel = sourceInstance.api.getPanel(panelId);
  if (!panel) return false;

  const title = panel.title ?? panelId;
  sourceInstance.api.removePanel(panel);
  originalZones.set(panelId, sourceZone);

  let x = 100;
  let y = 50;
  if (containerEl) {
    const rect = containerEl.getBoundingClientRect();
    if (rect) {
      x = Math.max(0, Math.round((rect.width - FLOAT_WIDTH) / 2));
      y = Math.max(0, Math.round((rect.height - FLOAT_HEIGHT) / 2));
    }
  }

  const opts: AddPanelOptions = {
    id: panelId,
    component: panelId,
    title,
    floating: {
      x,
      y,
      width: FLOAT_WIDTH,
      height: FLOAT_HEIGHT,
    },
  };
  centerApi.addPanel(opts);

  return true;
}

export function dockPanel(
  panelId: string,
  centerApi: DockviewApi,
  zoneApis: Map<Zone, DockviewInstance>,
  targetZone?: Zone,
): boolean {
  const originalZone = originalZones.get(panelId);
  if (!originalZone && !targetZone) return false;

  const zone = targetZone ?? originalZone!;
  const targetInstance = zoneApis.get(zone);
  if (!targetInstance) return false;

  const panel = centerApi.getPanel(panelId);
  if (!panel) return false;

  const title = panel.title ?? panelId;
  centerApi.removePanel(panel);

  targetInstance.api.addPanel({
    id: panelId,
    component: panelId,
    title,
  });

  originalZones.delete(panelId);
  return true;
}

export function resetFloatManager(): void {
  originalZones.clear();
}
