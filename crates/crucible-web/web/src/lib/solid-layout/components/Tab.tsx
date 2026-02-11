import { type Component, createMemo, onMount, onCleanup } from "solid-js";
import { render } from "solid-js/web";
import { CLASSES } from "../../flexlayout/core/Types";
import type { IJsonTabNode } from "../../flexlayout/types";
import type { TabNode } from "../../flexlayout/model/TabNode";
import { useLayoutContext } from "../context";

export interface TabProps {
  /** JSON node reference from the store */
  node?: IJsonTabNode;
  /** Actual model node (avoids ID lookup issues) */
  modelNode?: TabNode;
  /** Optional ID for reference */
  nodeId?: string;
}

export const Tab: Component<TabProps> = (props) => {
  const ctx = useLayoutContext();
  const mapClass = (cls: string) => ctx.classNameMapper?.(cls) ?? cls;
  let containerRef: HTMLDivElement | undefined;
  let contentContainer: HTMLDivElement | undefined;
  let disposeFn: (() => void) | undefined;

  const tabNode = createMemo((): IJsonTabNode | undefined => {
    if (props.node) return props.node;
    if (!props.nodeId) return undefined;
    const layout = ctx.bridge.store.layout;
    if (!layout) return undefined;
    return findTab(layout, props.nodeId);
  });

  const path = createMemo(() => tabNode()?.path as string | undefined);

  const effectiveId = createMemo(() => tabNode()?.id ?? props.nodeId);

  onMount(() => {
    if (!containerRef) return;

    contentContainer = document.createElement("div");
    contentContainer.style.cssText = "width: 100%; height: 100%;";

    // Prefer direct model node to avoid ID stripping issues
    const modelNode = props.modelNode;
    if (modelNode) {
      disposeFn = render(() => ctx.factory(modelNode), contentContainer);
    } else {
      // Fallback to ID lookup
      const id = effectiveId();
      if (id) {
        const lookedUpNode = ctx.model.getNodeById(id) as TabNode | undefined;
        if (lookedUpNode) {
          disposeFn = render(() => ctx.factory(lookedUpNode), contentContainer);
        }
      }
    }

    containerRef.appendChild(contentContainer);
  });

  onCleanup(() => {
    disposeFn?.();
    disposeFn = undefined;
  });

  return (
    <div
      ref={containerRef}
      class={mapClass(CLASSES.FLEXLAYOUT__TAB)}
      data-layout-path={path()}
      style={{ height: "100%", width: "100%" }}
    />
  );
};

export function findTab(node: any, id: string): IJsonTabNode | undefined {
  if (node.type === "tab" && node.id === id) return node as IJsonTabNode;
  if (node.children) {
    for (const child of node.children) {
      const found = findTab(child, id);
      if (found) return found;
    }
  }
  return undefined;
}
