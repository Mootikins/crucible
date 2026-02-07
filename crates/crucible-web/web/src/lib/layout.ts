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
 * Migrate old layout formats to new docked panel format.
 * 
 * Old formats:
 * - crucible:layout (single key, pre-zone split)
 * - crucible:layout:center (per-zone keys)
 * - crucible:zones (zone visibility state)
 * - crucible:zone-widths (zone sizes)
 * - crucible:drawer:* (drawer state, if existed)
 * 
 * New format:
 * - crucible:layout (single SerializedDockview with dockedGroups)
 * 
 * Migration strategy:
 * 1. Check if new format already exists → skip migration
 * 2. Read old center layout (crucible:layout:center)
 * 3. Read old zone state/widths
 * 4. Construct SerializedDockview with dockedGroups
 * 5. Save as single crucible:layout key
 * 6. Clear old keys
 */
export function migrateToDockedLayout(): void {
  const newKey = LAYOUT_STORAGE_KEY;
  
  if (localStorage.getItem(newKey)) {
    return;
  }
  
  try {
    const centerLayout = localStorage.getItem('crucible:layout:center');
    const zoneState = loadZoneState();
    const zoneWidths = loadZoneWidths();
    
    let migratedLayout: any = null;
    
    if (centerLayout) {
      try {
        migratedLayout = JSON.parse(centerLayout);
      } catch {
        migratedLayout = null;
      }
    }
    
    if (migratedLayout && typeof migratedLayout === 'object') {
      const dockedGroups: any[] = [];
      
      const sideMap: Record<string, 'left' | 'right' | 'bottom'> = {
        left: 'left',
        right: 'right',
        bottom: 'bottom',
      };
      
      for (const [zone, side] of Object.entries(sideMap)) {
        const mode = zoneState[zone as keyof ZoneState];
        const size = zoneWidths[zone as keyof ZoneWidths];
        const collapsed = mode === 'hidden';
        
        dockedGroups.push({
          side,
          size,
          collapsed,
          data: {
            views: [],
            activeView: 0,
            id: `docked-${side}`,
          },
        });
      }
      
      if (dockedGroups.length > 0) {
        migratedLayout.dockedGroups = dockedGroups;
      }
      
      localStorage.setItem(newKey, JSON.stringify(migratedLayout));
    }
    
    localStorage.removeItem('crucible:layout:center');
    localStorage.removeItem('crucible:layout:left');
    localStorage.removeItem('crucible:layout:right');
    localStorage.removeItem('crucible:layout:bottom');
    localStorage.removeItem(ZONE_STATE_KEY);
    localStorage.removeItem(ZONE_WIDTHS_KEY);
    
    const drawerKeys = ['crucible:drawer:left', 'crucible:drawer:right', 'crucible:drawer:bottom'];
    for (const key of drawerKeys) {
      localStorage.removeItem(key);
    }
  } catch (error) {
    console.warn('Layout migration failed, using defaults:', error);
  }
}

/**
 * Migrate old single-key layout format to per-zone keys.
 * @deprecated Use migrateToDockedLayout() instead
 */
export function migrateOldLayout(): void {
  const oldKey = LAYOUT_STORAGE_KEY;
  const oldLayout = localStorage.getItem(oldKey);
  
  if (!oldLayout) return;
  
  try {
    const parsed = JSON.parse(oldLayout);
    
    if (!parsed || typeof parsed.grid !== 'object' || typeof parsed.panels !== 'object') {
      localStorage.removeItem(oldKey);
      return;
    }
    
    const centerKey = `crucible:layout:center`;
    localStorage.setItem(centerKey, oldLayout);
    
    localStorage.removeItem(oldKey);
  } catch {
    localStorage.removeItem(oldKey);
  }
}

