import { Component, ParentComponent, createSignal, onMount, onCleanup } from 'solid-js';
import { createSolidDockview, type DockviewInstance, type Zone } from '@/lib/solid-dockview';
import { SessionPanel } from '@/components/SessionPanel';
import { FilesPanel } from '@/components/FilesPanel';
import { EditorPanel } from '@/components/EditorPanel';
import { BreadcrumbNav } from '@/components/BreadcrumbNav';
import { loadZoneState, saveZoneState, saveDockviewLayout, loadDockviewLayout, type ZoneState } from '@/lib/layout';
import 'dockview-core/dist/styles/dockview.css';

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

export const ChatPanel: ParentComponent = (props) => (
  <div class="h-full flex flex-col bg-neutral-900">{props.children}</div>
);

export const BottomPanel: Component = () => (
  <div class="h-full flex items-center justify-center bg-neutral-900 text-neutral-500">
    <div class="text-center">
      <div class="text-2xl mb-1">📋</div>
      <div class="text-xs">Output / Terminal</div>
    </div>
  </div>
);

interface DockLayoutProps {
  chatContent: Component;
}

function debounce<T extends (...args: unknown[]) => void>(fn: T, delay: number): T {
  let timeoutId: ReturnType<typeof setTimeout>;
  return ((...args: unknown[]) => {
    clearTimeout(timeoutId);
    timeoutId = setTimeout(() => fn(...args), delay);
  }) as T;
}

type CollapseMode = 'hidden' | 'iconRail';

