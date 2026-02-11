import { describe, it, expect } from "vitest";
import { Model } from "../../flexlayout/model/Model";
import { Action } from "../../flexlayout/model/Action";
import { DockLocation } from "../../flexlayout/core/DockLocation";
import { DropInfo } from "../../flexlayout/core/DropInfo";
import { Rect } from "../../flexlayout/core/Rect";
import { CLASSES } from "../../flexlayout/core/Types";
import type { IJsonModel } from "../../flexlayout/types";
import {
  buildOutlineClass,
  buildGhostClass,
  isDroppableNode,
  computeRelativePosition,
  exceedsThreshold,
  dropInfoToTarget,
  buildMoveAction,
  DRAG_THRESHOLD,
  type DropTarget,
} from "../dnd/DndContext";
import { isTargetNode, getDropLocationForNode } from "../dnd/useDropTarget";

const twoTabsets: IJsonModel = {
  global: {},
  borders: [],
  layout: {
    type: "row",
    weight: 100,
    children: [
      {
        type: "tabset",
        id: "ts0",
        weight: 50,
        children: [
          { type: "tab", id: "tab1", name: "One", component: "text" },
          { type: "tab", id: "tab2", name: "Two", component: "text" },
        ],
      },
      {
        type: "tabset",
        id: "ts1",
        weight: 50,
        children: [
          { type: "tab", id: "tab3", name: "Three", component: "text" },
        ],
      },
    ],
  },
};

const withBorders: IJsonModel = {
  global: {},
  borders: [
    {
      type: "border",
      location: "left",
      children: [
        { type: "tab", id: "btab1", name: "BorderTab", component: "text" },
      ],
    },
  ],
  layout: {
    type: "row",
    weight: 100,
    children: [
      {
        type: "tabset",
        id: "ts0",
        weight: 100,
        children: [
          { type: "tab", id: "tab1", name: "One", component: "text" },
        ],
      },
    ],
  },
};

function makeModel(json: IJsonModel = twoTabsets): Model {
  return Model.fromJson(json);
}

describe("DRAG_THRESHOLD", () => {
  it("is 5 pixels", () => {
    expect(DRAG_THRESHOLD).toBe(5);
  });
});

describe("exceedsThreshold", () => {
  it("returns false when within threshold", () => {
    expect(exceedsThreshold(100, 100, 103, 103)).toBe(false);
  });

  it("returns true when X exceeds threshold", () => {
    expect(exceedsThreshold(100, 100, 106, 100)).toBe(true);
  });

  it("returns true when Y exceeds threshold", () => {
    expect(exceedsThreshold(100, 100, 100, 106)).toBe(true);
  });

  it("returns false at exactly threshold", () => {
    expect(exceedsThreshold(100, 100, 105, 100)).toBe(false);
  });

  it("returns true for negative direction", () => {
    expect(exceedsThreshold(100, 100, 94, 100)).toBe(true);
  });

  it("supports custom threshold", () => {
    expect(exceedsThreshold(100, 100, 112, 100, 10)).toBe(true);
    expect(exceedsThreshold(100, 100, 108, 100, 10)).toBe(false);
  });
});

describe("computeRelativePosition", () => {
  it("computes position relative to container", () => {
    const pos = computeRelativePosition(150, 250, { left: 50, top: 100 });
    expect(pos.x).toBe(100);
    expect(pos.y).toBe(150);
  });

  it("handles zero offset", () => {
    const pos = computeRelativePosition(42, 84, { left: 0, top: 0 });
    expect(pos.x).toBe(42);
    expect(pos.y).toBe(84);
  });

  it("handles negative results when pointer is outside container", () => {
    const pos = computeRelativePosition(10, 10, { left: 50, top: 50 });
    expect(pos.x).toBe(-40);
    expect(pos.y).toBe(-40);
  });
});

