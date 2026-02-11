import { describe, it, expect } from "vitest";
import { createRoot } from "solid-js";
import { Model } from "../../flexlayout/model/Model";
import { Action } from "../../flexlayout/model/Action";
import type { IJsonModel, IJsonRowNode, IJsonTabSetNode } from "../../flexlayout/types";
import { createLayoutBridge } from "../bridge";
import { isHorizontal, totalWeight, childPath } from "../components/Row";
import { calculateSplitterDelta } from "../components/Splitter";

const twoTabsets: IJsonModel = {
  global: { splitterSize: 8 },
  borders: [],
  layout: {
    type: "row",
    id: "root",
    weight: 100,
    children: [
      {
        type: "tabset",
        id: "ts0",
        weight: 50,
        children: [
          { type: "tab", id: "tab1", name: "One", component: "text" },
        ],
      },
      {
        type: "tabset",
        id: "ts1",
        weight: 50,
        children: [
          { type: "tab", id: "tab2", name: "Two", component: "text" },
        ],
      },
    ],
  },
};

const threeTabsets: IJsonModel = {
  global: { splitterSize: 8 },
  borders: [],
  layout: {
    type: "row",
    id: "root",
    weight: 100,
    children: [
      {
        type: "tabset",
        id: "ts0",
        weight: 25,
        children: [
          { type: "tab", id: "t0", name: "A", component: "text" },
        ],
      },
      {
        type: "tabset",
        id: "ts1",
        weight: 50,
        children: [
          { type: "tab", id: "t1", name: "B", component: "text" },
        ],
      },
      {
        type: "tabset",
        id: "ts2",
        weight: 25,
        children: [
          { type: "tab", id: "t2", name: "C", component: "text" },
        ],
      },
    ],
  },
};

const nestedRows: IJsonModel = {
  global: { splitterSize: 4 },
  borders: [],
  layout: {
    type: "row",
    id: "root",
    weight: 100,
    children: [
      {
        type: "tabset",
        id: "ts0",
        weight: 50,
        children: [
          { type: "tab", id: "tab1", name: "Left", component: "text" },
        ],
      },
      {
        type: "row",
        id: "nested-row",
        weight: 50,
        children: [
          {
            type: "tabset",
            id: "ts1",
            weight: 50,
            children: [
              { type: "tab", id: "tab2", name: "TopRight", component: "text" },
            ],
          },
          {
            type: "tabset",
            id: "ts2",
            weight: 50,
            children: [
              { type: "tab", id: "tab3", name: "BottomRight", component: "text" },
            ],
          },
        ],
      },
    ],
  },
};

const emptyRow: IJsonModel = {
  global: { splitterSize: 8 },
  borders: [],
  layout: {
    type: "row",
    id: "root",
    weight: 100,
    children: [],
  },
};

function makeModel(json: IJsonModel = twoTabsets): Model {
  return Model.fromJson(json);
}

describe("isHorizontal", () => {
  it("root row is horizontal when rootOrientationVertical is false", () => {
    const node: IJsonRowNode = { type: "row", weight: 100 };
    expect(isHorizontal(node, "/", false)).toBe(true);
  });

  it("root row is vertical when rootOrientationVertical is true", () => {
    const node: IJsonRowNode = { type: "row", weight: 100 };
    expect(isHorizontal(node, "/", true)).toBe(false);
  });

  it("nested row flips orientation from parent", () => {
    const node: IJsonRowNode = { type: "row", weight: 100 };
    expect(isHorizontal(node, "/r0", false)).toBe(false);
    expect(isHorizontal(node, "/r0", true)).toBe(true);
  });

  it("double-nested row returns to root orientation", () => {
    const node: IJsonRowNode = { type: "row", weight: 100 };
    expect(isHorizontal(node, "/r0/r1", false)).toBe(true);
    expect(isHorizontal(node, "/r0/r1", true)).toBe(false);
  });
});

describe("totalWeight", () => {
  it("sums weights of children", () => {
    const children = [
      { type: "tabset", weight: 25 },
      { type: "tabset", weight: 50 },
      { type: "tabset", weight: 25 },
    ];
    expect(totalWeight(children)).toBe(100);
  });

  it("defaults missing weights to 100", () => {
    const children = [
      { type: "tabset" },
      { type: "tabset", weight: 50 },
    ];
    expect(totalWeight(children)).toBe(150);
  });

  it("returns 1 for empty children to prevent division by zero", () => {
    expect(totalWeight([])).toBe(1);
  });
});

describe("childPath", () => {
  it("generates /rN path for row children", () => {
    const child: IJsonRowNode = { type: "row", weight: 50 };
    expect(childPath("/", child, 0)).toBe("//r0");
    expect(childPath("/r0", child, 2)).toBe("/r0/r2");
  });

  it("generates /tsN path for tabset children", () => {
    const child: IJsonTabSetNode = { type: "tabset", weight: 50 };
    expect(childPath("/", child, 0)).toBe("//ts0");
    expect(childPath("/r0", child, 1)).toBe("/r0/ts1");
  });
});

