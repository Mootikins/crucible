import { describe, it, expect, beforeEach } from 'vitest';
import { produce } from 'solid-js/store';
import { windowStore, setStore, windowActions } from '@/stores/windowStore';
import { statusBarStore, statusBarActions } from '@/stores/statusBarStore';
import type { EdgePanelPosition, LayoutNode, Tab, TabGroup } from '@/types/windowTypes';
import { getGlobalRegistry, resetGlobalRegistry } from '../panel-registry';
import { openPanelTab, findTabByContentType } from '../panel-actions';

const StubComponent = () => null;

function resetToState(overrides: Partial<{
  tabGroups: Record<string, TabGroup>;
  edgePanels: Record<EdgePanelPosition, {
    id: string;
    tabGroupId: string;
    isCollapsed: boolean;
    width?: number;
    height?: number;
  }>;
  layout: LayoutNode;
  activePaneId: string | null;
}>) {
  setStore(
    produce((s) => {
      if (overrides.tabGroups !== undefined) s.tabGroups = overrides.tabGroups;
      if (overrides.edgePanels !== undefined) s.edgePanels = overrides.edgePanels as never;
      if (overrides.layout !== undefined) s.layout = overrides.layout;
      if (overrides.activePaneId !== undefined) s.activePaneId = overrides.activePaneId;
      s.flyoutState = null;
    })
  );
}

const makeTabGroup = (id: string, tabs: Tab[], activeTabId: string | null = tabs[0]?.id ?? null): TabGroup => ({
  id,
  tabs,
  activeTabId,
});

const makeEdgePanel = (position: EdgePanelPosition, tabGroupId: string, isCollapsed = false) => ({
  id: `${position}-panel`,
  tabGroupId,
  isCollapsed,
  ...(position === 'bottom' ? { height: 200 } : { width: 250 }),
});

const simpleLayout = (paneId: string, groupId: string): LayoutNode => ({
  id: paneId,
  type: 'pane' as const,
  tabGroupId: groupId,
});

beforeEach(() => {
  resetGlobalRegistry();
  const registry = getGlobalRegistry();
  registry.register('settings', 'Settings', StubComponent, 'center', '⚙️');
  registry.register('skills', 'Skills', StubComponent, 'left', '🎯');
  registry.register('plugins', 'Plugins', StubComponent, 'left', '🔌');
  registry.register('files', 'Files', StubComponent, 'left', '📁');

  resetToState({
    tabGroups: {
      'center-group': makeTabGroup('center-group', [
        { id: 'tab-chat-x', title: 'Chat', contentType: 'chat', metadata: { sessionId: 'x' } },
      ]),
      'left-group': makeTabGroup('left-group', [
        { id: 'sessions-tab', title: 'Sessions', contentType: 'sessions' },
      ]),
      'right-group': makeTabGroup('right-group', [], null),
      'bottom-group': makeTabGroup('bottom-group', [], null),
    },
    edgePanels: {
      left: makeEdgePanel('left', 'left-group', true),
      right: makeEdgePanel('right', 'right-group', true),
      bottom: makeEdgePanel('bottom', 'bottom-group', true),
    },
    layout: simpleLayout('pane-1', 'center-group'),
    activePaneId: 'pane-1',
  });
});

describe('openPanelTab', () => {
  it('opens a center-zone panel as an active tab in the first center group', () => {
    openPanelTab('settings');

    const group = windowStore.tabGroups['center-group'];
    const tab = group.tabs.find((t) => t.contentType === 'settings');
    expect(tab).toBeDefined();
    expect(tab?.id).toBe('tab-settings');
    expect(tab?.title).toBe('Settings');
    expect(group.activeTabId).toBe('tab-settings');
  });

  it('opens an edge-zone panel in its edge group and expands the collapsed panel', () => {
    openPanelTab('skills');

    const group = windowStore.tabGroups['left-group'];
    const tab = group.tabs.find((t) => t.contentType === 'skills');
    expect(tab).toBeDefined();
    expect(group.activeTabId).toBe('tab-skills');
    expect(windowStore.edgePanels.left.isCollapsed).toBe(false);
  });

  it('focuses the existing tab instead of duplicating', () => {
    openPanelTab('plugins');
    // move focus away, then reopen
    setStore(produce((s) => { s.tabGroups['left-group'].activeTabId = 'sessions-tab'; }));

    openPanelTab('plugins');

    const group = windowStore.tabGroups['left-group'];
    expect(group.tabs.filter((t) => t.contentType === 'plugins')).toHaveLength(1);
    expect(group.activeTabId).toBe('tab-plugins');
  });

  it('re-expands a collapsed edge panel when focusing an existing tab', () => {
    openPanelTab('files');
    setStore(produce((s) => { s.edgePanels.left.isCollapsed = true; }));

    openPanelTab('files');

    expect(windowStore.edgePanels.left.isCollapsed).toBe(false);
    expect(windowStore.tabGroups['left-group'].activeTabId).toBe('tab-files');
  });

  it('focuses an existing center tab found by content type', () => {
    openPanelTab('settings');
    setStore(produce((s) => { s.tabGroups['center-group'].activeTabId = 'tab-chat-x'; }));

    openPanelTab('settings');

    expect(windowStore.tabGroups['center-group'].activeTabId).toBe('tab-settings');
    expect(windowStore.tabGroups['center-group'].tabs.filter((t) => t.contentType === 'settings')).toHaveLength(1);
  });

  it('is a safe no-op for an unregistered content type', () => {
    expect(() => openPanelTab('outline')).not.toThrow();
    const allTabs = Object.values(windowStore.tabGroups).flatMap((g) => g.tabs);
    expect(allTabs.find((t) => t.contentType === 'outline')).toBeUndefined();
  });
});

describe('findTabByContentType', () => {
  it('finds tabs across groups', () => {
    expect(findTabByContentType('sessions')?.groupId).toBe('left-group');
    expect(findTabByContentType('settings')).toBeNull();
  });
});

describe('active-session focus tracking', () => {
  // Session-scoped commands (Ctrl+K clear, switch-model) target
  // statusBarStore.activeSessionId; it must follow tab/pane focus, not just
  // the last-bootstrapped chat.
  it('setActiveTab on a chat tab updates the active session', () => {
    statusBarActions.setActiveSessionId(null);
    windowActions.setActiveTab('center-group', 'tab-chat-x');
    expect(statusBarStore.activeSessionId()).toBe('x');
  });

  it('addTab with session metadata updates the active session', () => {
    statusBarActions.setActiveSessionId(null);
    windowActions.addTab('center-group', {
      id: 'tab-chat-y',
      title: 'Chat Y',
      contentType: 'chat',
      metadata: { sessionId: 'y' },
    });
    expect(statusBarStore.activeSessionId()).toBe('y');
  });

  it('activating a non-session tab leaves the active session unchanged', () => {
    statusBarActions.setActiveSessionId('x');
    windowActions.setActiveTab('left-group', 'sessions-tab');
    expect(statusBarStore.activeSessionId()).toBe('x');
  });

  it('focusing a pane adopts its visible chat session', () => {
    statusBarActions.setActiveSessionId(null);
    windowActions.setActivePane('pane-1');
    expect(statusBarStore.activeSessionId()).toBe('x');
  });
});
