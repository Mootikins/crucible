import { createSignal } from 'solid-js';

/**
 * Which kilns this device keeps offline, and how much of each.
 *
 * `notes` keeps every note and fetches an attachment when it is first opened.
 * `everything` fetches the attachments too, which is the mode for a user who
 * wants a kiln on a plane with its images present. Attachments are the whole
 * storage budget, so they are a choice rather than a silent consequence —
 * decision log, 2026-09-11.
 *
 * In `localStorage`, not the offline store: it is a handful of paths, the app
 * reads it during first paint, and it must survive a user clearing the cache
 * they are about to rebuild.
 */

export type OfflineMode = 'notes' | 'everything';

export const KEPT_KILNS_KEY = 'crucible:offlineKilns';

export interface KeptKiln {
  mode: OfflineMode;
}

type KeptMap = Record<string, KeptKiln>;

/**
 * Read the stored map, keeping only entries this app understands.
 *
 * Exported so the filtering can be tested on its own. `load()` runs once at
 * import, so a test that writes to `localStorage` afterwards is never parsed
 * — which is how the gate for this ended up asserting only that the answer
 * was one of the values its return type already permits.
 */
export function parseKept(raw: string | null): KeptMap {
  try {
    const parsed: unknown = raw ? JSON.parse(raw) : null;
    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) return {};
    const out: KeptMap = {};
    for (const [path, value] of Object.entries(parsed as Record<string, unknown>)) {
      const mode = (value as KeptKiln | null)?.mode;
      if (mode === 'notes' || mode === 'everything') out[path] = { mode };
    }
    return out;
  } catch {
    return {}; // storage a human edited into nonsense
  }
}

function load(): KeptMap {
  try {
    return parseKept(localStorage.getItem(KEPT_KILNS_KEY));
  } catch {
    return {}; // private mode: no storage at all
  }
}

const [kept, setKept] = createSignal<KeptMap>(load());

function persist(next: KeptMap): void {
  setKept(next);
  try {
    localStorage.setItem(KEPT_KILNS_KEY, JSON.stringify(next));
  } catch {
    /* private mode: this session only */
  }
}

export { kept };

/** The mode a kiln is kept in, or null when it is not kept. */
export function keptMode(kilnPath: string): OfflineMode | null {
  return kept()[kilnPath]?.mode ?? null;
}

export const keptActions = {
  /** Keep a kiln offline, or change how much of it is kept. */
  keep(kilnPath: string, mode: OfflineMode): void {
    persist({ ...kept(), [kilnPath]: { mode } });
  },

  /** Stop keeping a kiln. The caller removes what it stored. */
  forget(kilnPath: string): void {
    const { [kilnPath]: _dropped, ...rest } = kept();
    void _dropped;
    persist(rest);
  },
};
