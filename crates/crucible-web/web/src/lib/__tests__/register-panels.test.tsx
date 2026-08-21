import { describe, it, expect, beforeEach } from 'vitest';
import { getGlobalRegistry, resetGlobalRegistry } from '../panel-registry';
import { registerPanels } from '../register-panels';

describe('registerPanels', () => {
  beforeEach(() => {
    resetGlobalRegistry();
    registerPanels();
  });

  // Identity left, working context right. Sessions and the file tree must be
  // separate panels: as two scopes of one Navigator they could never be on
  // screen together, which is what made switching session context expensive.
  it('puts Sessions on the left', () => {
    const def = getGlobalRegistry().get('sessions');
    expect(def?.title).toBe('Sessions');
    expect(def?.defaultZone).toBe('left');
  });

  // Search spans files, notes and sessions, so a rail would claim a scope it
  // does not have — and a ~250px rail cannot show a path:line hit anyway.
  it('keeps Search off both rails', () => {
    const def = getGlobalRegistry().get('search');
    expect(def?.title).toBe('Search');
    expect(def?.defaultZone).toBe('center');
  });

  it('puts Files on the right, opposite the session list', () => {
    const def = getGlobalRegistry().get('files');
    expect(def?.title).toBe('Files');
    expect(def?.defaultZone).toBe('right');
  });

  it('retires the unified Navigator panel', () => {
    expect(getGlobalRegistry().get('navigator')).toBeUndefined();
  });

  it('registers every panel openPanelTab is wired to', () => {
    const registry = getGlobalRegistry();
    expect(registry.get('settings')?.defaultZone).toBe('center');
    expect(registry.get('plugins')?.defaultZone).toBe('left');
    expect(registry.get('skills')?.defaultZone).toBe('left');
  });
});
