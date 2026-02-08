import { describe, it, expect } from "vitest";
import { twoTabs, withBorders, threeTabs } from "./fixtures";
import { Model } from "../model/Model";
import { Action } from "../model/Action";
import { TabNode } from "../model/TabNode";
import { RowNode } from "../model/RowNode";
import { TabSetNode } from "../model/TabSetNode";
import { canDockToWindow } from "../model/Utils";
import type { IJsonModel } from "../types";

// Global variables for pathMap and tabs
let pathMap: Record<string, any> = {};
let tabs = "";

/**
 * textRender converts a model tree to a path string format for test assertions
 * Format: /ts0/t0[TabName]* where * indicates selected tab
 */
function textRender(model: Model): string {
  pathMap = {};
  const tabsArray: string[] = [];

  // Process borders first
  if (model.getBorderSet()) {
    const borderSet = model.getBorderSet();
    for (const location of ["top", "bottom", "left", "right"]) {
      const border = borderSet.getBorder(location as any);
      if (border && border.getChildren().length > 0) {
        textRenderBorder(border, `/b/${location}`, tabsArray);
      }
    }
  }

  // Process layout
  if (model.getRoot()) {
    textRenderNode(model.getRoot(), "", tabsArray);
  }

  tabs = tabsArray.join(",");
  return tabs;
}

function textRenderBorder(
  border: any,
  path: string,
  tabsArray: string[]
): void {
  pathMap[path] = border;
  let index = 0;
  for (const tab of border.getChildren()) {
    const tabPath = `${path}/t${index}`;
    pathMap[tabPath] = tab;
    const name = tab.getName() || "";
    const isSelected = index === (border as any).getSelected();
    tabsArray.push(`${tabPath}[${name}]${isSelected ? "*" : ""}`);
    index++;
  }
}

function textRenderNode(node: any, path: string, tabsArray: string[]): void {
  pathMap[path] = node;
  const type = node.getType();

  if (type === "row") {
    let index = 0;
    for (const child of node.getChildren()) {
      if (child.getType() === "row") {
        const newPath = `${path}/r${index}`;
        textRenderNode(child, newPath, tabsArray);
        index++;
      } else if (child.getType() === "tabset") {
        const tsPath = `${path}/ts${index}`;
        textRenderTabset(child, tsPath, tabsArray);
        index++;
      }
    }
  } else if (type === "tabset") {
    textRenderTabset(node, path, tabsArray);
  }
}

function textRenderTabset(tabset: any, path: string, tabsArray: string[]): void {
  pathMap[path] = tabset;
  let tabIndex = 0;
  for (const tab of tabset.getChildren()) {
    const tabPath = `${path}/t${tabIndex}`;
    pathMap[tabPath] = tab;
    const name = tab.getName() || "";
    const isSelected = tabIndex === tabset.getSelected();
    tabsArray.push(`${tabPath}[${name}]${isSelected ? "*" : ""}`);
    tabIndex++;
  }
}