describe("isDroppableNode", () => {
  it("returns false for undefined", () => {
    expect(isDroppableNode(undefined)).toBe(false);
  });

  it("returns true for tab nodes", () => {
    const model = makeModel();
    const tab = model.getNodeById("tab1");
    expect(isDroppableNode(tab)).toBe(true);
  });

  it("returns true for tabset nodes", () => {
    const model = makeModel();
    const tabset = model.getNodeById("ts0");
    expect(isDroppableNode(tabset)).toBe(true);
  });

  it("returns false for row nodes", () => {
    const model = makeModel();
    const root = model.getRoot();
    expect(isDroppableNode(root)).toBe(false);
  });
});

describe("buildOutlineClass", () => {
  it("returns mapped className for normal drop zone", () => {
    const result = buildOutlineClass({
      className: CLASSES.FLEXLAYOUT__OUTLINE_RECT,
      rect: { width: 100 },
    });
    expect(result).toBe(CLASSES.FLEXLAYOUT__OUTLINE_RECT);
  });

  it("adds tab_reorder suffix for narrow rects (width <= 5)", () => {
    const result = buildOutlineClass({
      className: CLASSES.FLEXLAYOUT__OUTLINE_RECT,
      rect: { width: 3 },
    });
    expect(result).toContain(CLASSES.FLEXLAYOUT__OUTLINE_RECT);
    expect(result).toContain("_tab_reorder");
  });

  it("applies classNameMapper", () => {
    const mapper = (cls: string) => `custom-${cls}`;
    const result = buildOutlineClass(
      { className: CLASSES.FLEXLAYOUT__OUTLINE_RECT, rect: { width: 100 } },
      mapper,
    );
    expect(result).toBe(`custom-${CLASSES.FLEXLAYOUT__OUTLINE_RECT}`);
  });

  it("applies mapper to both classes for narrow rects", () => {
    const mapper = (cls: string) => `x-${cls}`;
    const result = buildOutlineClass(
      { className: CLASSES.FLEXLAYOUT__OUTLINE_RECT, rect: { width: 2 } },
      mapper,
    );
    expect(result).toContain("x-" + CLASSES.FLEXLAYOUT__OUTLINE_RECT);
    expect(result).toContain("x-" + CLASSES.FLEXLAYOUT__OUTLINE_RECT + "_tab_reorder");
  });
});

describe("buildGhostClass", () => {
  it("returns tab button + selected classes", () => {
    const result = buildGhostClass();
    expect(result).toContain(CLASSES.FLEXLAYOUT__TAB_BUTTON);
    expect(result).toContain(CLASSES.FLEXLAYOUT__TAB_BUTTON + "--selected");
  });

  it("applies classNameMapper", () => {
    const mapper = (cls: string) => `m-${cls}`;
    const result = buildGhostClass(mapper);
    expect(result).toBe(
      `m-${CLASSES.FLEXLAYOUT__TAB_BUTTON} m-${CLASSES.FLEXLAYOUT__TAB_BUTTON}--selected`,
    );
  });
});

describe("dropInfoToTarget", () => {
  it("converts DropInfo to DropTarget", () => {
    const model = makeModel();
    const node = model.getNodeById("ts0")!;
    const info = new DropInfo(
      node as any,
      new Rect(10, 20, 300, 200),
      DockLocation.CENTER,
      0,
      CLASSES.FLEXLAYOUT__OUTLINE_RECT,
    );

    const target = dropInfoToTarget(info);

    expect(target.nodeId).toBe("ts0");
    expect(target.location).toBe("center");
    expect(target.index).toBe(0);
    expect(target.className).toBe(CLASSES.FLEXLAYOUT__OUTLINE_RECT);
    expect(target.rect).toEqual({ x: 10, y: 20, width: 300, height: 200 });
  });

  it("preserves location name from DockLocation", () => {
    const model = makeModel();
    const node = model.getNodeById("ts1")!;
    const info = new DropInfo(
      node as any,
      new Rect(0, 0, 100, 50),
      DockLocation.LEFT,
      2,
      "some_class",
    );

    const target = dropInfoToTarget(info);
    expect(target.location).toBe("left");
    expect(target.index).toBe(2);
  });
});

