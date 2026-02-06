import { type Component, type JSXElement, Show, For, onMount, onCleanup } from 'solid-js';
import { type DrawerState, type DrawerMode, cycleMode } from '@/lib/drawer-state';

type DrawerZone = 'left' | 'right' | 'bottom';

export interface DrawerProps {
  zone: DrawerZone;
  state: DrawerState;
  onModeChange: (mode: DrawerMode) => void;
  onFlyoutOpen: (panelId: string) => void;
  onFlyoutClose: () => void;
  onFlyoutPin: () => void;
  onPromote?: (panelId: string) => void;
  renderPanel: (panelId: string) => JSXElement;
  getPanelIcon: (panelId: string) => string;
  getPanelTitle: (panelId: string) => string;
}

function basisForMode(mode: DrawerMode, zone: DrawerZone): string {
  switch (mode) {
    case 'hidden':
      return '0px';
    case 'iconStrip':
      return '40px';
    case 'pinned':
      return zone === 'bottom' ? '200px' : '280px';
  }
}

function isHorizontal(zone: DrawerZone): boolean {
  return zone === 'bottom';
}

export const Drawer: Component<DrawerProps> = (props) => {
  let wrapperRef: HTMLDivElement | undefined;
  let flyoutRef: HTMLDivElement | undefined;

  const style = (): Record<string, string> => ({
    'flex-basis': basisForMode(props.state.mode, props.zone),
    'flex-shrink': '0',
    'flex-grow': '0',
    overflow: props.state.mode === 'hidden' ? 'hidden' : 'visible',
    transition: 'flex-basis 200ms ease-out',
    position: 'relative',
  });

  const handleClickAway = (e: MouseEvent) => {
    if (!props.state.activeFlyoutPanel) return;
    const target = e.target as Node;
    if (flyoutRef?.contains(target)) return;
    if (wrapperRef?.contains(target)) return;
    props.onFlyoutClose();
  };

  onMount(() => {
    document.addEventListener('mousedown', handleClickAway);
  });

  onCleanup(() => {
    document.removeEventListener('mousedown', handleClickAway);
  });

  const railStyle = (): Record<string, string> => ({
    display: 'flex',
    'flex-direction': isHorizontal(props.zone) ? 'row' : 'column',
    'align-items': 'center',
    gap: '4px',
    padding: '4px',
    width: isHorizontal(props.zone) ? '100%' : '40px',
    height: isHorizontal(props.zone) ? '40px' : '100%',
    'box-sizing': 'border-box',
  });

  const flyoutStyle = (): Record<string, string> => {
    const base: Record<string, string> = {
      position: 'absolute',
      'background-color': '#1a1a1a',
      border: '1px solid #333',
      'border-radius': '6px',
      'box-shadow': '0 8px 32px rgba(0,0,0,0.5)',
      'z-index': '50',
      width: '280px',
      'min-height': '200px',
      overflow: 'auto',
    };

    if (props.zone === 'left') {
      base.left = '40px';
      base.top = '0';
      base.bottom = '0';
    } else if (props.zone === 'right') {
      base.right = '40px';
      base.top = '0';
      base.bottom = '0';
    } else {
      base.bottom = '40px';
      base.left = '0';
      base.width = '320px';
      base.height = '240px';
    }

    return base;
  };

  const toggleStyle = (): Record<string, string> => ({
    background: 'none',
    border: 'none',
    color: '#888',
    cursor: 'pointer',
    'font-size': '10px',
    padding: '2px 4px',
    'line-height': '1',
    'margin-top': isHorizontal(props.zone) ? '0' : 'auto',
    'margin-left': isHorizontal(props.zone) ? 'auto' : '0',
  });

  const iconBtnStyle = (): Record<string, string> => ({
    background: 'none',
    border: 'none',
    color: '#ccc',
    cursor: 'pointer',
    'font-size': '16px',
    width: '32px',
    height: '32px',
    display: 'flex',
    'align-items': 'center',
    'justify-content': 'center',
    'border-radius': '4px',
    padding: '0',
  });

  const pinBtnStyle = (): Record<string, string> => ({
    position: 'absolute',
    top: '4px',
    right: '4px',
    background: 'none',
    border: 'none',
    color: '#ccc',
    cursor: 'pointer',
    'font-size': '14px',
    padding: '2px 6px',
    'border-radius': '4px',
  });

  const pinnedStyle = (): Record<string, string> => ({
    width: '100%',
    height: '100%',
    overflow: 'auto',
  });

  return (
    <div
      ref={wrapperRef}
      data-zone={props.zone}
      data-testid={`drawer-${props.zone}`}
      style={style()}
    >
      <Show when={props.state.mode === 'iconStrip'}>
        <div style={railStyle()}>
          <For each={props.state.panels}>
            {(panelId) => (
              <button
                data-testid={`drawer-icon-${panelId}`}
                style={iconBtnStyle()}
                onClick={() => props.onFlyoutOpen(panelId)}
                title={props.getPanelTitle(panelId)}
              >
                {props.getPanelIcon(panelId)}
              </button>
            )}
          </For>
          <button
            data-testid={`drawer-toggle-${props.zone}`}
            style={toggleStyle()}
            onClick={() => props.onModeChange(cycleMode(props.state.mode))}
            title="Toggle drawer mode"
          >
            ⋯
          </button>
        </div>

        <Show when={props.state.activeFlyoutPanel !== null}>
          <div
            ref={flyoutRef}
            data-testid="drawer-flyout"
            style={flyoutStyle()}
          >
            <button
              data-testid="drawer-flyout-pin"
              style={pinBtnStyle()}
              onClick={() => props.onFlyoutPin()}
              title="Pin panel"
            >
              📌
            </button>
            {props.renderPanel(props.state.activeFlyoutPanel!)}
          </div>
        </Show>
      </Show>

      <Show when={props.state.mode === 'pinned'}>
        <div data-testid="drawer-pinned-content" style={pinnedStyle()}>
          <Show when={props.state.panels.length > 0}>
            {props.renderPanel(props.state.panels[0])}
          </Show>
        </div>
        <button
          data-testid={`drawer-toggle-${props.zone}`}
          style={{
            ...toggleStyle(),
            position: 'absolute',
            bottom: '4px',
            right: '4px',
          }}
          onClick={() => props.onModeChange(cycleMode(props.state.mode))}
          title="Toggle drawer mode"
        >
          ⋯
        </button>
      </Show>

      <Show when={props.state.mode === 'hidden'}>
        <button
          data-testid={`drawer-toggle-${props.zone}`}
          style={{
            ...toggleStyle(),
            position: 'absolute',
            top: '0',
            left: '0',
            'z-index': '10',
          }}
          onClick={() => props.onModeChange(cycleMode(props.state.mode))}
          title="Toggle drawer mode"
        >
          ⋯
        </button>
      </Show>
    </div>
  );
};
