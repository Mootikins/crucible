/**
 * Recently opened files — "pick up where you left off" on the empty center.
 * Recorded by openFileInGroup (every open path funnels through it); read by
 * the center composer's quick actions.
 *
 * The server (`/api/recents`, stored next to the layout blob) is the source
 * of truth so the list survives across browsers, ports, and debug
 * instances; localStorage is only a same-origin warm start while the
 * server list loads.
 */
import { createSignal } from 'solid-js';
import { fetchRecentsOnce, recordRecentOnce, type RecentFile } from '@/lib/query/recents';

const STORAGE_KEY = 'crucible:recentFiles';
const MAX_RECENTS = 20;

function load(): RecentFile[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    const parsed = raw ? (JSON.parse(raw) as unknown) : [];
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(
      (e): e is RecentFile =>
        !!e && typeof e === 'object' && typeof (e as RecentFile).absPath === 'string',
    );
  } catch {
    return [];
  }
}

function persistLocal(next: RecentFile[]): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
  } catch {
    /* private mode: in-memory only */
  }
}

const [recentFiles, setRecentFiles] = createSignal<RecentFile[]>(load());

export { recentFiles };

/**
 * Replace the warm-start list with the server's.
 *
 * There is no once-per-page-load flag any more. The server's list is held
 * under `keys.recents()`, and that entry decides when to ask again — so a
 * second composer mounting costs nothing, and the first one mounting after a
 * file was opened elsewhere gets the list that open produced rather than the
 * one from before it.
 */
export function syncRecentsFromServer(): void {
  void fetchRecentsOnce()
    .then((server) => {
      if (server.length > 0) {
        setRecentFiles(server);
        persistLocal(server);
      }
    })
    .catch(() => {
      /* offline/legacy server: keep the localStorage list */
    });
}

export function recordRecentFile(absPath: string, name: string): void {
  const next = [
    { absPath, name },
    ...recentFiles().filter((r) => r.absPath !== absPath),
  ].slice(0, MAX_RECENTS);
  setRecentFiles(next);
  persistLocal(next);
  // The write makes the held list wrong, so the next reader of it sees this
  // open. A refusal is the write's own business: the file is open either way.
  void recordRecentOnce(absPath, name);
}
