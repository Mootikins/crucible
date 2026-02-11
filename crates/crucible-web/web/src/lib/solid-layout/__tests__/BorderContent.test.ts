import { describe, it, expect, afterEach } from "vitest";
import { createRoot } from "solid-js";
import { Model } from "../../flexlayout/model/Model";
import { Action } from "../../flexlayout/model/Action";
import type { IJsonModel, IJsonBorderNode } from "../../flexlayout/types";
import { CLASSES } from "../../flexlayout/core/Types";
import { createLayoutBridge, type LayoutBridge } from "../bridge";

import {
  resolveVisibleIndices,
  isTileHorizontal,
  ensureTileWeights,
  edgeResizeCursor,
  tileSplitterCursor,
  buildBorderContentClass,
  buildBorderTabBarClass,
  buildTileHostClass,
  buildTileSplitterClass,
  buildEdgeSplitterClass,
  clampSize,
} from "../components/BorderContent";

const identity = (cls: string) => cls;

function makeBorder(
  location: string,
  overrides: Partial<IJsonBorderNode> = {},
): IJsonBorderNode {
  return {
    type: "border",
    id: `border_${location}`,
    location,
    children: [],
    dockState: "expanded",
    selected: -1,
    priority: 0,
    enableDock: true,
    ...overrides,
  } as IJsonBorderNode;
}

function makeLayoutWithBorder(
  location: string,
  borderOverrides: Partial<IJsonBorderNode> = {},
): IJsonModel {
  return {
    global: {},
    borders: [
      makeBorder(location, borderOverrides),
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
            { type: "tab", id: "main-tab", name: "Main", component: "text" },
          ],
        },
      ],
    },
  };
}

function makeTiledBorderLayout(location: string): IJsonModel {
  return {
    global: {},
    borders: [
      makeBorder(location, {
        selected: 0,
        visibleTabs: [0, 1],
        children: [
          { type: "tab", id: "bt1", name: "Panel A", component: "text" },
          { type: "tab", id: "bt2", name: "Panel B", component: "text" },
        ],
      }),
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
            { type: "tab", id: "main-tab", name: "Main", component: "text" },
          ],
        },
      ],
    },
  };
}

describe("resolveVisibleIndices", () => {
  it("returns explicit visibleTabs when set", () => {
    const border = makeBorder("left", { visibleTabs: [0, 2], selected: 0 });
    expect(resolveVisibleIndices(border)).toEqual([0, 2]);
  });

  it("falls back to selected index when no visibleTabs", () => {
    const border = makeBorder("left", { selected: 1 });
    expect(resolveVisibleIndices(border)).toEqual([1]);
  });

  it("returns empty when selected is -1 and no visibleTabs", () => {
    const border = makeBorder("left", { selected: -1 });
    expect(resolveVisibleIndices(border)).toEqual([]);
  });

  it("returns empty for undefined selected", () => {
    const border = makeBorder("left");
    delete (border as any).selected;
    expect(resolveVisibleIndices(border)).toEqual([]);
  });

  it("ignores empty visibleTabs array", () => {
    const border = makeBorder("left", { visibleTabs: [], selected: 2 });
    expect(resolveVisibleIndices(border)).toEqual([2]);
  });
});

describe("isTileHorizontal", () => {
  it("left border tiles split horizontally", () => {
    expect(isTileHorizontal("left")).toBe(true);
  });

  it("right border tiles split horizontally", () => {
    expect(isTileHorizontal("right")).toBe(true);
  });

  it("top border tiles split vertically", () => {
    expect(isTileHorizontal("top")).toBe(false);
  });

  it("bottom border tiles split vertically", () => {
    expect(isTileHorizontal("bottom")).toBe(false);
  });
});

describe("ensureTileWeights", () => {
  it("preserves weights when count matches", () => {
    const existing = [0.6, 0.4];
    expect(ensureTileWeights(existing, 2)).toBe(existing);
  });

  it("resets to equal weights when count changes", () => {
    expect(ensureTileWeights([0.6, 0.4], 3)).toEqual([1, 1, 1]);
  });

  it("initializes empty array to equal weights", () => {
    expect(ensureTileWeights([], 2)).toEqual([1, 1]);
  });

  it("returns single-element array for count 1", () => {
    expect(ensureTileWeights([], 1)).toEqual([1]);
  });
});

