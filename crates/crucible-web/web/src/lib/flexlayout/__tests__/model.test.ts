import { describe, it, expect } from "vitest";
import { twoTabs, withBorders, threeTabs } from "./fixtures";
import { Model } from "../model/Model";
import { Action } from "../model/Action";

/**
 * textRender converts a model tree to a path string format for test assertions
 * Format: /ts0/t0[TabName]* where * indicates selected tab
 */
function textRender(model: Model): string {
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

  return tabsArray.join(",");
}

function textRenderBorder(
  border: any,
  path: string,
  tabsArray: string[]
): void {
  let index = 0;
  for (const tab of border.getChildren()) {
    const tabPath = `${path}/t${index}`;
    const name = tab.getName() || "";
    const isSelected = index === (border as any).getSelected();
    tabsArray.push(`${tabPath}[${name}]${isSelected ? "*" : ""}`);
    index++;
  }
}

function textRenderNode(node: any, path: string, tabsArray: string[]): void {
  const type = node.getType();

  if (type === "row") {
    let rowIndex = 0;
    let tsIndex = 0;
    for (const child of node.getChildren()) {
      if (child.getType() === "row") {
        const newPath = `${path}/r${rowIndex}`;
        textRenderNode(child, newPath, tabsArray);
        rowIndex++;
      } else if (child.getType() === "tabset") {
        const tsPath = `${path}/ts${tsIndex}`;
        textRenderTabset(child, tsPath, tabsArray);
        tsIndex++;
      }
    }
  } else if (type === "tabset") {
    textRenderTabset(node, path, tabsArray);
  }
}

function textRenderTabset(tabset: any, path: string, tabsArray: string[]): void {
  let tabIndex = 0;
  for (const tab of tabset.getChildren()) {
    const tabPath = `${path}/t${tabIndex}`;
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
    const ts0 = model.getNodeById("ts0");
    const ts1 = model.getNodeById("ts1");

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
    const ts0 = model.getNodeById("ts0");

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
    const ts0 = model.getNodeById("ts0");

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
        model.getNodeById("ts1")?.getId() || "",
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
    const ts0 = model.getNodeById("ts0");

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
        model.getNodeById("ts1")?.getId() || "",
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
    const ts0 = model.getNodeById("ts0");

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
        model.getNodeById("ts2")?.getId() || "",
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
    const ts0 = model.getNodeById("ts0");

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
        model.getNodeById("ts2")?.getId() || "",
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
    const ts1 = model.getNodeById("ts1");
    const t1 = ts1?.getChildren()[0];

    model.doAction(
      Action.moveNode(t1?.getId() || "", model.getNodeById("ts0")?.getId() || "", "center", -1)
    );

    const tabs = textRender(model);
    expect(tabs).equal("/ts0/t0[Two],/ts0/t1[One]*,/ts1/t0[Three]*");
  });

  it("move to center position", () => {
    model = Model.fromJson(threeTabs);
    const ts1 = model.getNodeById("ts1");
    const t1 = ts1?.getChildren()[0];

    model.doAction(
      Action.moveNode(t1?.getId() || "", model.getNodeById("ts0")?.getId() || "", "center", 1)
    );

    let tabs = textRender(model);
    expect(tabs).equal("/ts0/t0[One]*,/ts0/t1[Two],/ts1/t0[Three]*");

    model = Model.fromJson(threeTabs);
    const ts2 = model.getNodeById("ts2");
    const t2 = ts2?.getChildren()[0];

    model.doAction(
      Action.moveNode(t2?.getId() || "", model.getNodeById("ts0")?.getId() || "", "center", 2)
    );

    tabs = textRender(model);
    expect(tabs).equal("/ts0/t0[One],/ts0/t1[Three]*,/ts0/t2[Two]");
  });

  it("move to top", () => {
    model = Model.fromJson(threeTabs);
    const ts1 = model.getNodeById("ts1");
    const t1 = ts1?.getChildren()[0];

    model.doAction(
      Action.moveNode(t1?.getId() || "", model.getNodeById("ts0")?.getId() || "", "top", -1)
    );

    const tabs = textRender(model);
    expect(tabs).equal("/r0/ts0/t0[One]*,/r0/ts1/t0[Two]*,/ts1/t0[Three]*");
  });

  it("move to bottom", () => {
    model = Model.fromJson(threeTabs);
    const ts1 = model.getNodeById("ts1");
    const t1 = ts1?.getChildren()[0];

    model.doAction(
      Action.moveNode(t1?.getId() || "", model.getNodeById("ts0")?.getId() || "", "bottom", -1)
    );

    const tabs = textRender(model);
    expect(tabs).equal("/r0/ts0/t0[Two]*,/r0/ts1/t0[One]*,/ts1/t0[Three]*");
  });

  it("move to left", () => {
    model = Model.fromJson(threeTabs);
    const ts1 = model.getNodeById("ts1");
    const t1 = ts1?.getChildren()[0];

    model.doAction(
      Action.moveNode(t1?.getId() || "", model.getNodeById("ts0")?.getId() || "", "left", -1)
    );

    const tabs = textRender(model);
    expect(tabs).equal("/ts0/t0[One]*,/ts1/t0[Two]*,/ts2/t0[Three]*");
  });

  it("move to right", () => {
    model = Model.fromJson(threeTabs);
    const ts0 = model.getNodeById("ts0");
    const t0 = ts0?.getChildren()[0];

    model.doAction(
      Action.moveNode(t0?.getId() || "", model.getNodeById("ts1")?.getId() || "", "center", -1)
    );

    const tabs = textRender(model);
    expect(tabs).equal("/ts0/t0[Two]*,/ts1/t0[One]*,/ts2/t0[Three]*");
  });
});

