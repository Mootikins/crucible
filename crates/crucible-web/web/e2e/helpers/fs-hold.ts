import type { Page } from '@playwright/test';

/** The page-side control that `holdFsEvents` installs. */
interface FsHold {
  hold(): void;
  release(): void;
  held(): number;
}

/**
 * Holds the kiln watcher's events in the page until the spec releases them.
 *
 * The watcher report of a write arrives some time after the write. A spec
 * that must put a keystroke before that report cannot win the race with a
 * sleep. It holds the stream instead, and releases it when the order is set.
 * Install it before `page.goto`: it wraps the listeners that the app adds to
 * the shared `/api/events` connection, and matches its `system` topic (the
 * one the filesystem watcher's frames travel on) — a topic parameter, not a
 * dedicated route, since Simplification Plan step 19 folded `/api/fs/events`
 * into the one stream.
 */
export async function holdFsEvents(page: Page): Promise<{
  hold(): Promise<void>;
  release(): Promise<void>;
  held(): Promise<number>;
}> {
  await page.addInitScript(() => {
    const queue: Array<() => void> = [];
    let holding = false;
    const control: FsHold = {
      hold: () => {
        holding = true;
      },
      release: () => {
        holding = false;
        for (const deliver of queue.splice(0)) deliver();
      },
      held: () => queue.length,
    };
    (window as unknown as { __fsHold: FsHold }).__fsHold = control;

    const add = EventSource.prototype.addEventListener;
    EventSource.prototype.addEventListener = function (
      this: EventSource,
      type: string,
      listener: EventListenerOrEventListenerObject | null,
      options?: boolean | AddEventListenerOptions,
    ) {
      // The shared connection's query names every topic it carries
      // (`?topics=a,b,...`); the filesystem watcher's frames always travel
      // the `system` topic, so a source that names it is the one to hold.
      const url = new URL(this.url, window.location.origin);
      const topics = (url.searchParams.get('topics') ?? '').split(',');
      if (url.pathname !== '/api/events' || !topics.includes('system') || typeof listener !== 'function') {
        return add.call(this, type, listener, options);
      }
      const held = function (this: EventSource, event: Event) {
        if (holding) queue.push(() => listener.call(this, event));
        else listener.call(this, event);
      };
      return add.call(this, type, held, options);
    } as typeof EventSource.prototype.addEventListener;
  });

  const call = <T>(name: keyof FsHold) =>
    page.evaluate((key) => (window as unknown as { __fsHold: FsHold }).__fsHold[key](), name) as Promise<T>;
  return {
    hold: () => call<void>('hold'),
    release: () => call<void>('release'),
    held: () => call<number>('held'),
  };
}
