/**
 * Drawer state machine with 4-state cycle and localStorage persistence.
 * Pure logic module — no UI components, no dockview dependency.
 *
 * State cycle: hidden → iconStrip → pinned → hidden
 * Flyout transitions: open → close, open → pin
 * Panel management: add, remove (auto-collapse if last removed)
 */

export type DrawerMode = 'hidden' | 'iconStrip' | 'pinned';
export type Zone = 'left' | 'right' | 'bottom';

export interface DrawerState {
  mode: DrawerMode;
  panels: string[];
  activeFlyoutPanel: string | null;
}

const DEFAULT_DRAWER_STATE: DrawerState = {
  mode: 'hidden',
  panels: [],
  activeFlyoutPanel: null,
};

/**
 * Cycle to next mode in the state machine.
 * hidden → iconStrip → pinned → hidden
 */
export function cycleMode(current: DrawerMode): DrawerMode {
  switch (current) {
    case 'hidden':
      return 'iconStrip';
    case 'iconStrip':
      return 'pinned';
    case 'pinned':
      return 'hidden';
    default:
      const _exhaustive: never = current;
      return _exhaustive;
  }
}

/**
 * Open a flyout panel in the drawer.
 * Transitions to iconStrip mode if currently hidden.
 */
export function openFlyout(
  state: DrawerState,
  panelId: string
): DrawerState {
  // Ensure panel exists in drawer
  const panels = state.panels.includes(panelId)
    ? state.panels
    : [...state.panels, panelId];

  // If hidden, transition to iconStrip to show flyout
  const mode = state.mode === 'hidden' ? 'iconStrip' : state.mode;

  return {
    ...state,
    mode,
    panels,
    activeFlyoutPanel: panelId,
  };
}

/**
 * Close the active flyout panel.
 * Collapses to hidden if no panels remain.
 */
export function closeFlyout(state: DrawerState): DrawerState {
  const newState: DrawerState = {
    ...state,
    activeFlyoutPanel: null,
  };

  // Auto-collapse to hidden if no panels
  if (newState.panels.length === 0) {
    newState.mode = 'hidden';
  }

  return newState;
}

/**
 * Pin the active flyout panel.
 * Transitions to pinned mode.
 * Requires an active flyout panel.
 */
export function pinFlyout(state: DrawerState): DrawerState {
  if (!state.activeFlyoutPanel) {
    return state;
  }

  return {
    ...state,
    mode: 'pinned',
    activeFlyoutPanel: null,
  };
}

/**
 * Add a panel to the drawer.
 * Does not change mode or activate flyout.
 */
export function addPanel(state: DrawerState, panelId: string): DrawerState {
  if (state.panels.includes(panelId)) {
    return state;
  }

  return {
    ...state,
    panels: [...state.panels, panelId],
  };
}

/**
 * Remove a panel from the drawer.
 * Auto-collapses to hidden if last panel removed.
 * Closes flyout if removed panel was active.
 */
export function removePanel(state: DrawerState, panelId: string): DrawerState {
  const panels = state.panels.filter((p) => p !== panelId);

  let newState: DrawerState = {
    ...state,
    panels,
  };

  // Close flyout if removed panel was active
  if (newState.activeFlyoutPanel === panelId) {
    newState.activeFlyoutPanel = null;
  }

  // Auto-collapse to hidden if no panels remain
  if (panels.length === 0) {
    newState.mode = 'hidden';
  }

  return newState;
}

/**
 * Save drawer state to localStorage.
 * Key format: crucible:drawer:{zone}
 */
export function saveDrawerState(zone: Zone, state: DrawerState): void {
  const key = `crucible:drawer:${zone}`;
  localStorage.setItem(key, JSON.stringify(state));
}

/**
 * Load drawer state from localStorage.
 * Returns default state if not found or invalid.
 */
export function loadDrawerState(zone: Zone): DrawerState {
  const key = `crucible:drawer:${zone}`;
  const stored = localStorage.getItem(key);

  if (!stored) {
    return { ...DEFAULT_DRAWER_STATE };
  }

  try {
    const parsed = JSON.parse(stored);

    // Validate structure
    if (
      !parsed ||
      typeof parsed.mode !== 'string' ||
      !Array.isArray(parsed.panels) ||
      (parsed.activeFlyoutPanel !== null &&
        typeof parsed.activeFlyoutPanel !== 'string')
    ) {
      return { ...DEFAULT_DRAWER_STATE };
    }

    // Validate mode is valid
    if (!['hidden', 'iconStrip', 'pinned'].includes(parsed.mode)) {
      return { ...DEFAULT_DRAWER_STATE };
    }

    return {
      mode: parsed.mode as DrawerMode,
      panels: parsed.panels,
      activeFlyoutPanel: parsed.activeFlyoutPanel,
    };
  } catch {
    return { ...DEFAULT_DRAWER_STATE };
  }
}

/**
 * Clear drawer state from localStorage.
 */
export function clearDrawerState(zone: Zone): void {
  const key = `crucible:drawer:${zone}`;
  localStorage.removeItem(key);
}