describe("Tree > Actions > Add", () => {
  let model: Model;

  it("empty tabset", () => {
    model = Model.fromJson({
      global: {},
      borders: [],
      layout: {
        type: "tabset",
        weight: 100,
        children: [
          {
            type: "tab",
            name: "newtab1",
            component: "grid",
          },
        ],
      },
    });

    const tabs = textRender(model);
    expect(tabs).toBe("/ts0/t0[newtab1]*");

    const tab = model.getNodeById("2") as any;
    expect(tab?.getId()).toBe("2");
    expect(tab?.getComponent()).toBe("grid");
  });

  it("add to tabset center", () => {
    model = Model.fromJson(twoTabs);
    textRender(model);
    const ts0 = tabset("/ts0");
    const ts1 = tabset("/ts1");

    model.doAction(
      Action.addNode(
        {
          type: "tab",
          name: "newtab1",
          component: "grid",
        },
        ts0?.getId() || "",
        "center",
        -1
      )
    );

    let tabs = textRender(model);
    expect(tabs).toBe("/ts0/t0[One],/ts0/t1[newtab1]*,/ts1/t0[Two]*");

    model.doAction(
      Action.addNode(
        {
          type: "tab",
          name: "newtab2",
          component: "grid",
        },
        ts1?.getId() || "",
        "center",
        -1
      )
    );

    tabs = textRender(model);
    expect(tabs).toBe(
      "/ts0/t0[One],/ts0/t1[newtab1]*,/ts1/t0[Two],/ts1/t1[newtab2]*"
    );
  });

  it("add to tabset at position", () => {
    model = Model.fromJson(twoTabs);
    textRender(model);
    const ts0 = tabset("/ts0");

    model.doAction(
      Action.addNode(
        {
          type: "tab",
          name: "newtab1",
          component: "grid",
        },
        ts0?.getId() || "",
        "center",
        0
      )
    );

    let tabs = textRender(model);
    expect(tabs).toBe("/ts0/t0[newtab1]*,/ts0/t1[One],/ts1/t0[Two]*");

    model.doAction(
      Action.addNode(
        {
          type: "tab",
          name: "newtab2",
          component: "grid",
        },
        ts0?.getId() || "",
        "center",
        1
      )
    );

    tabs = textRender(model);
    expect(tabs).toBe(
      "/ts0/t0[newtab1],/ts0/t1[newtab2]*,/ts0/t2[One],/ts1/t0[Two]*"
    );

    model.doAction(
      Action.addNode(
        {
          type: "tab",
          name: "newtab3",
          component: "grid",
        },
        ts0?.getId() || "",
        "center",
        3
      )
    );

    tabs = textRender(model);
    expect(tabs).toBe(
      "/ts0/t0[newtab1],/ts0/t1[newtab2],/ts0/t2[One],/ts0/t3[newtab3]*,/ts1/t0[Two]*"
    );
  });

  it("add to tabset top", () => {
    model = Model.fromJson(twoTabs);
    textRender(model);
    const ts0 = tabset("/ts0");

    model.doAction(
      Action.addNode(
        {
          type: "tab",
          name: "newtab1",
          component: "grid",
        },
        ts0?.getId() || "",
        "top",
        -1
      )
    );

    let tabs = textRender(model);
    expect(tabs).toBe("/r0/ts0/t0[newtab1]*,/r0/ts1/t0[One]*,/ts1/t0[Two]*");

    model.doAction(
      Action.addNode(
        {
          type: "tab",
          name: "newtab2",
          component: "grid",
        },
        tabset("/ts1")?.getId() || "",
        "top",
        -1
      )
    );

    tabs = textRender(model);
    expect(tabs).toBe(
      "/r0/ts0/t0[newtab1]*,/r0/ts1/t0[One]*,/r1/ts0/t0[newtab2]*,/r1/ts1/t0[Two]*"
    );
  });

  it("add to tabset bottom", () => {
    model = Model.fromJson(twoTabs);
    textRender(model);
    const ts0 = tabset("/ts0");

    model.doAction(
      Action.addNode(
        {
          type: "tab",
          name: "newtab1",
          component: "grid",
        },
        ts0?.getId() || "",
        "bottom",
        -1
      )
    );

    let tabs = textRender(model);
    expect(tabs).toBe("/r0/ts0/t0[One]*,/r0/ts1/t0[newtab1]*,/ts1/t0[Two]*");

    model.doAction(
      Action.addNode(
        {
          type: "tab",
          name: "newtab2",
          component: "grid",
        },
        tabset("/ts1")?.getId() || "",
        "bottom",
        -1
      )
    );

    tabs = textRender(model);
    expect(tabs).toBe(
      "/r0/ts0/t0[One]*,/r0/ts1/t0[newtab1]*,/r1/ts0/t0[Two]*,/r1/ts1/t0[newtab2]*"
    );
  });

  it("add to tabset left", () => {
    model = Model.fromJson(twoTabs);
    textRender(model);
    const ts0 = tabset("/ts0");

    model.doAction(
      Action.addNode(
        {
          type: "tab",
          name: "newtab1",
          component: "grid",
        },
        ts0?.getId() || "",
        "left",
        -1
      )
    );

    let tabs = textRender(model);
    expect(tabs).toBe("/ts0/t0[newtab1]*,/ts1/t0[One]*,/ts2/t0[Two]*");

    model.doAction(
      Action.addNode(
        {
          type: "tab",
          name: "newtab2",
          component: "grid",
        },
        tabset("/ts2")?.getId() || "",
        "left",
        -1
      )
    );

    tabs = textRender(model);
    expect(tabs).toBe(
      "/ts0/t0[newtab1]*,/ts1/t0[One]*,/ts2/t0[newtab2]*,/ts3/t0[Two]*"
    );
  });

  it("add to tabset right", () => {
    model = Model.fromJson(twoTabs);
    textRender(model);
    const ts0 = tabset("/ts0");

    model.doAction(
      Action.addNode(
        {
          type: "tab",
          name: "newtab1",
          component: "grid",
        },
        ts0?.getId() || "",
        "right",
        -1
      )
    );

    let tabs = textRender(model);
    expect(tabs).toBe("/ts0/t0[One]*,/ts1/t0[newtab1]*,/ts2/t0[Two]*");

    model.doAction(
      Action.addNode(
        {
          type: "tab",
          name: "newtab2",
          component: "grid",
        },
        tabset("/ts2")?.getId() || "",
        "right",
        -1
      )
    );

    tabs = textRender(model);
    expect(tabs).toBe(
      "/ts0/t0[One]*,/ts1/t0[newtab1]*,/ts2/t0[Two]*,/ts3/t0[newtab2]*"
    );
  });

  it("add to top border", () => {
    model = Model.fromJson(withBorders);
    const topBorder = model.getBorderSet().getBorder("top");

    model.doAction(
      Action.addNode(
        {
          type: "tab",
          name: "newtab1",
          component: "grid",
        },
        topBorder?.getId() || "",
        "center",
        -1
      )
    );

    let tabs = textRender(model);
    expect(tabs).toBe("/b/top/t0[top1],/b/top/t1[newtab1]*,/b/bottom/t0[bottom1],/b/bottom/t1[bottom2],/b/left/t0[left1],/b/right/t0[right1],/ts0/t0[One]*");

    model.doAction(
      Action.addNode(
        {
          type: "tab",
          name: "newtab2",
          component: "grid",
        },
        topBorder?.getId() || "",
        "center",
        0
      )
    );

    tabs = textRender(model);
    expect(tabs).toBe("/b/top/t0[newtab2]*,/b/top/t1[top1],/b/top/t2[newtab1],/b/bottom/t0[bottom1],/b/bottom/t1[bottom2],/b/left/t0[left1],/b/right/t0[right1],/ts0/t0[One]*");

    model.doAction(
      Action.addNode(
        {
          type: "tab",
          name: "newtab3",
          component: "grid",
        },
        topBorder?.getId() || "",
        "center",
        1
      )
    );

    tabs = textRender(model);
    expect(tabs).toBe(
      "/b/top/t0[newtab2],/b/top/t1[newtab3]*,/b/top/t2[top1],/b/top/t3[newtab1],/b/bottom/t0[bottom1],/b/bottom/t1[bottom2],/b/left/t0[left1],/b/right/t0[right1],/ts0/t0[One]*"
    );
  });

  it("add to bottom border", () => {
    model = Model.fromJson(withBorders);
    const bottomBorder = model.getBorderSet().getBorder("bottom");

    model.doAction(
      Action.addNode(
        {
          type: "tab",
          name: "newtab1",
          component: "grid",
        },
        bottomBorder?.getId() || "",
        "center",
        -1
      )
    );

    let tabs = textRender(model);
    expect(tabs).toBe(
      "/b/top/t0[top1],/b/bottom/t0[bottom1],/b/bottom/t1[bottom2],/b/bottom/t2[newtab1]*,/b/left/t0[left1],/b/right/t0[right1],/ts0/t0[One]*"
    );

    model.doAction(
      Action.addNode(
        {
          type: "tab",
          name: "newtab2",
          component: "grid",
        },
        bottomBorder?.getId() || "",
        "center",
        0
      )
    );

    tabs = textRender(model);
    expect(tabs).toBe(
      "/b/top/t0[top1],/b/bottom/t0[newtab2]*,/b/bottom/t1[bottom1],/b/bottom/t2[bottom2],/b/bottom/t3[newtab1],/b/left/t0[left1],/b/right/t0[right1],/ts0/t0[One]*"
    );

    model.doAction(
      Action.addNode(
        {
          type: "tab",
          name: "newtab3",
          component: "grid",
        },
        bottomBorder?.getId() || "",
        "center",
        1
      )
    );

    tabs = textRender(model);
    expect(tabs).toBe(
      "/b/top/t0[top1],/b/bottom/t0[newtab2],/b/bottom/t1[newtab3]*,/b/bottom/t2[bottom1],/b/bottom/t3[bottom2],/b/bottom/t4[newtab1],/b/left/t0[left1],/b/right/t0[right1],/ts0/t0[One]*"
    );
  });

  it("add to left border", () => {
    model = Model.fromJson(withBorders);
    const leftBorder = model.getBorderSet().getBorder("left");

    model.doAction(
      Action.addNode(
        {
          type: "tab",
          name: "newtab1",
          component: "grid",
        },
        leftBorder?.getId() || "",
        "center",
        -1
      )
    );

    let tabs = textRender(model);
    expect(tabs).toBe("/b/top/t0[top1],/b/bottom/t0[bottom1],/b/bottom/t1[bottom2],/b/left/t0[left1],/b/left/t1[newtab1]*,/b/right/t0[right1],/ts0/t0[One]*");

    model.doAction(
      Action.addNode(
        {
          type: "tab",
          name: "newtab2",
          component: "grid",
        },
        leftBorder?.getId() || "",
        "center",
        0
      )
    );

    tabs = textRender(model);
    expect(tabs).toBe("/b/top/t0[top1],/b/bottom/t0[bottom1],/b/bottom/t1[bottom2],/b/left/t0[newtab2]*,/b/left/t1[left1],/b/left/t2[newtab1],/b/right/t0[right1],/ts0/t0[One]*");

    model.doAction(
      Action.addNode(
        {
          type: "tab",
          name: "newtab3",
          component: "grid",
        },
        leftBorder?.getId() || "",
        "center",
        1
      )
    );

    tabs = textRender(model);
    expect(tabs).toBe(
      "/b/top/t0[top1],/b/bottom/t0[bottom1],/b/bottom/t1[bottom2],/b/left/t0[newtab2],/b/left/t1[newtab3]*,/b/left/t2[left1],/b/left/t3[newtab1],/b/right/t0[right1],/ts0/t0[One]*"
    );
  });

  it("add to right border", () => {
    model = Model.fromJson(withBorders);
    const rightBorder = model.getBorderSet().getBorder("right");

    model.doAction(
      Action.addNode(
        {
          type: "tab",
          name: "newtab1",
          component: "grid",
        },
        rightBorder?.getId() || "",
        "center",
        -1
      )
    );

    let tabs = textRender(model);
    expect(tabs).toBe("/b/top/t0[top1],/b/bottom/t0[bottom1],/b/bottom/t1[bottom2],/b/left/t0[left1],/b/right/t0[right1],/b/right/t1[newtab1]*,/ts0/t0[One]*");

    model.doAction(
      Action.addNode(
        {
          type: "tab",
          name: "newtab2",
          component: "grid",
        },
        rightBorder?.getId() || "",
        "center",
        0
      )
    );

    tabs = textRender(model);
    expect(tabs).toBe(
      "/b/top/t0[top1],/b/bottom/t0[bottom1],/b/bottom/t1[bottom2],/b/left/t0[left1],/b/right/t0[newtab2]*,/b/right/t1[right1],/b/right/t2[newtab1],/ts0/t0[One]*"
    );

    model.doAction(
      Action.addNode(
        {
          type: "tab",
          name: "newtab3",
          component: "grid",
        },
        rightBorder?.getId() || "",
        "center",
        1
      )
    );

    tabs = textRender(model);
    expect(tabs).toBe(
      "/b/top/t0[top1],/b/bottom/t0[bottom1],/b/bottom/t1[bottom2],/b/left/t0[left1],/b/right/t0[newtab2],/b/right/t1[newtab3]*,/b/right/t2[right1],/b/right/t3[newtab1],/ts0/t0[One]*"
    );
  });
});

