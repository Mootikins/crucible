import { Component, onMount, onCleanup } from 'solid-js';
import { Layout } from '@/lib/solid-flexlayout';
import { Model } from '@/lib/flexlayout/model/Model';
import { TabNode } from '@/lib/flexlayout/model/TabNode';
import { Action } from '@/lib/flexlayout/model/Action';
import { getGlobalRegistry } from '@/lib/panel-registry';
import { SessionPanel } from '@/components/SessionPanel';
import { FilesPanel } from '@/components/FilesPanel';
import { EditorPanel } from '@/components/EditorPanel';
import type { IJsonModel } from '@/lib/flexlayout/types';

export const BottomPanel: Component = () => (
  <div class="h-full flex items-center justify-center bg-neutral-900 text-neutral-500">
    <div class="text-center">
      <div class="text-2xl mb-1">📋</div>
      <div class="text-xs">Output / Terminal</div>
    </div>
  </div>
);

const LAYOUT_STORAGE_KEY = 'crucible:flexlayout';

const DEFAULT_LAYOUT: IJsonModel = {
  global: {
    splitterSize: 4,
    splitterExtra: 4,
    tabSetEnableMaximize: true,
    tabSetEnableClose: false,
    tabEnableClose: false,
    tabEnableDrag: true,
    tabSetEnableTabStrip: true,
    borderSize: 250,
    borderMinSize: 100,
    borderEnableAutoHide: true,
  },
  borders: [
    {
      type: 'border',
      location: 'left',
      size: 280,
      children: [
        { type: 'tab', name: 'Sessions', component: 'sessions', id: 'sessions' },
        { type: 'tab', name: 'Files', component: 'files', id: 'files' },
      ],
    },
    {
      type: 'border',
      location: 'right',
      size: 350,
      children: [
        { type: 'tab', name: 'Editor', component: 'editor', id: 'editor' },
      ],
    },
    {
      type: 'border',
      location: 'bottom',
      size: 200,
      children: [
        { type: 'tab', name: 'Terminal', component: 'terminal', id: 'terminal' },
      ],
    },
  ],
  layout: {
    type: 'row',
    children: [
      {
        type: 'tabset',
        children: [
          { type: 'tab', name: 'Chat', component: 'chat', id: 'chat' },
        ],
      },
    ],
  },
};

interface FlexLayoutProps {
  chatContent: Component;
  containerRef?: (el: HTMLDivElement) => void;
  onReady?: (model: Model) => void;
}

function registerDefaultPanels(chatContent: Component): void {
  const registry = getGlobalRegistry();
  registry.register('sessions', 'Sessions', SessionPanel, 'left', 'list');
  registry.register('files', 'Files', FilesPanel, 'left', 'folder');
  registry.register('chat', 'Chat', chatContent, 'center', 'message');
  registry.register('editor', 'Editor', EditorPanel, 'right', 'code');
  registry.register('terminal', 'Terminal', BottomPanel, 'bottom', 'terminal');
}

export const FlexLayout: Component<FlexLayoutProps> = (props) => {
  let model: Model;

  registerDefaultPanels(props.chatContent);

  const registry = getGlobalRegistry();
  const componentMap = registry.getComponentMap();

  const savedLayout = localStorage.getItem(LAYOUT_STORAGE_KEY);
  let layoutJson: IJsonModel;

  if (savedLayout) {
    try {
      layoutJson = JSON.parse(savedLayout);
    } catch {
      layoutJson = DEFAULT_LAYOUT;
    }
  } else {
    localStorage.removeItem('crucible:layout');
    layoutJson = DEFAULT_LAYOUT;
  }

  model = Model.fromJson(layoutJson);

  const factory = (node: TabNode) => {
    const componentName = node.getComponent();
    if (componentName) {
      const Comp = componentMap.get(componentName);
      if (Comp) {
        return <Comp />;
      }
    }
    return (
      <div class="h-full flex items-center justify-center text-neutral-500">
        <span>Unknown panel: {componentName ?? node.getName()}</span>
      </div>
    );
  };

  const onModelChange = () => {
    try {
      const json = model.toJson();
      localStorage.setItem(LAYOUT_STORAGE_KEY, JSON.stringify(json));
    } catch { }
  };

  const handleKeyDown = (event: KeyboardEvent) => {
    const target = event.target;
    const isEditable =
      target instanceof HTMLInputElement ||
      target instanceof HTMLTextAreaElement ||
      (target instanceof HTMLElement && target.contentEditable === 'true');
    if (isEditable) return;

    const userAgentData = (navigator as Navigator & { userAgentData?: { platform?: string } })
      .userAgentData;
    const isMac =
      userAgentData?.platform === 'macOS' || /Mac|iPod|iPhone|iPad/.test(navigator.userAgent);
    const modifier = isMac ? event.metaKey : event.ctrlKey;
    if (!modifier) return;

    let borderLocation: string | null = null;
    if (event.code === 'KeyB' && !event.shiftKey) borderLocation = 'left';
    else if (event.code === 'KeyB' && event.shiftKey) borderLocation = 'right';
    else if (event.code === 'KeyJ') borderLocation = 'bottom';

    if (borderLocation) {
      event.preventDefault();
      toggleBorder(borderLocation);
    }
  };

  const toggleBorder = (location: string) => {
    const borderSet = model.getBorderSet();
    const border = borderSet.getBorder(location);
    if (!border) return;

    const selected = border.getSelected();
    if (selected === -1) {
      if (border.getChildren().length > 0) {
        const firstTab = border.getChildren()[0] as TabNode;
        model.doAction(Action.selectTab(firstTab.getId()));
      }
    } else {
      const selectedTab = border.getChildren()[selected] as TabNode;
      model.doAction(Action.selectTab(selectedTab.getId()));
    }
  };

  onMount(() => {
    document.addEventListener('keydown', handleKeyDown);
    props.onReady?.(model);
  });

  onCleanup(() => {
    document.removeEventListener('keydown', handleKeyDown);
  });

  return (
    <div
      ref={(el) => props.containerRef?.(el)}
      class="h-full w-full"
    >
      <Layout
        model={model}
        factory={factory}
        onModelChange={onModelChange}
      />
    </div>
  );
};
