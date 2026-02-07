import { Component, onMount, onCleanup } from 'solid-js';
import { createSolidDockview, type DockviewInstance, type DockviewApi, type DockedSide } from '@/lib/solid-dockview';
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

    const zoneToSide: Record<string, DockedSide> = {
      left: 'left',
      right: 'right',
      bottom: 'bottom',
    };

    for (const [zone, side] of Object.entries(zoneToSide)) {
      const panelIds = defaultLayout[zone as keyof typeof defaultLayout];
      if (panelIds.length === 0) continue;

      const firstPanelId = panelIds[0];
      const firstDef = registry.get(firstPanelId);
      if (!firstDef) continue;

      const firstPanel = instance.api.addPanel({
        id: firstDef.id,
        component: firstDef.id,
        title: firstDef.title,
      });

      instance.component.addDockedGroup(firstPanel, {
        side,
        size: 300,
        collapsed: false,
      });

      for (let i = 1; i < panelIds.length; i++) {
        const def = registry.get(panelIds[i]);
        if (!def) continue;

        const dockedGroups = instance.component.getDockedGroups(side);
        if (dockedGroups.length === 0) continue;

        const group = dockedGroups[0].group;
        instance.api.addPanel({
          id: def.id,
          component: def.id,
          title: def.title,
          position: {
            referenceGroup: group.id,
            direction: 'within',
          },
        });
      }
    }

    const layoutDisposable = instance.api.onDidLayoutChange(() => {
      if (instance) {
        const serialized = JSON.stringify(instance.api.toJSON());
        saveZoneLayout('center', serialized);
      }
    });

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

      let side: DockedSide | null = null;
      if (event.code === 'KeyB' && !event.shiftKey) side = 'left';
      else if (event.code === 'KeyB' && event.shiftKey) side = 'right';
      else if (event.code === 'KeyJ') side = 'bottom';

      if (side && instance) {
        event.preventDefault();
        instance.component.toggleDockedSide(side);
      }
    };

    document.addEventListener('keydown', handleKeyDown);

    props.onReady?.(instance.api);

    onCleanup(() => {
      document.removeEventListener('keydown', handleKeyDown);
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
