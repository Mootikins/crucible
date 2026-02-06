import { Component, ParentComponent, onMount, onCleanup } from 'solid-js';
import { createSolidDockview, type DockviewInstance } from '@/lib/solid-dockview';
import { getGlobalRegistry, type Zone } from '@/lib/panel-registry';
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
  const defaultLayout = registry.getDefaultLayout();
  const panelIds = defaultLayout[zone];
  const componentMap = registry.getComponentMap();

  const panels = panelIds.map((id, index) => {
    const def = registry.get(id)!;
    return {
      id: def.id,
      title: def.title,
      component: def.component,
      position: index > 0 ? { referencePanel: panelIds[0], direction: 'within' as const } : undefined,
    };
  });

  return createSolidDockview({
    container,
    panels,
    componentMap,
    className: 'dockview-theme-abyss',
  });
}

export const DockLayout: Component<DockLayoutProps> = (props) => {
  const instances = new Map<Zone, DockviewInstance>();
  let leftRef: HTMLDivElement | undefined;
  let centerRef: HTMLDivElement | undefined;
  let rightRef: HTMLDivElement | undefined;
  let bottomRef: HTMLDivElement | undefined;

  onMount(() => {
    registerDefaultPanels(props.chatContent);

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
    }

    onCleanup(() => {
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
    />
  );
};
