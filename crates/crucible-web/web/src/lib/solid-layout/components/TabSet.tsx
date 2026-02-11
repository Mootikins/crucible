import { type Component, createMemo, Show } from "solid-js";
import { CLASSES } from "../../flexlayout/core/Types";
import { Action } from "../../flexlayout/model/Action";
import type { IJsonTabSetNode, IJsonTabNode } from "../../flexlayout/types";
import { useLayoutContext } from "../context";
import { TabBar } from "./TabBar";
import { Tab } from "./Tab";

export interface TabSetProps {
  nodeId: string;
}

export const TabSet: Component<TabSetProps> = (props) => {
  const ctx = useLayoutContext();
  const mapClass = (cls: string) => ctx.classNameMapper?.(cls) ?? cls;

  const tabsetNode = createMemo((): IJsonTabSetNode | undefined => {
    const layout = ctx.bridge.store.layout;
    if (!layout) return undefined;
    return findTabSet(layout, props.nodeId);
  });

  const isActive = createMemo(() => ctx.bridge.activeTabsetId() === props.nodeId);

  const isMaximized = createMemo(() => tabsetNode()?.maximized === true);

  const canMaximize = createMemo(() => {
    const node = tabsetNode();
    if (!node) return false;
    return node.enableMaximize !== false;
  });

  const selectedIndex = createMemo(() => {
    const node = tabsetNode();
    if (!node) return -1;
    const sel = node.selected;
    return typeof sel === "number" ? sel : 0;
  });

  const tabs = createMemo((): IJsonTabNode[] => {
    const node = tabsetNode();
    return (node?.children ?? []) as IJsonTabNode[];
  });

  const selectedTab = createMemo((): IJsonTabNode | undefined => {
    const idx = selectedIndex();
    const t = tabs();
    if (idx < 0 || idx >= t.length) return undefined;
    return t[idx];
  });

  const tabLocation = createMemo(() => {
    const node = tabsetNode();
    return (node?.tabLocation as string) ?? "top";
  });

  const path = createMemo(() => tabsetNode()?.path as string | undefined);

  const tabsetClass = createMemo(() => {
    let cls = mapClass(CLASSES.FLEXLAYOUT__TABSET);
    if (isActive()) {
      cls += ` ${mapClass(CLASSES.FLEXLAYOUT__TABSET_SELECTED)}`;
    }
    if (isMaximized()) {
      cls += ` ${mapClass(CLASSES.FLEXLAYOUT__TABSET_MAXIMIZED)}`;
    }
    return cls;
  });

  const handleTabStripPointerDown = (e: PointerEvent) => {
    if (!isActive()) {
      ctx.doAction(Action.setActiveTabset(props.nodeId));
    }
    e.stopPropagation();
  };

  const handleMaximizeClick = (e: MouseEvent) => {
    ctx.doAction(Action.maximizeToggle(props.nodeId));
    e.stopPropagation();
  };

  return (
    <Show when={tabsetNode()}>
      <div
        class={mapClass(CLASSES.FLEXLAYOUT__TABSET_CONTAINER)}
        data-layout-path={path() ? `${path()}/container` : undefined}
      >
        <div
          class={tabsetClass()}
          data-layout-path={path()}
        >
          <Show when={tabLocation() === "top"} fallback={
            <>
              <div
                class={mapClass(CLASSES.FLEXLAYOUT__TABSET_CONTENT)}
                data-layout-path={path() ? `${path()}/content` : undefined}
              >
                <Show when={selectedTab()}>
                  {(tab) => <Tab nodeId={tab().id!} />}
                </Show>
              </div>
              <div
                class={tabStripOuterClass(mapClass, tabLocation(), isActive(), isMaximized())}
                data-layout-path={path() ? `${path()}/tabstrip` : undefined}
                onPointerDown={handleTabStripPointerDown}
              >
                <div class={`${mapClass(CLASSES.FLEXLAYOUT__TABSET_TABBAR_INNER)} ${mapClass(CLASSES.FLEXLAYOUT__TABSET_TABBAR_INNER_ + tabLocation())}`}>
                  <TabBar tabsetId={props.nodeId} />
                </div>
                <div class={mapClass(CLASSES.FLEXLAYOUT__TAB_TOOLBAR)}>
                  <Show when={canMaximize()}>
                    <button
                      type="button"
                      class={`${mapClass(CLASSES.FLEXLAYOUT__TAB_TOOLBAR_BUTTON)} ${mapClass(CLASSES.FLEXLAYOUT__TAB_TOOLBAR_BUTTON_ + (isMaximized() ? "max" : "min"))}`}
                      title={isMaximized() ? "Restore" : "Maximize"}
                      data-layout-path={path() ? `${path()}/button/max` : undefined}
                      onPointerDown={(e) => e.stopPropagation()}
                      onClick={handleMaximizeClick}
                    >
                      {isMaximized() ? "⊡" : "⊞"}
                    </button>
                  </Show>
                </div>
              </div>
            </>
          }>
            <div
              class={tabStripOuterClass(mapClass, tabLocation(), isActive(), isMaximized())}
              data-layout-path={path() ? `${path()}/tabstrip` : undefined}
              onPointerDown={handleTabStripPointerDown}
            >
              <div class={`${mapClass(CLASSES.FLEXLAYOUT__TABSET_TABBAR_INNER)} ${mapClass(CLASSES.FLEXLAYOUT__TABSET_TABBAR_INNER_ + tabLocation())}`}>
                <TabBar tabsetId={props.nodeId} />
              </div>
              <div class={mapClass(CLASSES.FLEXLAYOUT__TAB_TOOLBAR)}>
                <Show when={canMaximize()}>
                  <button
                    type="button"
                    class={`${mapClass(CLASSES.FLEXLAYOUT__TAB_TOOLBAR_BUTTON)} ${mapClass(CLASSES.FLEXLAYOUT__TAB_TOOLBAR_BUTTON_ + (isMaximized() ? "max" : "min"))}`}
                    title={isMaximized() ? "Restore" : "Maximize"}
                    data-layout-path={path() ? `${path()}/button/max` : undefined}
                    onPointerDown={(e) => e.stopPropagation()}
                    onClick={handleMaximizeClick}
                  >
                    {isMaximized() ? "⊡" : "⊞"}
                  </button>
                </Show>
              </div>
            </div>
            <div
              class={mapClass(CLASSES.FLEXLAYOUT__TABSET_CONTENT)}
              data-layout-path={path() ? `${path()}/content` : undefined}
            >
              <Show when={selectedTab()}>
                {(tab) => <Tab nodeId={tab().id!} />}
              </Show>
            </div>
          </Show>
        </div>
      </div>
    </Show>
  );
};

export function tabStripOuterClass(
  mapClass: (cls: string) => string,
  tabLocation: string,
  isActive: boolean,
  isMaximized: boolean,
): string {
  let cls = mapClass(CLASSES.FLEXLAYOUT__TABSET_TABBAR_OUTER);
  cls += ` ${CLASSES.FLEXLAYOUT__TABSET_TABBAR_OUTER_ + tabLocation}`;
  if (isActive) {
    cls += ` ${mapClass(CLASSES.FLEXLAYOUT__TABSET_SELECTED)}`;
  }
  if (isMaximized) {
    cls += ` ${mapClass(CLASSES.FLEXLAYOUT__TABSET_MAXIMIZED)}`;
  }
  return cls;
}

export function findTabSet(node: any, id: string): IJsonTabSetNode | undefined {
  if (node.type === "tabset" && node.id === id) {
    return node as IJsonTabSetNode;
  }
  if (node.children) {
    for (const child of node.children) {
      const found = findTabSet(child, id);
      if (found) return found;
    }
  }
  return undefined;
}
