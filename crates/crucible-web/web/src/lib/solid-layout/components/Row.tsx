import { For, Show, type Component } from "solid-js";
import { CLASSES } from "../../flexlayout/core/Types";
import type { IJsonRowNode, IJsonTabSetNode } from "../../flexlayout/types";
import { useLayoutContext } from "../context";
import { Splitter } from "./Splitter";
import { TabSet } from "./TabSet";

export interface RowProps {
  node: IJsonRowNode;
  path: string;
  isNested?: boolean;
  parentHorizontal?: boolean;
}

/**
 * Orientation alternates at each nesting level. Root is horizontal
 * unless model.isRootOrientationVertical(). Depth derived from
 * path "/rN" segments since the JSON store has no orientation field.
 */
export function isHorizontal(_node: IJsonRowNode, path: string, isRootVertical: boolean): boolean {
  const depth = path.split("/").filter((s) => s.startsWith("r")).length;
  const rootHorizontal = !isRootVertical;
  return depth % 2 === 0 ? rootHorizontal : !rootHorizontal;
}

export function totalWeight(
  children: (IJsonRowNode | IJsonTabSetNode | any)[],
): number {
  let sum = 0;
  for (const c of children) {
    sum += c.weight ?? 100;
  }
  return sum || 1;
}

export function childPath(
  parentPath: string,
  child: IJsonRowNode | IJsonTabSetNode,
  index: number,
): string {
  return child.type === "row"
    ? `${parentPath}/r${index}`
    : `${parentPath}/ts${index}`;
}

interface InterleavedItem {
  kind: "child" | "splitter";
  index: number;
  child?: IJsonRowNode | IJsonTabSetNode;
}

export const Row: Component<RowProps> = (props) => {
  const ctx = useLayoutContext();

  const horizontal = () =>
    isHorizontal(props.node, props.path, ctx.model.isRootOrientationVertical());

  const mapClassName = (cls: string) =>
    ctx.classNameMapper?.(cls) ?? cls;

  const children = () => props.node.children ?? [];

  const splitterSize = () => ctx.model.getSplitterSize();

  const interleavedItems = (): InterleavedItem[] => {
    const items: InterleavedItem[] = [];
    const c = children();
    for (let i = 0; i < c.length; i++) {
      if (i > 0) {
        items.push({ kind: "splitter", index: i });
      }
      items.push({
        kind: "child",
        index: i,
        child: c[i] as IJsonRowNode | IJsonTabSetNode,
      });
    }
    return items;
  };

  return (
    <div
      class={mapClassName(CLASSES.FLEXLAYOUT__ROW)}
      data-layout-path={props.path}
      style={{
        display: "flex",
        "flex-direction": horizontal() ? "row" : "column",
        flex: "1 1 0%",
        overflow: "hidden",
        position: "relative",
        width: "100%",
        height: "100%",
      }}
    >
      <For each={interleavedItems()}>
        {(item) => (
          <Show
            when={item.kind === "child" && item.child}
            fallback={
              <Splitter
                index={item.index}
                parentId={props.node.id ?? ""}
                parentPath={props.path}
                horizontal={horizontal()}
                splitterSize={splitterSize()}
              />
            }
          >
            <RowChild
              child={item.child!}
              index={item.index}
              parentPath={props.path}
              horizontal={horizontal()}
              totalWeight={totalWeight(children())}
            />
          </Show>
        )}
      </For>
    </div>
  );
};

interface RowChildProps {
  child: IJsonRowNode | IJsonTabSetNode;
  index: number;
  parentPath: string;
  horizontal: boolean;
  totalWeight: number;
}

const RowChild: Component<RowChildProps> = (props) => {
  const weight = () => props.child.weight ?? 100;
  const flexGrow = () => weight() / props.totalWeight;
  const path = () => childPath(props.parentPath, props.child, props.index);

  return (
    <Show
      when={props.child.type === "row"}
      fallback={
        <div
          data-layout-path={path()}
          style={{
            flex: `${flexGrow()} 1 0%`,
            overflow: "hidden",
            position: "relative",
          }}
        >
          <TabSet nodeId={(props.child as IJsonTabSetNode).id ?? ""} />
        </div>
      }
    >
      <div
        style={{
          flex: `${flexGrow()} 1 0%`,
          overflow: "hidden",
          position: "relative",
          display: "flex",
        }}
      >
        <Row
          node={props.child as IJsonRowNode}
          path={path()}
          isNested={true}
          parentHorizontal={props.horizontal}
        />
      </div>
    </Show>
  );
};
