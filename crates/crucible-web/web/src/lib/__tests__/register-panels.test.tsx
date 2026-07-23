import { describe, it, expect, beforeEach } from 'vitest';
import { getGlobalRegistry, resetGlobalRegistry } from '../panel-registry';
import { registerPanels } from '../register-panels';

describe('registerPanels', () => {
  beforeEach(() => {
    resetGlobalRegistry();
    registerPanels();
  });

  it('registers the navigator panel (left zone) — the unified entry point', () => {
    const def = getGlobalRegistry().get('navigator');
    expect(def).toBeDefined();
    expect(def?.title).toBe('Navigator');
    expect(def?.defaultZone).toBe('left');
  });

  it('retires the separate files/sessions/search left tabs', () => {
    const registry = getGlobalRegistry();
    expect(registry.get('files')).toBeUndefined();
    expect(registry.get('sessions')).toBeUndefined();
    expect(registry.get('search')).toBeUndefined();
  });

  it('registers every panel openPanelTab is wired to', () => {
    const registry = getGlobalRegistry();
    expect(registry.get('settings')?.defaultZone).toBe('center');
    expect(registry.get('plugins')?.defaultZone).toBe('left');
    expect(registry.get('skills')?.defaultZone).toBe('left');
  });
});
