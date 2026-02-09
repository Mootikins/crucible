import { describe, it, expect } from "vitest";
import { Model } from "../model/Model";
import type { IJsonModel } from "../types";

/** Fixture with explicit priority values */
const priorityFixture: IJsonModel = {
  global: {
    borderPriority: 0, // default model-level priority
  },
  borders: [
    {
      type: "border",
      location: "top",
      selected: 0,
      priority: 0,
      children: [{ type: "tab", name: "Top Tab", component: "text" }],
    },
    {
      type: "border",
      location: "bottom",
      selected: 0,
      priority: 0,
      children: [{ type: "tab", name: "Bottom Tab", component: "text" }],
    },
    {
      type: "border",
      location: "left",
      selected: 0,
      priority: 1,
      children: [{ type: "tab", name: "Left Tab", component: "text" }],
    },
    {
      type: "border",
      location: "right",
      selected: 0,
      priority: 1,
      children: [{ type: "tab", name: "Right Tab", component: "text" }],
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

/** Fixture with model-level borderPriority inheritance */
const inheritanceFixture: IJsonModel = {
  global: {
    borderPriority: 5, // all borders inherit this
  },
  borders: [
    {
      type: "border",
      location: "top",
      selected: 0,
      // no priority specified, should inherit 5
      children: [{ type: "tab", name: "Top Tab", component: "text" }],
    },
    {
      type: "border",
      location: "left",
      selected: 0,
      priority: 3, // override global 5
      children: [{ type: "tab", name: "Left Tab", component: "text" }],
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

/** Fixture for backward compat: minimized → hidden */
const legacyMinimizedFixture: IJsonModel = {
  global: {},
  borders: [
    {
      type: "border",
      location: "left",
      selected: 0,
      dockState: "minimized" as any, // old name - testing backward compat
      children: [{ type: "tab", name: "Explorer", component: "text" }],
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

describe("BorderNode priority attribute", () => {
  it("should read priority attribute from JSON", () => {
    const model = new Model(priorityFixture);
    const topBorder = model.getBorderSet().getBorders()[0];
    const leftBorder = model.getBorderSet().getBorders()[2];

    expect(topBorder.getPriority()).toBe(0);
    expect(leftBorder.getPriority()).toBe(1);
  });

  it("should inherit priority from model-level borderPriority", () => {
    const model = new Model(inheritanceFixture);
    const topBorder = model.getBorderSet().getBorders()[0];
    const leftBorder = model.getBorderSet().getBorders()[1];

    // top border has no explicit priority, should inherit 5
    expect(topBorder.getPriority()).toBe(5);
    // left border has explicit priority 3, overrides global 5
    expect(leftBorder.getPriority()).toBe(3);
  });

  it("should default priority to 0 when not specified and no model default", () => {
    const fixture: IJsonModel = {
      global: {},
      borders: [
        {
          type: "border",
          location: "top",
          selected: 0,
          // no priority, no global default
          children: [{ type: "tab", name: "Top", component: "text" }],
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
    const model = new Model(fixture);
    const topBorder = model.getBorderSet().getBorders()[0];
    expect(topBorder.getPriority()).toBe(0);
  });
});

describe("BorderSet.getBordersByPriority()", () => {
  it("should sort borders by priority descending", () => {
    const model = new Model(priorityFixture);
    const sorted = model.getBorderSet().getBordersByPriority();

    // Expected order: right(1), left(1), top(0), bottom(0)
    // Within same priority, tie-break by location order: [top, right, bottom, left]
    expect(sorted.length).toBe(4);
    expect(sorted[0].getLocation().getName()).toBe("right"); // priority 1
    expect(sorted[1].getLocation().getName()).toBe("left"); // priority 1
    expect(sorted[2].getLocation().getName()).toBe("top"); // priority 0
    expect(sorted[3].getLocation().getName()).toBe("bottom"); // priority 0
  });

  it("should break ties by location order [top, right, bottom, left]", () => {
    const fixture: IJsonModel = {
      global: {},
      borders: [
        {
          type: "border",
          location: "top",
          selected: 0,
          priority: 2,
          children: [{ type: "tab", name: "Top", component: "text" }],
        },
        {
          type: "border",
          location: "right",
          selected: 0,
          priority: 2,
          children: [{ type: "tab", name: "Right", component: "text" }],
        },
        {
          type: "border",
          location: "bottom",
          selected: 0,
          priority: 2,
          children: [{ type: "tab", name: "Bottom", component: "text" }],
        },
        {
          type: "border",
          location: "left",
          selected: 0,
          priority: 2,
          children: [{ type: "tab", name: "Left", component: "text" }],
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
    const model = new Model(fixture);
    const sorted = model.getBorderSet().getBordersByPriority();

    // All have same priority, so order by location: top, right, bottom, left
    expect(sorted[0].getLocation().getName()).toBe("top");
    expect(sorted[1].getLocation().getName()).toBe("right");
    expect(sorted[2].getLocation().getName()).toBe("bottom");
    expect(sorted[3].getLocation().getName()).toBe("left");
  });
});

describe("Dock state: hidden (renamed from minimized)", () => {
  it("should accept 'hidden' as a valid dock state", () => {
    const fixture: IJsonModel = {
      global: {},
      borders: [
        {
          type: "border",
          location: "left",
          selected: 0,
          dockState: "hidden",
          children: [{ type: "tab", name: "Explorer", component: "text" }],
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
    const model = new Model(fixture);
    const leftBorder = model.getBorderSet().getBorders()[0];
    expect(leftBorder.getDockState()).toBe("hidden");
  });

  it("should load 'minimized' from JSON and convert to 'hidden'", () => {
    const model = new Model(legacyMinimizedFixture);
    const leftBorder = model.getBorderSet().getBorders()[0];
    expect(leftBorder.getDockState()).toBe("hidden");
  });

  it("should serialize 'hidden' state back to JSON as 'hidden'", () => {
    const model = new Model(legacyMinimizedFixture);
    const json = model.toJson();
    const leftBorderJson = json.borders![0];
    expect(leftBorderJson.dockState).toBe("hidden");
  });

  it("should cycle through dock states: expanded → collapsed → hidden → expanded", () => {
    const fixture: IJsonModel = {
      global: { borderEnableDock: true },
      borders: [
        {
          type: "border",
          location: "left",
          selected: 0,
          dockState: "expanded",
          children: [{ type: "tab", name: "Explorer", component: "text" }],
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
    const model = new Model(fixture);
    const leftBorder = model.getBorderSet().getBorders()[0];

    expect(leftBorder.getDockState()).toBe("expanded");

    // Cycle to collapsed
    const action1 = { type: "SET_DOCK_STATE", data: { nodeId: leftBorder.getId(), state: "collapsed" } };
    model.doAction(action1);
    expect(leftBorder.getDockState()).toBe("collapsed");

    // Cycle to hidden
    const action2 = { type: "SET_DOCK_STATE", data: { nodeId: leftBorder.getId(), state: "hidden" } };
    model.doAction(action2);
    expect(leftBorder.getDockState()).toBe("hidden");

    // Cycle back to expanded
    const action3 = { type: "SET_DOCK_STATE", data: { nodeId: leftBorder.getId(), state: "expanded" } };
    model.doAction(action3);
    expect(leftBorder.getDockState()).toBe("expanded");
  });
});

describe("Serialization round-trip with priority and hidden", () => {
  it("should preserve priority and hidden state through JSON round-trip", () => {
    const model1 = new Model(priorityFixture);
    const json = model1.toJson();
    const model2 = new Model(json);

    const borders1 = model1.getBorderSet().getBorders();
    const borders2 = model2.getBorderSet().getBorders();

    for (let i = 0; i < borders1.length; i++) {
      expect(borders2[i].getPriority()).toBe(borders1[i].getPriority());
    }
  });

  it("should convert legacy minimized to hidden in round-trip", () => {
    const model1 = new Model(legacyMinimizedFixture);
    const json = model1.toJson();

    // JSON should now have "hidden", not "minimized"
    expect(json.borders![0].dockState).toBe("hidden");

    // Loading again should still work
    const model2 = new Model(json);
    const leftBorder = model2.getBorderSet().getBorders()[0];
    expect(leftBorder.getDockState()).toBe("hidden");
  });
});
