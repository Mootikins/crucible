import '@testing-library/jest-dom';

// Pure Node suites do not need browser shims.
if (typeof document !== 'undefined') {
  // jsdom doesn't provide ResizeObserver — stub it for component tests
  if (typeof globalThis.ResizeObserver === 'undefined') {
    globalThis.ResizeObserver = class ResizeObserver {
      observe() {}
      unobserve() {}
      disconnect() {}
    } as any;
  }

  // jsdom doesn't implement scrollIntoView — cmdk-solid calls it on every
  // selection change to keep the active item visible. Stub it so tests
  // don't throw.
  if (typeof Element !== 'undefined' && !Element.prototype.scrollIntoView) {
    Element.prototype.scrollIntoView = function () {};
  }

  // Node 24 installs its own `localStorage` accessor on the global. It is
  // UNAVAILABLE without `--localstorage-file`, so it warns once and answers
  // `undefined`. Vitest's jsdom environment merges the window INTO that global,
  // where Node's accessor wins — `localStorage.clear()` in a test then threw
  // "Cannot read properties of undefined" with no storage reachable at all.
  //
  // Define a real in-memory Storage over it. Tests need working persistence,
  // not a mock: several read back what the code under test wrote.
  if (typeof globalThis.localStorage === 'undefined') {
    class MemoryStorage implements Storage {
      #items = new Map<string, string>();
      get length(): number {
        return this.#items.size;
      }
      key(index: number): string | null {
        return [...this.#items.keys()][index] ?? null;
      }
      getItem(key: string): string | null {
        return this.#items.get(String(key)) ?? null;
      }
      setItem(key: string, value: string): void {
        this.#items.set(String(key), String(value));
      }
      removeItem(key: string): void {
        this.#items.delete(String(key));
      }
      clear(): void {
        this.#items.clear();
      }
      [name: string]: any;
    }
    for (const name of ['localStorage', 'sessionStorage']) {
      Object.defineProperty(globalThis, name, {
        value: new MemoryStorage(),
        configurable: true,
        writable: true,
      });
    }
  }
}
