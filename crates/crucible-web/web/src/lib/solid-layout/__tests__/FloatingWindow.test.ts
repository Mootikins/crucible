import { describe, it, expect } from "vitest";
import { CLASSES } from "../../flexlayout/core/Types";

import {
  computeResizedRect,
  clampToViewport,
  buildResizeHandleClass,
  buildPanelClass,
  buildTitleBarClass,
  buildTitleClass,
  buildButtonsClass,
  buildContentClass,
  buildPanelStyle,
  RESIZE_EDGES,
  MIN_WIDTH,
  MIN_HEIGHT,
  MIN_VIEWPORT_VISIBILITY,
  type FloatingWindowRect,
} from "../components/FloatingWindow";

import {
  getZIndex,
  bringToFront,
  syncZOrder,
  Z_INDEX_BASE,
} from "../components/OverlayContainer";

describe("computeResizedRect", () => {
  const start = { x: 100, y: 100, width: 300, height: 200 };

  it("resizes east edge by increasing width", () => {
    const result = computeResizedRect(50, 0, start, "e");
    expect(result).toEqual({ x: 100, y: 100, width: 350, height: 200 });
  });

  it("resizes south edge by increasing height", () => {
    const result = computeResizedRect(0, 40, start, "s");
    expect(result).toEqual({ x: 100, y: 100, width: 300, height: 240 });
  });

  it("resizes west edge by decreasing x and increasing width", () => {
    const result = computeResizedRect(-30, 0, start, "w");
    expect(result).toEqual({ x: 70, y: 100, width: 330, height: 200 });
  });

  it("resizes north edge by decreasing y and increasing height", () => {
    const result = computeResizedRect(0, -25, start, "n");
    expect(result).toEqual({ x: 100, y: 75, width: 300, height: 225 });
  });

  it("resizes southeast corner (both width and height)", () => {
    const result = computeResizedRect(60, 40, start, "se");
    expect(result).toEqual({ x: 100, y: 100, width: 360, height: 240 });
  });

  it("resizes northwest corner (both axes, position shifts)", () => {
    const result = computeResizedRect(-20, -30, start, "nw");
    expect(result).toEqual({ x: 80, y: 70, width: 320, height: 230 });
  });

  it("resizes northeast corner", () => {
    const result = computeResizedRect(50, -20, start, "ne");
    expect(result).toEqual({ x: 100, y: 80, width: 350, height: 220 });
  });

  it("resizes southwest corner", () => {
    const result = computeResizedRect(-40, 30, start, "sw");
    expect(result).toEqual({ x: 60, y: 100, width: 340, height: 230 });
  });

  it("clamps width to MIN_WIDTH when east edge shrinks too much", () => {
    const result = computeResizedRect(-200, 0, start, "e");
    expect(result.width).toBe(MIN_WIDTH);
  });

  it("clamps height to MIN_HEIGHT when south edge shrinks too much", () => {
    const result = computeResizedRect(0, -200, start, "s");
    expect(result.height).toBe(MIN_HEIGHT);
  });

  it("clamps west edge and fixes right edge position", () => {
    const result = computeResizedRect(200, 0, start, "w");
    expect(result.width).toBe(MIN_WIDTH);
    expect(result.x).toBe(start.x + (start.width - MIN_WIDTH));
  });

  it("clamps north edge and fixes bottom edge position", () => {
    const result = computeResizedRect(0, 200, start, "n");
    expect(result.height).toBe(MIN_HEIGHT);
    expect(result.y).toBe(start.y + (start.height - MIN_HEIGHT));
  });
});

describe("clampToViewport", () => {
  const container = { width: 1000, height: 800 };

  it("leaves rect unchanged when fully inside container", () => {
    const rect: FloatingWindowRect = { x: 100, y: 100, width: 300, height: 200 };
    expect(clampToViewport(rect, container.width, container.height)).toEqual(rect);
  });

  it("clamps rect that goes too far right", () => {
    const rect: FloatingWindowRect = { x: 950, y: 100, width: 300, height: 200 };
    const result = clampToViewport(rect, container.width, container.height);
    expect(result.x).toBe(container.width - MIN_VIEWPORT_VISIBILITY);
  });

  it("clamps rect that goes too far left", () => {
    const rect: FloatingWindowRect = { x: -300, y: 100, width: 200, height: 200 };
    const result = clampToViewport(rect, container.width, container.height);
    expect(result.x).toBe(-(rect.width - MIN_VIEWPORT_VISIBILITY));
  });

  it("clamps rect that goes too far down", () => {
    const rect: FloatingWindowRect = { x: 100, y: 750, width: 200, height: 200 };
    const result = clampToViewport(rect, container.width, container.height);
    expect(result.y).toBe(container.height - MIN_VIEWPORT_VISIBILITY);
  });

  it("clamps rect that goes too far up", () => {
    const rect: FloatingWindowRect = { x: 100, y: -250, width: 200, height: 200 };
    const result = clampToViewport(rect, container.width, container.height);
    expect(result.y).toBe(-(rect.height - MIN_VIEWPORT_VISIBILITY));
  });

  it("preserves width and height (never modifies size)", () => {
    const rect: FloatingWindowRect = { x: -9999, y: -9999, width: 400, height: 300 };
    const result = clampToViewport(rect, container.width, container.height);
    expect(result.width).toBe(400);
    expect(result.height).toBe(300);
  });

  it("accepts custom minVisible parameter", () => {
    const rect: FloatingWindowRect = { x: 980, y: 100, width: 200, height: 200 };
    const result = clampToViewport(rect, container.width, container.height, 50);
    expect(result.x).toBe(container.width - 50);
  });
});

