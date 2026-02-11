import { type ParentComponent, type JSX, onCleanup, onMount } from "solid-js";
import type { Model } from "../../flexlayout/model/Model";
import type { TabNode } from "../../flexlayout/model/TabNode";
import type { LayoutAction } from "../../flexlayout/model/Action";
import type { Node } from "../../flexlayout/model/Node";
import { CLASSES } from "../../flexlayout/core/Types";
import { createLayoutBridge } from "../bridge";
import { LayoutContext, type LayoutContextValue } from "../context";

export interface LayoutProps {
  model: Model;
  factory: (node: TabNode) => JSX.Element;
  classNameMapper?: (defaultClassName: string) => string;
  onModelChange?: (model: Model, action: LayoutAction) => void;
  onAction?: (action: LayoutAction) => LayoutAction | undefined;
  onContextMenu?: (node: TabNode, event: MouseEvent) => void;
  onAllowDrop?: (dragNode: Node, dropInfo: unknown) => boolean;
}

export const Layout: ParentComponent<LayoutProps> = (props) => {
  let rootRef: HTMLDivElement | undefined;
  let resizeObserver: ResizeObserver | undefined;

  const bridge = createLayoutBridge(props.model);

  const doAction = (action: LayoutAction) => {
    const intercepted = props.onAction?.(action);
    const finalAction = intercepted === undefined && props.onAction
      ? undefined
      : (intercepted ?? action);

    if (finalAction) {
      props.model.doAction(finalAction);
    }
  };

  const actionDisposable = props.model.onDidAction((action) => {
    props.onModelChange?.(props.model, action);
  });

  const mapClassName = (defaultClassName: string): string =>
    props.classNameMapper?.(defaultClassName) ?? defaultClassName;

  const ctxValue: LayoutContextValue = {
    bridge,
    model: props.model,
    doAction,
    classNameMapper: props.classNameMapper,
    factory: props.factory,
  };

  onMount(() => {
    if (!rootRef) return;

    updateRect();

    resizeObserver = new ResizeObserver(() => {
      requestAnimationFrame(() => updateRect());
    });
    resizeObserver.observe(rootRef);
  });

  onCleanup(() => {
    if (resizeObserver && rootRef) {
      resizeObserver.unobserve(rootRef);
      resizeObserver.disconnect();
    }
    resizeObserver = undefined;
    actionDisposable.dispose();
    bridge.dispose();
  });

  function updateRect(): void {
    if (!rootRef) return;
    const domRect = rootRef.getBoundingClientRect();
    if (domRect.width > 0 && domRect.height > 0) {
      props.model.doAction({
        type: "UPDATE_MODEL_ATTRIBUTES",
        data: {
          attributes: {
            rect: {
              x: 0,
              y: 0,
              width: domRect.width,
              height: domRect.height,
            },
          },
        },
      });
    }
  }

  return (
    <LayoutContext.Provider value={ctxValue}>
      <div
        ref={rootRef}
        class={mapClassName(CLASSES.FLEXLAYOUT__LAYOUT)}
        data-layout-path="/"
        style={{ width: "100%", height: "100%", position: "relative" }}
      >
        {props.children}
      </div>
    </LayoutContext.Provider>
  );
};
