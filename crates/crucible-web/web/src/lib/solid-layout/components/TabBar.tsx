import { type Component, createMemo, For, Show } from "solid-js";
import { CLASSES } from "../../flexlayout/core/Types";
import { Action } from "../../flexlayout/model/Action";
import type { IJsonTabSetNode, IJsonTabNode } from "../../flexlayout/types";
import { useLayoutContext } from "../context";
import { findTabSet } from "./TabSet";

export interface TabBarProps {
  /** Direct node reference from the store (preferred) */
  node?: IJsonTabSetNode;
  /** Fallback: look up by ID */
  tabsetId?: string;
}

export const TabBar: Component<TabBarProps> = (props) => {
  const ctx = useLayoutContext();
  const mapClass = (cls: string) => ctx.classNameMapper?.(cls) ?? cls;

  const tabsetNode = createMemo((): IJsonTabSetNode | undefined => {
    if (props.node) return props.node;
    if (!props.tabsetId) return undefined;
    const layout = ctx.bridge.store.layout;
    if (!layout) return undefined;
    return findTabSet(layout, props.tabsetId);
  });

  const tabs = createMemo((): IJsonTabNode[] => {
    const node = tabsetNode();
    return (node?.children ?? []) as IJsonTabNode[];
  });

  const selectedIndex = createMemo(() => {
    const node = tabsetNode();
    if (!node) return -1;
    const sel = node.selected;
    return typeof sel === "number" ? sel : 0;
  });

  const tabLocation = createMemo(() => {
    const node = tabsetNode();
    return (node?.tabLocation as string) ?? "top";
  });

  const tabsetPath = createMemo(() => tabsetNode()?.path as string | undefined);

  return (
    <div
      class={`${mapClass(CLASSES.FLEXLAYOUT__TABSET_TABBAR_INNER_TAB_CONTAINER)} ${mapClass(CLASSES.FLEXLAYOUT__TABSET_TABBAR_INNER_TAB_CONTAINER_ + tabLocation())}`}
      data-layout-path={tabsetPath() ? `${tabsetPath()}/tabs` : undefined}
    >
      <For each={tabs()}>
        {(tab, index) => {
          const isSelected = createMemo(() => index() === selectedIndex());
          const buttonPath = createMemo(() =>
            tabsetPath() ? `${tabsetPath()}/tb${index()}` : undefined,
          );

          const buttonClass = createMemo(() => {
            let cls = mapClass(CLASSES.FLEXLAYOUT__TAB_BUTTON);
            cls += ` ${mapClass(CLASSES.FLEXLAYOUT__TAB_BUTTON + "_" + tabLocation())}`;
            cls += ` ${mapClass(CLASSES.FLEXLAYOUT__TAB_BUTTON + (isSelected() ? "--selected" : "--unselected"))}`;
            if (tab.className) cls += ` ${tab.className}`;
            return cls;
          });

          const handleClick = () => {
            if (!isSelected() && tab.id) {
              ctx.doAction(Action.selectTab(tab.id));
            }
          };

          const handleClose = (e: MouseEvent) => {
            if (tab.id) {
              ctx.doAction(Action.deleteTab(tab.id));
            }
            e.stopPropagation();
          };

           const enableClose = createMemo(() => tab.enableClose !== false);
           
           const closeType = createMemo(() => {
             const type = tab.closeType as number | undefined;
             return type ?? 0;
           });
           
           const shouldRenderCloseButton = createMemo(() => {
             if (!enableClose()) return false;
             const type = closeType();
             if (type === 2) return false;
             return true;
           });
           
            const closeButtonClass = createMemo(() => {
              const type = closeType();
              if (type === 1) {
                return `${mapClass(CLASSES.FLEXLAYOUT__TAB_BUTTON_TRAILING)} flexlayout__tab_button--close-on-hover`;
              }
              return mapClass(CLASSES.FLEXLAYOUT__TAB_BUTTON_TRAILING);
            });

            const isUrlIcon = createMemo(() => {
              const icon = tab.icon as string | undefined;
              if (!icon) return false;
              return /^(https?:\/\/|\/|data:)/.test(icon);
            });

            return (
              <>
                <div
                  class={buttonClass()}
                  data-layout-path={buttonPath()}
                  data-state={isSelected() ? "selected" : "unselected"}
                  title={tab.helpText as string ?? ""}
                  onClick={handleClick}
                >
                  <Show when={tab.icon}>
                    <div class={mapClass(CLASSES.FLEXLAYOUT__TAB_BUTTON_LEADING)}>
                      <Show when={isUrlIcon()} fallback={<span class={mapClass("flexlayout__tab_button_icon_text")}>{tab.icon as string}</span>}>
                        <img src={tab.icon as string} alt="" />
                      </Show>
                    </div>
                  </Show>
                 <div class={mapClass(CLASSES.FLEXLAYOUT__TAB_BUTTON_CONTENT)}>
                   {tab.name ?? ""}
                 </div>
                 <Show when={shouldRenderCloseButton()}>
                   <div
                     class={closeButtonClass()}
                     data-layout-path={buttonPath() ? `${buttonPath()}/button/close` : undefined}
                     title="Close"
                     onPointerDown={(e) => e.stopPropagation()}
                     onClick={handleClose}
                   >
                     ✕
                   </div>
                 </Show>
               </div>
              <Show when={index() < tabs().length - 1}>
                <div class={mapClass(CLASSES.FLEXLAYOUT__TABSET_TAB_DIVIDER)} />
              </Show>
            </>
          );
        }}
      </For>
    </div>
  );
};