describe("Tree > Actions > Move", () => {
  let model: Model;

  it("move to center", () => {
    model = Model.fromJson(threeTabs);
    textRender(model);
    const ts0 = tabset("/ts0");
    const t0 = ts0?.getChildren()[0];

    model.doAction(
      Action.moveNode(t0?.getId() || "", tabset("/ts1")?.getId() || "", "center", -1)
    );

    const tabs = textRender(model);
    expect(tabs).toBe("/ts0/t0[Two],/ts0/t1[One]*,/ts1/t0[Three]*");
  });

  it("move to center position", () => {
    model = Model.fromJson(threeTabs);
    textRender(model);
    const ts0 = tabset("/ts0");
    const t0 = ts0?.getChildren()[0];

    model.doAction(
      Action.moveNode(t0?.getId() || "", tabset("/ts1")?.getId() || "", "center", 0)
    );

    let tabs = textRender(model);
    expect(tabs).toBe("/ts0/t0[One]*,/ts0/t1[Two],/ts1/t0[Three]*");

    const ts1_after = tabset("/ts1");
    const t1_after = ts1_after?.getChildren()[0];

    model.doAction(
      Action.moveNode(t1_after?.getId() || "", tabset("/ts0")?.getId() || "", "center", 1)
    );

    tabs = textRender(model);
    expect(tabs).toBe("/ts0/t0[One],/ts0/t1[Three]*,/ts0/t2[Two]");
  });

  it("move to top", () => {
    model = Model.fromJson(threeTabs);
    textRender(model);
    const ts0 = tabset("/ts0");
    const t0 = ts0?.getChildren()[0];

    model.doAction(
      Action.moveNode(t0?.getId() || "", tabset("/ts1")?.getId() || "", "top", -1)
    );

    const tabs = textRender(model);
    expect(tabs).toBe("/r0/ts0/t0[One]*,/r0/ts1/t0[Two]*,/ts1/t0[Three]*");
  });

  it("move to bottom", () => {
    model = Model.fromJson(threeTabs);
    textRender(model);
    const ts0 = tabset("/ts0");
    const t0 = ts0?.getChildren()[0];

    model.doAction(
      Action.moveNode(t0?.getId() || "", tabset("/ts1")?.getId() || "", "bottom", -1)
    );

    const tabs = textRender(model);
    expect(tabs).toBe("/r0/ts0/t0[Two]*,/r0/ts1/t0[One]*,/ts1/t0[Three]*");
  });

  it("move to left", () => {
    model = Model.fromJson(threeTabs);
    textRender(model);
    const ts0 = tabset("/ts0");
    const t0 = ts0?.getChildren()[0];

    model.doAction(
      Action.moveNode(t0?.getId() || "", tabset("/ts1")?.getId() || "", "left", -1)
    );

    const tabs = textRender(model);
    expect(tabs).toBe("/ts0/t0[One]*,/ts1/t0[Two]*,/ts2/t0[Three]*");
  });

  it("move to right", () => {
    model = Model.fromJson(threeTabs);
    textRender(model);
    const ts0 = tabset("/ts0");
    const t0 = ts0?.getChildren()[0];

    model.doAction(
      Action.moveNode(t0?.getId() || "", tabset("/ts1")?.getId() || "", "right", -1)
    );

    const tabs = textRender(model);
    expect(tabs).toBe("/ts0/t0[Two]*,/ts1/t0[One]*,/ts2/t0[Three]*");
  });
});

describe("Tree > Actions > Move to/from borders", () => {
  let model: Model;

  it("move to border top", () => {
    model = Model.fromJson(withBorders);
    textRender(model);
    const ts0 = tabset("/ts0");
    const t0 = ts0?.getChildren()[0];

    model.doAction(
      Action.moveNode(
        t0?.getId() || "",
        model.getBorderSet().getBorder("top")?.getId() || "",
        "center",
        -1
      )
    );

    const tabs = textRender(model);
    expect(tabs).toBe(
      "/b/top/t0[top1],/b/top/t1[One],/b/bottom/t0[bottom1],/b/bottom/t1[bottom2],/b/left/t0[left1],/b/right/t0[right1]"
    );
  });

  it("move to border bottom", () => {
    model = Model.fromJson(withBorders);
    textRender(model);
    const ts0 = tabset("/ts0");
    const t0 = ts0?.getChildren()[0];

    model.doAction(
      Action.moveNode(
        t0?.getId() || "",
        model.getBorderSet().getBorder("bottom")?.getId() || "",
        "center",
        -1
      )
    );

    const tabs = textRender(model);
    expect(tabs).toBe(
      "/b/top/t0[top1],/b/bottom/t0[bottom1],/b/bottom/t1[bottom2],/b/bottom/t2[One],/b/left/t0[left1],/b/right/t0[right1]"
    );
  });

  it("move to border left", () => {
    model = Model.fromJson(withBorders);
    textRender(model);
    const ts0 = tabset("/ts0");
    const t0 = ts0?.getChildren()[0];

    model.doAction(
      Action.moveNode(
        t0?.getId() || "",
        model.getBorderSet().getBorder("left")?.getId() || "",
        "center",
        -1
      )
    );

    const tabs = textRender(model);
    expect(tabs).toBe(
      "/b/top/t0[top1],/b/bottom/t0[bottom1],/b/bottom/t1[bottom2],/b/left/t0[left1],/b/left/t1[One],/b/right/t0[right1]"
    );
  });

  it("move to border right", () => {
    model = Model.fromJson(withBorders);
    textRender(model);
    const ts0 = tabset("/ts0");
    const t0 = ts0?.getChildren()[0];

    model.doAction(
      Action.moveNode(
        t0?.getId() || "",
        model.getBorderSet().getBorder("right")?.getId() || "",
        "center",
        -1
      )
    );

    const tabs = textRender(model);
    expect(tabs).toBe(
      "/b/top/t0[top1],/b/bottom/t0[bottom1],/b/bottom/t1[bottom2],/b/left/t0[left1],/b/right/t0[right1],/b/right/t1[One]"
    );
  });

  it("move from border top", () => {
    model = Model.fromJson(withBorders);
    textRender(model);
    const topBorder = model.getBorderSet().getBorder("top");
    const t0 = topBorder?.getChildren()[0];

    model.doAction(
      Action.moveNode(
        t0?.getId() || "",
        model.getRoot()?.getId() || "",
        "center",
        -1
      )
    );

    const tabs = textRender(model);
    expect(tabs).toBe(
      "/b/bottom/t0[bottom1],/b/bottom/t1[bottom2],/b/left/t0[left1],/b/right/t0[right1],/ts0/t0[One]*,/ts1/t0[top1]*"
    );
  });

  it("move from border bottom", () => {
    model = Model.fromJson(withBorders);
    textRender(model);
    const bottomBorder = model.getBorderSet().getBorder("bottom");
    const t0 = bottomBorder?.getChildren()[0];

    model.doAction(
      Action.moveNode(
        t0?.getId() || "",
        model.getRoot()?.getId() || "",
        "center",
        -1
      )
    );

    const tabs = textRender(model);
    expect(tabs).toBe(
      "/b/top/t0[top1],/b/bottom/t0[bottom2],/b/left/t0[left1],/b/right/t0[right1],/ts0/t0[One]*,/ts1/t0[bottom1]*"
    );
  });

  it("move from border left", () => {
    model = Model.fromJson(withBorders);
    textRender(model);
    const leftBorder = model.getBorderSet().getBorder("left");
    const t0 = leftBorder?.getChildren()[0];

    model.doAction(
      Action.moveNode(
        t0?.getId() || "",
        model.getRoot()?.getId() || "",
        "center",
        -1
      )
    );

    const tabs = textRender(model);
    expect(tabs).toBe(
      "/b/top/t0[top1],/b/bottom/t0[bottom1],/b/bottom/t1[bottom2],/b/right/t0[right1],/ts0/t0[One]*,/ts1/t0[left1]*"
    );
  });

  it("move from border right", () => {
    model = Model.fromJson(withBorders);
    textRender(model);
    const rightBorder = model.getBorderSet().getBorder("right");
    const t0 = rightBorder?.getChildren()[0];

    model.doAction(
      Action.moveNode(
        t0?.getId() || "",
        model.getRoot()?.getId() || "",
        "center",
        -1
      )
    );

    const tabs = textRender(model);
    expect(tabs).toBe(
      "/b/top/t0[top1],/b/bottom/t0[bottom1],/b/bottom/t1[bottom2],/b/left/t0[left1],/ts0/t0[One]*,/ts1/t0[right1]*"
    );
  });
});

