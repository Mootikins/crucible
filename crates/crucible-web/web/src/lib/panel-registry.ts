import type { Component } from 'solid-js';

export type Zone = 'left' | 'center' | 'right' | 'bottom';

export interface PanelDefinition {
  id: string;
  title: string;
  component: Component;
  defaultZone: Zone;
  icon: string;
}

export type DefaultLayout = Record<Zone, string[]>;

export class PanelRegistry {
  private panels = new Map<string, PanelDefinition>();

  register(id: string, title: string, component: Component, defaultZone: Zone, icon: string): void {
    this.panels.set(id, { id, title, component, defaultZone, icon });
  }

  get(id: string): PanelDefinition | undefined {
    return this.panels.get(id);
  }

  list(): PanelDefinition[] {
    return Array.from(this.panels.values());
  }

  getDefaultLayout(): DefaultLayout {
    const layout: DefaultLayout = { left: [], center: [], right: [], bottom: [] };
    for (const panel of this.panels.values()) {
      layout[panel.defaultZone].push(panel.id);
    }
    return layout;
  }

  /** Get the internal component map for SolidContentRenderer compatibility */
  getComponentMap(): Map<string, Component> {
    const map = new Map<string, Component>();
    for (const [id, def] of this.panels) {
      map.set(id, def.component);
    }
    return map;
  }
}

/** Singleton registry shared across all dockview instances */
let globalRegistry: PanelRegistry | null = null;

export function getGlobalRegistry(): PanelRegistry {
  if (!globalRegistry) {
    globalRegistry = new PanelRegistry();
  }
  return globalRegistry;
}

/** Reset registry (for testing) */
export function resetGlobalRegistry(): void {
  globalRegistry = null;
}
