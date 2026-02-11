import { describe, it, expect } from "vitest";
import { CLASSES } from "../../flexlayout/core/Types";

import {
  clampIndex,
  buildPopupItemClass,
  buildPopupContainerClass,
  buildPopupMenuClass,
  buildPopupStyle,
  type PopupPosition,
} from "../components/Popup";

import {
  clampContextIndex,
  buildContextMenuItemClass,
  buildContextMenuStyle,
} from "../components/ContextMenu";

describe("clampIndex", () => {
  it("returns -1 when max is 0", () => {
    expect(clampIndex(0, 0)).toBe(-1);
  });

  it("wraps negative index to last item", () => {
    expect(clampIndex(-1, 5)).toBe(4);
  });

  it("wraps past-end index to first item", () => {
    expect(clampIndex(5, 5)).toBe(0);
  });

  it("returns index unchanged when within bounds", () => {
    expect(clampIndex(2, 5)).toBe(2);
  });

  it("handles single-item list", () => {
    expect(clampIndex(0, 1)).toBe(0);
    expect(clampIndex(1, 1)).toBe(0);
    expect(clampIndex(-1, 1)).toBe(0);
  });
});

describe("buildPopupItemClass", () => {
  it("returns base class when not selected", () => {
    expect(buildPopupItemClass(false)).toBe(CLASSES.FLEXLAYOUT__POPUP_MENU_ITEM);
  });

  it("appends selected modifier when selected", () => {
    const result = buildPopupItemClass(true);
    expect(result).toContain(CLASSES.FLEXLAYOUT__POPUP_MENU_ITEM);
    expect(result).toContain(CLASSES.FLEXLAYOUT__POPUP_MENU_ITEM__SELECTED);
  });

  it("applies mapper to both base and selected classes", () => {
    const mapper = (cls: string) => `custom-${cls}`;
    const result = buildPopupItemClass(true, mapper);
    expect(result).toContain(`custom-${CLASSES.FLEXLAYOUT__POPUP_MENU_ITEM}`);
    expect(result).toContain(`custom-${CLASSES.FLEXLAYOUT__POPUP_MENU_ITEM__SELECTED}`);
  });

  it("applies mapper to base class when not selected", () => {
    const mapper = (cls: string) => `prefix_${cls}`;
    expect(buildPopupItemClass(false, mapper)).toBe(
      `prefix_${CLASSES.FLEXLAYOUT__POPUP_MENU_ITEM}`,
    );
  });
});

describe("buildPopupContainerClass", () => {
  it("returns default container class without mapper", () => {
    expect(buildPopupContainerClass()).toBe(CLASSES.FLEXLAYOUT__POPUP_MENU_CONTAINER);
  });

  it("applies mapper to container class", () => {
    const mapper = (cls: string) => cls.toUpperCase();
    expect(buildPopupContainerClass(mapper)).toBe(
      CLASSES.FLEXLAYOUT__POPUP_MENU_CONTAINER.toUpperCase(),
    );
  });
});

describe("buildPopupMenuClass", () => {
  it("returns default menu class without mapper", () => {
    expect(buildPopupMenuClass()).toBe(CLASSES.FLEXLAYOUT__POPUP_MENU);
  });

  it("applies mapper to menu class", () => {
    const mapper = (cls: string) => `themed-${cls}`;
    expect(buildPopupMenuClass(mapper)).toBe(`themed-${CLASSES.FLEXLAYOUT__POPUP_MENU}`);
  });
});

