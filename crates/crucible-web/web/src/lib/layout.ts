// src/lib/layout.ts

const LAYOUT_STORAGE_KEY = 'crucible:layout';

export interface LayoutState {
  // Dockview serialized state
  grid: unknown;
  panels: Record<string, PanelState>;
}

export interface PanelState {
  visible: boolean;
  // Position info from dockview
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

export function clearLayout(): void {
  localStorage.removeItem(LAYOUT_STORAGE_KEY);
}