describe("Tree > Actions > Delete", () => {
  let model: Model;

  it("delete from tabset with 1 tab", () => {
    model = Model.fromJson(threeTabs);
    textRender(model);
    const ts0 = tabset("/ts0");
    const t0 = ts0?.getChildren()[0];

    model.doAction(Action.deleteTab(t0?.getId() || ""));

    const tabs = textRender(model);
    expect(tabs).toBe("/ts0/t0[Two]*,/ts1/t0[Three]*");
  });

  it("delete tab from tabset with 3 tabs", () => {
    model = Model.fromJson(threeTabs);
    textRender(model);
    const ts0 = tabset("/ts0");
    const t0 = ts0?.getChildren()[0];

    model.doAction(
      Action.moveNode(
        t0?.getId() || "",
        tabset("/ts1")?.getId() || "",
        "center",
        -1
      )
    );

    textRender(model);
    const ts1_2 = tabset("/ts1");
    const t1_2 = ts1_2?.getChildren()[0];

    model.doAction(
      Action.moveNode(
        t1_2?.getId() || "",
        tabset("/ts0")?.getId() || "",
        "center",
        -1
      )
    );

    textRender(model);
    model.doAction(Action.deleteTab(tabset("/ts0")?.getChildren()[1]?.getId() || ""));

    const tabs = textRender(model);
    expect(tabs).toBe("/ts0/t0[Two],/ts0/t1[Three]*");
  });

  it("delete tabset", () => {
    model = Model.fromJson(threeTabs);
    textRender(model);
    const ts0 = tabset("/ts0");
    const ts1 = tabset("/ts1");

    model.doAction(
      Action.moveNode(
        ts0?.getChildren()[0]?.getId() || "",
        ts1?.getId() || "",
        "center",
        -1
      )
    );

    textRender(model);
    model.doAction(Action.deleteTabset(tabset("/ts0")?.getId() || ""));

    const tabs = textRender(model);
    expect(tabs).toBe("/ts0/t0[Three]*");
  });

  it("delete tab from borders", () => {
    model = Model.fromJson(withBorders);
    textRender(model);
    const topBorder = model.getBorderSet().getBorder("top");
    const t0 = topBorder?.getChildren()[0];

    model.doAction(Action.deleteTab(t0?.getId() || ""));

    let tabs = textRender(model);
    expect(tabs).toBe(
      "/b/bottom/t0[bottom1],/b/bottom/t1[bottom2],/b/left/t0[left1],/b/right/t0[right1],/ts0/t0[One]*"
    );

    const bottomBorder = model.getBorderSet().getBorder("bottom");
    const t1 = bottomBorder?.getChildren()[0];
    model.doAction(Action.deleteTab(t1?.getId() || ""));

    tabs = textRender(model);
    expect(tabs).toBe(
      "/b/bottom/t0[bottom2],/b/left/t0[left1],/b/right/t0[right1],/ts0/t0[One]*"
    );

    const bottomBorder2 = model.getBorderSet().getBorder("bottom");
    const t2 = bottomBorder2?.getChildren()[0];
    model.doAction(Action.deleteTab(t2?.getId() || ""));

    tabs = textRender(model);
    expect(tabs).toBe(
      "/b/left/t0[left1],/b/right/t0[right1],/ts0/t0[One]*"
    );

    const leftBorder = model.getBorderSet().getBorder("left");
    const t3 = leftBorder?.getChildren()[0];
    model.doAction(Action.deleteTab(t3?.getId() || ""));

    tabs = textRender(model);
    expect(tabs).toBe("/b/right/t0[right1],/ts0/t0[One]*");

    const rightBorder = model.getBorderSet().getBorder("right");
    const t4 = rightBorder?.getChildren()[0];
    model.doAction(Action.deleteTab(t4?.getId() || ""));

    tabs = textRender(model);
    expect(tabs).toBe("/ts0/t0[One]*");
  });
});

