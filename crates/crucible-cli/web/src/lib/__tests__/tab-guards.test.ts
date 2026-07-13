import { describe, it, expect, vi, beforeEach } from 'vitest';
import type { Tab } from '@/types/windowTypes';
import { confirmTabClose } from '../tab-guards';

const tab = (overrides: Partial<Tab> = {}): Tab => ({
  id: 'tab-1',
  title: 'note.md',
  contentType: 'file',
  ...overrides,
});

describe('confirmTabClose — unsaved-changes guard (bug 6)', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('allows closing an unmodified tab without prompting', () => {
    const confirm = vi.spyOn(window, 'confirm');
    expect(confirmTabClose(tab())).toBe(true);
    expect(confirmTabClose(tab({ isModified: false }))).toBe(true);
    expect(confirm).not.toHaveBeenCalled();
  });

  it('prompts for a modified tab and blocks close on cancel', () => {
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(false);
    expect(confirmTabClose(tab({ isModified: true }))).toBe(false);
    expect(confirm).toHaveBeenCalledOnce();
  });

  it('allows closing a modified tab when the user confirms', () => {
    vi.spyOn(window, 'confirm').mockReturnValue(true);
    expect(confirmTabClose(tab({ isModified: true }))).toBe(true);
  });
});