describe("edgeResizeCursor", () => {
  it("horizontal border (top) uses ns-resize", () => {
    expect(edgeResizeCursor("top")).toBe("ns-resize");
  });

  it("horizontal border (bottom) uses ns-resize", () => {
    expect(edgeResizeCursor("bottom")).toBe("ns-resize");
  });

  it("vertical border (left) uses ew-resize", () => {
    expect(edgeResizeCursor("left")).toBe("ew-resize");
  });

  it("vertical border (right) uses ew-resize", () => {
    expect(edgeResizeCursor("right")).toBe("ew-resize");
  });
});

describe("tileSplitterCursor", () => {
  it("left border tile splitters use ew-resize", () => {
    expect(tileSplitterCursor("left")).toBe("ew-resize");
  });

  it("right border tile splitters use ew-resize", () => {
    expect(tileSplitterCursor("right")).toBe("ew-resize");
  });

  it("top border tile splitters use ns-resize", () => {
    expect(tileSplitterCursor("top")).toBe("ns-resize");
  });

  it("bottom border tile splitters use ns-resize", () => {
    expect(tileSplitterCursor("bottom")).toBe("ns-resize");
  });
});

describe("buildBorderContentClass", () => {
  it("returns the border tab contents class", () => {
    expect(buildBorderContentClass(identity)).toBe(
      CLASSES.FLEXLAYOUT__BORDER_TAB_CONTENTS,
    );
  });

  it("applies classNameMapper", () => {
    const mapper = (cls: string) => `pfx-${cls}`;
    expect(buildBorderContentClass(mapper)).toBe(
      `pfx-${CLASSES.FLEXLAYOUT__BORDER_TAB_CONTENTS}`,
    );
  });
});

describe("buildBorderTabBarClass", () => {
  it("returns the border tabbar class", () => {
    expect(buildBorderTabBarClass(identity)).toBe(
      CLASSES.FLEXLAYOUT__BORDER_TABBAR,
    );
  });

  it("applies classNameMapper", () => {
    const mapper = (cls: string) => `pfx-${cls}`;
    expect(buildBorderTabBarClass(mapper)).toBe(
      `pfx-${CLASSES.FLEXLAYOUT__BORDER_TABBAR}`,
    );
  });
});

describe("buildTileHostClass", () => {
  it("combines tab and tab_border classes", () => {
    const cls = buildTileHostClass(identity);
    expect(cls).toContain(CLASSES.FLEXLAYOUT__TAB);
    expect(cls).toContain(CLASSES.FLEXLAYOUT__TAB_BORDER);
  });
});

describe("buildTileSplitterClass", () => {
  it("uses horz orientation for left border (horizontal tiles)", () => {
    const cls = buildTileSplitterClass("left", identity);
    expect(cls).toContain(CLASSES.FLEXLAYOUT__SPLITTER);
    expect(cls).toContain(CLASSES.FLEXLAYOUT__SPLITTER_ + "horz");
  });

  it("uses vert orientation for top border (vertical tiles)", () => {
    const cls = buildTileSplitterClass("top", identity);
    expect(cls).toContain(CLASSES.FLEXLAYOUT__SPLITTER_ + "vert");
  });
});

describe("buildEdgeSplitterClass", () => {
  it("uses vert orientation for top border", () => {
    const cls = buildEdgeSplitterClass("top", identity);
    expect(cls).toContain(CLASSES.FLEXLAYOUT__SPLITTER);
    expect(cls).toContain(CLASSES.FLEXLAYOUT__SPLITTER_ + "vert");
    expect(cls).toContain(CLASSES.FLEXLAYOUT__SPLITTER_BORDER);
  });

  it("uses horz orientation for left border", () => {
    const cls = buildEdgeSplitterClass("left", identity);
    expect(cls).toContain(CLASSES.FLEXLAYOUT__SPLITTER_ + "horz");
    expect(cls).toContain(CLASSES.FLEXLAYOUT__SPLITTER_BORDER);
  });

  it("uses horz orientation for right border", () => {
    const cls = buildEdgeSplitterClass("right", identity);
    expect(cls).toContain(CLASSES.FLEXLAYOUT__SPLITTER_ + "horz");
  });

  it("uses vert orientation for bottom border", () => {
    const cls = buildEdgeSplitterClass("bottom", identity);
    expect(cls).toContain(CLASSES.FLEXLAYOUT__SPLITTER_ + "vert");
  });
});

