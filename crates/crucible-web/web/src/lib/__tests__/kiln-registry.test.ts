import { describe, it, expect } from 'vitest';
import { attachableKilns, kilnNameForPath, kilnPathForName } from '@/lib/kiln-registry';
import type { KilnListEntry } from '@/lib/types';

const registry: KilnListEntry[] = [
  { path: '/home/u/vault', name: 'vault', last_access_secs_ago: null, open: true, registered: true },
  { path: '/home/u/work/notes', name: 'notes', last_access_secs_ago: null, open: true, registered: true },
];

describe('kilnPathForName', () => {
  it('resolves a registered name to its directory', () => {
    expect(kilnPathForName('vault', registry)).toBe('/home/u/vault');
    expect(kilnPathForName('notes', registry)).toBe('/home/u/work/notes');
  });

  // The load-bearing case. Every caller feeds this into a search root or a
  // note-resolution scope, and the empty string is a real directory to those:
  // it is the daemon data dir, so an unresolvable kiln would quietly widen to
  // everything under it. Nothing may resolve to a root here except a match.
  it('is null — never the empty string, never a fallback entry — for a name it has never issued', () => {
    expect(kilnPathForName('ghost', registry)).toBeNull();
    expect(kilnPathForName('', registry)).toBeNull();
    expect(kilnPathForName(null, registry)).toBeNull();
    expect(kilnPathForName(undefined, registry)).toBeNull();
  });

  // An empty registry is the state on first paint, before `GET /api/kilns`
  // answers. A resolver that treated "nothing registered" as "match anything"
  // would give the first render of every session the widest possible scope.
  it('is null for every name while the registry is empty', () => {
    expect(kilnPathForName('vault', [])).toBeNull();
  });

  // A path is not a name. A session written before names, or a stale client,
  // sends one — and `/home/u/vault` must not match the entry it happens to
  // name, or the join silently accepts the spelling the wire no longer uses.
  it('does not resolve a path used in place of a name', () => {
    expect(kilnPathForName('/home/u/vault', registry)).toBeNull();
  });
});

describe('kilnNameForPath', () => {
  it('names a registered directory, trailing slash or not', () => {
    expect(kilnNameForPath('/home/u/vault', registry)).toBe('vault');
    expect(kilnNameForPath('/home/u/vault/', registry)).toBe('vault');
  });

  it('is null for a directory no entry claims, rather than inventing a basename', () => {
    expect(kilnNameForPath('/home/u/elsewhere', registry)).toBeNull();
    expect(kilnNameForPath('', registry)).toBeNull();
    expect(kilnNameForPath(null, registry)).toBeNull();
  });

  // Prefix matching would credit `/home/u/vault-archive` to `vault`, and the
  // shell would then attach the user to a corpus they did not pick.
  it('matches the whole directory, not a prefix of it', () => {
    expect(kilnNameForPath('/home/u/vault-archive', registry)).toBeNull();
    expect(kilnNameForPath('/home/u/vault/sub', registry)).toBeNull();
  });
});

describe('attachableKilns', () => {
  // The client half of the daemon's own rule: `kiln.list` never publishes a
  // name the attach cannot resolve, and a row it cannot name says so with
  // `registered: false`. Offering such a row posts a name the daemon refuses
  // and shows the 422 as a toast, which reads as a broken picker.
  it('leaves out a row the daemon reports as unregistered', () => {
    const rows: KilnListEntry[] = [
      { path: '/home/u/vault', name: 'vault', registered: true, last_access_secs_ago: null, open: true },
      { path: '/home/u/.crucible/sessions', name: '', registered: false, last_access_secs_ago: null, open: true },
    ];

    expect(attachableKilns(rows).map((k) => k.name)).toEqual(['vault']);
  });

  // The flag is the authority, not the label. A daemon that sends a name
  // alongside `registered: false` is still describing a directory no session
  // can attach, and a picker that trusted the label would offer it.
  it('leaves out an unregistered row even when it carries a label', () => {
    const rows: KilnListEntry[] = [{ path: '/home/u/docs', name: 'docs', registered: false, last_access_secs_ago: null, open: true }];

    expect(attachableKilns(rows)).toEqual([]);
  });

  // An older daemon omits the field. Absent is not false: dropping every row
  // would empty the picker against a daemon that works.
  it('keeps a named row from a daemon that does not send the flag', () => {
    const rows: KilnListEntry[] = [{ path: '/home/u/vault', name: 'vault', last_access_secs_ago: null, open: true, registered: true }];

    expect(attachableKilns(rows).map((k) => k.name)).toEqual(['vault']);
  });

  // A row with no name cannot be attached whatever the flag says: there is
  // nothing to send.
  it('leaves out a row with no usable name', () => {
    const rows: KilnListEntry[] = [
      // The daemon sends an empty name for a row it cannot name; `name` is a
      // `String` on the wire, never null.
      { path: '/a', name: '', registered: true, last_access_secs_ago: null, open: true },
      { path: '/b', name: '   ', registered: true, last_access_secs_ago: null, open: true },
    ];

    expect(attachableKilns(rows)).toEqual([]);
  });
});