describe("Tree > Actions > Other Actions", () => {
  let model: Model;

  it("rename tab", () => {
    model = Model.fromJson(twoTabs);
    textRender(model);
    const ts0 = tabset("/ts0");
    const t0 = ts0?.getChildren()[0];

    model.doAction(Action.renameTab(t0?.getId() || "", "renamed"));

    const tabs = textRender(model);
    expect(tabs).toBe("/ts0/t0[renamed]*,/ts1/t0[Two]*");
  });

  it("select tab", () => {
    model = Model.fromJson(twoTabs);
    textRender(model);
    const ts0 = tabset("/ts0");

    model.doAction(
      Action.addNode(
        {
          type: "tab",
          name: "newtab1",
          component: "grid",
        },
        ts0?.getId() || "",
        "center",
        -1
      )
    );

    const t0 = ts0?.getChildren()[0];
    model.doAction(Action.selectTab(t0?.getId() || ""));

    const tabs = textRender(model);
    expect(tabs).toBe("/ts0/t0[One]*,/ts0/t1[newtab1],/ts1/t0[Two]*");
  });

  it("set active tabset", () => {
    model = Model.fromJson(twoTabs);
    textRender(model);
    const ts0 = tabset("/ts0") as any;
    const ts1 = tabset("/ts1") as any;

    expect(ts0?.isActive()).toBe(false);
    expect(ts1?.isActive()).toBe(false);

    model.doAction(Action.selectTab(ts0?.getChildren()[0]?.getId() || ""));
    expect(ts0?.isActive()).toBe(true);
    expect(ts1?.isActive()).toBe(false);

    model.doAction(Action.selectTab(ts1?.getChildren()[0]?.getId() || ""));
    expect(ts0?.isActive()).toBe(false);
    expect(ts1?.isActive()).toBe(true);

    model.doAction(Action.setActiveTabset(ts0?.getId() || ""));
    expect(ts0?.isActive()).toBe(true);
    expect(ts1?.isActive()).toBe(false);
  });

  it("maximize tabset", () => {
    model = Model.fromJson(twoTabs);
    textRender(model);
    const ts0 = tabset("/ts0");
    const ts1 = tabset("/ts1");

    // First toggle: maximize ts0
    model.doAction(Action.maximizeToggle(ts0?.getId() || ""));

    expect(ts0?.isMaximized()).toBe(true);
    expect(ts1?.isMaximized()).toBe(false);
    expect(model.getMaximizedTabset()).toBe(ts0);

    // Second toggle: un-maximize ts0
    model.doAction(Action.maximizeToggle(ts0?.getId() || ""));

    expect(ts0?.isMaximized()).toBe(false);
    expect(ts1?.isMaximized()).toBe(false);
    expect(model.getMaximizedTabset()).toBe(undefined);
  });

  it("set tab attributes", () => {
    model = Model.fromJson(twoTabs);
    textRender(model);
    const ts1 = tabset("/ts1");
    const t0 = ts1?.getChildren()[0];

    model.doAction(
      Action.updateNodeAttributes(t0?.getId() || "", {
        config: "newConfig",
      })
    );

    expect(t0?.getConfig()).toBe("newConfig");
  });

  it("set tab icon", () => {
    model = Model.fromJson(twoTabs);
    textRender(model);
    const ts0 = tabset("/ts0");
    const tab = ts0?.getChildren()[0] as TabNode;
    model.doAction(Action.setTabIcon(tab.getId(), "new-icon"));
    expect(tab.getIcon()).toBe("new-icon");
  });

  it("set tab component", () => {
    model = Model.fromJson(twoTabs);
    textRender(model);
    const ts0 = tabset("/ts0");
    const tab = ts0?.getChildren()[0] as TabNode;
    model.doAction(Action.setTabComponent(tab.getId(), "new-component"));
    expect(tab.getComponent()).toBe("new-component");
  });

  it("set tab config", () => {
    model = Model.fromJson(twoTabs);
    textRender(model);
    const ts0 = tabset("/ts0");
    const tab = ts0?.getChildren()[0] as TabNode;
    const newConfig = { key: "value", nested: { data: 123 } };
    model.doAction(Action.setTabConfig(tab.getId(), newConfig));
    expect(tab.getConfig()).toEqual(newConfig);
  });

  it("set tab enable close", () => {
    model = Model.fromJson(twoTabs);
    textRender(model);
    const ts0 = tabset("/ts0");
    const tab = ts0?.getChildren()[0] as TabNode;
    model.doAction(Action.setTabEnableClose(tab.getId(), false));
    expect(tab.isEnableClose()).toBe(false);
  });

  it("set model attributes", () => {
    model = Model.fromJson(twoTabs);

    model.doAction(
      Action.updateModelAttributes({
        splitterSize: 10,
      })
    );

    expect(model.getSplitterSize()).toBe(10);
  });
});

describe("Tree > Node events", () => {
  let model: Model;

  it("close tab", () => {
    model = Model.fromJson(twoTabs);
    textRender(model);
    const ts0 = tabset("/ts0");
    const t0 = ts0?.getChildren()[0];

    let closed = false;
    t0?.setEventListener("close", () => {
      closed = true;
    });

    model.doAction(Action.deleteTab(t0?.getId() || ""));

    expect(closed).toBe(true);
  });

  it("save tab", () => {
    model = Model.fromJson(twoTabs);
    textRender(model);
    const ts0 = tabset("/ts0");
    const t0 = ts0?.getChildren()[0];

    let saved = false;
    t0?.setEventListener("save", () => {
      saved = true;
    });

    model.doAction(Action.updateNodeAttributes(t0?.getId() || "", {}));

    expect(saved).toBe(true);
  });
});

function tabset(path: string): any {
  return pathMap[path];
}

describe("Model > getRoot", () => {
  it("getRoot with default windowId", () => {
    const model = Model.fromJson(twoTabs);
    const root = model.getRoot();
    expect(root).toBeDefined();
    expect(root?.getType()).toBe("row");
  });

  it("getRoot with explicit MAIN_WINDOW_ID", () => {
    const model = Model.fromJson(twoTabs);
    const root = model.getRoot(Model.MAIN_WINDOW_ID);
    expect(root).toBeDefined();
    expect(root?.getType()).toBe("row");
  });

  it("getRoot with default and explicit windowId return same result", () => {
    const model = Model.fromJson(twoTabs);
    const rootDefault = model.getRoot();
    const rootExplicit = model.getRoot(Model.MAIN_WINDOW_ID);
    expect(rootDefault).toBe(rootExplicit);
  });

  it("getRoot with non-existent windowId returns undefined", () => {
    const model = Model.fromJson(twoTabs);
    const root = model.getRoot("non-existent-window");
    expect(root).toBeUndefined();
  });
});

function countTabs(model: Model, windowId?: string): number {
  let count = 0;
  const windows = model.getwindowsMap();
  for (const [wId, lw] of windows) {
    if (windowId && wId !== windowId) continue;
    if (lw.root) {
      lw.root.forEachNode((node) => {
        if (node instanceof TabNode) count++;
      }, 0);
    }
  }
  return count;
}

function tabNamesInWindow(model: Model, windowId: string): string[] {
  const names: string[] = [];
  const lw = model.getwindowsMap().get(windowId);
  if (lw?.root) {
    lw.root.forEachNode((node) => {
      if (node instanceof TabNode) names.push((node as TabNode).getName());
    }, 0);
  }
  return names;
}

function nonMainWindowIds(model: Model): string[] {
  const ids: string[] = [];
  for (const [wId] of model.getwindowsMap()) {
    if (wId !== Model.MAIN_WINDOW_ID) ids.push(wId);
  }
  return ids;
}

describe("popout actions", () => {
  it("popoutTab moves a tab to a new window", () => {
    const model = Model.fromJson(twoTabs);
    textRender(model);
    const ts0 = tabset("/ts0");
    const tab = ts0.getChildren()[0] as TabNode;
    const tabId = tab.getId();

    expect(model.getwindowsMap().size).toBe(1);

    model.doAction(Action.popoutTab(tabId));

    expect(model.getwindowsMap().size).toBe(2);
    const newWindowIds = nonMainWindowIds(model);
    expect(newWindowIds.length).toBe(1);
    const newNames = tabNamesInWindow(model, newWindowIds[0]);
    expect(newNames).toContain("One");

    const mainNames = tabNamesInWindow(model, Model.MAIN_WINDOW_ID);
    expect(mainNames).not.toContain("One");
    expect(mainNames).toContain("Two");
  });

  it("popoutTab with nonexistent tab is a no-op", () => {
    const model = Model.fromJson(twoTabs);
    model.doAction(Action.popoutTab("nonexistent"));
    expect(model.getwindowsMap().size).toBe(1);
  });

  it("popoutTabset moves a tabset to a new window", () => {
    const model = Model.fromJson(twoTabs);
    textRender(model);
    const ts0 = tabset("/ts0");
    const tabsetId = ts0.getId();

    expect(model.getwindowsMap().size).toBe(1);

    model.doAction(Action.popoutTabset(tabsetId));

    expect(model.getwindowsMap().size).toBe(2);
    const newWindowIds = nonMainWindowIds(model);
    expect(newWindowIds.length).toBe(1);
    const newNames = tabNamesInWindow(model, newWindowIds[0]);
    expect(newNames).toContain("One");

    const mainNames = tabNamesInWindow(model, Model.MAIN_WINDOW_ID);
    expect(mainNames).not.toContain("One");
    expect(mainNames).toContain("Two");
  });

  it("popoutTabset clears maximized state when popping maximized tabset", () => {
    const model = Model.fromJson(twoTabs);
    textRender(model);
    const ts0 = tabset("/ts0");
    const tabsetId = ts0.getId();

    model.doAction(Action.maximizeToggle(tabsetId));
    expect(model.getMaximizedTabset()).toBe(ts0);

    model.doAction(Action.popoutTabset(tabsetId));

    expect(model.getMaximizedTabset()).toBeUndefined();
  });

  it("popoutTabset with nonexistent tabset is a no-op", () => {
    const model = Model.fromJson(twoTabs);
    model.doAction(Action.popoutTabset("nonexistent"));
    expect(model.getwindowsMap().size).toBe(1);
  });
});

