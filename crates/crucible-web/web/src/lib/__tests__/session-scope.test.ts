import { describe, it, expect } from 'vitest';
import { sessionDefaultKiln, sessionWorkspace } from '@/lib/session-scope';
import type { Session } from '@/lib/types';

/** A whole session scope, so the kiln set is visible even where it no longer counts. */
const scope = (kilns: string[], workspace: string | null): Pick<Session, 'kilns' | 'workspace'> => ({
  kilns,
  workspace,
});

describe('sessionDefaultKiln', () => {
  it('names the first attached kiln', () => {
    expect(sessionDefaultKiln({ kilns: ['/kilns/main', '/kilns/extra'] })).toBe('/kilns/main');
  });

  // Zero kilns is a legitimate session shape (a tools-only agent), so the
  // label has to be *absent* rather than a stand-in path — '' reaches
  // kilnLabel() as the home data dir and renders "Home kiln", inventing an
  // attachment the session does not have.
  it('is null for a kiln-less session rather than an empty path', () => {
    expect(sessionDefaultKiln({ kilns: [] })).toBeNull();
  });

  it('is null when the payload carries no kilns field at all', () => {
    expect(sessionDefaultKiln({ kilns: undefined as unknown as string[] })).toBeNull();
  });
});

describe('sessionWorkspace', () => {
  it('names the workspace the daemon reported', () => {
    expect(sessionWorkspace({ workspace: '/repos/crucible' })).toBe('/repos/crucible');
  });

  // The daemon answers `null` for a session with no workspace. Nothing else
  // may be read as one — an empty string reaches pathBasename() and the
  // grouping key as a real directory.
  it('is null when the daemon says the session has no workspace', () => {
    expect(sessionWorkspace({ workspace: null })).toBeNull();
    expect(sessionWorkspace({ workspace: '' })).toBeNull();
    expect(sessionWorkspace({ workspace: undefined as unknown as string })).toBeNull();
  });
});

describe('sessionWorkspace and the kiln set', () => {
  // The daemon used to write `workspace = kilns[0]` as its "no workspace"
  // sentinel, and this side kept an INDEPENDENT copy of that rule — nothing
  // failed to compile when the two drifted. The kiln set no longer enters into
  // it at all: a workspace that happens to EQUAL the kiln is one the user
  // chose, and must render as one.
  it('is the workspace even when it equals the first kiln', () => {
    expect(sessionWorkspace(scope(['/kilns/main'], '/kilns/main'))).toBe('/kilns/main');
  });

  it('is null whatever the kiln set says, when the daemon reports none', () => {
    expect(sessionWorkspace(scope(['/kilns/main'], null))).toBeNull();
    expect(sessionWorkspace(scope([], null))).toBeNull();
  });

  it('is the project of a kiln-less session that has one', () => {
    expect(sessionWorkspace(scope([], '/repos/crucible'))).toBe('/repos/crucible');
  });
});
