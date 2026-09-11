import { describe, it, expect, vi, beforeEach } from 'vitest';

const { load, autosave } = vi.hoisted(() => ({
  load: vi.fn().mockResolvedValue(undefined),
  autosave: vi.fn(),
}));
vi.mock('@/lib/layout-persistence', () => ({
  loadLayoutOnStartup: load,
  setupLayoutAutoSave: autosave,
}));

import { startLayoutPersistence } from '@/lib/shell-boot';

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
