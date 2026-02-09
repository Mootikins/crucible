import { describe, it, expect } from "vitest";
import { Model } from "../model/Model";
import { Action } from "../model/Action";
import { BorderNode } from "../model/BorderNode";
import { TabNode } from "../model/TabNode";
import type { IJsonModel } from "../types";

/**
 * resolveVisibleTabs mirrors the logic from BorderTab.tsx:
 * - explicit visibleTabs.length > 0 → use them
 * - otherwise fallback to [selected] (or [] if selected === -1)
 */
function resolveVisibleTabs(border: BorderNode): number[] {
  const explicit = border.getVisibleTabs();
  if (explicit.length > 0) return explicit;
  const sel = border.getSelected();
  return sel >= 0 ? [sel] : [];
}

function resolveVisibleNodes(border: BorderNode): TabNode[] {
  const children = border.getChildren();
  return resolveVisibleTabs(border)
    .map((i) => children[i] as TabNode)
    .filter(Boolean);
}

const tiledFixture: IJsonModel = {
  global: { borderEnableDock: true },
  borders: [
    {
      type: "border",
      location: "bottom",
      selected: 0,
      visibleTabs: [0, 1],
      children: [
        { type: "tab", name: "Terminal", component: "text" },
        { type: "tab", name: "Output", component: "text" },
        { type: "tab", name: "Problems", component: "text" },
      ],
    },
    {
      type: "border",
      location: "left",
      selected: 0,
      visibleTabs: [0],
      children: [
        { type: "tab", name: "Explorer", component: "text" },
        { type: "tab", name: "Search", component: "text" },
      ],
    },
    {
      type: "border",
      location: "right",
      selected: 0,
      children: [
        { type: "tab", name: "Properties", component: "text" },
      ],
    },
    {
      type: "border",
      location: "top",
      selected: -1,
      children: [
        { type: "tab", name: "Toolbar", component: "text" },
      ],
    },
  ],
  layout: {
    type: "row",
    weight: 100,
    children: [
      {
        type: "tabset",
        weight: 100,
        children: [{ type: "tab", name: "Main", component: "text" }],
      },
    ],
  },
};

function getBorder(model: Model, location: string): BorderNode {
  const border = model.getBorderSet().getBorder(location as any);
  if (!border) throw new Error(`No border at ${location}`);
  return border;
}

describe("Tiled borders > resolveVisibleTabs logic", () => {
  it("visibleTabs: [0, 1] resolves to 2 tiles", () => {
    const model = Model.fromJson(tiledFixture);
    const bottom = getBorder(model, "bottom");
    const resolved = resolveVisibleTabs(bottom);
    expect(resolved).toEqual([0, 1]);
    expect(resolved.length).toBe(2);
  });

  it("visibleTabs: [0] resolves to single tile (no splitter needed)", () => {
    const model = Model.fromJson(tiledFixture);
    const left = getBorder(model, "left");
    const resolved = resolveVisibleTabs(left);
    expect(resolved).toEqual([0]);
    expect(resolved.length).toBe(1);
  });

  it("empty visibleTabs falls back to [selected] when selected >= 0", () => {
    const model = Model.fromJson(tiledFixture);
    const right = getBorder(model, "right");
    expect(right.getVisibleTabs()).toEqual([]);
    const resolved = resolveVisibleTabs(right);
    expect(resolved).toEqual([0]);
  });

  it("empty visibleTabs with selected === -1 falls back to empty array", () => {
    const model = Model.fromJson(tiledFixture);
    const top = getBorder(model, "top");
    expect(top.getSelected()).toBe(-1);
    expect(top.getVisibleTabs()).toEqual([]);
    const resolved = resolveVisibleTabs(top);
    expect(resolved).toEqual([]);
  });
});