describe("float actions", () => {
  it("floatTab moves a tab to a new floating window with given rect", () => {
    const model = Model.fromJson(twoTabs);
    textRender(model);
    const ts0 = tabset("/ts0");
    const tab = ts0.getChildren()[0] as TabNode;
    const tabId = tab.getId();

    model.doAction(Action.floatTab(tabId, 100, 200, 400, 300));

    expect(model.getwindowsMap().size).toBe(2);
    const newWindowIds = nonMainWindowIds(model);
    expect(newWindowIds.length).toBe(1);
    const newNames = tabNamesInWindow(model, newWindowIds[0]);
    expect(newNames).toContain("One");

    const newWindow = model.getwindowsMap().get(newWindowIds[0])!;
    expect(newWindow.rect.x).toBe(100);
    expect(newWindow.rect.y).toBe(200);
    expect(newWindow.rect.width).toBe(400);
    expect(newWindow.rect.height).toBe(300);

    const mainNames = tabNamesInWindow(model, Model.MAIN_WINDOW_ID);
    expect(mainNames).not.toContain("One");
  });

  it("floatTab with nonexistent tab is a no-op", () => {
    const model = Model.fromJson(twoTabs);
    model.doAction(Action.floatTab("nonexistent", 0, 0, 100, 100));
    expect(model.getwindowsMap().size).toBe(1);
  });

  it("floatTabset moves a tabset to a new floating window", () => {
    const model = Model.fromJson(twoTabs);
    textRender(model);
    const ts0 = tabset("/ts0");
    const tabsetId = ts0.getId();

    model.doAction(Action.floatTabset(tabsetId, 50, 75, 600, 400));

    expect(model.getwindowsMap().size).toBe(2);
    const newWindowIds = nonMainWindowIds(model);
    const newNames = tabNamesInWindow(model, newWindowIds[0]);
    expect(newNames).toContain("One");

    const newWindow = model.getwindowsMap().get(newWindowIds[0])!;
    expect(newWindow.rect.x).toBe(50);
    expect(newWindow.rect.y).toBe(75);
    expect(newWindow.rect.width).toBe(600);
    expect(newWindow.rect.height).toBe(400);
  });

  it("floatTabset clears maximized state when floating maximized tabset", () => {
    const model = Model.fromJson(twoTabs);
    textRender(model);
    const ts0 = tabset("/ts0");
    const tabsetId = ts0.getId();

    model.doAction(Action.maximizeToggle(tabsetId));
    expect(model.getMaximizedTabset()).toBe(ts0);

    model.doAction(Action.floatTabset(tabsetId, 0, 0, 500, 400));

    expect(model.getMaximizedTabset()).toBeUndefined();
  });

  it("floatTabset with nonexistent tabset is a no-op", () => {
    const model = Model.fromJson(twoTabs);
    model.doAction(Action.floatTabset("nonexistent", 0, 0, 100, 100));
    expect(model.getwindowsMap().size).toBe(1);
  });
});

describe("dock actions", () => {
  it("dockTab moves a floating tab back to main window", () => {
    const model = Model.fromJson(twoTabs);
    textRender(model);
    const ts0 = tabset("/ts0");
    const tab = ts0.getChildren()[0] as TabNode;
    const tabId = tab.getId();

    model.doAction(Action.floatTab(tabId, 100, 100, 400, 300));
    expect(model.getwindowsMap().size).toBe(2);
    expect(tabNamesInWindow(model, Model.MAIN_WINDOW_ID)).not.toContain("One");

    model.doAction(Action.dockTab(tabId, "center"));

    expect(model.getwindowsMap().size).toBe(1);
    const mainNames = tabNamesInWindow(model, Model.MAIN_WINDOW_ID);
    expect(mainNames).toContain("One");
    expect(mainNames).toContain("Two");
  });

  it("dockTab with nonexistent tab is a no-op", () => {
    const model = Model.fromJson(twoTabs);
    model.doAction(Action.dockTab("nonexistent", "center"));
    expect(model.getwindowsMap().size).toBe(1);
  });

  it("dockTabset moves a floating tabset back to main window", () => {
    const model = Model.fromJson(twoTabs);
    textRender(model);
    const ts0 = tabset("/ts0");
    const tabsetId = ts0.getId();

    model.doAction(Action.floatTabset(tabsetId, 50, 50, 600, 400));
    expect(model.getwindowsMap().size).toBe(2);

    model.doAction(Action.dockTabset(tabsetId, "center"));

    expect(model.getwindowsMap().size).toBe(1);
    const mainNames = tabNamesInWindow(model, Model.MAIN_WINDOW_ID);
    expect(mainNames).toContain("One");
    expect(mainNames).toContain("Two");
  });

  it("dockTabset with nonexistent tabset is a no-op", () => {
    const model = Model.fromJson(twoTabs);
    model.doAction(Action.dockTabset("nonexistent", "center"));
    expect(model.getwindowsMap().size).toBe(1);
  });

  it("dockTab to left creates new split in main", () => {
    const model = Model.fromJson(twoTabs);
    textRender(model);
    const ts0 = tabset("/ts0");
    const tab = ts0.getChildren()[0] as TabNode;
    const tabId = tab.getId();

    model.doAction(Action.floatTab(tabId, 100, 100, 400, 300));
    model.doAction(Action.dockTab(tabId, "left"));

    expect(model.getwindowsMap().size).toBe(1);
    const mainNames = tabNamesInWindow(model, Model.MAIN_WINDOW_ID);
    expect(mainNames).toContain("One");
    expect(mainNames).toContain("Two");
  });

  it("round-trip: float tab then dock preserves tab count", () => {
    const model = Model.fromJson(threeTabs);
    const totalBefore = countTabs(model);

    textRender(model);
    const ts0 = tabset("/ts0");
    const tab = ts0.getChildren()[0] as TabNode;
    const tabId = tab.getId();

    model.doAction(Action.floatTab(tabId, 0, 0, 300, 200));
    const totalDuring = countTabs(model);
    expect(totalDuring).toBe(totalBefore);

    model.doAction(Action.dockTab(tabId, "center"));
    const totalAfter = countTabs(model);
    expect(totalAfter).toBe(totalBefore);
  });
});