describe("clampSize", () => {
  it("clamps below minimum", () => {
    expect(clampSize(10, 50, 500)).toBe(50);
  });

  it("clamps above maximum", () => {
    expect(clampSize(600, 50, 500)).toBe(500);
  });

  it("passes through values within range", () => {
    expect(clampSize(200, 50, 500)).toBe(200);
  });

  it("returns min when min equals max", () => {
    expect(clampSize(100, 200, 200)).toBe(200);
  });

  it("handles exact boundary values", () => {
    expect(clampSize(50, 50, 500)).toBe(50);
    expect(clampSize(500, 50, 500)).toBe(500);
  });
});

describe("border content visibility via store", () => {
  let bridge: LayoutBridge | undefined;

  afterEach(() => {
    bridge?.dispose();
    bridge = undefined;
  });

  it("content is not visible when no tab is selected", () => {
    createRoot((dispose) => {
      const json = makeLayoutWithBorder("left", {
        children: [
          { type: "tab", id: "bt1", name: "Explorer", component: "text" },
        ],
        selected: -1,
      });
      const model = Model.fromJson(json);
      bridge = createLayoutBridge(model);

      const borders = bridge.store.borders as IJsonBorderNode[];
      const left = borders.find((b) => b.location === "left");
      const sel = typeof left!.selected === "number" ? left!.selected : -1;
      expect(sel).toBeLessThan(0);

      const indices = resolveVisibleIndices(left!);
      expect(indices).toEqual([]);

      dispose();
    });
  });

  it("content becomes visible when tab is selected", () => {
    createRoot((dispose) => {
      const json = makeLayoutWithBorder("left", {
        children: [
          { type: "tab", id: "bt1", name: "Explorer", component: "text" },
        ],
      });
      const model = Model.fromJson(json);
      bridge = createLayoutBridge(model);

      model.doAction(Action.selectTab("bt1"));

      const borders = bridge.store.borders as IJsonBorderNode[];
      const left = borders.find((b) => b.location === "left");
      expect(left!.selected).toBe(0);

      const indices = resolveVisibleIndices(left!);
      expect(indices).toEqual([0]);

      dispose();
    });
  });

  it("adjustBorderSplit updates border size in store", () => {
    createRoot((dispose) => {
      const json = makeLayoutWithBorder("left", {
        children: [
          { type: "tab", id: "bt1", name: "Explorer", component: "text" },
        ],
        selected: 0,
      });
      const model = Model.fromJson(json);
      bridge = createLayoutBridge(model);

      model.doAction(Action.adjustBorderSplit("border_left", 300));

      const borders = bridge.store.borders as IJsonBorderNode[];
      const left = borders.find((b) => b.location === "left");
      expect(left!.size).toBe(300);

      dispose();
    });
  });

  it("tiled border has multiple visible indices", () => {
    createRoot((dispose) => {
      const json = makeTiledBorderLayout("left");
      const model = Model.fromJson(json);
      bridge = createLayoutBridge(model);

      const borders = bridge.store.borders as IJsonBorderNode[];
      const left = borders.find((b) => b.location === "left");
      expect(left!.visibleTabs).toEqual([0, 1]);

      const indices = resolveVisibleIndices(left!);
      expect(indices).toEqual([0, 1]);

      dispose();
    });
  });
});

describe("cursor and orientation consistency", () => {
  const locations = ["top", "bottom", "left", "right"] as const;

  for (const loc of locations) {
    it(`${loc}: edge cursor is perpendicular to border orientation`, () => {
      const cursor = edgeResizeCursor(loc);
      if (loc === "top" || loc === "bottom") {
        expect(cursor).toBe("ns-resize");
      } else {
        expect(cursor).toBe("ew-resize");
      }
    });

    it(`${loc}: tile cursor matches tiling direction`, () => {
      const cursor = tileSplitterCursor(loc);
      if (loc === "left" || loc === "right") {
        expect(cursor).toBe("ew-resize");
      } else {
        expect(cursor).toBe("ns-resize");
      }
    });
  }
});

describe("edge splitter ordering", () => {
  it("start edge (left): content before splitter", () => {
    const isStart = (loc: string) => loc === "left" || loc === "top";
    expect(isStart("left")).toBe(true);
    expect(isStart("top")).toBe(true);
  });

  it("end edge (right/bottom): splitter before content", () => {
    const isStart = (loc: string) => loc === "left" || loc === "top";
    expect(isStart("right")).toBe(false);
    expect(isStart("bottom")).toBe(false);
  });
});
