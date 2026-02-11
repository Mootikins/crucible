import { describe, it, expect } from "vitest";
import {
  cn,
  mapClass,
  buildClassName,
  CLASSES,
  type ClassNameMapper,
} from "../classes";

describe("cn", () => {
  it("joins multiple class strings", () => {
    expect(cn("a", "b", "c")).toBe("a b c");
  });

  it("filters out falsy values", () => {
    expect(cn("a", "", undefined, null, false, "b")).toBe("a b");
  });

  it("returns empty string when all values are falsy", () => {
    expect(cn("", undefined, null, false)).toBe("");
  });

  it("handles single class", () => {
    expect(cn("flexlayout__tab")).toBe("flexlayout__tab");
  });
});

describe("mapClass", () => {
  it("returns default class when no mapper provided", () => {
    expect(mapClass("flexlayout__tab")).toBe("flexlayout__tab");
  });

  it("returns default class when mapper is undefined", () => {
    expect(mapClass("flexlayout__tab", undefined)).toBe("flexlayout__tab");
  });

  it("applies mapper function to class name", () => {
    const mapper: ClassNameMapper = (c) => `custom-${c}`;
    expect(mapClass("flexlayout__tab", mapper)).toBe("custom-flexlayout__tab");
  });

  it("mapper can return completely different class", () => {
    const mapper: ClassNameMapper = (c) =>
      c === "flexlayout__tab" ? "my-tab" : c;
    expect(mapClass("flexlayout__tab", mapper)).toBe("my-tab");
    expect(mapClass("flexlayout__row", mapper)).toBe("flexlayout__row");
  });
});

describe("buildClassName", () => {
  it("returns only base class when no modifiers are active", () => {
    expect(
      buildClassName("flexlayout__tabset", {
        "flexlayout__tabset-selected": false,
        "flexlayout__tabset-maximized": false,
      }),
    ).toBe("flexlayout__tabset");
  });

  it("includes active modifiers", () => {
    expect(
      buildClassName("flexlayout__tabset", {
        "flexlayout__tabset-selected": true,
        "flexlayout__tabset-maximized": false,
      }),
    ).toBe("flexlayout__tabset flexlayout__tabset-selected");
  });

  it("includes multiple active modifiers", () => {
    expect(
      buildClassName("flexlayout__tabset", {
        "flexlayout__tabset-selected": true,
        "flexlayout__tabset-maximized": true,
      }),
    ).toBe(
      "flexlayout__tabset flexlayout__tabset-selected flexlayout__tabset-maximized",
    );
  });

  it("applies mapper to base and all active modifiers", () => {
    const mapper: ClassNameMapper = (c) => `pfx-${c}`;
    expect(
      buildClassName(
        "flexlayout__tabset",
        { "flexlayout__tabset-selected": true },
        mapper,
      ),
    ).toBe("pfx-flexlayout__tabset pfx-flexlayout__tabset-selected");
  });

  it("handles empty modifiers object", () => {
    expect(buildClassName("flexlayout__row", {})).toBe("flexlayout__row");
  });
});

describe("CLASSES re-export", () => {
  it("exports CLASSES enum from Types.ts", () => {
    expect(CLASSES).toBeDefined();
    expect(typeof CLASSES).toBe("object");
  });

  it("contains expected layout class constants", () => {
    expect(CLASSES.FLEXLAYOUT__LAYOUT).toBe("flexlayout__layout");
    expect(CLASSES.FLEXLAYOUT__ROW).toBe("flexlayout__row");
    expect(CLASSES.FLEXLAYOUT__TAB).toBe("flexlayout__tab");
    expect(CLASSES.FLEXLAYOUT__TABSET).toBe("flexlayout__tabset");
    expect(CLASSES.FLEXLAYOUT__SPLITTER).toBe("flexlayout__splitter");
    expect(CLASSES.FLEXLAYOUT__BORDER).toBe("flexlayout__border");
  });

  it("contains BEM modifier classes", () => {
    expect(CLASSES.FLEXLAYOUT__TABSET_SELECTED).toBe(
      "flexlayout__tabset-selected",
    );
    expect(CLASSES.FLEXLAYOUT__TABSET_MAXIMIZED).toBe(
      "flexlayout__tabset-maximized",
    );
    expect(CLASSES.FLEXLAYOUT__BORDER_BUTTON__SELECTED).toBe(
      "flexlayout__border_button--selected",
    );
    expect(CLASSES.FLEXLAYOUT__BORDER_BUTTON__UNSELECTED).toBe(
      "flexlayout__border_button--unselected",
    );
  });
});

describe("integration: vanilla renderer patterns", () => {
  it("reproduces splitter class concatenation pattern", () => {
    const mapper: ClassNameMapper = (c) => c;
    const result = cn(
      mapClass(CLASSES.FLEXLAYOUT__SPLITTER, mapper),
      mapClass(CLASSES.FLEXLAYOUT__SPLITTER_ + "horizontal", mapper),
    );
    expect(result).toBe(
      "flexlayout__splitter flexlayout__splitter_horizontal",
    );
  });

  it("reproduces tab button selected/unselected pattern", () => {
    const selected = true;
    const result = buildClassName(
      CLASSES.FLEXLAYOUT__TAB_BUTTON,
      {
        [CLASSES.FLEXLAYOUT__TAB_BUTTON + "--selected"]: selected,
        [CLASSES.FLEXLAYOUT__TAB_BUTTON + "--unselected"]: !selected,
      },
    );
    expect(result).toBe(
      "flexlayout__tab_button flexlayout__tab_button--selected",
    );
  });

  it("reproduces border strip class pattern with mapper", () => {
    const mapper: ClassNameMapper = (c) => `theme-${c}`;
    const result = cn(
      mapClass(CLASSES.FLEXLAYOUT__BORDER, mapper),
      mapClass(CLASSES.FLEXLAYOUT__BORDER_ + "left", mapper),
      mapClass(CLASSES.FLEXLAYOUT__BORDER__COLLAPSED, mapper),
    );
    expect(result).toBe(
      "theme-flexlayout__border theme-flexlayout__border_left theme-flexlayout__border--collapsed",
    );
  });
});
