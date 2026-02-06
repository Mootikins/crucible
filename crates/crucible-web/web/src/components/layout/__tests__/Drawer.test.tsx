import { describe, it, expect, vi } from 'vitest';
import { render, fireEvent } from '@solidjs/testing-library';
import { Drawer, type DrawerProps } from '../Drawer';
import type { DrawerState } from '@/lib/drawer-state';

function makeProps(overrides: Partial<DrawerProps> & { state: DrawerState }): DrawerProps {
  return {
    zone: 'left',
    onModeChange: vi.fn(),
    onFlyoutOpen: vi.fn(),
    onFlyoutClose: vi.fn(),
    onFlyoutPin: vi.fn(),
    renderPanel: (id: string) => <span data-testid={`panel-${id}`}>Panel {id}</span>,
    getPanelIcon: (id: string) => `icon-${id}`,
    getPanelTitle: (id: string) => `Title ${id}`,
    ...overrides,
  };
}

function hiddenState(panels: string[] = []): DrawerState {
  return { mode: 'hidden', panels, activeFlyoutPanel: null };
}

function iconStripState(panels: string[], activeFlyout: string | null = null): DrawerState {
  return { mode: 'iconStrip', panels, activeFlyoutPanel: activeFlyout };
}

function pinnedState(panels: string[]): DrawerState {
  return { mode: 'pinned', panels, activeFlyoutPanel: null };
}

