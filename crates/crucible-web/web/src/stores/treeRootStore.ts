/**
 * Which root the file tree shows, PER SESSION.
 *
 * The tree follows the active session. Picking a root pins it for that session
 * and stops the following, so browsing a kiln, glancing at another session and
 * coming back returns you to where you were — the reason the pin is keyed by
 * session id rather than being one app-wide preference. One global pin would
 * stop the tree following for EVERY session at once, which is the behaviour
 * this replaces wearing a new name.
 *
 * localStorage, not the daemon: this is display state of one surface, and no
 * other client consumes it (the TUI has no file tree). The daemon owns the
 * session's kilns and workspace; it has no opinion about which of them you are
 * currently looking at.
 */
import { createSignal } from 'solid-js';
import type { TreeRoot } from '@/lib/tree-root';
import { rootKey } from '@/lib/tree-root';

export const TREE_ROOT_STORAGE_KEY = 'crucible:treeRoot.bySession';
/** The pre-session global preference. Read once to drop it, never written. */
const LEGACY_GLOBAL_KEY = 'crucible:treeRoot';

type PinMap = Record<string, string>;

function load(): PinMap {
  try {
    localStorage.removeItem(LEGACY_GLOBAL_KEY);
    const raw = localStorage.getItem(TREE_ROOT_STORAGE_KEY);
    const parsed: unknown = raw ? JSON.parse(raw) : null;
    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) return {};
    // Hand-edited or half-written storage: keep only string→string pairs
    // rather than letting a stray value reach `rootKey` comparisons.
    const out: PinMap = {};
    for (const [id, key] of Object.entries(parsed as Record<string, unknown>)) {
      if (typeof key === 'string' && key) out[id] = key;
    }
    return out;
  } catch {
    return {}; // private mode / storage disabled
  }
}

const [pins, setPins] = createSignal<PinMap>(load());

function persist(next: PinMap): void {
  setPins(next);
  try {
    localStorage.setItem(TREE_ROOT_STORAGE_KEY, JSON.stringify(next));
  } catch {
    /* private mode: in-memory only */
  }
}

/**
 * The root key this session is pinned to, or `null` when it follows.
 *
 * A PREFERENCE, not the display state: `resolveSessionRoot` checks it against
 * the session's live roots and falls back, so a stale key can never show a
 * root the tree is not rendering.
 */
export function pinnedRootKey(sessionId: string | null | undefined): string | null {
  return sessionId ? (pins()[sessionId] ?? null) : null;
}

export const treeRootActions = {
  /** Pin this session to a root — the tree stops following until it is cleared. */
  pin(sessionId: string, root: TreeRoot): void {
    persist({ ...pins(), [sessionId]: rootKey(root) });
  },

  /** Follow the session again. */
  unpin(sessionId: string): void {
    const { [sessionId]: _dropped, ...rest } = pins();
    void _dropped;
    persist(rest);
  },

  /**
   * Forget pins for sessions that no longer exist.
   *
   * Deleting a session leaves its pin behind, and the map is written on every
   * pick — without this it only ever grows. Called with the live session ids
   * whenever the list refreshes; an empty list is treated as "not loaded yet"
   * and prunes nothing, so a failed fetch cannot wipe every pin.
   */
  prune(liveSessionIds: readonly string[]): void {
    if (liveSessionIds.length === 0) return;
    const live = new Set(liveSessionIds);
    const current = pins();
    const kept = Object.entries(current).filter(([id]) => live.has(id));
    if (kept.length === Object.keys(current).length) return;
    persist(Object.fromEntries(kept));
  },
};
