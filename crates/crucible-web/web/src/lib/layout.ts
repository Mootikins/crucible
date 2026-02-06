// src/lib/layout.ts

const LAYOUT_STORAGE_KEY = 'crucible:layout';
const ZONE_STATE_KEY = 'crucible:zones';

export type SerializedDockview = unknown;

// Zone visibility state (separate from layout)
export interface ZoneState {
  left: boolean;
  right: boolean;
  bottom: boolean;
}

export function loadDockviewLayout(): SerializedDockview | null {
  const stored = localStorage.getItem(LAYOUT_STORAGE_KEY);
  if (!stored) return null;
  try {
    const parsed = JSON.parse(stored);
    // Validate it has expected structure
    if (parsed && typeof parsed.grid === 'object' && typeof parsed.panels === 'object') {
      return parsed as SerializedDockview;
    }
    return null;
  } catch {
    return null;
  }
}

export function saveDockviewLayout(state: SerializedDockview): void {
  localStorage.setItem(LAYOUT_STORAGE_KEY, JSON.stringify(state));
}

export function loadZoneState(): ZoneState {
  const stored = localStorage.getItem(ZONE_STATE_KEY);
  if (!stored) {
    return { left: true, right: true, bottom: false };
  }
  try {
    return JSON.parse(stored);
  } catch {
    return { left: true, right: true, bottom: false };
  }
}

export function saveZoneState(state: ZoneState): void {
  localStorage.setItem(ZONE_STATE_KEY, JSON.stringify(state));
}

export function clearLayout(): void {
  localStorage.removeItem(LAYOUT_STORAGE_KEY);
  localStorage.removeItem(ZONE_STATE_KEY);
}

// Legacy support - remove after migration
export interface LayoutState {
  grid: unknown;
  panels: Record<string, { visible: boolean }>;
}

export function loadLayout(): LayoutState | null {
  const stored = localStorage.getItem(LAYOUT_STORAGE_KEY);
  if (!stored) return null;
  try {
    return JSON.parse(stored);
  } catch {
    return null;
  }
}

export function saveLayout(state: LayoutState): void {
  localStorage.setItem(LAYOUT_STORAGE_KEY, JSON.stringify(state));
}
