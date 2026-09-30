import type { components, operations } from '../api-schema';
import { useQuery, type QueryClient } from '@tanstack/solid-query';
import type { Accessor } from 'solid-js';
import type { KilnListEntry } from '@/lib/types';
import { getQueryClient } from './client';
import { keys } from './keys';
import { isUnder } from './notes';
import { queryBase, writeBaseProperty, createBaseEntry, reorderBaseGroups } from '../api';

type Schemas = components['schemas'];
/** The document's own query parameters of `GET /api/bases/query`. */
type BaseQueryParams = NonNullable<operations['query_base']['parameters']['query']>;

export type BaseValue = Schemas['BaseValue'];
/** `movable`: the daemon says whether a drag can move this row to another group. */
export type BaseRow = Schemas['Row'];
/**
 * `write_value`: the exact JSON that `set_property` writes to put a row in
 * this group. Null for the group with no value, which deletes the property.
 */
export type BaseGroup = Schemas['Group'];
export type BaseResult = Schemas['QueryResult'];
export type SetPropertyParams = Schemas['SetPropertyParams'];
export type CreateEntryParams = Schemas['CreateEntryParams'];
export type ReorderGroupsParams = Schemas['ReorderGroupsParams'];
/** What a write did. A stale hash and a refusal arrive as HTTP errors (409, 403 or 422). */
export type WriteOutcome = Schemas['WriteOutcome'];

/**
 * What one base query asks for.
 *
 * Client-local: the route (`GET /api/bases/query`) takes `path`/`yaml` as two
 * separate OPTIONAL query parameters — a shape the design rules forbid here
 * ("no field made optional to merge shapes") — because exactly one of them is
 * ever present. `source` keeps that as a discriminated union instead, so a
 * caller cannot build a request naming neither or both. Each field's type
 * still comes from the document's own query parameters, so a rename there
 * fails `tsc` here.
 */
export interface BaseRequest {
  kiln: BaseQueryParams['kiln'];
  source: { path: NonNullable<BaseQueryParams['path']> } | { yaml: NonNullable<BaseQueryParams['yaml']> };
  view?: BaseQueryParams['view'];
  this?: BaseQueryParams['this'];
}
export function useBase(request: Accessor<BaseRequest | null>) {
  return useQuery(() => ({ queryKey: keys.baseQuery(request()), enabled: request() !== null,
    queryFn: () => queryBase(request()!), }), getQueryClient);
}

/**
 * Sends one base write, then marks every base query stale. A write changes a
 * note, and any open base can select that note. A refused write marks them
 * stale too: a stale hash (409) or a note that is gone (404) means the view
 * shows old data.
 */
async function baseWrite<A>(send: (request: A) => Promise<WriteOutcome>, request: A): Promise<WriteOutcome> {
  try {
    return await send(request);
  } finally {
    await getQueryClient().invalidateQueries({ queryKey: keys.bases() });
  }
}
export const setBaseProperty = (request: SetPropertyParams) => baseWrite(writeBaseProperty, request);
export const newBaseEntry = (request: CreateEntryParams) => baseWrite(createBaseEntry, request);
export const setBaseGroupOrder = (request: ReorderGroupsParams) => baseWrite(reorderBaseGroups, request);

/** A burst of file events (a save, a git checkout) refreshes each base once. */
const BASE_REFRESH_DELAY_MS = 200;

interface PendingBaseRefresh {
  /** The kiln names to refresh, or `all` when a path could not be placed. */
  kilns: Set<string> | 'all';
  timer: ReturnType<typeof setTimeout>;
}
const pendingRefresh = new WeakMap<QueryClient, PendingBaseRefresh>();

/**
 * The kiln names whose bases a change of `paths` can make stale, or `all`
 * when the kiln roster is not loaded yet. A base selects only notes of its
 * own kiln, so a change in another kiln leaves it alone.
 */
function kilnsOf(client: QueryClient, paths: readonly string[]): Set<string> | 'all' {
  const roster = client.getQueryData<KilnListEntry[]>(keys.kilns());
  if (!roster) return 'all';
  return new Set(roster.filter(kiln => paths.some(path => isUnder(path, kiln.path))).map(kiln => kiln.name));
}

