/**
 * Recently opened files — a small localStorage ring so an empty center can
 * offer "pick up where you left off" instead of a blank stare. Recorded by
 * openFileInGroup (every open path funnels through it); read by the center
 * composer's quick actions.
 */
import { createSignal } from 'solid-js';

export interface RecentFile {
  absPath: string;
  name: string;
}

const STORAGE_KEY = 'crucible:recentFiles';
const MAX_RECENTS = 8;

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

const [recentFiles, setRecentFiles] = createSignal<RecentFile[]>(load());

export { recentFiles };

export function recordRecentFile(absPath: string, name: string): void {
  const next = [
    { absPath, name },
    ...recentFiles().filter((r) => r.absPath !== absPath),
  ].slice(0, MAX_RECENTS);
  setRecentFiles(next);
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
  } catch {
    /* private mode: in-memory only */
  }
}
