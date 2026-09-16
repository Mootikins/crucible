/**
 * The route of the filesystem stream: one event, one cache write.
 *
 * The daemon watches a kiln and says what moved. Two kinds of reader hold
 * what a move makes wrong: the one that asked for the file's bytes, and the
 * one that asked for its folder's listing, which carries a size and a modified
 * time beside every name. Both are invalidated, and neither is patched: the
 * frame carries a path and no content, so there is nothing here to write.
 *
 * Paths on this stream are ABSOLUTE (`web/fs_events.rs`), so the folder of an
 * event is the text before its last separator. The keys this route names are
 * absolute for the same reason, and `useListDir` keys its listing the same way.
 *
 * The route runs once per event inside the shared root of `lib/query/sse.ts`,
 * so the panel and the editor together invalidate one key once.
 */
import type { FsEvent } from '@/lib/types';
// One definition of the folder of a path, shared with the readers. A second
// copy here would let a reader's key and an event's key drift apart, which is
// a panel that never refreshes and a test that still passes.
import { folderOf } from '../fs';
import { keys } from '../keys';
import { setFsEventRoute, type SseRouteContext } from '../sse';

/** The paths one event names: one for a change or a delete, two for a move. */
function pathsOf(event: FsEvent): string[] {
  switch (event.type) {
    case 'changed':
    case 'deleted':
      return event.path ? [event.path] : [];
    case 'moved':
      return [event.from, event.to].filter(Boolean);
    default:
      return [];
  }
}

/** Turns one filesystem event into the cache writes it owes every reader. */
export function routeFsEvent(event: FsEvent, { client }: SseRouteContext): void {
  const paths = pathsOf(event);
  for (const path of paths) {
    void client.invalidateQueries({ queryKey: keys.fsFile(path) });
  }

  // The folders after the files, and each folder once: a rename that stays in
  // one folder names it at both ends, and two invalidations of one listing are
  // two refetches for one event.
  const folders = new Set(paths.map(folderOf));
  for (const folder of folders) {
    void client.invalidateQueries({ queryKey: keys.fsDir(folder) });
  }
}

/**
 * Names this module the route of the filesystem stream.
 *
 * The app calls it once, at start (`src/index.tsx`), because a route installed
 * by the first reader to import a module would leave the cache stale for
 * whichever panel opened before that import ran. A test calls it too, after
 * `resetSseForTests` forgets it.
 */
export function installFsEventRoute(): void {
  setFsEventRoute(routeFsEvent);
}
