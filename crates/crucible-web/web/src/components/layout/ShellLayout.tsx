import { type Component, type JSXElement, createSignal, onMount, onCleanup } from 'solid-js';
import { BreadcrumbNav } from '@/components/BreadcrumbNav';
import { loadZoneState, saveZoneState, loadZoneWidths, saveZoneWidths, type ZoneState, type ZoneMode, type ZoneWidths } from '@/lib/layout';
import { ZoneWrapper } from './ZoneWrapper';

const GearIcon: Component = () => (
  <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor" class="w-5 h-5">
    <path fill-rule="evenodd" d="M11.078 2.25c-.917 0-1.699.663-1.85 1.567L9.05 4.889c-.02.12-.115.26-.297.348a7.493 7.493 0 00-.986.57c-.166.115-.334.126-.45.083L6.3 5.508a1.875 1.875 0 00-2.282.819l-.922 1.597a1.875 1.875 0 00.432 2.385l.84.692c.095.078.17.229.154.43a7.598 7.598 0 000 1.139c.015.2-.059.352-.153.43l-.841.692a1.875 1.875 0 00-.432 2.385l.922 1.597a1.875 1.875 0 002.282.818l1.019-.382c.115-.043.283-.031.45.082.312.214.641.405.985.57.182.088.277.228.297.35l.178 1.071c.151.904.933 1.567 1.85 1.567h1.844c.916 0 1.699-.663 1.85-1.567l.178-1.072c.02-.12.114-.26.297-.349.344-.165.673-.356.985-.57.167-.114.335-.125.45-.082l1.02.382a1.875 1.875 0 002.28-.819l.923-1.597a1.875 1.875 0 00-.432-2.385l-.84-.692c-.095-.078-.17-.229-.154-.43a7.614 7.614 0 000-1.139c-.016-.2.059-.352.153-.43l.84-.692c.708-.582.891-1.59.433-2.385l-.922-1.597a1.875 1.875 0 00-2.282-.818l-1.02.382c-.114.043-.282.031-.449-.083a7.49 7.49 0 00-.985-.57c-.183-.087-.277-.227-.297-.348l-.179-1.072a1.875 1.875 0 00-1.85-1.567h-1.843zM12 15.75a3.75 3.75 0 100-7.5 3.75 3.75 0 000 7.5z" clip-rule="evenodd" />
  </svg>
);

const SidebarIcon: Component<{ side: 'left' | 'right' }> = (props) => (
  <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" class="w-4 h-4">
    <path
      fill-rule="evenodd"
      d={props.side === 'left'
        ? "M2 4.75A.75.75 0 012.75 4h14.5a.75.75 0 010 1.5H2.75A.75.75 0 012 4.75zM2 10a.75.75 0 01.75-.75h7.5a.75.75 0 010 1.5h-7.5A.75.75 0 012 10zm0 5.25a.75.75 0 01.75-.75h14.5a.75.75 0 010 1.5H2.75a.75.75 0 01-.75-.75z"
        : "M2 4.75A.75.75 0 012.75 4h14.5a.75.75 0 010 1.5H2.75A.75.75 0 012 4.75zm7.5 5.25a.75.75 0 01.75-.75h7a.75.75 0 010 1.5h-7a.75.75 0 01-.75-.75zM2 15.25a.75.75 0 01.75-.75h14.5a.75.75 0 010 1.5H2.75a.75.75 0 01-.75-.75z"
      }
      clip-rule="evenodd"
    />
  </svg>
);

const BottomPanelIcon: Component = () => (
  <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" class="w-4 h-4">
    <path fill-rule="evenodd" d="M2 4.75A.75.75 0 012.75 4h14.5a.75.75 0 010 1.5H2.75A.75.75 0 012 4.75zM2 10a.75.75 0 01.75-.75h14.5a.75.75 0 010 1.5H2.75A.75.75 0 012 10zm0 5.25a.75.75 0 01.75-.75h7.5a.75.75 0 010 1.5h-7.5A.75.75 0 012 15.25z" clip-rule="evenodd" />
  </svg>
);

export type ToggleableZone = 'left' | 'right' | 'bottom';

const isZoneExpanded = (mode: ZoneMode): boolean => mode === 'visible' || mode === 'pinned';

