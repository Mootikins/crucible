import { describe, it, expect, beforeEach, vi } from 'vitest';
import { createRoot } from 'solid-js';
import { useOverflowDropdown } from '../useOverflowDropdown';
import type { OverflowDropdownResult } from '../useOverflowDropdown';

describe('useOverflowDropdown', () => {
  describe('dropdown state management', () => {
    let result: OverflowDropdownResult;
    let dispose: () => void;

    beforeEach(() => {
      createRoot((d) => {
        dispose = d;
        result = useOverflowDropdown({
          containerRef: () => undefined,
          deps: () => null,
        });
      });
    });

    it('showDropdown starts false', () => {
      expect(result.showDropdown()).toBe(false);
      dispose();
    });

    it('toggleDropdown flips state to true', () => {
      result.toggleDropdown();
      expect(result.showDropdown()).toBe(true);
      dispose();
    });

    it('toggleDropdown flips state back to false', () => {
      result.toggleDropdown();
      result.toggleDropdown();
      expect(result.showDropdown()).toBe(false);
      dispose();
    });

    it('setShowDropdown sets state directly to true', () => {
      result.setShowDropdown(true);
      expect(result.showDropdown()).toBe(true);
      dispose();
    });

    it('setShowDropdown sets state directly to false', () => {
      result.setShowDropdown(true);
      result.setShowDropdown(false);
      expect(result.showDropdown()).toBe(false);
      dispose();
    });

    it('setShowDropdown(true) is idempotent', () => {
      result.setShowDropdown(true);
      result.setShowDropdown(true);
      expect(result.showDropdown()).toBe(true);
      dispose();
    });

    it('isOverflowing starts false when no container', () => {
      expect(result.isOverflowing()).toBe(false);
      dispose();
    });
  });

  describe('overflow detection', () => {
    function flushMicrotasks(): Promise<void> {
      return new Promise((resolve) => queueMicrotask(resolve));
    }

    it('detects overflow when scrollWidth > clientWidth', async () => {
      const container = document.createElement('div');
      Object.defineProperty(container, 'scrollWidth', { value: 500, configurable: true });
      Object.defineProperty(container, 'clientWidth', { value: 300, configurable: true });

      let result!: OverflowDropdownResult;
      let dispose!: () => void;

      createRoot((d) => {
        dispose = d;
        result = useOverflowDropdown({
          containerRef: () => container,
          deps: () => null,
        });
      });

      await flushMicrotasks();
      expect(result.isOverflowing()).toBe(true);
      dispose();
    });

    it('does not detect overflow when scrollWidth <= clientWidth', async () => {
      const container = document.createElement('div');
      Object.defineProperty(container, 'scrollWidth', { value: 200, configurable: true });
      Object.defineProperty(container, 'clientWidth', { value: 300, configurable: true });

      let result!: OverflowDropdownResult;
      let dispose!: () => void;

      createRoot((d) => {
        dispose = d;
        result = useOverflowDropdown({
          containerRef: () => container,
          deps: () => null,
        });
      });

      await flushMicrotasks();
      expect(result.isOverflowing()).toBe(false);
      dispose();
    });

    it('does not detect overflow when scrollWidth equals clientWidth', async () => {
      const container = document.createElement('div');
      Object.defineProperty(container, 'scrollWidth', { value: 300, configurable: true });
      Object.defineProperty(container, 'clientWidth', { value: 300, configurable: true });

      let result!: OverflowDropdownResult;
      let dispose!: () => void;

      createRoot((d) => {
        dispose = d;
        result = useOverflowDropdown({
          containerRef: () => container,
          deps: () => null,
        });
      });

      await flushMicrotasks();
      expect(result.isOverflowing()).toBe(false);
      dispose();
    });
  });

  describe('ResizeObserver integration', () => {
    it('creates a ResizeObserver and observes the container', async () => {
      function flushMicrotasks(): Promise<void> {
        return new Promise((resolve) => queueMicrotask(resolve));
      }

      const observeSpy = vi.fn();
      const disconnectSpy = vi.fn();

      const OriginalResizeObserver = globalThis.ResizeObserver;
      globalThis.ResizeObserver = class MockResizeObserver {
        observe = observeSpy;
        unobserve = vi.fn();
        disconnect = disconnectSpy;
        constructor(public callback: ResizeObserverCallback) {}
      } as any;

      const container = document.createElement('div');
      Object.defineProperty(container, 'scrollWidth', { value: 100, configurable: true });
      Object.defineProperty(container, 'clientWidth', { value: 100, configurable: true });

      let dispose!: () => void;
      createRoot((d) => {
        dispose = d;
        useOverflowDropdown({
          containerRef: () => container,
          deps: () => null,
        });
      });

      await flushMicrotasks();
      expect(observeSpy).toHaveBeenCalledWith(container);
      dispose();

      globalThis.ResizeObserver = OriginalResizeObserver;
    });
  });
});
