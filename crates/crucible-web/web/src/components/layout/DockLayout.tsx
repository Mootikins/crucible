import { Component, onMount, onCleanup } from 'solid-js';
import { createSolidDockview, type DockviewInstance, type DockviewApi } from '@/lib/solid-dockview';
import { getGlobalRegistry } from '@/lib/panel-registry';
import { loadZoneLayout, saveZoneLayout } from '@/lib/layout';
import { SessionPanel } from '@/components/SessionPanel';
import { FilesPanel } from '@/components/FilesPanel';
import { EditorPanel } from '@/components/EditorPanel';
import 'dockview-core/dist/styles/dockview.css';

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
  containerRef?: (el: HTMLDivElement) => void;
  onReady?: (api: DockviewApi) => void;
}

function registerDefaultPanels(chatContent: Component): void {
  const registry = getGlobalRegistry();
  registry.register('sessions', 'Sessions', SessionPanel, 'left', 'list');
  registry.register('files', 'Files', FilesPanel, 'left', 'folder');
  registry.register('chat', 'Chat', chatContent, 'center', 'message');
  registry.register('editor', 'Editor', EditorPanel, 'right', 'code');
  registry.register('terminal', 'Terminal', BottomPanel, 'bottom', 'terminal');
}

export const DockLayout: Component<DockLayoutProps> = (props) => {
  let containerRef: HTMLDivElement | undefined;
  let instance: DockviewInstance | undefined;

  onMount(() => {
    registerDefaultPanels(props.chatContent);

    if (!containerRef) return;

    const registry = getGlobalRegistry();
    const componentMap = registry.getComponentMap();
    const defaultLayout = registry.getDefaultLayout();
    const centerPanelIds = defaultLayout.center;

    const panels = centerPanelIds.map((id, index) => {
      const def = registry.get(id);
      if (!def) {
        console.warn(`Panel definition not found for id: ${id}, skipping`);
        return null;
      }
      return {
        id: def.id,
        title: def.title,
        component: def.component,
        position: index > 0 ? { referencePanel: centerPanelIds[0], direction: 'within' as const } : undefined,
      };
    }).filter((p): p is NonNullable<typeof p> => p !== null);

    const savedLayout = loadZoneLayout('center');

    instance = createSolidDockview({
      container: containerRef,
      panels,
      componentMap,
      className: 'dockview-theme-abyss',
    });

    if (savedLayout) {
      try {
        const parsed = JSON.parse(savedLayout);
        instance.api.fromJSON(parsed);
      } catch {
        /* keep default layout on restore failure */
      }
    }

    const layoutDisposable = instance.api.onDidLayoutChange(() => {
      if (instance) {
        const serialized = JSON.stringify(instance.api.toJSON());
        saveZoneLayout('center', serialized);
      }
    });

    props.onReady?.(instance.api);

    onCleanup(() => {
      layoutDisposable.dispose();
      instance?.dispose();
      instance = undefined;
    });
  });

  return (
    <div
      ref={(el) => {
        containerRef = el;
        props.containerRef?.(el);
      }}
      class="h-full w-full"
    />
  );
};
