import { type Component, createMemo, For, Show } from "solid-js";
import { CLASSES } from "../../flexlayout/core/Types";
import { Action } from "../../flexlayout/model/Action";
import type { IJsonBorderNode, IJsonTabNode } from "../../flexlayout/types";
import { useLayoutContext } from "../context";

export const BORDER_BAR_SIZE = 38;

export interface BorderStripProps {
  borderNode: IJsonBorderNode;
  location: string;
  path: string;
}

export function getDockIcon(
  dockState: string,
  location: string,
): string {
  if (dockState === "collapsed") {
    if (location === "left") return "▶";
    if (location === "right") return "◀";
    if (location === "top") return "▼";
    return "▲";
  }
  if (location === "left") return "◀";
  if (location === "right") return "▶";
  if (location === "top") return "▲";
  return "▼";
}

export function buildBorderStripClass(
  location: string,
  isCollapsed: boolean,
  mapper: (cls: string) => string,
  customClassName?: string,
): string {
  let cls = `${mapper(CLASSES.FLEXLAYOUT__BORDER)} ${mapper(CLASSES.FLEXLAYOUT__BORDER_ + location)}`;
  if (customClassName) {
    cls += ` ${customClassName}`;
  }
  if (isCollapsed) {
    cls += ` ${mapper(CLASSES.FLEXLAYOUT__BORDER__COLLAPSED)}`;
  }
  return cls;
}

export function buildBorderButtonClass(
  location: string,
  isSelected: boolean,
  mapper: (cls: string) => string,
  customClassName?: string,
): string {
  let cls = `${mapper(CLASSES.FLEXLAYOUT__BORDER_BUTTON)} ${mapper(CLASSES.FLEXLAYOUT__BORDER_BUTTON_ + location)}`;
  cls += ` ${mapper(isSelected ? CLASSES.FLEXLAYOUT__BORDER_BUTTON__SELECTED : CLASSES.FLEXLAYOUT__BORDER_BUTTON__UNSELECTED)}`;
  if (customClassName) {
    cls += ` ${customClassName}`;
  }
  return cls;
}

export const BorderStrip: Component<BorderStripProps> = (props) => {
  const ctx = useLayoutContext();
  const mapClass = (cls: string) => ctx.classNameMapper?.(cls) ?? cls;

  const dockState = createMemo(() =>
    (props.borderNode.dockState as string) ?? "expanded",
  );

  const isCollapsed = createMemo(() => dockState() === "collapsed");

  const selected = createMemo(() => {
    const sel = props.borderNode.selected;
    return typeof sel === "number" ? sel : -1;
  });

  const isVertical = createMemo(
    () => props.location === "left" || props.location === "right",
  );

  const tabs = createMemo(
    (): IJsonTabNode[] => (props.borderNode.children ?? []) as IJsonTabNode[],
  );

  const stripSize = createMemo(() => {
    if (dockState() === "expanded" && selected() !== -1) return 0;
    return BORDER_BAR_SIZE;
  });

  const showTabButtons = createMemo(() => {
    if (stripSize() === 0) return false;
    if (dockState() === "expanded" && selected() !== -1) return false;
    return true;
  });

  const needsVerticalText = createMemo(
    () => isVertical() && (isCollapsed() || selected() === -1),
  );

  const enableDock = createMemo(
    () => props.borderNode.enableDock !== false,
  );

  const stripClass = createMemo(() =>
    buildBorderStripClass(
      props.location,
      isCollapsed(),
      mapClass,
      props.borderNode.className as string | undefined,
    ),
  );

  const sizeStyle = createMemo(() => {
    const size = stripSize();
    if (size === 0) {
      return isVertical()
        ? { width: "0px", "min-width": "0px" }
        : { height: "0px", "min-height": "0px" };
    }
    if (isVertical()) {
      return {
        width: `${size}px`,
        "min-width": `${size}px`,
      };
    }
    return {
      height: `${size}px`,
      "min-height": `${size}px`,
    };
  });

  const handleDockToggle = () => {
    const borderId = props.borderNode.id;
    if (!borderId) return;
    const newState = dockState() === "collapsed" ? "expanded" : "collapsed";
    ctx.doAction(Action.setDockState(borderId, newState));
  };

  const handleTabClick = (tabId: string) => {
    ctx.doAction(Action.selectTab(tabId));
  };

  return (
    <div
      class={stripClass()}
      data-layout-path={props.path}
      data-state={dockState()}
      data-edge={props.location}
      style={{
        "justify-content": "flex-start",
        transition: "width 250ms ease, height 250ms ease, min-width 250ms ease, min-height 250ms ease",
        ...sizeStyle(),
      }}
    >
      <Show when={showTabButtons()}>
        <Show when={enableDock()}>
          <button
            type="button"
            class={mapClass(CLASSES.FLEXLAYOUT__BORDER_DOCK_BUTTON)}
            data-layout-path={`${props.path}/button/dock`}
            data-dock-context={isCollapsed() ? "collapsed-strip" : "expanded-toolbar"}
            data-dock-location={props.location}
            title={isCollapsed() ? "Expand" : "Collapse"}
            style={{
              [isVertical() ? "margin-bottom" : "margin-right"]: "4px",
              ...(needsVerticalText()
                ? {
                    "writing-mode": "vertical-rl",
                    transform: props.location === "left" ? "rotate(180deg)" : undefined,
                  }
                : {}),
            }}
            onPointerDown={(e) => e.stopPropagation()}
            onClick={(e) => {
              e.stopPropagation();
              handleDockToggle();
            }}
          >
            {getDockIcon(dockState(), props.location)}
          </button>
        </Show>
        <For each={tabs()}>
          {(tab, index) => {
            const isTabSelected = createMemo(() => index() === selected());
            const buttonPath = createMemo(() => `${props.path}/tb${index()}`);

            const buttonClass = createMemo(() =>
              buildBorderButtonClass(
                props.location,
                isTabSelected(),
                mapClass,
                tab.className as string | undefined,
              ),
            );

            return (
              <div
                class={buttonClass()}
                data-layout-path={buttonPath()}
                data-state={isTabSelected() ? "selected" : "unselected"}
                data-border="true"
                data-border-location={props.location}
                title={(tab.helpText as string) ?? ""}
                style={{
                  ...(needsVerticalText()
                    ? {
                        "writing-mode": "vertical-rl",
                        transform:
                          props.location === "left"
                            ? "rotate(180deg)"
                            : undefined,
                      }
                    : {}),
                }}
                onClick={() => tab.id && handleTabClick(tab.id)}
              >
                <span class={mapClass(CLASSES.FLEXLAYOUT__BORDER_BUTTON_CONTENT)}>
                  {tab.name ?? ""}
                </span>
              </div>
            );
          }}
        </For>
      </Show>
    </div>
  );
};