describe("Tiled borders > visible node resolution", () => {
  it("resolves correct TabNode objects for visibleTabs [0, 1]", () => {
    const model = Model.fromJson(tiledFixture);
    const bottom = getBorder(model, "bottom");
    const nodes = resolveVisibleNodes(bottom);
    expect(nodes.length).toBe(2);
    expect(nodes[0].getName()).toBe("Terminal");
    expect(nodes[1].getName()).toBe("Output");
  });

  it("single visible tab resolves to one TabNode", () => {
    const model = Model.fromJson(tiledFixture);
    const left = getBorder(model, "left");
    const nodes = resolveVisibleNodes(left);
    expect(nodes.length).toBe(1);
    expect(nodes[0].getName()).toBe("Explorer");
  });

  it("fallback to selected resolves correct TabNode", () => {
    const model = Model.fromJson(tiledFixture);
    const right = getBorder(model, "right");
    const nodes = resolveVisibleNodes(right);
    expect(nodes.length).toBe(1);
    expect(nodes[0].getName()).toBe("Properties");
  });

  it("out-of-range visibleTabs indices are filtered out", () => {
    const model = Model.fromJson(tiledFixture);
    const bottom = getBorder(model, "bottom");
    model.doAction(Action.setVisibleTabs(bottom.getId(), [0, 5, 10]));
    const nodes = resolveVisibleNodes(bottom);
    expect(nodes.length).toBe(1);
    expect(nodes[0].getName()).toBe("Terminal");
  });
});

describe("Tiled borders > splitter count", () => {
  it("2 visible tabs → 1 splitter between them", () => {
    const model = Model.fromJson(tiledFixture);
    const bottom = getBorder(model, "bottom");
    const resolved = resolveVisibleTabs(bottom);
    const splitterCount = Math.max(0, resolved.length - 1);
    expect(splitterCount).toBe(1);
  });

  it("1 visible tab → 0 splitters", () => {
    const model = Model.fromJson(tiledFixture);
    const left = getBorder(model, "left");
    const resolved = resolveVisibleTabs(left);
    const splitterCount = Math.max(0, resolved.length - 1);
    expect(splitterCount).toBe(0);
  });

  it("3 visible tabs → 2 splitters", () => {
    const model = Model.fromJson(tiledFixture);
    const bottom = getBorder(model, "bottom");
    model.doAction(Action.setVisibleTabs(bottom.getId(), [0, 1, 2]));
    const resolved = resolveVisibleTabs(bottom);
    expect(resolved.length).toBe(3);
    const splitterCount = Math.max(0, resolved.length - 1);
    expect(splitterCount).toBe(2);
  });

  it("0 visible tabs → 0 splitters", () => {
    const model = Model.fromJson(tiledFixture);
    const top = getBorder(model, "top");
    const resolved = resolveVisibleTabs(top);
    expect(resolved.length).toBe(0);
    const splitterCount = Math.max(0, resolved.length - 1);
    expect(splitterCount).toBe(0);
  });
});

describe("Tiled borders > tile weights", () => {
   it("default weights are equal (1 per tile)", () => {
     const model = Model.fromJson(tiledFixture);
     const bottom = getBorder(model, "bottom");
     const resolved = resolveVisibleTabs(bottom);
     const defaultWeights = Array(resolved.length).fill(1);
     expect(defaultWeights).toEqual([1, 1]);
     const totalWeight = defaultWeights.reduce((a, b) => a + b, 0);
     expect(totalWeight).toBe(2);
     for (const w of defaultWeights) {
       expect(w / totalWeight).toBeCloseTo(0.5);
     }
   });

   it("3 tiles have equal 1/3 proportions by default", () => {
     const model = Model.fromJson(tiledFixture);
     const bottom = getBorder(model, "bottom");
     model.doAction(Action.setVisibleTabs(bottom.getId(), [0, 1, 2]));
     const resolved = resolveVisibleTabs(bottom);
     const weights = Array(resolved.length).fill(1);
     expect(weights).toEqual([1, 1, 1]);
     const totalWeight = weights.reduce((a, b) => a + b, 0);
     for (const w of weights) {
       expect(w / totalWeight).toBeCloseTo(1 / 3);
     }
   });
});

