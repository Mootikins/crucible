import { describe, it, expect, beforeEach } from 'vitest';
import { configureWindowing, windowStore, windowActions } from '@/windowing/store';
import { neutralPolicy } from '@/windowing/testing/neutralPolicy';

function paneIdOf(layout: { type: string; id: string }): string {
  if (layout.type !== 'pane') throw new Error('the seed layout is not a single pane');
  return layout.id;
}

describe('edge modes', () => {
  beforeEach(() => configureWindowing(neutralPolicy()));

  it('setEdgeMode writes the mode and the cue', () => {
    windowActions.setEdgeMode('left', 'hidden', { cue: 'none' });
    expect(windowStore.edgePanels.left.mode).toBe('hidden');
    expect(windowStore.edgePanels.left.cue).toBe('none');
  });

  it('setEdgeMode without a cue keeps the stored cue', () => {
    windowActions.setEdgeMode('left', 'hidden', { cue: 'none' });
    windowActions.setEdgeMode('left', 'flyout');
    expect(windowStore.edgePanels.left.mode).toBe('flyout');
    // The cue is a stored preference. Only `hidden` reads it, but a change
    // to another mode does not erase it.
    expect(windowStore.edgePanels.left.cue).toBe('none');
    windowActions.setEdgeMode('left', 'hidden');
    expect(windowStore.edgePanels.left.cue).toBe('none');
  });

  it('setEdgeMode changes only the named rail', () => {
    const before = windowStore.edgePanels.right.mode;
    windowActions.setEdgeMode('left', 'flyout');
    expect(windowStore.edgePanels.right.mode).toBe(before);
  });

  it('toggleEdgePanel moves between docked and strip only', () => {
    windowActions.setEdgeMode('left', 'hidden');
    windowActions.toggleEdgePanel('left');
    expect(windowStore.edgePanels.left.mode).toBe('docked');
    windowActions.toggleEdgePanel('left');
    expect(windowStore.edgePanels.left.mode).toBe('strip');
    windowActions.setEdgeMode('left', 'flyout');
    windowActions.toggleEdgePanel('left');
    expect(windowStore.edgePanels.left.mode).toBe('docked');
  });

  it('setEdgePanelCollapsed keeps its meaning', () => {
    windowActions.setEdgePanelCollapsed('right', false);
    expect(windowStore.edgePanels.right.mode).toBe('docked');
    windowActions.setEdgePanelCollapsed('right', true);
    expect(windowStore.edgePanels.right.mode).toBe('strip');
  });

  it('setPaneReveal writes the reveal on a pane in a rail', () => {
    const paneId = paneIdOf(windowStore.edgePanels.right.layout);
    windowActions.setPaneReveal(paneId, 'hover');
    expect(windowActions.findPaneById(paneId)?.reveal).toBe('hover');
    windowActions.setPaneReveal(paneId, 'click');
    expect(windowActions.findPaneById(paneId)?.reveal).toBe('click');
  });

  it('setPaneReveal writes the reveal on a pane in the centre', () => {
    const paneId = paneIdOf(windowStore.layout);
    windowActions.setPaneReveal(paneId, 'hover');
    expect(windowActions.findPaneById(paneId)?.reveal).toBe('hover');
    const railPane = paneIdOf(windowStore.edgePanels.left.layout);
    expect(windowActions.findPaneById(railPane)?.reveal).toBeUndefined();
  });

  it('setPaneReveal with an unknown id changes nothing', () => {
    const before = JSON.stringify(windowStore);
    windowActions.setPaneReveal('no-such-pane', 'hover');
    expect(JSON.stringify(windowStore)).toBe(before);
  });
});