describe("buildMoveAction", () => {
  it("creates MOVE_NODE action with correct data", () => {
    const target: DropTarget = {
      nodeId: "ts1",
      location: "center",
      index: 0,
      className: "outline",
      rect: { x: 0, y: 0, width: 100, height: 100 },
    };

    const action = buildMoveAction("tab1", target);

    expect(action.type).toBe("MOVE_NODE");
    expect(action.data).toEqual({
      fromNode: "tab1",
      toNode: "ts1",
      location: "center",
      index: 0,
      select: undefined,
    });
  });

  it("creates action for border targets", () => {
    const target: DropTarget = {
      nodeId: "border_left",
      location: "left",
      index: 1,
      className: "outline",
      rect: { x: 0, y: 0, width: 5, height: 300 },
    };

    const action = buildMoveAction("tab2", target);

    expect(action.type).toBe("MOVE_NODE");
    const data = (action as any).data;
    expect(data.fromNode).toBe("tab2");
    expect(data.toNode).toBe("border_left");
    expect(data.location).toBe("left");
    expect(data.index).toBe(1);
  });
});

describe("isTargetNode", () => {
  it("returns true when target matches nodeId", () => {
    const target: DropTarget = {
      nodeId: "ts0",
      location: "center",
      index: 0,
      className: "",
      rect: { x: 0, y: 0, width: 100, height: 100 },
    };
    expect(isTargetNode(target, "ts0")).toBe(true);
  });

  it("returns false when target has different nodeId", () => {
    const target: DropTarget = {
      nodeId: "ts0",
      location: "center",
      index: 0,
      className: "",
      rect: { x: 0, y: 0, width: 100, height: 100 },
    };
    expect(isTargetNode(target, "ts1")).toBe(false);
  });

  it("returns false for null target", () => {
    expect(isTargetNode(null, "ts0")).toBe(false);
  });
});

describe("getDropLocationForNode", () => {
  it("returns location when target matches", () => {
    const target: DropTarget = {
      nodeId: "ts0",
      location: "top",
      index: 0,
      className: "",
      rect: { x: 0, y: 0, width: 100, height: 100 },
    };
    expect(getDropLocationForNode(target, "ts0")).toBe("top");
  });

  it("returns null for non-matching target", () => {
    const target: DropTarget = {
      nodeId: "ts0",
      location: "top",
      index: 0,
      className: "",
      rect: { x: 0, y: 0, width: 100, height: 100 },
    };
    expect(getDropLocationForNode(target, "ts1")).toBeNull();
  });

  it("returns null for null target", () => {
    expect(getDropLocationForNode(null, "ts0")).toBeNull();
  });
});

describe("model integration: moveNode action", () => {
  it("moves tab between tabsets", () => {
    const model = makeModel();

    model.doAction(Action.moveNode("tab1", "ts1", "center", 0));

    const ts0 = model.getNodeById("ts0");
    const ts1 = model.getNodeById("ts1");
    expect(ts0?.getChildren().length).toBe(1);
    expect(ts1?.getChildren().length).toBe(2);
    expect(ts1?.getChildren()[0].getId()).toBe("tab1");
  });

  it("reorders tab within tabset", () => {
    const model = makeModel();
    model.doAction(Action.moveNode("tab2", "ts0", "center", 0));

    const ts0 = model.getNodeById("ts0");
    expect(ts0?.getChildren()[0].getId()).toBe("tab2");
    expect(ts0?.getChildren()[1].getId()).toBe("tab1");
  });

  it("moves tab to different location (left split)", () => {
    const model = makeModel();
    model.doAction(Action.moveNode("tab1", "ts1", "left", -1));

    const root = model.getRoot();
    const children = root?.getChildren() ?? [];
    expect(children.length).toBeGreaterThanOrEqual(2);
  });
});

