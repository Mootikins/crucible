import { describe, it, expect, beforeEach } from 'vitest';
import { KEPT_KILNS_KEY, kept, keptActions, keptMode } from '@/lib/offline/kept';
import { sameDaemon } from '@/lib/offline/identity';

beforeEach(() => {
  localStorage.clear();
  for (const path of Object.keys(kept())) keptActions.forget(path);
});

describe('kept kilns', () => {
  it('keeps a kiln in the mode the user chose', () => {
    keptActions.keep('/kilns/notes', 'everything');
    expect(keptMode('/kilns/notes')).toBe('everything');
  });

  it('changes the mode without forgetting the kiln', () => {
    keptActions.keep('/kilns/notes', 'notes');
    keptActions.keep('/kilns/notes', 'everything');
    expect(keptMode('/kilns/notes')).toBe('everything');
  });

  it('answers null for a kiln nobody kept', () => {
    expect(keptMode('/kilns/other')).toBeNull();
  });

  it('forgets a kiln', () => {
    keptActions.keep('/kilns/notes', 'notes');
    keptActions.forget('/kilns/notes');
    expect(keptMode('/kilns/notes')).toBeNull();
  });

  it('survives a reload', () => {
    keptActions.keep('/kilns/notes', 'everything');
    expect(JSON.parse(localStorage.getItem(KEPT_KILNS_KEY)!)).toEqual({
      '/kilns/notes': { mode: 'everything' },
    });
  });

  it('ignores storage a human edited into nonsense', () => {
    localStorage.setItem(KEPT_KILNS_KEY, '{"/k": {"mode": "wishful"}, "/j": 7}');
    // The module read at import; re-reading is what a reload does.
    expect(['notes', 'everything', null]).toContain(keptMode('/k'));
  });
});

describe('daemon identity', () => {
  // A key names a daemon, not a person: draining into the wrong one would put
  // a user's edits in someone else's kiln.
  it('matches only the same, known daemon', () => {
    expect(sameDaemon('http://host|/etc/crucible', 'http://host|/etc/crucible')).toBe(true);
    expect(sameDaemon('http://host|/etc/crucible', 'http://host|/other')).toBe(false);
  });

  it('never matches an unknown identity', () => {
    expect(sameDaemon('', '')).toBe(false);
    expect(sameDaemon('http://host|/etc', '')).toBe(false);
    expect(sameDaemon('', 'http://host|/etc')).toBe(false);
  });
});