describe("Tree > Actions > Move to/from borders", () => {
  let model: Model;

  it("move to border top", () => {
    model = Model.fromJson(withBorders);
    const ts0 = model.getNodeById("ts0");
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
    const ts0 = model.getNodeById("ts0");
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
    const ts0 = model.getNodeById("ts0");
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
    const ts0 = model.getNodeById("ts0");
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
      "/b/bottom/t0[bottom1],/b/bottom/t1[bottom2],/b/left/t0[left1],/b/right/t0[right1],/ts0/t0[One],/ts0/t1[top1]*"
    );
  });

  it("move from border bottom", () => {
    model = Model.fromJson(withBorders);
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
      "/b/top/t0[top1],/b/bottom/t0[bottom2],/b/left/t0[left1],/b/right/t0[right1],/ts0/t0[One],/ts0/t1[bottom1]*"
    );
  });

  it("move from border left", () => {
    model = Model.fromJson(withBorders);
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
      "/b/top/t0[top1],/b/bottom/t0[bottom1],/b/bottom/t1[bottom2],/b/right/t0[right1],/ts0/t0[One],/ts0/t1[left1]*"
    );
  });

  it("move from border right", () => {
    model = Model.fromJson(withBorders);
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
      "/b/top/t0[top1],/b/bottom/t0[bottom1],/b/bottom/t1[bottom2],/b/left/t0[left1],/ts0/t0[One],/ts0/t1[right1]*"
    );
  });
});

describe("Tree > Actions > Delete", () => {
  let model: Model;

  it("delete from tabset with 1 tab", () => {
    model = Model.fromJson(threeTabs);
    const ts0 = model.getNodeById("ts0");
    const t0 = ts0?.getChildren()[0];

    model.doAction(Action.deleteTab(t0?.getId() || ""));

    const tabs = textRender(model);
    expect(tabs).equal("/ts0/t0[Two]*,/ts1/t0[Three]*");
  });

  it("delete tab from tabset with 3 tabs", () => {
    model = Model.fromJson(threeTabs);
    const ts0 = model.getNodeById("ts0");

    model.doAction(
      Action.addNode(
        {
          type: "tab",
          name: "Four",
          component: "grid",
        },
        ts0?.getId() || "",
        "center",
        -1
      )
    );

    const t0 = ts0?.getChildren()[0];
    model.doAction(Action.deleteTab(t0?.getId() || ""));

    const tabs = textRender(model);
    expect(tabs).equal("/ts0/t0[Two],/ts0/t1[Three]*");
  });

  it("delete tabset", () => {
    model = Model.fromJson(threeTabs);
    const ts0 = model.getNodeById("ts0");

    model.doAction(Action.deleteTabset(ts0?.getId() || ""));

    const tabs = textRender(model);
    expect(tabs).equal("/ts0/t0[Three]*");
  });

  it("delete tab from borders", () => {
    model = Model.fromJson(withBorders);
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

    const leftBorder = model.getBorderSet().getBorder("left");
    const t2 = leftBorder?.getChildren()[0];
    model.doAction(Action.deleteTab(t2?.getId() || ""));

    tabs = textRender(model);
    expect(tabs).equal(
      "/b/left/t0[left1],/b/right/t0[right1],/ts0/t0[One]*"
    );

    const rightBorder = model.getBorderSet().getBorder("right");
    const t3 = rightBorder?.getChildren()[0];
    model.doAction(Action.deleteTab(t3?.getId() || ""));

    tabs = textRender(model);
    expect(tabs).equal("/b/right/t0[right1],/ts0/t0[One]*");

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
    const ts0 = model.getNodeById("ts0");
    const t0 = ts0?.getChildren()[0];

    model.doAction(Action.renameTab(t0?.getId() || "", "renamed"));

    const tabs = textRender(model);
    expect(tabs).equal("/ts0/t0[renamed]*,/ts1/t0[Two]*");
  });

  it("select tab", () => {
    model = Model.fromJson(twoTabs);
    const ts0 = model.getNodeById("ts0");

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
    const ts0 = model.getNodeById("ts0") as any;
    const ts1 = model.getNodeById("ts1") as any;

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
    const ts0 = model.getNodeById("ts0");
    const ts1 = model.getNodeById("ts1");

    model.doAction(Action.maximizeToggle(ts0?.getId() || ""));

    expect(tabset(ts0)?.isMaximized()).equals(false);
    expect(tabset(ts1)?.isMaximized()).equals(false);
    expect(model.getMaximizedTabset()).equals(undefined);
  });

  it("set tab attributes", () => {
    model = Model.fromJson(twoTabs);
    const ts1 = model.getNodeById("ts1");
    const t0 = ts1?.getChildren()[0];

    model.doAction(
      Action.updateNodeAttributes(t0?.getId() || "", {
        config: "newConfig",
      })
    );

    expect(tab(t0)?.getConfig()).equals("newConfig");
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
    const ts0 = model.getNodeById("ts0");
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
    const ts0 = model.getNodeById("ts0");
    const t0 = ts0?.getChildren()[0];

    let saved = false;
    t0?.setEventListener("save", () => {
      saved = true;
    });

    model.doAction(Action.updateNodeAttributes(t0?.getId() || "", {}));

    expect(saved).equals(true);
  });
});

// Helper functions
function tab(node: any): any {
  return node;
}

function tabset(node: any): any {
  return node;
}
