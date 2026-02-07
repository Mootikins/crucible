import { describe, it, expect } from "vitest";
import { twoTabs, withBorders, threeTabs } from "./fixtures";
import { Model } from "../model/Model";
import { Action } from "../model/Action";

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
    expect(tabs).equal("/ts0/t0[newtab1]*");

    const tab = model.getNodeById("2") as any;
    expect(tab?.getId()).equal("2");
    expect(tab?.getComponent()).equal("grid");
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
    expect(tabs).equal("/ts0/t0[One],/ts0/t1[newtab1]*,/ts1/t0[Two]*");

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
    expect(tabs).equal(
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
    expect(tabs).equal("/ts0/t0[newtab1]*,/ts0/t1[One],/ts1/t0[Two]*");

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
    expect(tabs).equal(
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
    expect(tabs).equal(
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
    expect(tabs).equal("/r0/ts0/t0[newtab1]*,/r0/ts1/t0[One]*,/ts1/t0[Two]*");

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
    expect(tabs).equal(
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
    expect(tabs).equal("/r0/ts0/t0[One]*,/r0/ts1/t0[newtab1]*,/ts1/t0[Two]*");

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
    expect(tabs).equal(
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
    expect(tabs).equal("/ts0/t0[newtab1]*,/ts1/t0[One]*,/ts2/t0[Two]*");

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
    expect(tabs).equal(
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
    expect(tabs).equal("/ts0/t0[One]*,/ts1/t0[newtab1]*,/ts2/t0[Two]*");

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
    expect(tabs).equal(
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
    expect(tabs).equal("/b/top/t0[top1],/b/top/t1[newtab1]*,/b/bottom/t0[bottom1],/b/bottom/t1[bottom2],/b/left/t0[left1],/b/right/t0[right1],/ts0/t0[One]*");

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
    expect(tabs).equal("/b/top/t0[newtab2]*,/b/top/t1[top1],/b/top/t2[newtab1],/b/bottom/t0[bottom1],/b/bottom/t1[bottom2],/b/left/t0[left1],/b/right/t0[right1],/ts0/t0[One]*");

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
    expect(tabs).equal(
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
    expect(tabs).equal(
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
    expect(tabs).equal(
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
    expect(tabs).equal(
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
    expect(tabs).equal("/b/top/t0[top1],/b/bottom/t0[bottom1],/b/bottom/t1[bottom2],/b/left/t0[left1],/b/left/t1[newtab1]*,/b/right/t0[right1],/ts0/t0[One]*");

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
    expect(tabs).equal("/b/top/t0[top1],/b/bottom/t0[bottom1],/b/bottom/t1[bottom2],/b/left/t0[newtab2]*,/b/left/t1[left1],/b/left/t2[newtab1],/b/right/t0[right1],/ts0/t0[One]*");

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
    expect(tabs).equal(
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
    expect(tabs).equal("/b/top/t0[top1],/b/bottom/t0[bottom1],/b/bottom/t1[bottom2],/b/left/t0[left1],/b/right/t0[right1],/b/right/t1[newtab1]*,/ts0/t0[One]*");

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
    expect(tabs).equal(
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
    expect(tabs).equal(
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
    expect(tabs).equal("/ts0/t0[Two],/ts0/t1[One]*,/ts1/t0[Three]*");
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
    expect(tabs).equal("/ts0/t0[One]*,/ts0/t1[Two],/ts1/t0[Three]*");

    const ts1_after = tabset("/ts1");
    const t1_after = ts1_after?.getChildren()[0];

    model.doAction(
      Action.moveNode(t1_after?.getId() || "", tabset("/ts0")?.getId() || "", "center", 1)
    );

    tabs = textRender(model);
    expect(tabs).equal("/ts0/t0[One],/ts0/t1[Three]*,/ts0/t2[Two]");
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
    expect(tabs).equal("/r0/ts0/t0[One]*,/r0/ts1/t0[Two]*,/ts1/t0[Three]*");
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
    expect(tabs).equal("/r0/ts0/t0[Two]*,/r0/ts1/t0[One]*,/ts1/t0[Three]*");
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
    expect(tabs).equal("/ts0/t0[One]*,/ts1/t0[Two]*,/ts2/t0[Three]*");
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
    expect(tabs).equal("/ts0/t0[Two]*,/ts1/t0[One]*,/ts2/t0[Three]*");
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
    expect(tabs).equal(
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
    expect(tabs).equal(
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
    expect(tabs).equal(
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
    expect(tabs).equal(
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
    expect(tabs).equal(
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
    expect(tabs).equal(
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
    expect(tabs).equal(
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
    expect(tabs).equal(
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
    expect(tabs).equal("/ts0/t0[Two]*,/ts1/t0[Three]*");
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
    expect(tabs).equal("/ts0/t0[Two],/ts0/t1[Three]*");
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
    expect(tabs).equal("/ts0/t0[Three]*");
  });

  it("delete tab from borders", () => {
    model = Model.fromJson(withBorders);
    textRender(model);
    const topBorder = model.getBorderSet().getBorder("top");
    const t0 = topBorder?.getChildren()[0];

    model.doAction(Action.deleteTab(t0?.getId() || ""));

    let tabs = textRender(model);
    expect(tabs).equal(
      "/b/bottom/t0[bottom1],/b/bottom/t1[bottom2],/b/left/t0[left1],/b/right/t0[right1],/ts0/t0[One]*"
    );

    const bottomBorder = model.getBorderSet().getBorder("bottom");
    const t1 = bottomBorder?.getChildren()[0];
    model.doAction(Action.deleteTab(t1?.getId() || ""));

    tabs = textRender(model);
    expect(tabs).equal(
      "/b/bottom/t0[bottom2],/b/left/t0[left1],/b/right/t0[right1],/ts0/t0[One]*"
    );

    const bottomBorder2 = model.getBorderSet().getBorder("bottom");
    const t2 = bottomBorder2?.getChildren()[0];
    model.doAction(Action.deleteTab(t2?.getId() || ""));

    tabs = textRender(model);
    expect(tabs).equal(
      "/b/left/t0[left1],/b/right/t0[right1],/ts0/t0[One]*"
    );

    const leftBorder = model.getBorderSet().getBorder("left");
    const t3 = leftBorder?.getChildren()[0];
    model.doAction(Action.deleteTab(t3?.getId() || ""));

    tabs = textRender(model);
    expect(tabs).equal("/b/right/t0[right1],/ts0/t0[One]*");

    const rightBorder = model.getBorderSet().getBorder("right");
    const t4 = rightBorder?.getChildren()[0];
    model.doAction(Action.deleteTab(t4?.getId() || ""));

    tabs = textRender(model);
    expect(tabs).equal("/ts0/t0[One]*");
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
    expect(tabs).equal("/ts0/t0[renamed]*,/ts1/t0[Two]*");
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
    expect(tabs).equal("/ts0/t0[One]*,/ts0/t1[newtab1],/ts1/t0[Two]*");
  });

  it("set active tabset", () => {
    model = Model.fromJson(twoTabs);
    textRender(model);
    const ts0 = tabset("/ts0") as any;
    const ts1 = tabset("/ts1") as any;

    expect(ts0?.isActive()).equal(false);
    expect(ts1?.isActive()).equal(false);

    model.doAction(Action.selectTab(ts0?.getChildren()[0]?.getId() || ""));
    expect(ts0?.isActive()).equal(true);
    expect(ts1?.isActive()).equal(false);

    model.doAction(Action.selectTab(ts1?.getChildren()[0]?.getId() || ""));
    expect(ts0?.isActive()).equal(false);
    expect(ts1?.isActive()).equal(true);

    model.doAction(Action.setActiveTabset(ts0?.getId() || ""));
    expect(ts0?.isActive()).equal(true);
    expect(ts1?.isActive()).equal(false);
  });

  it("maximize tabset", () => {
    model = Model.fromJson(twoTabs);
    textRender(model);
    const ts0 = tabset("/ts0");
    const ts1 = tabset("/ts1");

    model.doAction(Action.maximizeToggle(ts0?.getId() || ""));

    expect(ts0?.isMaximized()).equals(false);
    expect(ts1?.isMaximized()).equals(false);
    expect(model.getMaximizedTabset()).equals(undefined);
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

    expect(t0?.getConfig()).equals("newConfig");
  });

  it("set model attributes", () => {
    model = Model.fromJson(twoTabs);

    model.doAction(
      Action.updateModelAttributes({
        splitterSize: 10,
      })
    );

    expect(model.getSplitterSize()).equals(10);
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

    expect(closed).equals(true);
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

    expect(saved).equals(true);
  });
});

function tabset(path: string): any {
  return pathMap[path];
}