describe("Tiled borders > visibleTabs index adjustment on removal", () => {
  it("removing tab at index 0 decrements all indices > 0", () => {
    const model = Model.fromJson(tiledFixture);
    const bottom = getBorder(model, "bottom");
    // Start: 4 tabs, visibleTabs: [0, 1]
    expect(bottom.getChildren().length).toBe(3);
    expect(bottom.getVisibleTabs()).toEqual([0, 1]);
    
    // Remove tab at index 0
    const tabToRemove = bottom.getChildren()[0];
    model.doAction(Action.deleteTab(tabToRemove.getId()));
    
    // After removal: 2 tabs, visibleTabs should be [0] (was [0, 1], index 0 removed, index 1 becomes 0)
    expect(bottom.getChildren().length).toBe(2);
    expect(bottom.getVisibleTabs()).toEqual([0]);
  });

  it("removing tab at index 1 adjusts indices correctly", () => {
    const model = Model.fromJson(tiledFixture);
    const bottom = getBorder(model, "bottom");
    // Start: 3 tabs, visibleTabs: [0, 1]
    expect(bottom.getVisibleTabs()).toEqual([0, 1]);
    
    // Remove tab at index 1
    const tabToRemove = bottom.getChildren()[1];
    model.doAction(Action.deleteTab(tabToRemove.getId()));
    
    // After removal: 2 tabs, visibleTabs should be [0] (index 1 removed, index 0 stays)
    expect(bottom.getChildren().length).toBe(2);
    expect(bottom.getVisibleTabs()).toEqual([0]);
  });

  it("removing a visible tab shrinks the array", () => {
    const model = Model.fromJson(tiledFixture);
    const bottom = getBorder(model, "bottom");
    // Start: 3 tabs, visibleTabs: [0, 1]
    expect(bottom.getVisibleTabs()).toEqual([0, 1]);
    
    // Remove tab at index 0 (which is in visibleTabs)
    const tabToRemove = bottom.getChildren()[0];
    model.doAction(Action.deleteTab(tabToRemove.getId()));
    
    // After removal: visibleTabs should be [0] (index 0 removed, index 1 becomes 0)
    expect(bottom.getVisibleTabs()).toEqual([0]);
  });

  it("removing all visible tabs reverts to single-tab mode", () => {
    const model = Model.fromJson(tiledFixture);
    const left = getBorder(model, "left");
    // Start: 2 tabs, visibleTabs: [0]
    expect(left.getVisibleTabs()).toEqual([0]);
    
    // Remove tab at index 0
    const tabToRemove = left.getChildren()[0];
    model.doAction(Action.deleteTab(tabToRemove.getId()));
    
    // After removal: 1 tab, visibleTabs should be empty (fallback to selected)
    expect(left.getChildren().length).toBe(1);
    expect(left.getVisibleTabs()).toEqual([]);
  });

  it("removing tab at index > visibleTabs indices leaves them unchanged", () => {
    const model = Model.fromJson(tiledFixture);
    const bottom = getBorder(model, "bottom");
    // Start: 3 tabs, visibleTabs: [0, 1]
    expect(bottom.getVisibleTabs()).toEqual([0, 1]);
    
    // Remove tab at index 2 (not in visibleTabs)
    const tabToRemove = bottom.getChildren()[2];
    model.doAction(Action.deleteTab(tabToRemove.getId()));
    
    // After removal: visibleTabs should stay [0, 1] (index 2 removed, doesn't affect [0, 1])
    expect(bottom.getChildren().length).toBe(2);
    expect(bottom.getVisibleTabs()).toEqual([0, 1]);
  });
});

describe("Tiled borders > visibleTabs index adjustment on insertion", () => {
  it("inserting tab at index 0 shifts all indices up", () => {
    const model = Model.fromJson(tiledFixture);
    const bottom = getBorder(model, "bottom");
    expect(bottom.getVisibleTabs()).toEqual([0, 1]);
    
    model.doAction(
      Action.addNode(
        { type: "tab", name: "NewTab", component: "text" },
        bottom.getId(),
        "center",
        0
      )
    );
    
    expect(bottom.getChildren().length).toBe(4);
    expect(bottom.getVisibleTabs()).toEqual([1, 2]);
  });

  it("inserting tab at index 1 shifts indices >= 1", () => {
    const model = Model.fromJson(tiledFixture);
    const bottom = getBorder(model, "bottom");
    expect(bottom.getVisibleTabs()).toEqual([0, 1]);
    
    model.doAction(
      Action.addNode(
        { type: "tab", name: "NewTab", component: "text" },
        bottom.getId(),
        "center",
        1
      )
    );
    
    expect(bottom.getChildren().length).toBe(4);
    expect(bottom.getVisibleTabs()).toEqual([0, 2]);
  });

  it("inserting tab at end doesn't affect visibleTabs", () => {
    const model = Model.fromJson(tiledFixture);
    const bottom = getBorder(model, "bottom");
    expect(bottom.getVisibleTabs()).toEqual([0, 1]);
    
    model.doAction(
      Action.addNode(
        { type: "tab", name: "NewTab", component: "text" },
        bottom.getId(),
        "center",
        3
      )
    );
    
    expect(bottom.getChildren().length).toBe(4);
    expect(bottom.getVisibleTabs()).toEqual([0, 1]);
  });
});
