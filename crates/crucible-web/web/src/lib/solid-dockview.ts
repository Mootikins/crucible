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

interface Disposable {
  dispose(): void;
}

export interface PanelParams {
  api: DockviewApi;
  panelId: string;
  title: string;
}

export type { DockviewApi, DockviewComponent, SerializedDockview };

export type Zone = 'left' | 'center' | 'right' | 'bottom';

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
    private readonly registry: PanelRegistry,
    private readonly getApi: () => DockviewApi
  ) {
    this._element = document.createElement('div');
    this._element.className = 'h-full w-full';
  }

  get element(): HTMLElement {
    return this._element;
  }

  init(params: GroupPanelPartInitParameters): void {
    const PanelComponent = this.registry.get(this.componentName);
    if (PanelComponent) {
      const panelParams: PanelParams = {
        api: this.getApi(),
        panelId: params.params?.id as string ?? this.componentName,
        title: params.title,
      };
      this._dispose = render(() => createComponent(PanelComponent, panelParams), this._element);
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
  initialLayout?: SerializedDockview;
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
  const groupZoneMap = new Map<string, Zone>();
  const hiddenGroups = new Set<string>();
  let toggleDepth = 0;
  const disposables: Disposable[] = [];
  let initTimeoutId: ReturnType<typeof setTimeout> | null = null;

  for (const panel of options.panels) {
    registry.set(panel.id, panel.component);
  }

  let dockviewApi: DockviewApi | null = null;
  const getApi = (): DockviewApi => {
    if (!dockviewApi) throw new Error('Dockview API not initialized');
    return dockviewApi;
  };

  const dockview = new DockviewComponent(options.container, {
    createComponent: (opts: CreateComponentOptions) => {
      return new SolidContentRenderer(opts.name, registry, getApi);
    },
    disableFloatingGroups: false,
    floatingGroupBounds: 'boundedWithinViewport',
    className: options.className,
  });
  
  dockviewApi = dockview.api;

  if (options.initialLayout) {
    type FromJSONParam = Parameters<typeof dockview.fromJSON>[0];
    dockview.fromJSON(options.initialLayout as FromJSONParam);
  } else {
    for (const { id, title, position, floating } of options.panels) {
      const panelOpts: AddPanelOptions = { id, component: id, title };
      if (position) panelOpts.position = position;
      if (floating) panelOpts.floating = floating;
      dockview.addPanel(panelOpts);
    }
  }

  const recalculateZones = (): void => {
    const containerRect = options.container.getBoundingClientRect();
    if (containerRect.width === 0 || containerRect.height === 0) {
      return;
    }
    
    for (const group of dockview.api.groups) {
      if (!hiddenGroups.has(group.id) && group.api.isVisible) {
        const newZone = detectGroupZone(group, containerRect);
        const previousZone = groupZoneMap.get(group.id);
        if (newZone === 'center' && previousZone && previousZone !== 'center') {
          continue;
        }
        groupZoneMap.set(group.id, newZone);
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
    toggleDepth++;
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
      requestAnimationFrame(() => {
        toggleDepth--;
      });
    });
  };

  const scheduleRecalculation = (): void => {
    requestAnimationFrame(() => {
      if (toggleDepth === 0) {
        recalculateZones();
      }
    });
  };

  initTimeoutId = setTimeout(() => scheduleRecalculation(), 200);

  if (options.onLayoutChange) {
    const layoutDisposable = dockview.onDidLayoutChange(() => {
      if (toggleDepth === 0) {
        scheduleRecalculation();
      }
      options.onLayoutChange!();
    });
    disposables.push(layoutDisposable);
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
      if (initTimeoutId !== null) {
        clearTimeout(initTimeoutId);
        initTimeoutId = null;
      }
      for (const d of disposables) {
        d.dispose();
      }
      disposables.length = 0;
      dockview.dispose();
    },
  };
}