/**
 * Marks stale the base queries of each kiln that holds one of `paths`. The
 * refresh waits for a short quiet time, so a burst of events costs one
 * query per open base.
 */
export function invalidateBasesUnder(client: QueryClient, paths: readonly string[]): void {
  if (paths.length === 0) return;
  const kilns = kilnsOf(client, paths);
  if (kilns !== 'all' && kilns.size === 0) return;
  const pending = pendingRefresh.get(client);
  if (pending) {
    if (pending.kilns !== 'all') {
      if (kilns === 'all') pending.kilns = 'all';
      else for (const kiln of kilns) pending.kilns.add(kiln);
    }
    return;
  }
  const entry: PendingBaseRefresh = {
    kilns,
    timer: setTimeout(() => {
      pendingRefresh.delete(client);
      const names = entry.kilns;
      void client.invalidateQueries({
        queryKey: keys.bases(),
        predicate: query => {
          if (names === 'all') return true;
          const request = query.queryKey[1] as Partial<BaseRequest> | undefined;
          return typeof request?.kiln === 'string' && names.has(request.kiln);
        },
      });
    }, BASE_REFRESH_DELAY_MS),
  };
  pendingRefresh.set(client, entry);
}

export function baseText(value: BaseValue | undefined): string {
  if (!value || value.type === 'null') return '';
  switch (value.type) {
    case 'list': return value.value.map(baseText).join(', ');
    case 'link': return value.value.display ?? value.value.path;
    case 'regexp': return `/${value.value.pattern}/${value.value.flags}`;
    // The daemon words a duration (`value.text`); only a relative date must
    // update while the view is open, so only it is worded here.
    case 'duration': return value.value.text;
    case 'relativedate': {
      const delta = value.value - Date.now();
      const text = durationText(delta);
      return delta > 0 ? `in ${text}` : `${text} ago`;
    }
    case 'dateonly': return value.value;
    case 'date': return new Date(value.value).toLocaleString();
    case 'object': return JSON.stringify(value.value);
    case 'error': return `Error: ${value.value}`;
    default: return String(value.value ?? '');
  }
}

/** A short message for a refused base write, by HTTP status. */
export function baseWriteError(error: unknown): string {
  const message = error instanceof Error ? error.message : String(error);
  switch ((error as { status?: number } | null)?.status) {
    case 409: return `The entry changed after the view loaded. The view is up to date again; try the change again. (${message})`;
    case 404: return `The note or the base is not there any more. The view is up to date again. (${message})`;
    case 403: return `The change was refused. (${message})`;
    case 422: return `The change is not valid. (${message})`;
    default: return message;
  }
}

/** A sentence for a write that the daemon did not apply as asked, or null for an applied write. */
export function baseOutcomeNotice(outcome: WriteOutcome): string | null {
  switch (outcome.status) {
    case 'applied': return null;
    case 'unchanged': return 'The note already had this value. Nothing changed.';
    case 'proposed': return 'The change waits for review in the Inbox.';
    case 'stale': return 'The entry changed after the view loaded. The view is up to date again; try the change again.';
    case 'refused': return `The change was refused: ${outcome.reason}`;
  }
}

function durationText(milliseconds: number): string {
  const seconds = Math.round(Math.abs(milliseconds) / 1000);
  const minutes = Math.round(seconds / 60), hours = Math.round(minutes / 60), days = Math.round(hours / 24);
  if (seconds < 45) return 'a few seconds';
  if (seconds < 90) return 'a minute';
  if (minutes < 45) return `${minutes} minutes`;
  if (minutes < 90) return 'an hour';
  if (hours < 22) return `${hours} hours`;
  if (hours < 36) return 'a day';
  if (days < 26) return `${days} days`;
  if (days < 46) return 'a month';
  if (days < 320) return `${Math.round(days / 30.436875)} months`;
  if (days < 548) return 'a year';
  return `${Math.round(days / 365.2425)} years`;
}
