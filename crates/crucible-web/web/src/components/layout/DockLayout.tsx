import { Component, onMount, onCleanup } from 'solid-js';
import { createSolidDockview, type DockviewInstance } from '@/lib/solid-dockview';
import { getGlobalRegistry, type Zone } from '@/lib/panel-registry';
import { setupCrossZoneDnD } from '@/lib/dnd-bridge';
import { floatPanel, dockPanel, isFloating } from '@/lib/float-manager';
import { migrateOldLayout, loadZoneLayout, saveZoneLayout } from '@/lib/layout';
import type { DockviewGroupPanel, IHeaderActionsRenderer, IGroupHeaderProps } from 'dockview-core';
import { SessionPanel } from '@/components/SessionPanel';
import { FilesPanel } from '@/components/FilesPanel';
import { EditorPanel } from '@/components/EditorPanel';
import { ShellLayout } from './ShellLayout';
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
}

function registerDefaultPanels(chatContent: Component): void {
  const registry = getGlobalRegistry();
  registry.register('sessions', 'Sessions', SessionPanel, 'left');
  registry.register('files', 'Files', FilesPanel, 'left');
  registry.register('chat', 'Chat', chatContent, 'center');
  registry.register('editor', 'Editor', EditorPanel, 'right');
  registry.register('terminal', 'Terminal', BottomPanel, 'bottom');
}

function createFloatActionRenderer(
  zone: Zone,
  instances: Map<Zone, DockviewInstance>,
  getCenterContainer?: () => HTMLElement | undefined,
): (group: DockviewGroupPanel) => IHeaderActionsRenderer {
  return (_group: DockviewGroupPanel) => {
    const el = document.createElement('div');
    el.className = 'dv-float-action';
    el.style.display = 'flex';
    el.style.alignItems = 'center';
    el.style.paddingRight = '4px';

    const btn = document.createElement('button');
    btn.className = 'dv-float-action-btn';
    btn.style.background = 'none';
    btn.style.border = 'none';
    btn.style.cursor = 'pointer';
    btn.style.padding = '2px 4px';
    btn.style.color = 'inherit';
    btn.style.opacity = '0.6';
    btn.style.fontSize = '11px';
    btn.style.lineHeight = '1';
    btn.title = zone === 'center' ? 'Dock panel' : 'Float panel';
    btn.textContent = zone === 'center' ? '⊡' : '⊞';

    btn.addEventListener('mouseenter', () => { btn.style.opacity = '1'; });
    btn.addEventListener('mouseleave', () => { btn.style.opacity = '0.6'; });

    el.appendChild(btn);

    let headerParams: IGroupHeaderProps | null = null;

    btn.addEventListener('click', (e) => {
      e.stopPropagation();
      if (!headerParams) return;

      const activePanel = headerParams.group.activePanel;
      if (!activePanel) return;

      const panelId = activePanel.id;
      const centerInstance = instances.get('center');
      if (!centerInstance) return;

      if (isFloating(panelId)) {
        dockPanel(panelId, centerInstance.api, instances);
      } else if (zone !== 'center') {
        floatPanel(panelId, zone, centerInstance.api, instances, getCenterContainer?.());
      }
    });

    return {
      element: el,
      init(params: IGroupHeaderProps) {
        headerParams = params;
      },
      dispose() {
        headerParams = null;
      },
    };
  };
}

function createZoneDockview(
  container: HTMLElement,
  zone: Zone,
  instances: Map<Zone, DockviewInstance>,
  getCenterContainer?: () => HTMLElement | undefined,
): DockviewInstance {
  const registry = getGlobalRegistry();
  const componentMap = registry.getComponentMap();

  const savedLayout = loadZoneLayout(zone);
  const defaultLayout = registry.getDefaultLayout();
  const panelIds = defaultLayout[zone];

  const panels = panelIds.map((id, index) => {
    const def = registry.get(id);
    if (!def) {
      console.warn(`Panel definition not found for id: ${id}, skipping`);
      return null;
    }
    return {
      id: def.id,
      title: def.title,
      component: def.component,
      position: index > 0 ? { referencePanel: panelIds[0], direction: 'within' as const } : undefined,
    };
  }).filter((p): p is NonNullable<typeof p> => p !== null);

  const instance = createSolidDockview({
    container,
    panels,
    componentMap,
    className: 'dockview-theme-abyss',
    createRightHeaderActionComponent: createFloatActionRenderer(zone, instances, getCenterContainer),
  });

  if (savedLayout) {
    try {
      const parsed = JSON.parse(savedLayout);
      instance.api.fromJSON(parsed);
    } catch {
      /* keep default layout on restore failure */
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

    const layoutDisposables: Array<{ dispose(): void }> = [];

    for (const { zone, ref } of zones) {
      if (!ref) continue;
      const instance = createZoneDockview(ref, zone, instances, () => centerRef);
      instances.set(zone, instance);

      // Save layout on every change
      const disposable = instance.api.onDidLayoutChange(() => {
        const serialized = JSON.stringify(instance.api.toJSON());
        saveZoneLayout(zone, serialized);
      });
      layoutDisposables.push(disposable);
    }

    const cleanupDnD = setupCrossZoneDnD(instances);

    onCleanup(() => {
      for (const d of layoutDisposables) {
        d.dispose();
      }
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
