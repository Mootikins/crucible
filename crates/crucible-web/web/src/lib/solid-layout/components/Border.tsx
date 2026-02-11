import { type Component, type JSX, createMemo, Show } from "solid-js";
import { CLASSES } from "../../flexlayout/core/Types";
import type { IJsonBorderNode } from "../../flexlayout/types";
import { useLayoutContext } from "../context";
import { BorderStrip } from "./BorderStrip";

export type BorderLocation = "left" | "right" | "top" | "bottom";

export interface BorderProps {
  borderNode: IJsonBorderNode;
  location: BorderLocation;
  path: string;
  isOutermost: boolean;
  children: JSX.Element;
}

export function isHorizontalBorder(location: string): boolean {
  return location === "top" || location === "bottom";
}

export function isStartEdge(location: string): boolean {
  return location === "left" || location === "top";
}

export const Border: Component<BorderProps> = (props) => {
  const ctx = useLayoutContext();
  const mapClass = (cls: string) => ctx.classNameMapper?.(cls) ?? cls;

  const dockState = createMemo(
    () => (props.borderNode.dockState as string) ?? "expanded",
  );

  const horizontal = createMemo(() => isHorizontalBorder(props.location));
  const startEdge = createMemo(() => isStartEdge(props.location));

  const outerClass = createMemo(() =>
    mapClass(
      props.isOutermost
        ? CLASSES.FLEXLAYOUT__LAYOUT_BORDER_CONTAINER
        : CLASSES.FLEXLAYOUT__LAYOUT_BORDER_CONTAINER_INNER,
    ),
  );

  const innerClass = createMemo(() =>
    mapClass(CLASSES.FLEXLAYOUT__LAYOUT_BORDER_CONTAINER_INNER),
  );

  const flexDir = createMemo(
    (): "column" | "row" => (horizontal() ? "column" : "row"),
  );

  const strip = () => (
    <BorderStrip
      borderNode={props.borderNode}
      location={props.location}
      path={props.path}
    />
  );

  const innerContent = () => (
    <div
      class={innerClass()}
      style={{
        display: "flex",
        "flex-direction": flexDir(),
        flex: "1 1 0%",
      }}
    >
      {props.children}
    </div>
  );

  return (
    <div
      class={outerClass()}
      data-layout-path={`${props.path}/nest`}
      data-dock-state={dockState()}
      style={{
        display: "flex",
        "flex-direction": flexDir(),
        flex: "1 1 0%",
        overflow: "hidden",
        transition: "all 250ms ease",
      }}
    >
      <Show
        when={startEdge()}
        fallback={
          <>
            {innerContent()}
            {strip()}
          </>
        }
      >
        {strip()}
        {innerContent()}
      </Show>
    </div>
  );
};
