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
import { fetchRecents, recordRecent } from '@/lib/api';

interface RecentFile {
  absPath: string;
  name: string;
}

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

let synced = false;

/** Replace the warm-start list with the server's once per page load. */
export function syncRecentsFromServer(): void {
  if (synced) return;
  synced = true;
  void fetchRecents()
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
  void recordRecent(absPath, name).catch(() => {
    /* server write is best-effort; local list already updated */
  });
}
