import { it, expect, beforeEach, afterEach, describe } from 'vitest';
import { render } from '@solidjs/testing-library';
import { onMount, onCleanup } from 'solid-js';
import { produce } from 'solid-js/store';
import { WindowManager } from '../WindowManager';
import { windowStore, windowActions, setStore } from '@/stores/windowStore';
import { findFirstPane, primaryEdgeGroupId } from '@/windowing/model/tree';
import { defaultLayout } from '@/stores/defaultLayout';
import { getGlobalRegistry, resetGlobalRegistry } from '@/lib/panel-registry';

/**
 * A flip moves a panel from one side to the other. It must MOVE the panel,
 * not rebuild it. A rebuilt chat panel drops its provider, so it refetches the
 * transcript and paints an empty pane while the history loads.
 *
 * The probe stands in for that chat panel. It counts its own mounts and it
 * remembers the DOM node it drew, which are the two things a remount changes.
 */
let mounts = 0;
let cleanups = 0;

const ChatProbe = () => {
  mounts += 1;
  onMount(() => {});
  onCleanup(() => {
    cleanups += 1;
  });
  return <div data-testid="chat-probe">transcript</div>;
};

const probeNode = (container: HTMLElement) =>
  container.querySelector('[data-testid="chat-probe"]');

describe('swapSidePanels keeps the panels mounted', () => {
  beforeEach(() => {
    mounts = 0;
    cleanups = 0;
    resetGlobalRegistry();
    getGlobalRegistry().register('chat', 'Chat', ChatProbe, 'left');
    setStore(produce((s) => {
      const fresh = defaultLayout();
      s.layout = fresh.layout;
      s.tabGroups = fresh.tabGroups;
      s.edgePanels = fresh.edgePanels;
      s.floatingWindows = [];
      s.activePaneId = fresh.activePaneId;
      s.focusedRegion = 'center';
      s.nextZIndex = 100;
      // Both rails open: a collapsed rail is a different question.
      s.edgePanels.left.mode = 'docked';
      s.edgePanels.right.mode = 'docked';
    }));
  });

  /** One chat tab on the LEFT rail, and it is the tab on show. */
  const chatOnLeftRail = () => {
    const leftGroup = primaryEdgeGroupId(windowStore, 'left')!;
    windowActions.addTab(leftGroup, {
      id: 'tab-chat',
      title: 'session',
      contentType: 'chat',
    });
    windowActions.setActiveTab(leftGroup, 'tab-chat');
  };

  afterEach(() => {
    resetGlobalRegistry();
  });

  it('carries the same DOM node and the same component instance across a flip', () => {
    chatOnLeftRail();
    const { container } = render(() => <WindowManager />);

    const before = probeNode(container);
    expect(before).toBeTruthy();
    const mountsBefore = mounts;

    windowActions.swapSidePanels();

    // The panel moved to the other rail. It is the SAME element.
    expect(probeNode(container)).toBe(before);
    // And it was never torn down and rebuilt.
    expect(mounts).toBe(mountsBefore);
    expect(cleanups).toBe(0);
  });

  // The flip reverses the CENTRE columns as well as the rails, and a
  // conversation usually opens as a centre pane beside its editor. So the
  // centre needs the same guarantee, through a different mechanism: the
  // halves of every split are keyed, so a reversal reorders them.
  it('carries a centre pane across the flip that reverses its column', () => {
    const pane = findFirstPane(windowStore.layout)!;
    windowActions.openTabInNewPane(pane.id, 'left', {
      id: 'tab-chat-centre',
      title: 'session',
      contentType: 'chat',
    });

    const { container } = render(() => <WindowManager />);

    const before = probeNode(container);
    expect(before).toBeTruthy();
    const mountsBefore = mounts;

    windowActions.swapSidePanels();

    expect(probeNode(container)).toBe(before);
    expect(mounts).toBe(mountsBefore);
    expect(cleanups).toBe(0);
  });

  // The rail now SURVIVES its move, which is the point — but its chrome is
  // positional. The ribbon sits at the window edge, the toggle names a side,
  // and the collapsed drop target resolves a drop to a side. All three must
  // follow the rail to its new side rather than keep the side it booted on.
  it('moves the rail chrome to the side the rail landed on', () => {
    chatOnLeftRail();
    const { container } = render(() => <WindowManager />);

    const ids = () =>
      Array.from(container.querySelectorAll('[data-testid^="ribbon-toggle-"]')).map((el) =>
        el.getAttribute('data-testid'),
      );
    const drops = () =>
      Array.from(container.querySelectorAll('[data-testid^="edge-collapsed-drop-"]')).map((el) =>
        el.getAttribute('data-testid'),
      );

    expect(ids()).toEqual(['ribbon-toggle-left', 'ribbon-toggle-right']);

    windowActions.swapSidePanels();

    // Still one toggle per side, and still in window order. A stale instance
    // would show the same side twice.
    expect(ids()).toEqual(['ribbon-toggle-left', 'ribbon-toggle-right']);
    expect(drops()).toEqual(['edge-collapsed-drop-left', 'edge-collapsed-drop-right']);
  });
});
