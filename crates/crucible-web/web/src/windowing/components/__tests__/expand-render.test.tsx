import { it, expect, beforeEach, describe } from 'vitest';
import { fireEvent, render } from '@solidjs/testing-library';
import { onCleanup } from 'solid-js';
import { WindowManager } from '../WindowManager';
import { windowStore, windowActions } from '@/windowing/store';
import type { WindowingContextValue } from '@/windowing/components/context';
import { configureRails, neutralRenderer } from './fixtures';

beforeEach(() => configureRails());

/** Counts how often the centre's content unmounts: an expand must not unmount it. */
let centreCleanups = 0;
const CentreProbe = () => {
  onCleanup(() => {
    centreCleanups += 1;
  });
  return <div data-testid="centre-probe">editor</div>;
};

describe('an expanded rail covers the centre', () => {
  it('hides the centre and keeps it mounted, then shows it again', () => {
    centreCleanups = 0;
    const groupId = windowStore.layout.type === 'pane' ? windowStore.layout.tabGroupId! : '';
    windowActions.addTab(groupId, { id: 'probe-tab', title: 'Probe', contentType: 'probe' });
    const renderContent: WindowingContextValue['renderContent'] = (tab) =>
      tab().contentType === 'probe' ? <CentreProbe /> : neutralRenderer(tab);
    const { getByTestId } = render(() => <WindowManager renderContent={renderContent} slots={{}} />);

    const centreColumn = getByTestId('centre-column');
    expect(centreColumn.hidden).toBe(false);

    windowActions.expandEdge('right');
    expect(centreColumn.hidden).toBe(true);
    expect(getByTestId('edge-host-right').hasAttribute('data-edge-expanded')).toBe(true);
    // Still in the document, same node: nothing remounted.
    expect(getByTestId('centre-probe').isConnected).toBe(true);

    windowActions.collapseExpandedEdge();
    expect(centreColumn.hidden).toBe(false);
    expect(getByTestId('edge-host-right').hasAttribute('data-edge-expanded')).toBe(false);
    expect(centreCleanups).toBe(0);
  });

  it('Shift+Escape expands the focused rail, and a second press gives the centre back', () => {
    render(() => <WindowManager renderContent={neutralRenderer} slots={{}} />);
    windowActions.setEdgePanelCollapsed('right', false);
    windowActions.setEdgePanelActiveTab('right', 'beta-tab');
    expect(windowStore.focusedRegion).toBe('right');

    fireEvent.keyDown(document, { key: 'Escape', shiftKey: true });
    expect(windowStore.expandedEdge).toBe('right');
    fireEvent.keyDown(document, { key: 'Escape', shiftKey: true });
    expect(windowStore.expandedEdge).toBeNull();
  });

  it('Shift+Escape from the centre expands nothing', () => {
    render(() => <WindowManager renderContent={neutralRenderer} slots={{}} />);
    expect(windowStore.focusedRegion).toBe('center');
    fireEvent.keyDown(document, { key: 'Escape', shiftKey: true });
    expect(windowStore.expandedEdge).toBeNull();
  });
});
