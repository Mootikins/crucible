import { describe, it, expect, beforeEach } from 'vitest';
import { getGlobalRegistry, resetGlobalRegistry } from '../panel-registry';
import { registerPanels } from '../register-panels';

describe('registerPanels', () => {
  beforeEach(() => {
    resetGlobalRegistry();
    registerPanels();
  });

  it('registers the files panel (left zone) — the editor entry point', () => {
    const def = getGlobalRegistry().get('files');
    expect(def).toBeDefined();
    expect(def?.title).toBe('Files');
    expect(def?.defaultZone).toBe('left');
  });

  it('registers every panel openPanelTab is wired to', () => {
    const registry = getGlobalRegistry();
    expect(registry.get('settings')?.defaultZone).toBe('center');
    expect(registry.get('plugins')?.defaultZone).toBe('left');
    expect(registry.get('skills')?.defaultZone).toBe('left');
  });
});
