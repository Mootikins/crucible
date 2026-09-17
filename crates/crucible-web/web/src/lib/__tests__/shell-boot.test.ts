import { describe, it, expect, vi, beforeEach } from 'vitest';

const { load, autosave } = vi.hoisted(() => ({
  load: vi.fn().mockResolvedValue(undefined),
  autosave: vi.fn(),
}));
vi.mock('@/lib/query/layout', () => ({
  loadLayoutOnStartup: load,
  setupLayoutAutoSave: autosave,
}));

import { markShell, startLayoutPersistence } from '@/lib/shell-boot';

beforeEach(() => {
  load.mockClear();
  autosave.mockClear();
});

describe('startLayoutPersistence', () => {
  it('loads and autosaves the layout on the desktop shell', () => {
    startLayoutPersistence({ compact: false });
    expect(load).toHaveBeenCalledTimes(1);
    expect(autosave).toHaveBeenCalledTimes(1);
  });

  // The layout lives on the daemon (POST /api/layout). A phone that saved it
  // would overwrite the layout of every desktop on that daemon.
  it('never touches the layout on the compact shell', () => {
    startLayoutPersistence({ compact: true });
    expect(load).not.toHaveBeenCalled();
    expect(autosave).not.toHaveBeenCalled();
  });
});

describe('markShell', () => {
  /**
   * The stylesheet must agree with the shell, not with the viewport.
   * `isCompact()` is decided once at load; a live `@media` query is not, so
   * narrowing a desktop window past 767 px used to apply the phone's settings
   * layout to the two-column dialog still rendering.
   */
  it('stamps the document when the compact shell is drawing', () => {
    const root = document.createElement('html');
    markShell(true, root);
    expect(root.hasAttribute('data-compact-shell')).toBe(true);
  });

  it('leaves the document unstamped for the desktop shell', () => {
    const root = document.createElement('html');
    markShell(true, root);
    markShell(false, root);
    expect(root.hasAttribute('data-compact-shell')).toBe(false);
  });
});

