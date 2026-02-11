import { type ParentComponent, createMemo } from "solid-js";
import type { IJsonBorderNode } from "../../flexlayout/types";
import { useLayoutContext } from "../context";
import { Border, type BorderLocation } from "./Border";
import { BORDER_BAR_SIZE } from "./BorderStrip";

const LOCATION_TIE_ORDER: Record<string, number> = {
  top: 0,
  bottom: 1,
  left: 2,
  right: 3,
};

export function computeNestingOrder(
  borders: IJsonBorderNode[],
): IJsonBorderNode[] {
  return [...borders].sort((a, b) => {
    const priorityDiff = (b.priority ?? 0) - (a.priority ?? 0);
    if (priorityDiff !== 0) return priorityDiff;
    return (LOCATION_TIE_ORDER[a.location ?? ""] ?? 4)
      - (LOCATION_TIE_ORDER[b.location ?? ""] ?? 4);
  });
}

export function isVisibleBorder(border: IJsonBorderNode): boolean {
  if (border.show === false) return false;
  if (border.enableAutoHide && (!border.children || border.children.length === 0)) {
    return false;
  }
  return true;
}

export function computeInsets(borders: IJsonBorderNode[]): {
  top: number;
  right: number;
  bottom: number;
  left: number;
} {
  const insets = { top: 0, right: 0, bottom: 0, left: 0 };

  for (const border of borders) {
    if (!isVisibleBorder(border)) continue;

    const dockState = (border.dockState as string) ?? "expanded";
    const selected = typeof border.selected === "number" ? border.selected : -1;
    const isCollapsed = dockState === "collapsed";
    const isExpandedUnselected = dockState === "expanded" && selected === -1;

    if (!isCollapsed && !isExpandedUnselected) continue;

    const location = border.location ?? "";
    if (location === "top") insets.top = BORDER_BAR_SIZE;
    else if (location === "right") insets.right = BORDER_BAR_SIZE;
    else if (location === "bottom") insets.bottom = BORDER_BAR_SIZE;
    else if (location === "left") insets.left = BORDER_BAR_SIZE;
  }

  return insets;
}

export interface BorderLayoutProps {}

export const BorderLayout: ParentComponent<BorderLayoutProps> = (props) => {
  const ctx = useLayoutContext();

  const allBorders = createMemo(
    (): IJsonBorderNode[] => (ctx.bridge.store.borders ?? []) as IJsonBorderNode[],
  );

  const visibleBorders = createMemo(() =>
    allBorders().filter(isVisibleBorder),
  );

  const sortedBorders = createMemo(() =>
    computeNestingOrder(visibleBorders()),
  );

  const borderPath = (border: IJsonBorderNode): string => {
    const id = border.id ?? `border_${border.location}`;
    return `/border/${id}`;
  };

  return (
    <NestBorders
      borders={sortedBorders()}
      borderPath={borderPath}
      index={sortedBorders().length - 1}
    >
      {props.children}
    </NestBorders>
  );
};

interface NestBordersProps {
  borders: IJsonBorderNode[];
  borderPath: (border: IJsonBorderNode) => string;
  index: number;
  children: any;
}

const NestBorders: ParentComponent<NestBordersProps> = (props) => {
  if (props.index < 0) {
    return <>{props.children}</>;
  }

  const border = props.borders[props.index];
  const location = (border.location ?? "left") as BorderLocation;
  const path = props.borderPath(border);
  const isOutermost = props.index === 0;

  return (
    <Border
      borderNode={border}
      location={location}
      path={path}
      isOutermost={isOutermost}
    >
      <NestBorders
        borders={props.borders}
        borderPath={props.borderPath}
        index={props.index - 1}
      >
        {props.children}
      </NestBorders>
    </Border>
  );
};