export interface ShellLayoutProps {
  leftContent?: JSXElement;
  centerContent?: JSXElement;
  rightContent?: JSXElement;
  bottomContent?: JSXElement;
  leftRef?: HTMLDivElement | ((el: HTMLDivElement) => void);
  centerRef?: HTMLDivElement | ((el: HTMLDivElement) => void);
  rightRef?: HTMLDivElement | ((el: HTMLDivElement) => void);
  bottomRef?: HTMLDivElement | ((el: HTMLDivElement) => void);
  onZoneTransitionEnd?: (zone: ToggleableZone) => void;
}

export const ShellLayout: Component<ShellLayoutProps> = (props) => {
  const [showSettings, setShowSettings] = createSignal(false);
  const [zoneState, setZoneState] = createSignal<ZoneState>(loadZoneState());
  const [zoneWidths] = createSignal<ZoneWidths>(loadZoneWidths());
  const [ariaLiveMessage, setAriaLiveMessage] = createSignal('');

  const toggleZone = (zone: ToggleableZone) => {
    const current = zoneState()[zone];
    const next: ZoneMode = isZoneExpanded(current) ? 'hidden' : 'visible';
    const newState = { ...zoneState(), [zone]: next };
    setZoneState(newState);
    saveZoneState(newState);
    saveZoneWidths(zoneWidths());

    const zoneNames: Record<ToggleableZone, string> = {
      left: 'Left zone',
      right: 'Right zone',
      bottom: 'Bottom zone',
    };
    setAriaLiveMessage(`${zoneNames[zone]} ${isZoneExpanded(next) ? 'expanded' : 'collapsed'}`);
  };

  const handleTransitionEnd = (zone: ToggleableZone) => (event: TransitionEvent) => {
    if (event.target !== event.currentTarget) return;
    if (event.propertyName !== 'flex-basis') return;
    props.onZoneTransitionEnd?.(zone);
  };

  onMount(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      const target = event.target;
      const isEditable = target instanceof HTMLInputElement ||
        target instanceof HTMLTextAreaElement ||
        (target instanceof HTMLElement && target.contentEditable === 'true');
      if (isEditable) return;

      const userAgentData = (navigator as Navigator & { userAgentData?: { platform?: string } }).userAgentData;
      const isMac = userAgentData?.platform === 'macOS' ||
        /Mac|iPod|iPhone|iPad/.test(navigator.userAgent);
      const modifier = isMac ? event.metaKey : event.ctrlKey;
      if (!modifier) return;

      let zone: ToggleableZone | null = null;
      if (event.code === 'KeyB' && !event.shiftKey) zone = 'left';
      else if (event.code === 'KeyB' && event.shiftKey) zone = 'right';
      else if (event.code === 'KeyJ') zone = 'bottom';

      if (zone) {
        event.preventDefault();
        toggleZone(zone);
      }
    };

    document.addEventListener('keydown', handleKeyDown);
    onCleanup(() => document.removeEventListener('keydown', handleKeyDown));
  });

  const leftExpanded = () => isZoneExpanded(zoneState().left);
  const rightExpanded = () => isZoneExpanded(zoneState().right);
  const bottomExpanded = () => isZoneExpanded(zoneState().bottom);

  return (
    <div class="h-screen w-screen flex flex-col bg-neutral-950">
      <BreadcrumbNav />

      <div class="flex-1 flex overflow-hidden">
        {/* Left icon rail toggle */}
        <div class="flex flex-col justify-center border-r border-neutral-800 bg-neutral-900">
          <button
            data-testid="toggle-left"
            onClick={() => toggleZone('left')}
            aria-label="Toggle left sidebar"
            aria-expanded={leftExpanded()}
            aria-controls="sessions"
            class={`p-2 transition-colors ${leftExpanded() ? 'text-blue-400' : 'text-neutral-500 hover:text-neutral-300'}`}
            title="Toggle left sidebar (⌘B)"
          >
            <SidebarIcon side="left" />
          </button>
        </div>

        {/* Left collapsed icon rail */}
        {!leftExpanded() && (
          <div class="icon-rail icon-rail-left">
            <button
              data-testid="rail-expand-left"
              onClick={() => toggleZone('left')}
              aria-label="Expand left sidebar"
              class="p-2 text-neutral-400 hover:text-white transition-colors"
            >
              <SidebarIcon side="left" />
            </button>
          </div>
        )}

        {/* Left zone */}
        <ZoneWrapper zone="left" collapsed={!leftExpanded()} width={zoneWidths().left} ref={props.leftRef} onTransitionEnd={handleTransitionEnd('left')}>
          {props.leftContent}
        </ZoneWrapper>

        {/* Center column: center zone + bottom zone */}
        <div class="flex-1 flex flex-col overflow-hidden min-w-0">
          <ZoneWrapper zone="center" collapsed={false} ref={props.centerRef}>
            {props.centerContent}
          </ZoneWrapper>

          <ZoneWrapper zone="bottom" collapsed={!bottomExpanded()} height={zoneWidths().bottom} ref={props.bottomRef} onTransitionEnd={handleTransitionEnd('bottom')}>
            {props.bottomContent}
          </ZoneWrapper>

          {/* Bottom toggle bar */}
          <div class="flex justify-center border-t border-neutral-800 bg-neutral-900">
            <button
              data-testid="toggle-bottom"
              onClick={() => toggleZone('bottom')}
              aria-label="Toggle bottom panel"
              aria-expanded={bottomExpanded()}
              aria-controls="bottom"
              class={`p-1.5 transition-colors ${bottomExpanded() ? 'text-blue-400' : 'text-neutral-500 hover:text-neutral-300'}`}
              title="Toggle bottom panel (⌘J)"
            >
              <BottomPanelIcon />
            </button>
          </div>
        </div>

        {/* Right zone */}
        <ZoneWrapper zone="right" collapsed={!rightExpanded()} width={zoneWidths().right} ref={props.rightRef} onTransitionEnd={handleTransitionEnd('right')}>
          {props.rightContent}
        </ZoneWrapper>

        {/* Right collapsed icon rail */}
        {!rightExpanded() && (
          <div class="icon-rail icon-rail-right">
            <button
              data-testid="rail-expand-right"
              onClick={() => toggleZone('right')}
              aria-label="Expand right sidebar"
              class="p-2 text-neutral-400 hover:text-white transition-colors"
            >
              <SidebarIcon side="right" />
            </button>
          </div>
        )}

        {/* Right icon rail toggle */}
        <div class="flex flex-col justify-center border-l border-neutral-800 bg-neutral-900">
          <button
            data-testid="toggle-right"
            onClick={() => toggleZone('right')}
            aria-label="Toggle right sidebar"
            aria-expanded={rightExpanded()}
            aria-controls="editor"
            class={`p-2 transition-colors ${rightExpanded() ? 'text-blue-400' : 'text-neutral-500 hover:text-neutral-300'}`}
            title="Toggle right sidebar (⌘⇧B)"
          >
            <SidebarIcon side="right" />
          </button>
        </div>
      </div>

      {/* Settings overlay — outermost level, overlays ALL zones */}
      {showSettings() && (
        <div class="absolute inset-0 z-50 bg-black/60 flex items-center justify-center">
          <div class="bg-neutral-900 border border-neutral-700 rounded-lg p-6 max-w-lg w-full mx-4 shadow-2xl">
            <div class="flex items-center justify-between mb-4">
              <h2 class="text-lg font-semibold text-white">Settings</h2>
              <button
                onClick={() => setShowSettings(false)}
                class="p-1 rounded hover:bg-neutral-700 text-neutral-400 hover:text-white transition-colors"
                aria-label="Close settings"
              >
                <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" class="w-5 h-5">
                  <path d="M6.28 5.22a.75.75 0 00-1.06 1.06L8.94 10l-3.72 3.72a.75.75 0 101.06 1.06L10 11.06l3.72 3.72a.75.75 0 101.06-1.06L11.06 10l3.72-3.72a.75.75 0 00-1.06-1.06L10 8.94 6.28 5.22z" />
                </svg>
              </button>
            </div>
            <p class="text-neutral-400 text-sm">Settings panel placeholder</p>
          </div>
        </div>
      )}

      {/* Settings toggle button — floating over all content */}
      <button
        data-testid="toggle-settings"
        onClick={() => setShowSettings(!showSettings())}
        aria-label="Toggle settings panel"
        aria-expanded={showSettings()}
        aria-controls="settings"
        class="absolute top-12 right-2 z-40 p-2 rounded-lg bg-neutral-800/80 hover:bg-neutral-700 text-neutral-400 hover:text-white transition-colors"
        title="Settings"
      >
        <GearIcon />
      </button>

      <div
        aria-live="polite"
        aria-atomic="true"
        class="sr-only"
        role="status"
      >
        {ariaLiveMessage()}
      </div>
    </div>
  );
};
