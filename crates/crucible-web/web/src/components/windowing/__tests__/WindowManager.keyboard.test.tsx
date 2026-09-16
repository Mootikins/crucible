import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render } from '@solidjs/testing-library';
import { configureWindowing, windowStore } from '@/windowing/store';
import { LAYOUT_SHORTCUTS } from '@/windowing/shortcuts';
import { stubPolicy } from '@/windowing/__tests__/stubPolicy';
import { WindowManager } from '../WindowManager';

/**
 * The keyboard loop matches the policy's chord table. It runs the layout
 * chords itself, and hands every other chord to the policy. The browser
 * default goes off only for a chord that one of the two consumed.
 */
const onShortcut = vi.fn((action: string) => action === 'appConsumes');

beforeEach(() => {
  onShortcut.mockClear();
  configureWindowing(
    stubPolicy({
      shortcuts: [
        ...LAYOUT_SHORTCUTS,
        { key: 'k', modifiers: ['ctrl'], action: 'appConsumes', description: 'consumed' },
        { key: 'j', modifiers: ['ctrl'], action: 'appIgnores', description: 'ignored' },
      ],
      onShortcut,
    }),
  );
  render(() => <WindowManager renderContent={(tab) => <span>{tab().title}</span>} slots={{}} />);
});

const press = (key: string) => {
  const e = new KeyboardEvent('keydown', { key, ctrlKey: true, bubbles: true, cancelable: true });
  document.body.dispatchEvent(e);
  return e;
};

describe('WindowManager keyboard loop', () => {
  it('runs a layout chord itself', () => {
    const before = windowStore.edgePanels.left.mode;
    const e = press('b');
    expect(windowStore.edgePanels.left.mode).not.toBe(before);
    expect(e.defaultPrevented).toBe(true);
    expect(onShortcut).not.toHaveBeenCalled();
  });

  it('hands an app chord to the policy, and keeps the default off when the policy consumes it', () => {
    const e = press('k');
    expect(onShortcut).toHaveBeenCalledWith('appConsumes', e);
    expect(e.defaultPrevented).toBe(true);
  });

  it('leaves the default on when the policy does not consume the chord', () => {
    const e = press('j');
    expect(onShortcut).toHaveBeenCalledWith('appIgnores', e);
    expect(e.defaultPrevented).toBe(false);
  });

  it('ignores a chord outside the table', () => {
    const e = press('q');
    expect(onShortcut).not.toHaveBeenCalled();
    expect(e.defaultPrevented).toBe(false);
  });
});
