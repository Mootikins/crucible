import { Component, ParentComponent, onMount, onCleanup } from 'solid-js';
import { createSolidDockview, type DockviewInstance, type SerializedDockview } from '@/lib/solid-dockview';
import { SessionPanel } from '@/components/SessionPanel';
import { FilesPanel } from '@/components/FilesPanel';
import { EditorPanel } from '@/components/EditorPanel';
import { loadDockviewLayout, loadZoneState, saveDockviewLayout } from '@/lib/layout';
import { ShellLayout } from './ShellLayout';
import 'dockview-core/dist/styles/dockview.css';

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

interface DebouncedFn {
  (): void;
  cancel(): void;
}

function createDebouncedSave(fn: () => void, delay: number): DebouncedFn {
  let timeoutId: ReturnType<typeof setTimeout> | null = null;
  const debouncedFn: DebouncedFn = Object.assign(
    () => {
      if (timeoutId !== null) clearTimeout(timeoutId);
      timeoutId = setTimeout(fn, delay);
    },
    {
      cancel: () => {
        if (timeoutId !== null) {
          clearTimeout(timeoutId);
          timeoutId = null;
        }
      }
    }
  );
  return debouncedFn;
}

export const DockLayout: Component<DockLayoutProps> = (props) => {
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

  const debouncedSaveLayout = createDebouncedSave(() => {
    if (!dockviewInstance) return;
    const serialized = dockviewInstance.component.toJSON();
    saveDockviewLayout(serialized);
    updateZoneDataAttributes();
  }, 300);

  onMount(() => {
    if (!containerRef) return;

    const savedLayout = loadDockviewLayout();
    const zoneVisible = loadZoneState();

    try {
      dockviewInstance = createSolidDockview({
        container: containerRef,
        className: 'dockview-theme-abyss',
        initialLayout: savedLayout as SerializedDockview | undefined,
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
    } catch {
      console.warn('Failed to restore layout, clearing corrupted data and retrying');
      localStorage.removeItem('crucible:layout');
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
    }

    setTimeout(() => {
      if (!dockviewInstance) return;
      const zones: Array<'left' | 'right' | 'bottom'> = ['left', 'right', 'bottom'];
      for (const zone of zones) {
        if (zoneVisible[zone] !== 'visible' && zoneVisible[zone] !== 'pinned') {
          dockviewInstance.setZoneVisible(zone, false);
        }
      }
      updateZoneDataAttributes();
    }, 50);

    onCleanup(() => {
      debouncedSaveLayout.cancel();
      dockviewInstance?.dispose();
    });
  });

  return (
    <ShellLayout
      centerContent={<div ref={(el) => { containerRef = el; }} class="h-full w-full" />}
    />
  );
};
