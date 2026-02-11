/**
 * Reactive bridge: the ONLY place where FlexLayout Model meets SolidJS.
 *
 * Creates a SolidJS store from `model.toJson()`, reconciles on every change,
 * and maintains hot-path signals for high-frequency lookups.
 */

import { createStore, reconcile } from "solid-js/store";
import { createSignal, batch, type Accessor } from "solid-js";
import type { Model } from "../flexlayout/model/Model";
import type { IJsonModel } from "../flexlayout/types";
import type { IDisposable } from "../flexlayout/model/Event";

export interface DragState {
  draggingNodeId: string;
  dropTargetId?: string;
  dropLocation?: string;
}

export interface LayoutBridge {
  store: IJsonModel;
  selectedTabId: Accessor<string | undefined>;
  activeTabsetId: Accessor<string | undefined>;
  dragState: Accessor<DragState | null>;
  setDragState: (state: DragState | null) => void;
  dispose: () => void;
}

function extractSelectedTabId(model: Model): string | undefined {
  const activeTabset = model.getActiveTabset();
  if (!activeTabset) return undefined;
  return activeTabset.getSelectedNode()?.getId();
}

function extractActiveTabsetId(model: Model): string | undefined {
  return model.getActiveTabset()?.getId();
}

/**
 * Create a reactive SolidJS store that mirrors a FlexLayout Model.
 *
 * Reconciles with `{ key: "id" }` so unchanged subtrees keep referential
 * identity. Hot-path signals update inside `batch()` to prevent intermediate
 * renders.
 */
export function createLayoutBridge(model: Model): LayoutBridge {
  const [store, setStore] = createStore<IJsonModel>(model.toJson());

  const [selectedTabId, setSelectedTabId] = createSignal<string | undefined>(
    extractSelectedTabId(model),
  );
  const [activeTabsetId, setActiveTabsetId] = createSignal<string | undefined>(
    extractActiveTabsetId(model),
  );
  const [dragState, setDragState] = createSignal<DragState | null>(null);

  const disposables: IDisposable[] = [];

  const changeDisposable = model.onDidChange(() => {
    batch(() => {
      setStore(reconcile(model.toJson(), { key: "id" }));
      setSelectedTabId(extractSelectedTabId(model));
      setActiveTabsetId(extractActiveTabsetId(model));
    });
  });
  disposables.push(changeDisposable);

  function dispose() {
    for (const d of disposables) {
      d.dispose();
    }
    disposables.length = 0;
  }

  return {
    store,
    selectedTabId,
    activeTabsetId,
    dragState,
    setDragState,
    dispose,
  };
}

export function useLayoutStore(bridge: LayoutBridge): IJsonModel {
  return bridge.store;
}

export function useSelectedTab(bridge: LayoutBridge): Accessor<string | undefined> {
  return bridge.selectedTabId;
}

export function useDragState(bridge: LayoutBridge): Accessor<DragState | null> {
  return bridge.dragState;
}

export function useActiveTabset(bridge: LayoutBridge): Accessor<string | undefined> {
  return bridge.activeTabsetId;
}
