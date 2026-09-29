import { it, expect, beforeEach, describe } from 'vitest';
import { fireEvent, render } from '@solidjs/testing-library';
import { WindowManager } from '../WindowManager';
import { RIBBON_WIDTH_PX } from '../Ribbon';
import { windowStore, windowActions } from '@/windowing/store';
import { configureRails, neutralRenderer } from './fixtures';

beforeEach(() => configureRails());

const mount = () => render(() => <WindowManager renderContent={neutralRenderer} slots={{}} />);

describe('the ribbon placement', () => {
  it('puts the ribbon at the window edge by default, with no stub', () => {
    const { getByTestId, queryByTestId } = mount();
    expect(windowStore.ribbonPlacement).toBe('edge');
    const host = getByTestId('edge-host-left');
    expect(host.dataset.ribbonPlacement).toBe('edge');
    expect(getByTestId('edge-collapsed-drop-left').parentElement).toBe(host);
    expect(queryByTestId('ribbon-stub-left')).toBeNull();
  });

  it('inside the panel, the ribbon sits on the inside edge of the card', () => {
    windowActions.setRibbonPlacement('panel');
    const { getByTestId } = mount();
    const left = getByTestId('edge-host-left').querySelector('[data-edge-card="left"]')!;
    const right = getByTestId('edge-host-right').querySelector('[data-edge-card="right"]')!;
    // Next to the centre: last on the left rail, first on the right rail.
    expect(left.lastElementChild).toBe(getByTestId('edge-collapsed-drop-left'));
    expect(right.firstElementChild).toBe(getByTestId('edge-collapsed-drop-right'));
    // Nothing of the rail stays at the window edge.
    expect(getByTestId('edge-host-left').firstElementChild).not.toBe(getByTestId('edge-collapsed-drop-left'));
  });

  it('inside the panel, the whole rail slides, and a closed rail keeps its ribbon in view', () => {
    windowActions.setRibbonPlacement('panel');
    const { getByTestId } = mount();
    const frameOf = (side: 'left' | 'right') =>
      getByTestId(`edge-host-${side}`).querySelector(`[data-edge-card="${side}"]`)!.parentElement!
        .parentElement as HTMLElement;
    // Open (left): the ribbon, the body and its 1px handle.
    expect(frameOf('left').style.width).toBe(`${RIBBON_WIDTH_PX + 280 + 1}px`);
    // Closed (right): the ribbon alone, still painted, and the body hidden.
    expect(frameOf('right').style.width).toBe(`${RIBBON_WIDTH_PX}px`);
    expect(frameOf('right').style.visibility).toBe('visible');
    expect(
      getByTestId('edge-host-right').querySelector<HTMLElement>('[data-edge-panel-body]')!.style
        .visibility,
    ).toBe('hidden');
    // Its toggle opens it.
    fireEvent.click(getByTestId('ribbon-toggle-right'));
    expect(windowStore.edgePanels.right.mode).toBe('docked');
  });

  it('survives a layout reset: it is the user\'s setting, not the layout\'s', () => {
    windowActions.setRibbonPlacement('panel');
    windowActions.resetLayoutToDefaults();
    expect(windowStore.ribbonPlacement).toBe('panel');
  });
});
