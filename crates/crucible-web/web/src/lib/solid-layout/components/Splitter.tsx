import { type Component, onCleanup } from "solid-js";
import { CLASSES } from "../../flexlayout/core/Types";
import { Action } from "../../flexlayout/model/Action";
import type { RowNode } from "../../flexlayout/model/RowNode";
import { useLayoutContext } from "../context";

export interface SplitterProps {
  index: number;
  parentId: string;
  parentPath: string;
  horizontal: boolean;
  splitterSize: number;
}

export function calculateSplitterDelta(
  startPos: number,
  currentPos: number,
  bounds: [number, number],
): number {
  const clamped = Math.max(bounds[0], Math.min(bounds[1], currentPos));
  return clamped - startPos;
}

export const Splitter: Component<SplitterProps> = (props) => {
  const ctx = useLayoutContext();

  const mapClassName = (cls: string) =>
    ctx.classNameMapper?.(cls) ?? cls;

  const orientationName = () => (props.horizontal ? "horz" : "vert");

  const className = () =>
    `${mapClassName(CLASSES.FLEXLAYOUT__SPLITTER)} ${mapClassName(CLASSES.FLEXLAYOUT__SPLITTER_ + orientationName())}`;

  const path = () => `${props.parentPath}/s${props.index - 1}`;

  let dragging = false;
  let startPointerPos = 0;
  let rowNode: RowNode | undefined;
  let initialSizes: { initialSizes: number[]; sum: number; startPosition: number } | undefined;

  function onPointerDown(event: PointerEvent) {
    event.stopPropagation();
    event.preventDefault();

    const node = ctx.model.getNodeById(props.parentId);
    if (!node) return;

    rowNode = node as RowNode;
    initialSizes = rowNode.getSplitterInitials(props.index);

    startPointerPos = props.horizontal ? event.clientX : event.clientY;
    dragging = true;

    (event.target as HTMLElement).setPointerCapture(event.pointerId);
    document.addEventListener("pointermove", onPointerMove);
    document.addEventListener("pointerup", onPointerUp);
  }

  function onPointerMove(event: PointerEvent) {
    if (!dragging || !rowNode || !initialSizes) return;

    const currentPos = props.horizontal ? event.clientX : event.clientY;
    const bounds = rowNode.getSplitterBounds(props.index);
    const position = initialSizes.startPosition + (currentPos - startPointerPos);
    const clampedPosition = Math.max(bounds[0], Math.min(bounds[1], position));

    const weights = rowNode.calculateSplit(
      props.index,
      clampedPosition,
      initialSizes.initialSizes,
      initialSizes.sum,
      initialSizes.startPosition,
    );

    ctx.doAction(
      Action.adjustWeights(
        rowNode.getId(),
        weights,
        rowNode.getOrientation().getName(),
      ),
    );
  }

  function onPointerUp(_event: PointerEvent) {
    dragging = false;
    rowNode = undefined;
    initialSizes = undefined;
    document.removeEventListener("pointermove", onPointerMove);
    document.removeEventListener("pointerup", onPointerUp);
  }

  onCleanup(() => {
    document.removeEventListener("pointermove", onPointerMove);
    document.removeEventListener("pointerup", onPointerUp);
  });

  const sizeStyle = () => {
    const size = `${props.splitterSize}px`;
    if (props.horizontal) {
      return { width: size, "min-width": size, cursor: "ew-resize" as const };
    }
    return { height: size, "min-height": size, cursor: "ns-resize" as const };
  };

  return (
    <div
      class={className()}
      data-layout-path={path()}
      style={{
        "flex-direction": props.horizontal ? "column" : "row",
        ...sizeStyle(),
      }}
      onPointerDown={onPointerDown}
    />
  );
};
