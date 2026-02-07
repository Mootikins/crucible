import { vi } from 'vitest';

// NOTE(crucible): Jest compat shim for vitest migration.
// - `vi.fn()` returns non-constructable proxies; upstream tests use `new jest.fn()()`
// - vitest fake timers don't flush queueMicrotask; Jest's do
// - vitest lacks `runAllTicks`; Jest uses it for microtask queue

const _realQueueMicrotask = globalThis.queueMicrotask;
let _fakeTimersActive = false;

const jestCompat = Object.create(vi);

jestCompat.fn = (implementation?: (...args: any[]) => any) => {
    if (implementation) {
        const wrapper = function (this: any, ...args: any[]) {
            return implementation.apply(this, args);
        };
        return vi.fn(wrapper);
    }
    return vi.fn();
};

jestCompat.runAllTicks = () => {
    vi.runAllTimers();
};

const _origUseFakeTimers = vi.useFakeTimers.bind(vi);
const _origUseRealTimers = vi.useRealTimers.bind(vi);

jestCompat.useFakeTimers = (...args: any[]) => {
    _fakeTimersActive = true;
    return _origUseFakeTimers(...args);
};

jestCompat.useRealTimers = (...args: any[]) => {
    _fakeTimersActive = false;
    return _origUseRealTimers(...args);
};

globalThis.queueMicrotask = (callback: () => void) => {
    if (_fakeTimersActive) {
        setTimeout(callback, 0);
    } else {
        _realQueueMicrotask(callback);
    }
};

(globalThis as any).jest = jestCompat;
