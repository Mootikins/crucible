import type { Zone } from './panel-registry';
import type { DockviewInstance } from './solid-dockview';

interface Disposable {
  dispose(): void;
}

function findSourceInstance(
  instances: Map<Zone, DockviewInstance>,
  viewId: string,
): DockviewInstance | undefined {
  for (const [, instance] of instances) {
    if (instance.api.id === viewId) {
      return instance;
    }
  }
  return undefined;
}

/**
 * Bridge cross-zone DnD between multiple dockview instances via `onUnhandledDragOverEvent`.
 *
 * Relies on dockview's `LocalSelectionTransfer` singleton which stores
 * `PanelTransfer(viewId, groupId, panelId)` during tab drags —
 * `event.getData()` reads this to identify source instance and panel.
 *
 * @returns Cleanup function removing all event listeners.
 */
export function setupCrossZoneDnD(
  instances: Map<Zone, DockviewInstance>,
): () => void {
  const disposables: Disposable[] = [];

  for (const [, targetInstance] of instances) {
    const targetApi = targetInstance.api;

    const overDisposable = targetApi.onUnhandledDragOverEvent((event) => {
      const data = event.getData();
      if (data && data.viewId !== targetApi.id) {
        event.accept();
      }
    });
    disposables.push(overDisposable);

    const dropDisposable = targetApi.onDidDrop((event) => {
      const data = event.getData();
      if (!data || data.viewId === targetApi.id || !data.panelId) {
        return;
      }

      const sourceInstance = findSourceInstance(instances, data.viewId);
      if (!sourceInstance) {
        return;
      }

      const sourcePanel = sourceInstance.api.getPanel(data.panelId);
      if (!sourcePanel) {
        return;
      }

      const componentId = sourcePanel.id;
      const title = sourcePanel.title ?? componentId;

      sourceInstance.api.removePanel(sourcePanel);

      targetApi.addPanel({
        id: componentId,
        component: componentId,
        title,
        position: event.group
          ? { referenceGroup: event.group, direction: event.position }
          : undefined,
      });
    });
    disposables.push(dropDisposable);
  }

  return () => {
    for (const d of disposables) {
      d.dispose();
    }
  };
}
