import { createSignal } from 'solid-js';
import { listKilns } from '@/lib/api';
import { swrLocal } from '@/lib/local-cache';
import { kilnNameForPath, kilnPathForName } from '@/lib/kiln-registry';
import type { KilnListEntry } from '@/lib/types';

/**
 * The kiln registry, fetched once for the whole shell.
 *
 * A kiln crosses the wire as a name; a directory is what the note, file, graph
 * and skills endpoints take. Every surface that has one and needs the other
 * used to be free to invent its own join — and a per-message component (a
 * transcript's `data-kiln`) cannot afford a fetch of its own anyway. One
 * module-level signal, one fetch, one answer.
 *
 * Reads are reactive: a component that resolves a name before `GET /api/kilns`
 * answers gets `null` (which every caller treats as "no kiln", never as "every
 * kiln"), and re-renders with the directory once the list lands.
 */
const [kilns, setKilns] = createSignal<KilnListEntry[]>([]);
let started = false;

function ensureLoaded(): void {
  if (started) return;
  started = true;
  // Same last-known-value cache the panels use, under the same key, so the
  // first paint after a reload already has the registry.
  swrLocal('kilns', listKilns, setKilns);
}

/**
 * The directory a kiln name points at, or `null` — no such name, or the
 * registry has not answered yet.
 *
 * `null` means "no root": callers skip the query rather than falling back to a
 * wider one. Nothing here ever returns a directory for a name the daemon did
 * not issue.
 */
export function kilnPathOf(name: string | null | undefined): string | null {
  ensureLoaded();
  return kilnPathForName(name, kilns());
}

/** The registry name of a directory, or `null` when no entry claims it. */
export function kilnNameOf(path: string | null | undefined): string | null {
  ensureLoaded();
  return kilnNameForPath(path, kilns());
}
