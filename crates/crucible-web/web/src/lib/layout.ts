import { type SerializedDockview } from 'dockview-core';

const LAYOUT_STORAGE_KEY = 'crucible:layout';
const ZONE_STATE_KEY = 'crucible:zones';
const ZONE_WIDTHS_KEY = 'crucible:zone-widths';

export type { SerializedDockview };

export type ZoneMode = 'visible' | 'hidden' | 'pinned';

export interface ZoneState {
  left: ZoneMode;
  right: ZoneMode;
  bottom: ZoneMode;
}

export const DEFAULT_ZONE_STATE: ZoneState = { left: 'visible', right: 'visible', bottom: 'hidden' };

function isValidZoneMode(value: unknown): boolean {
  return value === 'visible' || value === 'hidden' || value === 'pinned';
}

export function migrateZoneState(value: unknown): ZoneState {
  // Handle null/undefined/garbage input
  if (value === null || value === undefined || typeof value !== 'object') {
    return DEFAULT_ZONE_STATE;
  }
  
  const obj = value as Record<string, unknown>;
  
  // Check if all required properties exist
  if (!('left' in obj) || !('right' in obj) || !('bottom' in obj)) {
    return DEFAULT_ZONE_STATE;
  }
  
  // If already in ZoneMode format, validate and return
  if (isValidZoneMode(obj.left) && isValidZoneMode(obj.right) && isValidZoneMode(obj.bottom)) {
    return {
      left: obj.left as ZoneMode,
      right: obj.right as ZoneMode,
      bottom: obj.bottom as ZoneMode,
    };
  }
  
  // Convert from boolean format
  if (typeof obj.left === 'boolean' && typeof obj.right === 'boolean' && typeof obj.bottom === 'boolean') {
    return {
      left: obj.left ? 'visible' : 'hidden',
      right: obj.right ? 'visible' : 'hidden',
      bottom: obj.bottom ? 'visible' : 'hidden',
    };
  }
  
  // Invalid format
  return DEFAULT_ZONE_STATE;
}

export function isValidZoneState(value: unknown): value is ZoneState {
  if (typeof value !== 'object' || value === null) return false;
  const obj = value as Record<string, unknown>;
  
  if (!('left' in obj) || !('right' in obj) || !('bottom' in obj)) return false;
  
  if (isValidZoneMode(obj.left) && isValidZoneMode(obj.right) && isValidZoneMode(obj.bottom)) {
    return true;
  }
  
  if (typeof obj.left === 'boolean' && typeof obj.right === 'boolean' && typeof obj.bottom === 'boolean') {
    return true;
  }
  
  return false;
}

export function loadZoneState(): ZoneState {
  const stored = localStorage.getItem(ZONE_STATE_KEY);
  if (!stored) return DEFAULT_ZONE_STATE;
  try {
    const parsed = JSON.parse(stored);
    return migrateZoneState(parsed);
  } catch {
    return DEFAULT_ZONE_STATE;
  }
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

export function saveZoneState(state: ZoneState): void {
  localStorage.setItem(ZONE_STATE_KEY, JSON.stringify(state));
}

export function clearLayout(): void {
  localStorage.removeItem(LAYOUT_STORAGE_KEY);
  localStorage.removeItem(ZONE_STATE_KEY);
  localStorage.removeItem(ZONE_WIDTHS_KEY);
}

export interface ZoneWidths {
  left: number;
  right: number;
  bottom: number;
}

export const DEFAULT_ZONE_WIDTHS: ZoneWidths = {
  left: 280,
  right: 350,
  bottom: 200,
};

export function loadZoneWidths(): ZoneWidths {
  const stored = localStorage.getItem(ZONE_WIDTHS_KEY);
  if (!stored) return { ...DEFAULT_ZONE_WIDTHS };
  try {
    const parsed = JSON.parse(stored);
    if (
      typeof parsed === 'object' &&
      parsed !== null &&
      typeof parsed.left === 'number' &&
      typeof parsed.right === 'number' &&
      typeof parsed.bottom === 'number' &&
      parsed.left > 0 &&
      parsed.right > 0 &&
      parsed.bottom > 0
    ) {
      return { left: parsed.left, right: parsed.right, bottom: parsed.bottom };
    }
    return { ...DEFAULT_ZONE_WIDTHS };
  } catch {
    return { ...DEFAULT_ZONE_WIDTHS };
  }
}

export function saveZoneWidths(widths: ZoneWidths): void {
  localStorage.setItem(ZONE_WIDTHS_KEY, JSON.stringify(widths));
}