describe("windowType", () => {
  it("main window has windowType 'main'", () => {
    const model = Model.fromJson(twoTabs);
    const mainWindow = model.getwindowsMap().get(Model.MAIN_WINDOW_ID)!;
    expect(mainWindow.windowType).toBe("main");
  });

  it("popoutTab creates a window with windowType 'popout'", () => {
    const model = Model.fromJson(twoTabs);
    textRender(model);
    const ts0 = tabset("/ts0");
    const tab = ts0.getChildren()[0] as TabNode;

    model.doAction(Action.popoutTab(tab.getId()));

    const newIds = nonMainWindowIds(model);
    expect(newIds.length).toBe(1);
    const newWindow = model.getwindowsMap().get(newIds[0])!;
    expect(newWindow.windowType).toBe("popout");
  });

  it("popoutTabset creates a window with windowType 'popout'", () => {
    const model = Model.fromJson(twoTabs);
    textRender(model);
    const ts0 = tabset("/ts0");

    model.doAction(Action.popoutTabset(ts0.getId()));

    const newIds = nonMainWindowIds(model);
    expect(newIds.length).toBe(1);
    const newWindow = model.getwindowsMap().get(newIds[0])!;
    expect(newWindow.windowType).toBe("popout");
  });

  it("floatTab creates a window with windowType 'float'", () => {
    const model = Model.fromJson(twoTabs);
    textRender(model);
    const ts0 = tabset("/ts0");
    const tab = ts0.getChildren()[0] as TabNode;

    model.doAction(Action.floatTab(tab.getId(), 10, 20, 300, 200));

    const newIds = nonMainWindowIds(model);
    expect(newIds.length).toBe(1);
    const newWindow = model.getwindowsMap().get(newIds[0])!;
    expect(newWindow.windowType).toBe("float");
  });

  it("floatTabset creates a window with windowType 'float'", () => {
    const model = Model.fromJson(twoTabs);
    textRender(model);
    const ts0 = tabset("/ts0");

    model.doAction(Action.floatTabset(ts0.getId(), 10, 20, 300, 200));

    const newIds = nonMainWindowIds(model);
    expect(newIds.length).toBe(1);
    const newWindow = model.getwindowsMap().get(newIds[0])!;
    expect(newWindow.windowType).toBe("float");
  });
});

describe("toJson multi-window serialization", () => {
  it("toJson with only main window has no 'windows' key", () => {
    const model = Model.fromJson(twoTabs);
    const json = model.toJson();
    expect(json.windows).toBeUndefined();
  });

  it("toJson includes floating windows", () => {
    const model = Model.fromJson(twoTabs);
    textRender(model);
    const ts0 = tabset("/ts0");
    const tab = ts0.getChildren()[0] as TabNode;

    model.doAction(Action.floatTab(tab.getId(), 100, 200, 400, 300));

    const json = model.toJson();
    expect(json.windows).toBeDefined();
    const windowEntries = Object.entries(json.windows!);
    expect(windowEntries.length).toBe(1);

    const [, windowJson] = windowEntries[0];
    expect(windowJson.windowType).toBe("float");
    expect(windowJson.rect).toBeDefined();
    expect(windowJson.layout).toBeDefined();
  });

  it("toJson includes popout windows", () => {
    const model = Model.fromJson(twoTabs);
    textRender(model);
    const ts0 = tabset("/ts0");
    const tab = ts0.getChildren()[0] as TabNode;

    model.doAction(Action.popoutTab(tab.getId()));

    const json = model.toJson();
    expect(json.windows).toBeDefined();
    const windowEntries = Object.entries(json.windows!);
    expect(windowEntries.length).toBe(1);

    const [, windowJson] = windowEntries[0];
    expect(windowJson.windowType).toBe("popout");
  });

  it("fromJson roundtrip preserves floating windows", () => {
    const model = Model.fromJson(twoTabs);
    textRender(model);
    const ts0 = tabset("/ts0");
    const tab = ts0.getChildren()[0] as TabNode;

    model.doAction(Action.floatTab(tab.getId(), 100, 200, 400, 300));
    const totalTabs = countTabs(model);

    const json = model.toJson();
    const restored = Model.fromJson(json);

    expect(restored.getwindowsMap().size).toBe(2);
    expect(countTabs(restored)).toBe(totalTabs);

    const mainNames = tabNamesInWindow(restored, Model.MAIN_WINDOW_ID);
    expect(mainNames).toContain("Two");
    expect(mainNames).not.toContain("One");

    const restoredWindowIds = nonMainWindowIds(restored);
    expect(restoredWindowIds.length).toBe(1);
    const restoredWindow = restored.getwindowsMap().get(restoredWindowIds[0])!;
    expect(restoredWindow.windowType).toBe("float");

    const floatNames = tabNamesInWindow(restored, restoredWindowIds[0]);
    expect(floatNames).toContain("One");
  });

  it("fromJson roundtrip preserves multiple non-main windows", () => {
    const model = Model.fromJson(threeTabs);
    textRender(model);
    const tab1Id = (tabset("/ts0").getChildren()[0] as TabNode).getId();
    const tab2Id = (tabset("/ts1").getChildren()[0] as TabNode).getId();

    model.doAction(Action.floatTab(tab1Id, 10, 10, 300, 200));
    model.doAction(Action.popoutTab(tab2Id));

    expect(model.getwindowsMap().size).toBe(3);
    const totalTabs = countTabs(model);

    const json = model.toJson();
    const restored = Model.fromJson(json);

    expect(restored.getwindowsMap().size).toBe(3);
    expect(countTabs(restored)).toBe(totalTabs);

    const restoredWindowIds = nonMainWindowIds(restored);
    expect(restoredWindowIds.length).toBe(2);
    const types = restoredWindowIds.map(id => restored.getwindowsMap().get(id)!.windowType);
    expect(types).toContain("float");
    expect(types).toContain("popout");
  });
});

describe("MOVE_WINDOW action", () => {
  it("updates float window rect via moveWindow action", () => {
    const model = Model.fromJson(twoTabs);
    textRender(model);
    const ts0 = tabset("/ts0");
    const tab = ts0.getChildren()[0] as TabNode;

    model.doAction(Action.floatTab(tab.getId(), 10, 20, 300, 200));

    const windowIds = nonMainWindowIds(model);
    expect(windowIds.length).toBe(1);
    const windowId = windowIds[0];

    model.doAction(Action.moveWindow(windowId, 50, 100, 400, 300));

    const lw = model.getwindowsMap().get(windowId)!;
    expect(lw.rect.x).toBe(50);
    expect(lw.rect.y).toBe(100);
    expect(lw.rect.width).toBe(400);
    expect(lw.rect.height).toBe(300);
  });

  it("moveWindow is no-op for non-existent window", () => {
    const model = Model.fromJson(twoTabs);
    model.doAction(Action.moveWindow("nonexistent", 50, 100, 400, 300));
    expect(model.getwindowsMap().size).toBe(1);
  });

  it("moveWindow preserves window contents", () => {
    const model = Model.fromJson(twoTabs);
    textRender(model);
    const ts0 = tabset("/ts0");
    const tab = ts0.getChildren()[0] as TabNode;
    const tabName = tab.getName();

    model.doAction(Action.floatTab(tab.getId(), 10, 20, 300, 200));
    const windowId = nonMainWindowIds(model)[0];
    const tabsBefore = tabNamesInWindow(model, windowId);

    model.doAction(Action.moveWindow(windowId, 200, 300, 500, 400));

    const tabsAfter = tabNamesInWindow(model, windowId);
    expect(tabsAfter).toEqual(tabsBefore);
    expect(tabsAfter).toContain(tabName);
  });

  describe("float z-order serialization", () => {
    it("serializes floatZOrder in toJson when float windows exist", () => {
      const modelJson: IJsonModel = {
        global: {},
        borders: [],
        layout: {
          type: "row",
          weight: 100,
          children: [
            {
              type: "tabset",
              weight: 50,
              children: [{ type: "tab", name: "Main" }],
            },
          ],
        } as any,
        windows: {
          float1: {
            layout: {
              type: "row",
              weight: 100,
              children: [
                {
                  type: "tabset",
                  weight: 100,
                  children: [{ type: "tab", name: "Float1" }],
                },
              ],
            },
            rect: { x: 100, y: 100, width: 400, height: 300 },
            windowType: "float",
          },
          float2: {
            layout: {
              type: "row",
              weight: 100,
              children: [
                {
                  type: "tabset",
                  weight: 100,
                  children: [{ type: "tab", name: "Float2" }],
                },
              ],
            },
            rect: { x: 200, y: 200, width: 400, height: 300 },
            windowType: "float",
          },
        },
        floatZOrder: ["float1", "float2"],
      };

      const testModel = Model.fromJson(modelJson);
      const json = testModel.toJson();
      expect(json.floatZOrder).toEqual(["float1", "float2"]);
    });

    it("does not include floatZOrder when no float windows", () => {
      const testModel = Model.fromJson(twoTabs);
      const json = testModel.toJson();
      expect(json.floatZOrder).toBeUndefined();
    });

    it("roundtrips floatZOrder through toJson/fromJson", () => {
      const original: IJsonModel = {
        global: {},
        borders: [],
        layout: {
          type: "row",
          weight: 100,
          children: [
            {
              type: "tabset",
              weight: 50,
              children: [{ type: "tab", name: "Main" }],
            },
          ],
        } as any,
        windows: {
          w5: {
            layout: {
              type: "row",
              weight: 100,
              children: [
                {
                  type: "tabset",
                  weight: 100,
                  children: [{ type: "tab", name: "F1" }],
                },
              ],
            },
            rect: { x: 100, y: 100, width: 400, height: 300 },
            windowType: "float",
          },
          w6: {
            layout: {
              type: "row",
              weight: 100,
              children: [
                {
                  type: "tabset",
                  weight: 100,
                  children: [{ type: "tab", name: "F2" }],
                },
              ],
            },
            rect: { x: 200, y: 200, width: 400, height: 300 },
            windowType: "float",
          },
        },
        floatZOrder: ["w6", "w5"],
      };

      const testModel = Model.fromJson(original);
      const json = testModel.toJson();
      expect(json.floatZOrder).toEqual(["w6", "w5"]);

      const testModel2 = Model.fromJson(json);
      const json2 = testModel2.toJson();
      expect(json2.floatZOrder).toEqual(["w6", "w5"]);
    });

    it("getter and setter work correctly", () => {
      const testModel = Model.fromJson(twoTabs);
      expect(testModel.getFloatZOrder()).toEqual([]);

      testModel.setFloatZOrder(["a", "b"]);
      expect(testModel.getFloatZOrder()).toEqual(["a", "b"]);
    });
  });
});