describe('Drawer', () => {
  it('renders with data-zone attribute', () => {
    const props = makeProps({ zone: 'left', state: hiddenState() });
    const { getByTestId } = render(() => <Drawer {...props} />);
    const el = getByTestId('drawer-left');
    expect(el).toBeInTheDocument();
    expect(el.getAttribute('data-zone')).toBe('left');
  });

  it('hidden mode sets flex-basis to 0px with overflow hidden', () => {
    const props = makeProps({ zone: 'left', state: hiddenState() });
    const { getByTestId } = render(() => <Drawer {...props} />);
    const el = getByTestId('drawer-left');
    expect(el.style.flexBasis).toBe('0px');
    expect(el.style.overflow).toBe('hidden');
  });

  it('icon strip mode sets flex-basis to 40px and renders icon buttons', () => {
    const props = makeProps({
      zone: 'left',
      state: iconStripState(['explorer', 'search']),
    });
    const { getByTestId } = render(() => <Drawer {...props} />);
    const el = getByTestId('drawer-left');
    expect(el.style.flexBasis).toBe('40px');
    expect(getByTestId('drawer-icon-explorer')).toBeInTheDocument();
    expect(getByTestId('drawer-icon-search')).toBeInTheDocument();
  });

  it('pinned mode sets flex-basis to 280px for sidebar zones', () => {
    const props = makeProps({
      zone: 'right',
      state: pinnedState(['explorer']),
    });
    const { getByTestId } = render(() => <Drawer {...props} />);
    const el = getByTestId('drawer-right');
    expect(el.style.flexBasis).toBe('280px');
    expect(getByTestId('drawer-pinned-content')).toBeInTheDocument();
  });

  it('pinned mode sets flex-basis to 200px for bottom zone', () => {
    const props = makeProps({
      zone: 'bottom',
      state: pinnedState(['terminal']),
    });
    const { getByTestId } = render(() => <Drawer {...props} />);
    const el = getByTestId('drawer-bottom');
    expect(el.style.flexBasis).toBe('200px');
  });

  it('icon click calls onFlyoutOpen with panel id', async () => {
    const onFlyoutOpen = vi.fn();
    const props = makeProps({
      zone: 'left',
      state: iconStripState(['explorer']),
      onFlyoutOpen,
    });
    const { getByTestId } = render(() => <Drawer {...props} />);
    await fireEvent.click(getByTestId('drawer-icon-explorer'));
    expect(onFlyoutOpen).toHaveBeenCalledWith('explorer');
  });

  it('flyout renders when activeFlyoutPanel is set', () => {
    const props = makeProps({
      zone: 'left',
      state: iconStripState(['explorer'], 'explorer'),
    });
    const { getByTestId } = render(() => <Drawer {...props} />);
    expect(getByTestId('drawer-flyout')).toBeInTheDocument();
    expect(getByTestId('panel-explorer')).toBeInTheDocument();
  });

  it('flyout pin button calls onFlyoutPin', async () => {
    const onFlyoutPin = vi.fn();
    const props = makeProps({
      zone: 'left',
      state: iconStripState(['explorer'], 'explorer'),
      onFlyoutPin,
    });
    const { getByTestId } = render(() => <Drawer {...props} />);
    await fireEvent.click(getByTestId('drawer-flyout-pin'));
    expect(onFlyoutPin).toHaveBeenCalledTimes(1);
  });

  it('toggle button calls onModeChange with next mode', async () => {
    const onModeChange = vi.fn();
    const props = makeProps({
      zone: 'left',
      state: iconStripState(['explorer']),
      onModeChange,
    });
    const { getByTestId } = render(() => <Drawer {...props} />);
    await fireEvent.click(getByTestId('drawer-toggle-left'));
    expect(onModeChange).toHaveBeenCalledWith('pinned');
  });

  it('hidden mode toggle cycles to iconStrip', async () => {
    const onModeChange = vi.fn();
    const props = makeProps({
      zone: 'left',
      state: hiddenState(),
      onModeChange,
    });
    const { getByTestId } = render(() => <Drawer {...props} />);
    await fireEvent.click(getByTestId('drawer-toggle-left'));
    expect(onModeChange).toHaveBeenCalledWith('iconStrip');
  });

  it('bottom drawer uses horizontal layout for icon rail', () => {
    const props = makeProps({
      zone: 'bottom',
      state: iconStripState(['terminal']),
    });
    const { getByTestId } = render(() => <Drawer {...props} />);
    const iconBtn = getByTestId('drawer-icon-terminal');
    const rail = iconBtn.parentElement!;
    expect(rail.style.flexDirection).toBe('row');
    expect(rail.style.height).toBe('40px');
  });

  it('left drawer uses vertical layout for icon rail', () => {
    const props = makeProps({
      zone: 'left',
      state: iconStripState(['explorer']),
    });
    const { getByTestId } = render(() => <Drawer {...props} />);
    const iconBtn = getByTestId('drawer-icon-explorer');
    const rail = iconBtn.parentElement!;
    expect(rail.style.flexDirection).toBe('column');
    expect(rail.style.width).toBe('40px');
  });

  it('click-away on flyout calls onFlyoutClose', async () => {
    const onFlyoutClose = vi.fn();
    const props = makeProps({
      zone: 'left',
      state: iconStripState(['explorer'], 'explorer'),
      onFlyoutClose,
    });
    render(() => <Drawer {...props} />);
    await fireEvent.mouseDown(document.body);
    expect(onFlyoutClose).toHaveBeenCalledTimes(1);
  });

  it('applies transition on outer wrapper', () => {
    const props = makeProps({ zone: 'left', state: hiddenState() });
    const { getByTestId } = render(() => <Drawer {...props} />);
    const el = getByTestId('drawer-left');
    expect(el.style.transition).toBe('flex-basis 200ms ease-out');
  });

  it('pinned mode renders first panel content', () => {
    const props = makeProps({
      zone: 'left',
      state: pinnedState(['explorer', 'search']),
    });
    const { getByTestId } = render(() => <Drawer {...props} />);
    expect(getByTestId('panel-explorer')).toBeInTheDocument();
  });

  it('does not render flyout when activeFlyoutPanel is null', () => {
    const props = makeProps({
      zone: 'left',
      state: iconStripState(['explorer'], null),
    });
    const { queryByTestId } = render(() => <Drawer {...props} />);
    expect(queryByTestId('drawer-flyout')).not.toBeInTheDocument();
  });
});
