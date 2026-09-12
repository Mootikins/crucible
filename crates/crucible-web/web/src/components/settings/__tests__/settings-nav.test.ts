import { describe, it, expect } from 'vitest';
import { createEffect, createRoot } from 'solid-js';
import { createSettingsStack } from '@/components/settings/settings-nav';
import type { NavStack } from '@/components/mobile/NavStack';
import type { SettingsPage } from '@/components/settings/settings-nav';

/**
 * A back stack that records what it was asked, and can fire a landing the way
 * the browser does — every layer above the one landed on closes at once.
 */
function fakeBack() {
  const layers: { id: number; onBack: () => void }[] = [];
  const dropped: number[] = [];
  let seq = 0;
  const stack: NavStack = {
    push(onBack) {
      const layer = { id: ++seq, onBack };
      layers.push(layer);
      return () => {
        const at = layers.indexOf(layer);
        if (at !== -1) layers.splice(at, 1);
      };
    },
    dropTop(n: number) {
      const count = Math.min(n, layers.length);
      if (count > 0) layers.splice(layers.length - count, count);
      dropped.push(n);
    },
    dispose() {
      layers.length = 0;
    },
  };
  return {
    stack,
    dropped,
    depth: () => layers.length,
    /** The phone's back button, landing on `id` (0 = below everything). */
    land(id: number) {
      while (layers.length > 0 && layers[layers.length - 1].id > id) layers.pop()!.onBack();
    },
  };
}

const page = (id: string): SettingsPage => ({ id, title: id, body: () => null });

describe('the settings drill-down', () => {
  it('starts at the root, with nothing above it', () =>
    createRoot((dispose) => {
      const nav = createSettingsStack(fakeBack().stack);
      expect(nav.pages()).toEqual([]);
      expect(nav.pop()).toBe(false);
      dispose();
    }));

  it('stacks pages in the order they were opened', () =>
    createRoot((dispose) => {
      const nav = createSettingsStack(fakeBack().stack);
      nav.push(page('a'));
      nav.push(page('b'));
      expect(nav.pages().map((p) => p.id)).toEqual(['a', 'b']);
      dispose();
    }));

  it('gives a history entry back when its page is popped', () =>
    createRoot((dispose) => {
      const back = fakeBack();
      const nav = createSettingsStack(back.stack);
      nav.push(page('a'));
      nav.push(page('b'));
      expect(back.depth()).toBe(2);

      expect(nav.pop()).toBe(true);
      expect(nav.pages().map((p) => p.id)).toEqual(['a']);
      expect(back.depth()).toBe(1);
      dispose();
    }));

  // The phone's own back button, not the bar's control.
  it('closes the page the browser left behind', () =>
    createRoot((dispose) => {
      const back = fakeBack();
      const nav = createSettingsStack(back.stack);
      nav.push(page('a'));
      nav.push(page('b'));

      back.land(1);
      expect(nav.pages().map((p) => p.id)).toEqual(['a']);
      dispose();
    }));

  // A back GESTURE can skip levels. `NavStack` delivers that as one `onBack`
  // per layer, so this pins the RESULT — no page outlives its history entry —
  // rather than how the slice is computed.
  it('closes every page above the one the browser landed on', () =>
    createRoot((dispose) => {
      const back = fakeBack();
      const nav = createSettingsStack(back.stack);
      nav.push(page('a'));
      nav.push(page('b'));
      nav.push(page('c'));

      back.land(0);
      expect(nav.pages()).toEqual([]);
      dispose();
    }));

  // Closing the dialog from three levels deep must not leave three entries
  // behind, or the next back press walks a dialog that is gone.
  it('gives every history entry back when the dialog closes', () =>
    createRoot((dispose) => {
      const back = fakeBack();
      const nav = createSettingsStack(back.stack);
      nav.push(page('a'));
      nav.push(page('b'));
      nav.push(page('c'));

      nav.reset();
      expect(nav.pages()).toEqual([]);
      expect(back.depth()).toBe(0);
      dispose();
    }));

  it('does not pop twice when the browser already closed a page', () =>
    createRoot((dispose) => {
      const back = fakeBack();
      const nav = createSettingsStack(back.stack);
      nav.push(page('a'));
      nav.push(page('b'));

      back.land(1); // browser closed 'b'
      expect(nav.pop()).toBe(true); // the bar's control closes 'a'
      expect(nav.pages()).toEqual([]);
      expect(back.depth()).toBe(0);
      dispose();
    }));

  it('is reactive, so the shell redraws when a page opens', async () => {
    const seen: string[] = [];
    let nav!: ReturnType<typeof createSettingsStack>;
    const dispose = createRoot((d) => {
      nav = createSettingsStack(fakeBack().stack);
      // Inside a real computation, and AWAITED. The earlier version called
      // the getter directly, outside any tracking scope, and asserted inside
      // a nested microtask that never ran — so it survived wrapping the read
      // in `untrack` twice over.
      createEffect(() => seen.push(nav.pages().map((p) => p.title).join('/')));
      return d;
    });
    await Promise.resolve();
    nav.push(page('a'));
    await Promise.resolve();
    expect(seen, 'the shell must re-render when a page is pushed').toEqual(['', 'a']);
    dispose();
  });

  // A section that closes the whole dialog from depth must give every entry
  // back. The jump to the line that pins a setting does exactly this.
  it('gives the entries back in ONE traversal, not one call per level', () =>
    createRoot((dispose) => {
      const back = fakeBack();
      const nav = createSettingsStack(back.stack);
      nav.push(page('a'));
      nav.push(page('b'));
      nav.push(page('c'));

      nav.reset();
      expect(back.dropped, 'one go(-n), not n back() calls').toEqual([3]);
      expect(back.depth()).toBe(0);
      dispose();
    }));
});