describe("buildPanelClass", () => {
  it("returns default class without mapper", () => {
    expect(buildPanelClass()).toBe(CLASSES.FLEXLAYOUT__FLOATING_PANEL);
  });

  it("applies mapper to class", () => {
    const mapper = (cls: string) => `custom-${cls}`;
    expect(buildPanelClass(mapper)).toBe(`custom-${CLASSES.FLEXLAYOUT__FLOATING_PANEL}`);
  });
});

describe("buildTitleBarClass", () => {
  it("returns default class without mapper", () => {
    expect(buildTitleBarClass()).toBe(CLASSES.FLEXLAYOUT__FLOATING_PANEL_TITLEBAR);
  });

  it("applies mapper", () => {
    const mapper = (cls: string) => cls.toUpperCase();
    expect(buildTitleBarClass(mapper)).toBe(CLASSES.FLEXLAYOUT__FLOATING_PANEL_TITLEBAR.toUpperCase());
  });
});

describe("buildTitleClass", () => {
  it("returns default class", () => {
    expect(buildTitleClass()).toBe(CLASSES.FLEXLAYOUT__FLOATING_PANEL_TITLEBAR_TITLE);
  });
});

describe("buildButtonsClass", () => {
  it("returns default class", () => {
    expect(buildButtonsClass()).toBe(CLASSES.FLEXLAYOUT__FLOATING_PANEL_TITLEBAR_BUTTONS);
  });
});

describe("buildContentClass", () => {
  it("returns default class", () => {
    expect(buildContentClass()).toBe(CLASSES.FLEXLAYOUT__FLOATING_PANEL_CONTENT);
  });
});

describe("buildResizeHandleClass", () => {
  it("returns single class for edges without extraClass", () => {
    const def = { className: "resize-n" };
    expect(buildResizeHandleClass(def)).toBe("resize-n");
  });

  it("appends extraClass when present", () => {
    const def = { className: "resize-se", extraClass: "handle" };
    expect(buildResizeHandleClass(def)).toBe("resize-se handle");
  });

  it("applies mapper to both classes", () => {
    const def = { className: "resize-se", extraClass: "handle" };
    const mapper = (cls: string) => `pfx-${cls}`;
    expect(buildResizeHandleClass(def, mapper)).toBe("pfx-resize-se pfx-handle");
  });
});

describe("buildPanelStyle", () => {
  it("sets position, dimensions, and z-index", () => {
    const rect: FloatingWindowRect = { x: 10, y: 20, width: 300, height: 200 };
    const style = buildPanelStyle(rect, 1005);
    expect(style.position).toBe("absolute");
    expect(style.left).toBe("10px");
    expect(style.top).toBe("20px");
    expect(style.width).toBe("300px");
    expect(style.height).toBe("200px");
    expect(style["z-index"]).toBe("1005");
  });
});

describe("RESIZE_EDGES", () => {
  it("contains exactly 8 edge definitions", () => {
    expect(RESIZE_EDGES).toHaveLength(8);
  });

  it("covers all 8 directions", () => {
    const edges = RESIZE_EDGES.map((d) => d.edge);
    expect(edges).toEqual(["n", "s", "e", "w", "nw", "ne", "sw", "se"]);
  });

  it("has extraClass only on se handle", () => {
    const withExtra = RESIZE_EDGES.filter((d) => d.extraClass);
    expect(withExtra).toHaveLength(1);
    expect(withExtra[0].edge).toBe("se");
  });
});

describe("getZIndex", () => {
  it("returns base + position for known window", () => {
    expect(getZIndex("a", ["a", "b", "c"])).toBe(Z_INDEX_BASE + 0);
    expect(getZIndex("c", ["a", "b", "c"])).toBe(Z_INDEX_BASE + 2);
  });

  it("returns base for unknown window", () => {
    expect(getZIndex("unknown", ["a", "b"])).toBe(Z_INDEX_BASE);
  });
});

describe("bringToFront", () => {
  it("moves window to end of order", () => {
    expect(bringToFront("a", ["a", "b", "c"])).toEqual(["b", "c", "a"]);
  });

  it("keeps order when window is already last", () => {
    expect(bringToFront("c", ["a", "b", "c"])).toEqual(["a", "b", "c"]);
  });

  it("handles single-item order", () => {
    expect(bringToFront("a", ["a"])).toEqual(["a"]);
  });

  it("adds window if not present", () => {
    expect(bringToFront("d", ["a", "b"])).toEqual(["a", "b", "d"]);
  });
});

describe("syncZOrder", () => {
  it("preserves existing order for known windows", () => {
    expect(syncZOrder(["a", "b", "c"], ["c", "a", "b"])).toEqual(["c", "a", "b"]);
  });

  it("removes closed windows from order", () => {
    expect(syncZOrder(["a", "c"], ["a", "b", "c"])).toEqual(["a", "c"]);
  });

  it("appends new windows at end", () => {
    expect(syncZOrder(["a", "b", "d"], ["a", "b"])).toEqual(["a", "b", "d"]);
  });

  it("handles empty current order", () => {
    expect(syncZOrder(["x", "y"], [])).toEqual(["x", "y"]);
  });

  it("handles empty window list", () => {
    expect(syncZOrder([], ["a", "b"])).toEqual([]);
  });
});
