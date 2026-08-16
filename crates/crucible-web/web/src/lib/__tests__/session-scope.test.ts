import { describe, it, expect } from 'vitest';
import { sessionDefaultKiln, sessionHasWorkspace } from '@/lib/session-scope';

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

describe('sessionHasWorkspace', () => {
  it('is false when the workspace is the kilns[0] sentinel', () => {
    expect(sessionHasWorkspace({ kilns: ['/kilns/main'], workspace: '/kilns/main' })).toBe(false);
  });

  it('is true when the workspace is a real project directory', () => {
    expect(sessionHasWorkspace({ kilns: ['/kilns/main'], workspace: '/repos/crucible' })).toBe(
      true,
    );
  });

  // A kiln-less session has no kilns[0] to compare against, and the daemon
  // writes PathBuf::default() — the empty string — as its workspace. Neither
  // is a project, so neither may read as one.
  it('is false for a kiln-less session with the empty-path workspace', () => {
    expect(sessionHasWorkspace({ kilns: [], workspace: '' })).toBe(false);
  });

  it('is true for a kiln-less session that does have a project', () => {
    expect(sessionHasWorkspace({ kilns: [], workspace: '/repos/crucible' })).toBe(true);
  });

  it('is false when the payload carries no kilns field at all', () => {
    expect(
      sessionHasWorkspace({ kilns: undefined as unknown as string[], workspace: '' }),
    ).toBe(false);
  });
});
