import { describe, it, expect, vi, beforeEach } from 'vitest';
import { createNavStack } from '@/components/mobile/NavStack';

/**
 * A window with a real little history: a list of entry states and a cursor.
 * `pressBack` is the user's back button; `firePop` delivers the popstate that
 * a `history.back()` call causes, which a browser sends a moment later.
 */
function fakeWindow(initialState: unknown = null) {
  const target = new EventTarget();
  const entries: unknown[] = [initialState];
  let cursor = 0;
  const history = {
    get state() {
      return entries[cursor];
    },
    pushState: vi.fn((state: unknown) => {
      entries.splice(cursor + 1);
      entries.push(state);
      cursor += 1;
    }),
    back: vi.fn(() => {
      cursor = Math.max(0, cursor - 1);
    }),
  };
  const firePop = () =>
    target.dispatchEvent(new PopStateEvent('popstate', { state: entries[cursor] as object }));
  return {
    win: Object.assign(target, { history }) as unknown as Window,
    history,
    firePop,
    pressBack: () => {
      cursor = Math.max(0, cursor - 1);
      firePop();
    },
  };
}

let fake: ReturnType<typeof fakeWindow>;
beforeEach(() => {
  fake = fakeWindow();
});

describe('createNavStack', () => {
  // A hash deep link (`/#note=…`) must survive the first layer a user opens.
  it('pushes a history entry with NO url argument, so the url and its hash stay', () => {
    const nav = createNavStack(fake.win);
    nav.push(() => {});
    expect(fake.history.pushState).toHaveBeenCalledTimes(1);
    expect(fake.history.pushState.mock.calls[0]).toHaveLength(2);
  });

  it('runs the top layer when the user presses back', () => {
    const nav = createNavStack(fake.win);
    const lower = vi.fn();
    const upper = vi.fn();
    nav.push(lower);
    nav.push(upper);
    fake.pressBack();
    expect(upper).toHaveBeenCalledTimes(1);
    expect(lower).not.toHaveBeenCalled();
    fake.pressBack();
    expect(lower).toHaveBeenCalledTimes(1);
  });

  it('does nothing on back once every layer is gone, so the browser can leave', () => {
    const nav = createNavStack(fake.win);
    const layer = vi.fn();
    nav.push(layer);
    fake.pressBack();
    fake.pressBack();
    expect(layer).toHaveBeenCalledTimes(1);
  });

  // A drawer closed by a scrim tap must take its history entry with it, or the
  // next back press would close a drawer that is already closed.
  it('removes its own entry when a layer closes some other way', () => {
    const nav = createNavStack(fake.win);
    const layer = vi.fn();
    const release = nav.push(layer);
    release();
    expect(fake.history.back).toHaveBeenCalledTimes(1);
    fake.firePop(); // the popstate that history.back() causes
    expect(layer).not.toHaveBeenCalled();
  });

  // The case the self-inflicted counter exists for: a sheet over a drawer
  // closes itself. Its history.back() fires popstate, and that popstate must
  // not close the drawer underneath.
  it('does not let a self-closed upper layer close the layer beneath it', () => {
    const nav = createNavStack(fake.win);
    const drawer = vi.fn();
    nav.push(drawer);
    const releaseSheet = nav.push(() => {});
    releaseSheet();
    fake.firePop(); // the popstate that history.back() causes
    expect(drawer).not.toHaveBeenCalled();
    fake.pressBack(); // a real back press
    expect(drawer).toHaveBeenCalledTimes(1);
  });

  // The failure a counter of self-caused popstates has: a history.back() whose
  // popstate never arrives must not swallow the NEXT real back press.
  it('still honours a real back press after a self-release yields no popstate', () => {
    const nav = createNavStack(fake.win);
    const drawer = vi.fn();
    nav.push(drawer);
    const releaseSheet = nav.push(() => {});
    releaseSheet(); // its popstate is never delivered
    fake.pressBack();
    expect(drawer).toHaveBeenCalledTimes(1);
  });

  // A reload keeps this tab's history, ids and all. A new layer must not
  // reuse an id an old entry still carries.
  it('numbers new layers above an id a reload left in history', () => {
    fake = fakeWindow({ crucibleNav: 7 });
    const nav = createNavStack(fake.win);
    const layer = vi.fn();
    nav.push(layer);
    expect(fake.history.pushState.mock.calls[0][0]).toEqual({ crucibleNav: 8 });
    fake.pressBack(); // lands on the stale entry, id 7
    expect(layer).toHaveBeenCalledTimes(1);
  });

  it('ignores a release that comes after back already ran the layer', () => {
    const nav = createNavStack(fake.win);
    const release = nav.push(() => {});
    fake.pressBack();
    release();
    expect(fake.history.back).not.toHaveBeenCalled();
  });

  it('stops listening when disposed', () => {
    const nav = createNavStack(fake.win);
    const layer = vi.fn();
    nav.push(layer);
    nav.dispose();
    fake.pressBack();
    expect(layer).not.toHaveBeenCalled();
  });
});
