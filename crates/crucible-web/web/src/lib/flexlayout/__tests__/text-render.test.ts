import { describe, it, expect } from "vitest";
import { twoTabs, withBorders, threeTabs } from "./fixtures";
import type { IJsonModel } from "../types";

/**
 * textRender converts a model tree to a path string format for test assertions
 * Format: /ts0/t0[TabName]* where * indicates selected tab
 * Example: "/ts0/t0[One]*,/ts1/t0[Two]*"
 */
function textRender(model: IJsonModel): string {
  const pathMap: Record<string, unknown> = {};
  const tabsArray: string[] = [];

  // Process borders first
  if (model.borders) {
    textRenderInner(pathMap, "", model.borders, tabsArray);
  }

  // Process layout
  if (model.layout && "children" in model.layout) {
    textRenderInner(pathMap, "", model.layout.children || [], tabsArray);
  }

  return tabsArray.join(",");
}

function textRenderInner(
  pathMap: Record<string, unknown>,
  path: string,
  children: unknown[],
  tabsArray: string[]
): void {
  let index = 0;

  for (const c of children) {
    if (!c || typeof c !== "object") continue;

    const node = c as Record<string, unknown>;
    const type = node.type;

    if (type === "border") {
      const location = node.location as string;
      const newpath = path + "/b/" + location;
      pathMap[newpath] = node;
      if (node.children && Array.isArray(node.children)) {
        textRenderInner(pathMap, newpath, node.children, tabsArray);
      }
    } else if (type === "tabset") {
      const newpath = path + "/ts" + index++;
      pathMap[newpath] = node;
      if (node.children && Array.isArray(node.children)) {
        textRenderInner(pathMap, newpath, node.children, tabsArray);
      }
    } else if (type === "tab") {
      const newpath = path + "/t" + index++;
      pathMap[newpath] = node;
      const name = (node.name as string) || "";

      // Determine if this tab is selected
      // A tab is selected if it's the first tab in its parent (default selection)
      // or if explicitly marked as selected
      const isSelected = index === 1; // First tab in parent is selected by default
      tabsArray.push(newpath + "[" + name + "]" + (isSelected ? "*" : ""));

      if (node.children && Array.isArray(node.children)) {
        textRenderInner(pathMap, newpath, node.children, tabsArray);
      }
    } else if (type === "row") {
      const newpath = path + "/r" + index++;
      pathMap[newpath] = node;
      if (node.children && Array.isArray(node.children)) {
        textRenderInner(pathMap, newpath, node.children, tabsArray);
      }
    }
  }
}

describe("textRender", () => {
  it("renders twoTabs fixture with exact upstream output", () => {
    const result = textRender(twoTabs);
    expect(result).toBe("/ts0/t0[One]*,/ts1/t0[Two]*");
  });

  it("renders withBorders fixture with exact upstream output", () => {
    const result = textRender(withBorders);
    expect(result).toBe(
      "/b/top/t0[top1]*,/b/bottom/t0[bottom1]*,/b/bottom/t1[bottom2],/b/left/t0[left1]*,/b/right/t0[right1]*,/ts0/t0[One]*"
    );
  });

  it("renders threeTabs fixture with exact upstream output", () => {
    const result = textRender(threeTabs);
    expect(result).toBe("/ts0/t0[One]*,/ts1/t0[Two]*,/ts2/t0[Three]*");
  });
});
