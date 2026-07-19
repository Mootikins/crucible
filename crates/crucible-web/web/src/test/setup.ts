import '@testing-library/jest-dom';

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