describe("model integration: border layout", () => {
  it("model with borders has border set", () => {
    const model = makeModel(withBorders);
    const borderSet = model.getBorderSet();
    expect(borderSet).toBeDefined();
  });

  it("tab can be retrieved from model", () => {
    const model = makeModel(withBorders);
    const btab = model.getNodeById("btab1");
    expect(btab).toBeDefined();
    expect((btab as any).getName?.()).toBe("BorderTab");
  });
});

describe("DockLocation reference", () => {
  it("has expected named locations", () => {
    expect(DockLocation.TOP.getName()).toBe("top");
    expect(DockLocation.BOTTOM.getName()).toBe("bottom");
    expect(DockLocation.LEFT.getName()).toBe("left");
    expect(DockLocation.RIGHT.getName()).toBe("right");
    expect(DockLocation.CENTER.getName()).toBe("center");
  });

  it("getByName resolves correctly", () => {
    expect(DockLocation.getByName("top")).toBe(DockLocation.TOP);
    expect(DockLocation.getByName("center")).toBe(DockLocation.CENTER);
  });
});

describe("DropInfo construction", () => {
  it("creates with all fields", () => {
    const model = makeModel();
    const node = model.getNodeById("ts0")!;
    const rect = new Rect(0, 0, 200, 100);
    const info = new DropInfo(
      node as any,
      rect,
      DockLocation.TOP,
      1,
      "test_class",
    );

    expect(info.node).toBe(node);
    expect(info.rect).toBe(rect);
    expect(info.location).toBe(DockLocation.TOP);
    expect(info.index).toBe(1);
    expect(info.className).toBe("test_class");
  });
});

describe("end-to-end: drag lifecycle simulation", () => {
  it("simulates full drag-drop: tab1 from ts0 to ts1", () => {
    const model = makeModel();

    const tab1 = model.getNodeById("tab1")!;
    expect(isDroppableNode(tab1)).toBe(true);

    const startPos = computeRelativePosition(100, 100, { left: 0, top: 0 });
    expect(startPos.x).toBe(100);
    expect(startPos.y).toBe(100);

    expect(exceedsThreshold(100, 100, 100, 100)).toBe(false);
    expect(exceedsThreshold(100, 100, 110, 110)).toBe(true);

    const target: DropTarget = {
      nodeId: "ts1",
      location: "center",
      index: 0,
      className: CLASSES.FLEXLAYOUT__OUTLINE_RECT,
      rect: { x: 200, y: 0, width: 200, height: 400 },
    };

    const action = buildMoveAction("tab1", target);
    expect(action.type).toBe("MOVE_NODE");

    model.doAction(action);

    const ts0 = model.getNodeById("ts0");
    const ts1 = model.getNodeById("ts1");
    expect(ts0?.getChildren().length).toBe(1);
    expect(ts1?.getChildren().length).toBe(2);
    expect(ts1?.getChildren()[0].getId()).toBe("tab1");
  });

  it("simulates drag cancel via escape (no action dispatched)", () => {
    const model = makeModel();
    const tab1 = model.getNodeById("tab1")!;

    expect(isDroppableNode(tab1)).toBe(true);
    expect(exceedsThreshold(100, 100, 110, 100)).toBe(true);

    const ts0Before = model.getNodeById("ts0")!.getChildren().length;
    const ts1Before = model.getNodeById("ts1")!.getChildren().length;

    expect(ts0Before).toBe(2);
    expect(ts1Before).toBe(1);
  });

  it("simulates reorder within tabset", () => {
    const model = makeModel();

    const target: DropTarget = {
      nodeId: "ts0",
      location: "center",
      index: 0,
      className: CLASSES.FLEXLAYOUT__OUTLINE_RECT,
      rect: { x: 0, y: 0, width: 5, height: 30 },
    };

    const outlineCls = buildOutlineClass(target);
    expect(outlineCls).toContain("_tab_reorder");

    const action = buildMoveAction("tab2", target);
    model.doAction(action);

    const ts0 = model.getNodeById("ts0");
    expect(ts0?.getChildren()[0].getId()).toBe("tab2");
    expect(ts0?.getChildren()[1].getId()).toBe("tab1");
  });
});
