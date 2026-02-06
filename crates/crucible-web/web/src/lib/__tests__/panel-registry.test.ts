import { describe, it, expect, beforeEach } from 'vitest';
import { PanelRegistry, getGlobalRegistry, resetGlobalRegistry } from '../panel-registry';

const StubComponent = () => null;
const AnotherStub = () => null;

describe('PanelRegistry', () => {
  let registry: PanelRegistry;

  beforeEach(() => {
    registry = new PanelRegistry();
  });

  it('registers and retrieves a panel by id', () => {
    registry.register('chat', 'Chat', StubComponent, 'center');
    const panel = registry.get('chat');
    expect(panel).toBeDefined();
    expect(panel!.id).toBe('chat');
    expect(panel!.title).toBe('Chat');
    expect(panel!.component).toBe(StubComponent);
    expect(panel!.defaultZone).toBe('center');
  });

  it('returns undefined for unknown panel id', () => {
    expect(registry.get('nonexistent')).toBeUndefined();
  });

  it('lists all registered panels', () => {
    registry.register('sessions', 'Sessions', StubComponent, 'left');
    registry.register('files', 'Files', AnotherStub, 'left');
    registry.register('chat', 'Chat', StubComponent, 'center');

    const panels = registry.list();
    expect(panels).toHaveLength(3);
    expect(panels.map(p => p.id)).toEqual(['sessions', 'files', 'chat']);
  });

  it('returns default layout grouped by zone', () => {
    registry.register('sessions', 'Sessions', StubComponent, 'left');
    registry.register('files', 'Files', AnotherStub, 'left');
    registry.register('chat', 'Chat', StubComponent, 'center');
    registry.register('editor', 'Editor', StubComponent, 'right');
    registry.register('terminal', 'Terminal', StubComponent, 'bottom');

    const layout = registry.getDefaultLayout();
    expect(layout.left).toEqual(['sessions', 'files']);
    expect(layout.center).toEqual(['chat']);
    expect(layout.right).toEqual(['editor']);
    expect(layout.bottom).toEqual(['terminal']);
  });

  it('returns empty arrays for zones with no panels', () => {
    registry.register('chat', 'Chat', StubComponent, 'center');

    const layout = registry.getDefaultLayout();
    expect(layout.left).toEqual([]);
    expect(layout.right).toEqual([]);
    expect(layout.bottom).toEqual([]);
    expect(layout.center).toEqual(['chat']);
  });

  it('overwrites panel when re-registered with same id', () => {
    registry.register('chat', 'Chat', StubComponent, 'center');
    registry.register('chat', 'Chat Updated', AnotherStub, 'right');

    const panel = registry.get('chat');
    expect(panel!.title).toBe('Chat Updated');
    expect(panel!.component).toBe(AnotherStub);
    expect(panel!.defaultZone).toBe('right');
    expect(registry.list()).toHaveLength(1);
  });

  it('produces a component map keyed by panel id', () => {
    registry.register('sessions', 'Sessions', StubComponent, 'left');
    registry.register('chat', 'Chat', AnotherStub, 'center');

    const map = registry.getComponentMap();
    expect(map.get('sessions')).toBe(StubComponent);
    expect(map.get('chat')).toBe(AnotherStub);
    expect(map.size).toBe(2);
  });
});

describe('global registry singleton', () => {
  beforeEach(() => {
    resetGlobalRegistry();
  });

  it('returns the same instance on repeated calls', () => {
    const a = getGlobalRegistry();
    const b = getGlobalRegistry();
    expect(a).toBe(b);
  });

  it('resets to a fresh instance', () => {
    const before = getGlobalRegistry();
    before.register('x', 'X', StubComponent, 'center');
    resetGlobalRegistry();
    const after = getGlobalRegistry();
    expect(after).not.toBe(before);
    expect(after.list()).toHaveLength(0);
  });
});
