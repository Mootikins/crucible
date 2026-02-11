import { type Component, createMemo, onMount, onCleanup } from "solid-js";
import { render } from "solid-js/web";
import { CLASSES } from "../../flexlayout/core/Types";
import type { IJsonTabNode } from "../../flexlayout/types";
import type { TabNode } from "../../flexlayout/model/TabNode";
import { useLayoutContext } from "../context";

export interface TabProps {
  nodeId: string;
}

export const Tab: Component<TabProps> = (props) => {
  const ctx = useLayoutContext();
  const mapClass = (cls: string) => ctx.classNameMapper?.(cls) ?? cls;
  let containerRef: HTMLDivElement | undefined;
  let contentContainer: HTMLDivElement | undefined;
  let disposeFn: (() => void) | undefined;

  const tabNode = createMemo((): IJsonTabNode | undefined => {
    const layout = ctx.bridge.store.layout;
    if (!layout) return undefined;
    return findTab(layout, props.nodeId);
  });

  const path = createMemo(() => tabNode()?.path as string | undefined);

  onMount(() => {
    if (!containerRef) return;

    contentContainer = document.createElement("div");
    contentContainer.style.cssText = "width: 100%; height: 100%;";

    const modelNode = ctx.model.getNodeById(props.nodeId) as TabNode | undefined;
    if (modelNode) {
      disposeFn = render(() => ctx.factory(modelNode), contentContainer);
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
