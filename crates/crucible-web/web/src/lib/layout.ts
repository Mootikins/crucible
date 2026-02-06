import type { Zone } from './panel-registry';

const LAYOUT_STORAGE_KEY = 'crucible:layout';
const ZONE_STATE_KEY = 'crucible:zones';
const ZONE_WIDTHS_KEY = 'crucible:zone-widths';

export type { Zone };

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

export function saveZoneState(state: ZoneState): void {
  localStorage.setItem(ZONE_STATE_KEY, JSON.stringify(state));
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

/**
 * Save layout for a specific zone to localStorage.
 * Each zone is stored independently with key: crucible:layout:{zone}
 */
export function saveZoneLayout(zone: Zone, serialized: string): void {
  const key = `crucible:layout:${zone}`;
  localStorage.setItem(key, serialized);
}

/**
 * Load layout for a specific zone from localStorage.
 * Returns null if zone layout doesn't exist or is invalid.
 * Each zone validates independently.
 */
export function loadZoneLayout(zone: Zone): string | null {
  const key = `crucible:layout:${zone}`;
  const stored = localStorage.getItem(key);
  if (!stored) return null;
  
  try {
    // Validate it's valid JSON and has expected structure
    const parsed = JSON.parse(stored);
    if (parsed && typeof parsed.grid === 'object' && typeof parsed.panels === 'object') {
      return stored;
    }
    return null;
  } catch {
    return null;
  }
}

/**
 * Migrate old single-key layout format to per-zone keys.
 * Old format: crucible:layout (single key with all zones)
 * New format: crucible:layout:left, crucible:layout:center, crucible:layout:right, crucible:layout:bottom
 * 
 * Migration strategy:
 * 1. Check if old key exists
 * 2. Parse old layout JSON
 * 3. Distribute panels to per-zone keys (or use defaults if parse fails)
 * 4. Clear old key after successful migration
 * 5. Fall back to defaults on error
 */
export function migrateOldLayout(): void {
  const oldKey = LAYOUT_STORAGE_KEY;
  const oldLayout = localStorage.getItem(oldKey);
  
  // No old layout to migrate
  if (!oldLayout) return;
  
  try {
    const parsed = JSON.parse(oldLayout);
    
    // Validate old layout has expected structure
    if (!parsed || typeof parsed.grid !== 'object' || typeof parsed.panels !== 'object') {
      // Invalid old layout, just clear it
      localStorage.removeItem(oldKey);
      return;
    }
    
    // Distribute panels to per-zone keys
    // For now, save the entire layout to center (main workspace)
    // In future, could parse panel positions to distribute across zones
    const centerKey = `crucible:layout:center`;
    localStorage.setItem(centerKey, oldLayout);
    
    // Clear old key after successful migration
    localStorage.removeItem(oldKey);
  } catch {
    // Parse error: clear old key and let zones use defaults
    localStorage.removeItem(oldKey);
  }
}

