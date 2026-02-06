import { Component, ParentComponent, onMount, onCleanup } from 'solid-js';
import { createSolidDockview, type DockviewInstance } from '@/lib/solid-dockview';
import { getGlobalRegistry, type Zone } from '@/lib/panel-registry';
import { setupCrossZoneDnD } from '@/lib/dnd-bridge';
import { migrateOldLayout, loadZoneLayout, saveZoneLayout } from '@/lib/layout';
import { SessionPanel } from '@/components/SessionPanel';
import { FilesPanel } from '@/components/FilesPanel';
import { EditorPanel } from '@/components/EditorPanel';
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

function registerDefaultPanels(chatContent: Component): void {
  const registry = getGlobalRegistry();
  registry.register('sessions', 'Sessions', SessionPanel, 'left');
  registry.register('files', 'Files', FilesPanel, 'left');
  registry.register('chat', 'Chat', chatContent, 'center');
  registry.register('editor', 'Editor', EditorPanel, 'right');
  registry.register('terminal', 'Terminal', BottomPanel, 'bottom');
}

function createZoneDockview(
  container: HTMLElement,
  zone: Zone,
): DockviewInstance {
  const registry = getGlobalRegistry();
  const componentMap = registry.getComponentMap();

  // Try to load per-zone layout first
  const savedLayout = loadZoneLayout(zone);
  
  // If no saved layout, use default layout for this zone
  const defaultLayout = registry.getDefaultLayout();
  const panelIds = defaultLayout[zone];

  const panels = panelIds.map((id, index) => {
    const def = registry.get(id)!;
    return {
      id: def.id,
      title: def.title,
      component: def.component,
      position: index > 0 ? { referencePanel: panelIds[0], direction: 'within' as const } : undefined,
    };
  });

  const instance = createSolidDockview({
    container,
    panels,
    componentMap,
    className: 'dockview-theme-abyss',
  });

  // If we have a saved layout, restore it
  if (savedLayout) {
    try {
      const parsed = JSON.parse(savedLayout);
      instance.api.fromJSON(parsed);
    } catch {
      // If restore fails, keep default layout
    }
  }

  return instance;
}

export const DockLayout: Component<DockLayoutProps> = (props) => {
  const instances = new Map<Zone, DockviewInstance>();
  let leftRef: HTMLDivElement | undefined;
  let centerRef: HTMLDivElement | undefined;
  let rightRef: HTMLDivElement | undefined;
  let bottomRef: HTMLDivElement | undefined;

  const handleZoneTransitionEnd = (_zone: Zone) => {
    for (const [z, instance] of instances.entries()) {
      const ref = { left: leftRef, center: centerRef, right: rightRef, bottom: bottomRef }[z];
      if (ref) {
        instance.api.layout(ref.clientWidth, ref.clientHeight, true);
      }
    }
  };

  onMount(() => {
    registerDefaultPanels(props.chatContent);

    // Migrate old single-key layout to per-zone keys
    migrateOldLayout();

    const zones: Array<{ zone: Zone; ref: HTMLDivElement | undefined }> = [
      { zone: 'left', ref: leftRef },
      { zone: 'center', ref: centerRef },
      { zone: 'right', ref: rightRef },
      { zone: 'bottom', ref: bottomRef },
    ];

    for (const { zone, ref } of zones) {
      if (!ref) continue;
      const instance = createZoneDockview(ref, zone);
      instances.set(zone, instance);

      // Save layout on every change
      instance.api.onDidLayoutChange(() => {
        const serialized = JSON.stringify(instance.api.toJSON());
        saveZoneLayout(zone, serialized);
      });
    }

    const cleanupDnD = setupCrossZoneDnD(instances);

    onCleanup(() => {
      cleanupDnD();
      for (const instance of instances.values()) {
        instance.dispose();
      }
      instances.clear();
    });
  });

  return (
    <ShellLayout
      leftRef={(el) => { leftRef = el; }}
      centerRef={(el) => { centerRef = el; }}
      rightRef={(el) => { rightRef = el; }}
      bottomRef={(el) => { bottomRef = el; }}
      onZoneTransitionEnd={handleZoneTransitionEnd}
    />
  );
};
