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
    registry.register('chat', 'Chat', StubComponent, 'center', 'message');
    const panel = registry.get('chat');
    expect(panel).toBeDefined();
    expect(panel!.id).toBe('chat');
    expect(panel!.title).toBe('Chat');
    expect(panel!.component).toBe(StubComponent);
    expect(panel!.defaultZone).toBe('center');
    expect(panel!.icon).toBe('message');
  });

  it('returns undefined for unknown panel id', () => {
    expect(registry.get('nonexistent')).toBeUndefined();
  });

  it('lists all registered panels', () => {
    registry.register('sessions', 'Sessions', StubComponent, 'left', 'list');
    registry.register('files', 'Files', AnotherStub, 'left', 'folder');
    registry.register('chat', 'Chat', StubComponent, 'center', 'message');

    const panels = registry.list();
    expect(panels).toHaveLength(3);
    expect(panels.map(p => p.id)).toEqual(['sessions', 'files', 'chat']);
  });

  it('returns default layout grouped by zone', () => {
    registry.register('sessions', 'Sessions', StubComponent, 'left', 'list');
    registry.register('files', 'Files', AnotherStub, 'left', 'folder');
    registry.register('chat', 'Chat', StubComponent, 'center', 'message');
    registry.register('editor', 'Editor', StubComponent, 'right', 'code');
    registry.register('terminal', 'Terminal', StubComponent, 'bottom', 'terminal');

    const layout = registry.getDefaultLayout();
    expect(layout.left).toEqual(['sessions', 'files']);
    expect(layout.center).toEqual(['chat']);
    expect(layout.right).toEqual(['editor']);
    expect(layout.bottom).toEqual(['terminal']);
  });

  it('returns empty arrays for zones with no panels', () => {
    registry.register('chat', 'Chat', StubComponent, 'center', 'message');

    const layout = registry.getDefaultLayout();
    expect(layout.left).toEqual([]);
    expect(layout.right).toEqual([]);
    expect(layout.bottom).toEqual([]);
    expect(layout.center).toEqual(['chat']);
  });

  it('overwrites panel when re-registered with same id', () => {
    registry.register('chat', 'Chat', StubComponent, 'center', 'message');
    registry.register('chat', 'Chat Updated', AnotherStub, 'right', 'chat');

    const panel = registry.get('chat');
    expect(panel!.title).toBe('Chat Updated');
    expect(panel!.component).toBe(AnotherStub);
    expect(panel!.defaultZone).toBe('right');
    expect(panel!.icon).toBe('chat');
    expect(registry.list()).toHaveLength(1);
  });

  it('produces a component map keyed by panel id', () => {
    registry.register('sessions', 'Sessions', StubComponent, 'left', 'list');
    registry.register('chat', 'Chat', AnotherStub, 'center', 'message');

    const map = registry.getComponentMap();
    expect(map.get('sessions')).toBe(StubComponent);
    expect(map.get('chat')).toBe(AnotherStub);
    expect(map.size).toBe(2);
  });

  it('all default panels have icons assigned', () => {
    registry.register('sessions', 'Sessions', StubComponent, 'left', 'list');
    registry.register('files', 'Files', StubComponent, 'left', 'folder');
    registry.register('chat', 'Chat', StubComponent, 'center', 'message');
    registry.register('editor', 'Editor', StubComponent, 'right', 'code');
    registry.register('terminal', 'Terminal', StubComponent, 'bottom', 'terminal');

    const panels = registry.list();
    expect(panels).toHaveLength(5);
    panels.forEach(panel => {
      expect(panel.icon).toBeDefined();
      expect(panel.icon.length).toBeGreaterThan(0);
    });
  });

  it('icon field is preserved in registry operations', () => {
    registry.register('test-panel', 'Test', StubComponent, 'center', 'star');
    
    const retrieved = registry.get('test-panel');
    expect(retrieved).toBeDefined();
    expect(retrieved!.icon).toBe('star');

    const listed = registry.list();
    const testPanel = listed.find(p => p.id === 'test-panel');
    expect(testPanel).toBeDefined();
    expect(testPanel!.icon).toBe('star');
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
    before.register('x', 'X', StubComponent, 'center', 'x');
    resetGlobalRegistry();
    const after = getGlobalRegistry();
    expect(after).not.toBe(before);
    expect(after.list()).toHaveLength(0);
  });
});