export const DockLayout: Component<DockLayoutProps> = (props) => {
  const [showSettings, setShowSettings] = createSignal(false);
  const [zoneVisible, setZoneVisible] = createSignal<ZoneState>(loadZoneState());
  const [ariaLiveMessage, setAriaLiveMessage] = createSignal('');
  
  const [collapseMode] = createSignal<CollapseMode>(
    (localStorage.getItem('crucible:collapse-mode') as CollapseMode) || 'hidden'
  );

  let containerRef: HTMLDivElement | undefined;
  let dockviewInstance: DockviewInstance | null = null;

  const updateZoneDataAttributes = () => {
    if (!dockviewInstance) return;
    const zones = dockviewInstance.getGroupZones();
    for (const group of dockviewInstance.api.groups) {
      const zone = zones.get(group.id);
      if (zone) {
        group.element.setAttribute('data-zone', zone);
      }
    }
  };

  const debouncedSaveLayout = debounce(() => {
    if (!dockviewInstance) return;
    const serialized = dockviewInstance.component.toJSON();
    saveDockviewLayout(serialized);
    updateZoneDataAttributes();
  }, 300);

  const toggleZone = (zone: Exclude<Zone, 'center'>) => {
    if (!dockviewInstance) return;

    const newVisible = !zoneVisible()[zone];
    setZoneVisible(prev => ({ ...prev, [zone]: newVisible }));
    saveZoneState({ ...zoneVisible(), [zone]: newVisible });

    dockviewInstance.setZoneVisible(zone, newVisible);

    const zoneNames: Record<Exclude<Zone, 'center'>, string> = {
      left: 'Left zone',
      right: 'Right zone',
      bottom: 'Bottom zone',
    };
    setAriaLiveMessage(`${zoneNames[zone]} ${newVisible ? 'expanded' : 'collapsed'}`);
  };

  onMount(() => {
    if (!containerRef) return;

    const savedLayout = loadDockviewLayout();
    
    dockviewInstance = createSolidDockview({
      container: containerRef,
      className: 'dockview-theme-abyss',
      panels: [
        {
          id: 'sessions',
          title: 'Sessions',
          component: SessionPanel,
          position: { direction: 'left' },
        },
        {
          id: 'files',
          title: 'Files',
          component: FilesPanel,
          position: { referencePanel: 'sessions', direction: 'within' },
        },
        {
          id: 'editor',
          title: 'Editor',
          component: EditorPanel,
        },
        {
          id: 'chat',
          title: 'Chat',
          component: props.chatContent,
          position: { direction: 'right' },
        },
        {
          id: 'terminal',
          title: 'Terminal',
          component: BottomPanel,
          position: { direction: 'below' },
        },
      ],
      onLayoutChange: debouncedSaveLayout,
    });

    if (savedLayout) {
      try {
        type FromJSONParam = Parameters<typeof dockviewInstance.component.fromJSON>[0];
        dockviewInstance.component.fromJSON(savedLayout as FromJSONParam);
      } catch {
        console.warn('Failed to restore layout, using default');
      }
    }

    setTimeout(() => {
      if (!dockviewInstance) return;
      const visible = zoneVisible();
      const zones: Array<'left' | 'right' | 'bottom'> = ['left', 'right', 'bottom'];
      for (const zone of zones) {
        if (!visible[zone]) dockviewInstance.setZoneVisible(zone, false);
      }
      updateZoneDataAttributes();
    }, 50);

    onCleanup(() => dockviewInstance?.dispose());

    const handleKeyDown = (event: KeyboardEvent) => {
      const target = event.target;
      const isEditable = target instanceof HTMLInputElement || 
          target instanceof HTMLTextAreaElement ||
          (target instanceof HTMLElement && target.contentEditable === 'true');
      if (isEditable) return;

      const modifier = navigator.platform.includes('Mac') ? event.metaKey : event.ctrlKey;
      if (!modifier) return;

      let zone: Exclude<Zone, 'center'> | null = null;
      if (event.key === 'b' && !event.shiftKey) zone = 'left';
      else if (event.key === 'B' && event.shiftKey) zone = 'right';
      else if (event.key === 'j') zone = 'bottom';

      if (zone) {
        event.preventDefault();
        toggleZone(zone);
      }
    };

    document.addEventListener('keydown', handleKeyDown);
    onCleanup(() => document.removeEventListener('keydown', handleKeyDown));
  });

  return (
    <div class="h-screen w-screen flex flex-col bg-neutral-950">
      <BreadcrumbNav />

      <div class="flex-1 flex overflow-hidden">
        <div class="flex flex-col justify-center border-r border-neutral-800 bg-neutral-900">
          <button
            data-testid="toggle-left"
            onClick={() => toggleZone('left')}
            aria-label="Toggle left sidebar"
            aria-expanded={zoneVisible().left}
            aria-controls="sessions"
            class={`p-2 transition-colors ${zoneVisible().left ? 'text-blue-400' : 'text-neutral-500 hover:text-neutral-300'}`}
            title="Toggle left sidebar (⌘B)"
          >
            <SidebarIcon side="left" />
          </button>
        </div>

        {!zoneVisible().left && collapseMode() === 'iconRail' && (
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

        <div class="flex-1 flex flex-col overflow-hidden">
          <div class="flex-1 overflow-hidden relative">
            <button
              data-testid="toggle-settings"
              onClick={() => setShowSettings(!showSettings())}
              aria-label="Toggle settings panel"
              aria-expanded={showSettings()}
              aria-controls="settings"
              class="absolute top-2 right-2 z-50 p-2 rounded-lg bg-neutral-800/80 hover:bg-neutral-700 text-neutral-400 hover:text-white transition-colors"
              title="Settings"
            >
              <GearIcon />
            </button>

            <div ref={containerRef} class="h-full w-full" />
          </div>

          <div class="flex justify-center border-t border-neutral-800 bg-neutral-900">
            <button
              data-testid="toggle-bottom"
              onClick={() => toggleZone('bottom')}
              aria-label="Toggle bottom panel"
              aria-expanded={zoneVisible().bottom}
              aria-controls="bottom"
              class={`p-1.5 transition-colors ${zoneVisible().bottom ? 'text-blue-400' : 'text-neutral-500 hover:text-neutral-300'}`}
              title="Toggle bottom panel (⌘J)"
            >
              <BottomPanelIcon />
            </button>
          </div>
        </div>

        {!zoneVisible().right && collapseMode() === 'iconRail' && (
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

        <div class="flex flex-col justify-center border-l border-neutral-800 bg-neutral-900">
          <button
            data-testid="toggle-right"
            onClick={() => toggleZone('right')}
            aria-label="Toggle right sidebar"
            aria-expanded={zoneVisible().right}
            aria-controls="editor"
            class={`p-2 transition-colors ${zoneVisible().right ? 'text-blue-400' : 'text-neutral-500 hover:text-neutral-300'}`}
            title="Toggle right sidebar (⌘⇧B)"
          >
            <SidebarIcon side="right" />
          </button>
        </div>
      </div>

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