describe("calculateSplitterDelta", () => {
  it("returns 0 when position equals start", () => {
    expect(calculateSplitterDelta(100, 100, [50, 200])).toBe(0);
  });

  it("clamps to lower bound", () => {
    expect(calculateSplitterDelta(100, 10, [50, 200])).toBe(-50);
  });

  it("clamps to upper bound", () => {
    expect(calculateSplitterDelta(100, 300, [50, 200])).toBe(100);
  });

  it("returns unclamped delta within bounds", () => {
    expect(calculateSplitterDelta(100, 150, [50, 200])).toBe(50);
  });
});

describe("Row store integration", () => {
  it("store reflects correct child weights", () => {
    createRoot((dispose) => {
      const model = makeModel(threeTabsets);
      const bridge = createLayoutBridge(model);

      const layout = bridge.store.layout as IJsonRowNode;
      expect(layout.children).toHaveLength(3);
      expect((layout.children![0] as IJsonTabSetNode).weight).toBe(25);
      expect((layout.children![1] as IJsonTabSetNode).weight).toBe(50);
      expect((layout.children![2] as IJsonTabSetNode).weight).toBe(25);

      bridge.dispose();
      dispose();
    });
  });

  it("store updates weights after ADJUST_WEIGHTS action", () => {
    createRoot((dispose) => {
      const model = makeModel(threeTabsets);
      const bridge = createLayoutBridge(model);

      const rootId = model.getRoot()!.getId();
      model.doAction(Action.adjustWeights(rootId, [30, 40, 30], "horizontal"));

      const layout = bridge.store.layout as IJsonRowNode;
      expect((layout.children![0] as IJsonTabSetNode).weight).toBe(30);
      expect((layout.children![1] as IJsonTabSetNode).weight).toBe(40);
      expect((layout.children![2] as IJsonTabSetNode).weight).toBe(30);

      bridge.dispose();
      dispose();
    });
  });

  it("empty row has no children in store", () => {
    createRoot((dispose) => {
      const model = makeModel(emptyRow);
      const bridge = createLayoutBridge(model);

      const layout = bridge.store.layout as IJsonRowNode;
      expect(layout.children ?? []).toHaveLength(0);

      bridge.dispose();
      dispose();
    });
  });

  it("nested row structure is preserved in store", () => {
    createRoot((dispose) => {
      const model = makeModel(nestedRows);
      const bridge = createLayoutBridge(model);

      const layout = bridge.store.layout as IJsonRowNode;
      expect(layout.children).toHaveLength(2);

      const firstChild = layout.children![0] as IJsonTabSetNode;
      expect(firstChild.type).toBe("tabset");
      expect(firstChild.id).toBe("ts0");

      const nestedRow = layout.children![1] as IJsonRowNode;
      expect(nestedRow.type).toBe("row");
      expect(nestedRow.id).toBe("nested-row");
      expect(nestedRow.children).toHaveLength(2);
      expect((nestedRow.children![0] as IJsonTabSetNode).id).toBe("ts1");
      expect((nestedRow.children![1] as IJsonTabSetNode).id).toBe("ts2");

      bridge.dispose();
      dispose();
    });
  });

  it("splitter count equals children count minus one", () => {
    createRoot((dispose) => {
      const model = makeModel(threeTabsets);
      const bridge = createLayoutBridge(model);

      const layout = bridge.store.layout as IJsonRowNode;
      const childCount = (layout.children ?? []).length;
      const expectedSplitters = Math.max(0, childCount - 1);
      expect(expectedSplitters).toBe(2);

      bridge.dispose();
      dispose();
    });
  });

  it("model getSplitterSize returns configured value", () => {
    const model = makeModel(twoTabsets);
    expect(model.getSplitterSize()).toBe(8);

    const model2 = makeModel(nestedRows);
    expect(model2.getSplitterSize()).toBe(4);
  });

  it("data-layout-path for children follows /tsN and /rN convention", () => {
    const tabsetChild: IJsonTabSetNode = { type: "tabset", weight: 50 };
    const rowChild: IJsonRowNode = { type: "row", weight: 50 };

    expect(childPath("/", tabsetChild, 0)).toBe("//ts0");
    expect(childPath("/", rowChild, 1)).toBe("//r1");
    expect(childPath("/r0", tabsetChild, 2)).toBe("/r0/ts2");
  });

  it("CSS class constants are correct", () => {
    const { CLASSES } = require("../../flexlayout/core/Types");
    expect(CLASSES.FLEXLAYOUT__ROW).toBe("flexlayout__row");
    expect(CLASSES.FLEXLAYOUT__SPLITTER).toBe("flexlayout__splitter");
    expect(CLASSES.FLEXLAYOUT__SPLITTER_).toBe("flexlayout__splitter_");
  });
});
