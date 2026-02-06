import { render } from 'solid-js/web';
import { createComponent } from 'solid-js';
import {
  DockviewComponent,
  type DockviewApi,
  type AddPanelOptions,
  type IContentRenderer,
  type GroupPanelPartInitParameters,
  type CreateComponentOptions,
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

export interface PanelConfig {
  id: string;
  title: string;
  component: Component;
  position?: AddPanelOptions['position'];
  floating?: AddPanelOptions['floating'];
}

type ComponentMap = Map<string, Component>;

class SolidContentRenderer implements IContentRenderer {
  private _element: HTMLElement;
  private _dispose: (() => void) | null = null;

  constructor(
    private readonly componentName: string,
    private readonly registry: ComponentMap,
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
  componentMap?: ComponentMap;
  className?: string;
  initialLayout?: SerializedDockview;
  onReady?: (api: DockviewApi) => void;
  onLayoutChange?: () => void;
}

export interface DockviewInstance {
  api: DockviewApi;
  component: DockviewComponent;
  dispose: () => void;
}

export function createSolidDockview(options: CreateDockviewOptions): DockviewInstance {
  const registry: ComponentMap = options.componentMap ?? new Map();
  const disposables: Disposable[] = [];

  if (!options.componentMap) {
    for (const panel of options.panels) {
      registry.set(panel.id, panel.component);
    }
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

  if (options.onLayoutChange) {
    const layoutDisposable = dockview.onDidLayoutChange(() => {
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
    dispose: () => {
      for (const d of disposables) {
        d.dispose();
      }
      disposables.length = 0;
      dockview.dispose();
    },
  };
}