describe("cross-window moves", () => {
  function collectRowNodes(node: any): RowNode[] {
    const rows: RowNode[] = [];
    if (node instanceof RowNode) {
      rows.push(node);
    }
    for (const child of node.getChildren()) {
      rows.push(...collectRowNodes(child));
    }
    return rows;
  }

  it("propagates windowId when moving tab from float to main", () => {
    const model = Model.fromJson(twoTabs);
    textRender(model);
    const ts0 = tabset("/ts0");
    const tab = ts0.getChildren()[0] as TabNode;
    const tabId = tab.getId();

    model.doAction(Action.floatTab(tabId, 100, 100, 400, 300));
    const floatWindowIds = nonMainWindowIds(model);
    expect(floatWindowIds.length).toBe(1);
    expect(tabNamesInWindow(model, floatWindowIds[0])).toContain("One");

    const mainRoot = model.getRoot(Model.MAIN_WINDOW_ID)!;
    let mainTabsetId = "";
    mainRoot.forEachNode((node: any) => {
      if (node instanceof TabSetNode && !mainTabsetId) {
        mainTabsetId = node.getId();
      }
    }, 0);

    model.doAction(Action.moveNode(tabId, mainTabsetId, "center", -1));

    expect(tab.getWindowId()).toBe(Model.MAIN_WINDOW_ID);
  });

  it("propagates windowId on nested RowNodes after cross-window move", () => {
    const withFloat: IJsonModel = {
      global: {},
      borders: [],
      layout: {
        type: "row",
        weight: 100,
        children: [
          {
            type: "tabset",
            id: "main-ts",
            weight: 100,
            children: [
              { type: "tab", name: "MainTab", component: "text" },
            ],
          },
        ],
      },
      windows: {
        float1: {
          layout: {
            type: "row",
            weight: 100,
            children: [
              {
                type: "row",
                weight: 50,
                children: [
                  {
                    type: "tabset",
                    id: "float-ts1",
                    weight: 50,
                    children: [
                      { type: "tab", name: "FloatA", component: "text" },
                    ],
                  },
                  {
                    type: "tabset",
                    id: "float-ts2",
                    weight: 50,
                    children: [
                      { type: "tab", name: "FloatB", component: "text" },
                    ],
                  },
                ],
              },
            ],
          },
          rect: { x: 100, y: 100, width: 400, height: 300 },
          windowType: "float",
        },
      },
    };

    const model = Model.fromJson(withFloat);

    const floatRoot = model.getRoot("float1")!;
    const floatRows = collectRowNodes(floatRoot);
    expect(floatRows.length).toBeGreaterThan(0);
    for (const row of floatRows) {
      expect(row.getWindowId()).toBe("float1");
    }

    const floatTabA = model.getNodeById("float-ts1")! as TabSetNode;
    const floatTab = floatTabA.getChildren()[0] as TabNode;
    const floatTabId = floatTab.getId();

    model.doAction(Action.moveNode(floatTabId, "main-ts", "left", -1));

    expect(floatTab.getWindowId()).toBe(Model.MAIN_WINDOW_ID);

    const mainRoot = model.getRoot(Model.MAIN_WINDOW_ID)!;
    const mainRows = collectRowNodes(mainRoot);
    for (const row of mainRows) {
      expect(row.getWindowId()).toBe(Model.MAIN_WINDOW_ID);
    }
  });

  it("removes empty float window after moving last tab out", () => {
    const model = Model.fromJson(twoTabs);
    textRender(model);
    const ts0 = tabset("/ts0");
    const tab = ts0.getChildren()[0] as TabNode;
    const tabId = tab.getId();

    model.doAction(Action.floatTab(tabId, 100, 100, 400, 300));
    expect(model.getwindowsMap().size).toBe(2);

    const floatWindowIds = nonMainWindowIds(model);
    expect(floatWindowIds.length).toBe(1);

    const mainRoot = model.getRoot(Model.MAIN_WINDOW_ID)!;
    let mainTabsetId = "";
    mainRoot.forEachNode((node: any) => {
      if (node instanceof TabSetNode && !mainTabsetId) {
        mainTabsetId = node.getId();
      }
    }, 0);

    model.doAction(Action.moveNode(tabId, mainTabsetId, "center", -1));

    expect(model.getwindowsMap().size).toBe(1);
    expect(model.getwindowsMap().has(Model.MAIN_WINDOW_ID)).toBe(true);
  });

  it("canDockToWindow returns true for tabs with default attributes", () => {
    const model = Model.fromJson(twoTabs);
    textRender(model);
    const ts0 = tabset("/ts0");
    const tab = ts0.getChildren()[0] as TabNode;

    expect(canDockToWindow(tab)).toBe(true);
  });

  it("canDockToWindow returns false for tabs with enablePopout explicitly false", () => {
    const model = Model.fromJson({
      global: {},
      borders: [],
      layout: {
        type: "tabset",
        weight: 100,
        children: [
          {
            type: "tab",
            name: "NoDock",
            component: "text",
            enablePopout: false,
          },
        ],
      },
    });

    const root = model.getRoot()!;
    let tab: TabNode | undefined;
    root.forEachNode((node: any) => {
      if (node instanceof TabNode && !tab) {
        tab = node;
      }
    }, 0);

    expect(tab).toBeDefined();
    expect(canDockToWindow(tab!)).toBe(false);
  });
});
