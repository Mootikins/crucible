import { render } from 'solid-js/web';
import { createComponent } from 'solid-js';
import {
  DockviewComponent,
  type DockviewApi,
  type AddPanelOptions,
  type IContentRenderer,
  type GroupPanelPartInitParameters,
  type CreateComponentOptions,
  type DockviewGroupPanel,
  type SerializedDockview,
} from 'dockview-core';
import type { Component } from 'solid-js';

export type { DockviewApi, DockviewComponent, SerializedDockview };

export type Zone = 'left' | 'center' | 'right' | 'bottom';

export interface ZoneThresholds {
  leftWidth: number;    // 0-1, groups with right edge <= this are "left"
  rightStart: number;   // 0-1, groups with left edge >= this are "right"  
  bottomStart: number;  // 0-1, groups with top edge >= this are "bottom"
}

const DEFAULT_THRESHOLDS: ZoneThresholds = {
  leftWidth: 0.35,
  rightStart: 0.65,
  bottomStart: 0.60,
};

export interface PanelConfig {
  id: string;
  title: string;
  component: Component;
  position?: AddPanelOptions['position'];
  floating?: AddPanelOptions['floating'];
}

type PanelRegistry = Map<string, Component>;

class SolidContentRenderer implements IContentRenderer {
  private _element: HTMLElement;
  private _dispose: (() => void) | null = null;

  constructor(
    private readonly componentName: string,
    private readonly registry: PanelRegistry
  ) {
    this._element = document.createElement('div');
    this._element.className = 'h-full w-full';
  }

  get element(): HTMLElement {
    return this._element;
  }

  init(_params: GroupPanelPartInitParameters): void {
    const PanelComponent = this.registry.get(this.componentName);
    if (PanelComponent) {
      this._dispose = render(() => createComponent(PanelComponent, {}), this._element);
    }
  }

  dispose(): void {
    if (this._dispose) {
      this._dispose();
      this._dispose = null;
    }
  }
}

export interface CreateDockviewOptions {
  container: HTMLElement;
  panels: PanelConfig[];
  className?: string;
  thresholds?: Partial<ZoneThresholds>;
  onReady?: (api: DockviewApi) => void;
  onLayoutChange?: () => void;
}

export interface DockviewInstance {
  api: DockviewApi;
  component: DockviewComponent;
  getGroupZones: () => Map<string, Zone>;
  getGroupsInZone: (zone: Zone) => DockviewGroupPanel[];
  setZoneVisible: (zone: Zone, visible: boolean) => void;
  recalculateZones: () => void;
  dispose: () => void;
}

export function detectGroupZone(
  group: DockviewGroupPanel,
  containerRect: DOMRect,
  _thresholds: ZoneThresholds
): Zone {
  const groupRect = group.element.getBoundingClientRect();
  
  const relativeLeft = (groupRect.left - containerRect.left) / containerRect.width;
  const relativeRight = (groupRect.right - containerRect.left) / containerRect.width;
  const relativeTop = (groupRect.top - containerRect.top) / containerRect.height;
  const relativeBottom = (groupRect.bottom - containerRect.top) / containerRect.height;
  
  const EDGE_TOLERANCE = 0.02;
  const touchesLeft = relativeLeft < EDGE_TOLERANCE;
  const touchesRight = relativeRight > (1 - EDGE_TOLERANCE);
  const touchesBottom = relativeBottom > (1 - EDGE_TOLERANCE);
  const touchesTop = relativeTop < EDGE_TOLERANCE;
  
  if (touchesBottom && !touchesTop) {
    return 'bottom';
  }
  
  if (touchesLeft && !touchesRight) {
    return 'left';
  }
  
  if (touchesRight && !touchesLeft) {
    return 'right';
  }
  
  return 'center';
}

export function createSolidDockview(options: CreateDockviewOptions): DockviewInstance {
  const registry: PanelRegistry = new Map();
  const thresholds: ZoneThresholds = { ...DEFAULT_THRESHOLDS, ...options.thresholds };
  
  const groupZoneMap = new Map<string, Zone>();
  const hiddenGroups = new Set<string>();
  let isTogglingZone = false;

  for (const panel of options.panels) {
    registry.set(panel.id, panel.component);
  }

  const dockview = new DockviewComponent(options.container, {
    createComponent: (opts: CreateComponentOptions) => {
      return new SolidContentRenderer(opts.name, registry);
    },
    disableFloatingGroups: false,
    floatingGroupBounds: 'boundedWithinViewport',
    className: options.className,
  });

  for (const panel of options.panels) {
    const addOpts: AddPanelOptions = {
      id: panel.id,
      component: panel.id,
      title: panel.title,
    };

    if (panel.position) {
      addOpts.position = panel.position;
    }

    if (panel.floating) {
      addOpts.floating = panel.floating;
    }

    dockview.addPanel(addOpts);
  }

  const recalculateZones = (): void => {
    const containerRect = options.container.getBoundingClientRect();
    if (containerRect.width === 0 || containerRect.height === 0) {
      return;
    }
    
    for (const group of dockview.api.groups) {
      if (!hiddenGroups.has(group.id) && group.api.isVisible) {
        groupZoneMap.set(group.id, detectGroupZone(group, containerRect, thresholds));
      }
    }
  };

  const getGroupZones = (): Map<string, Zone> => {
    return new Map(groupZoneMap);
  };

  const getGroupsInZone = (zone: Zone): DockviewGroupPanel[] => {
    return dockview.api.groups.filter(group => groupZoneMap.get(group.id) === zone);
  };

  const setZoneVisible = (zone: Zone, visible: boolean): void => {
    isTogglingZone = true;
    const groups = getGroupsInZone(zone);
    for (const group of groups) {
      if (visible) {
        hiddenGroups.delete(group.id);
      } else {
        hiddenGroups.add(group.id);
      }
      group.api.setVisible(visible);
    }
    requestAnimationFrame(() => {
      isTogglingZone = false;
    });
  };

  const scheduleRecalculation = (): void => {
    requestAnimationFrame(() => {
      if (!isTogglingZone) {
        recalculateZones();
      }
    });
  };

  setTimeout(() => scheduleRecalculation(), 200);

  if (options.onLayoutChange) {
    dockview.onDidLayoutChange(() => {
      if (!isTogglingZone) {
        scheduleRecalculation();
      }
      options.onLayoutChange!();
    });
  }

  if (options.onReady) {
    options.onReady(dockview.api);
  }

  return {
    api: dockview.api,
    component: dockview,
    getGroupZones,
    getGroupsInZone,
    setZoneVisible,
    recalculateZones,
    dispose: () => {
      dockview.dispose();
    },
  };
}

export function setGroupVisibleById(api: DockviewApi, groupId: string, visible: boolean): void {
  const group = api.getGroup(groupId);
  if (group) {
    group.api.setVisible(visible);
  }
}