describe("buildPopupStyle", () => {
  it("sets position absolute and z-index 1002", () => {
    const style = buildPopupStyle({});
    expect(style.position).toBe("absolute");
    expect(style["z-index"]).toBe("1002");
  });

  it("sets left and top when provided", () => {
    const style = buildPopupStyle({ left: "10px", top: "20px" });
    expect(style.left).toBe("10px");
    expect(style.top).toBe("20px");
    expect(style.right).toBeUndefined();
    expect(style.bottom).toBeUndefined();
  });

  it("sets right and bottom when provided", () => {
    const style = buildPopupStyle({ right: "30px", bottom: "40px" });
    expect(style.right).toBe("30px");
    expect(style.bottom).toBe("40px");
    expect(style.left).toBeUndefined();
    expect(style.top).toBeUndefined();
  });

  it("sets all four offsets when provided", () => {
    const pos: PopupPosition = { left: "1px", right: "2px", top: "3px", bottom: "4px" };
    const style = buildPopupStyle(pos);
    expect(style.left).toBe("1px");
    expect(style.right).toBe("2px");
    expect(style.top).toBe("3px");
    expect(style.bottom).toBe("4px");
  });

  it("omits offsets that are empty strings", () => {
    const style = buildPopupStyle({ left: "", top: "5px" });
    expect(style.left).toBeUndefined();
    expect(style.top).toBe("5px");
  });
});

describe("clampContextIndex", () => {
  it("returns -1 when max is 0", () => {
    expect(clampContextIndex(0, 0)).toBe(-1);
  });

  it("wraps negative to last", () => {
    expect(clampContextIndex(-1, 3)).toBe(2);
  });

  it("wraps past-end to first", () => {
    expect(clampContextIndex(3, 3)).toBe(0);
  });

  it("returns index unchanged when within bounds", () => {
    expect(clampContextIndex(1, 3)).toBe(1);
  });
});

describe("buildContextMenuItemClass", () => {
  it("returns base class when not focused", () => {
    expect(buildContextMenuItemClass(false)).toBe(CLASSES.FLEXLAYOUT__POPUP_MENU_ITEM);
  });

  it("appends selected modifier when focused", () => {
    const result = buildContextMenuItemClass(true);
    expect(result).toContain(CLASSES.FLEXLAYOUT__POPUP_MENU_ITEM);
    expect(result).toContain(CLASSES.FLEXLAYOUT__POPUP_MENU_ITEM__SELECTED);
  });

  it("applies mapper when provided", () => {
    const mapper = (cls: string) => `ctx-${cls}`;
    const result = buildContextMenuItemClass(true, mapper);
    expect(result).toContain(`ctx-${CLASSES.FLEXLAYOUT__POPUP_MENU_ITEM}`);
    expect(result).toContain(`ctx-${CLASSES.FLEXLAYOUT__POPUP_MENU_ITEM__SELECTED}`);
  });
});

describe("buildContextMenuStyle", () => {
  it("positions at given x/y coordinates", () => {
    const style = buildContextMenuStyle({ x: 100, y: 200 });
    expect(style.position).toBe("absolute");
    expect(style.left).toBe("100px");
    expect(style.top).toBe("200px");
  });

  it("handles zero coordinates", () => {
    const style = buildContextMenuStyle({ x: 0, y: 0 });
    expect(style.left).toBe("0px");
    expect(style.top).toBe("0px");
  });
});

describe("keyboard navigation index cycling", () => {
  it("ArrowDown from -1 goes to 0", () => {
    expect(clampIndex(-1 + 1, 5)).toBe(0);
  });

  it("ArrowDown from last wraps to 0", () => {
    expect(clampIndex(4 + 1, 5)).toBe(0);
  });

  it("ArrowUp from 0 wraps to last", () => {
    expect(clampIndex(0 - 1, 5)).toBe(4);
  });

  it("ArrowUp from -1 wraps to last", () => {
    expect(clampIndex(-1 - 1, 5)).toBe(4);
  });

  it("sequential ArrowDown cycles through all items", () => {
    let idx = -1;
    const results: number[] = [];
    for (let i = 0; i < 6; i++) {
      idx = clampIndex(idx + 1, 3);
      results.push(idx);
    }
    expect(results).toEqual([0, 1, 2, 0, 1, 2]);
  });

  it("sequential ArrowUp cycles through all items in reverse", () => {
    let idx = 0;
    const results: number[] = [];
    for (let i = 0; i < 6; i++) {
      idx = clampIndex(idx - 1, 3);
      results.push(idx);
    }
    expect(results).toEqual([2, 1, 0, 2, 1, 0]);
  });
});
