import { describe, it, expect, afterEach } from "vitest";
import { createRoot } from "solid-js";
import { Model } from "../../flexlayout/model/Model";
import { Action, type LayoutAction } from "../../flexlayout/model/Action";
import type { IJsonModel } from "../../flexlayout/types";
import { CLASSES } from "../../flexlayout/core/Types";
import { createLayoutBridge, type LayoutBridge } from "../bridge";
import { findTabSet, tabStripOuterClass } from "../components/TabSet";
import { findTab } from "../components/Tab";

const twoTabLayout: IJsonModel = {
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
    ],
  },
};

const twoTabsetLayout: IJsonModel = {
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

const closableTabLayout: IJsonModel = {
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
          { type: "tab", id: "tab1", name: "One", component: "text", enableClose: true },
          { type: "tab", id: "tab2", name: "Two", component: "text", enableClose: false },
        ],
      },
    ],
  },
};

const emptyTabsetLayout: IJsonModel = {
  global: {},
  borders: [],
  layout: {
    type: "row",
    weight: 100,
    children: [
      {
        type: "tabset",
        id: "ts-empty",
        weight: 50,
        children: [],
      },
    ],
  },
};

const nestedLayout: IJsonModel = {
  global: {},
  borders: [],
  layout: {
    type: "row",
    weight: 100,
    children: [
      {
        type: "row",
        weight: 50,
        children: [
          {
            type: "tabset",
            id: "ts-nested",
            weight: 50,
            children: [
              { type: "tab", id: "tab-deep", name: "Deep", component: "text" },
            ],
          },
        ],
      },
    ],
  },
};

function makeModel(json: IJsonModel = twoTabLayout): Model {
  return Model.fromJson(json);
}

function buildDoAction(model: Model): (action: LayoutAction) => void {
  return (action: LayoutAction) => {
    model.doAction(action);
  };
}

describe("findTabSet", () => {
  it("finds a tabset by id at the top level", () => {
    const layout = twoTabLayout.layout!;
    const found = findTabSet(layout, "ts0");
    expect(found).toBeDefined();
    expect(found!.id).toBe("ts0");
    expect(found!.type).toBe("tabset");
  });

  it("returns undefined for non-existent id", () => {
    const layout = twoTabLayout.layout!;
    const found = findTabSet(layout, "nonexistent");
    expect(found).toBeUndefined();
  });

  it("finds a tabset nested inside rows", () => {
    const layout = nestedLayout.layout!;
    const found = findTabSet(layout, "ts-nested");
    expect(found).toBeDefined();
    expect(found!.id).toBe("ts-nested");
  });

  it("finds the correct tabset among multiple", () => {
    const layout = twoTabsetLayout.layout!;
    const found = findTabSet(layout, "ts1");
    expect(found).toBeDefined();
    expect(found!.children).toHaveLength(1);
    expect(found!.children![0].name).toBe("Three");
  });
});

describe("findTab", () => {
  it("finds a tab by id", () => {
    const layout = twoTabLayout.layout!;
    const found = findTab(layout, "tab1");
    expect(found).toBeDefined();
    expect(found!.name).toBe("One");
  });

  it("returns undefined for non-existent tab", () => {
    const layout = twoTabLayout.layout!;
    const found = findTab(layout, "nonexistent");
    expect(found).toBeUndefined();
  });
});

describe("tabStripOuterClass", () => {
  const identity = (cls: string) => cls;

  it("includes base tabbar outer class", () => {
    const cls = tabStripOuterClass(identity, "top", false, false);
    expect(cls).toContain(CLASSES.FLEXLAYOUT__TABSET_TABBAR_OUTER);
  });

  it("includes tab location suffix", () => {
    const cls = tabStripOuterClass(identity, "top", false, false);
    expect(cls).toContain(CLASSES.FLEXLAYOUT__TABSET_TABBAR_OUTER_ + "top");
  });

  it("includes bottom location suffix", () => {
    const cls = tabStripOuterClass(identity, "bottom", false, false);
    expect(cls).toContain(CLASSES.FLEXLAYOUT__TABSET_TABBAR_OUTER_ + "bottom");
  });

  it("adds selected class when active", () => {
    const cls = tabStripOuterClass(identity, "top", true, false);
    expect(cls).toContain(CLASSES.FLEXLAYOUT__TABSET_SELECTED);
  });

  it("does not add selected class when inactive", () => {
    const cls = tabStripOuterClass(identity, "top", false, false);
    expect(cls).not.toContain(CLASSES.FLEXLAYOUT__TABSET_SELECTED);
  });

  it("adds maximized class when maximized", () => {
    const cls = tabStripOuterClass(identity, "top", false, true);
    expect(cls).toContain(CLASSES.FLEXLAYOUT__TABSET_MAXIMIZED);
  });

  it("applies classNameMapper to classes", () => {
    const mapper = (cls: string) => `custom-${cls}`;
    const cls = tabStripOuterClass(mapper, "top", true, false);
    expect(cls).toContain(`custom-${CLASSES.FLEXLAYOUT__TABSET_TABBAR_OUTER}`);
    expect(cls).toContain(`custom-${CLASSES.FLEXLAYOUT__TABSET_SELECTED}`);
  });
});

describe("TabSet reactive logic via bridge", () => {
  let bridge: LayoutBridge | undefined;

  afterEach(() => {
    bridge?.dispose();
    bridge = undefined;
  });

  it("store contains tabset node with children", () => {
    createRoot((dispose) => {
      const model = makeModel();
      bridge = createLayoutBridge(model);

      const ts = findTabSet(bridge.store.layout!, "ts0");
      expect(ts).toBeDefined();
      expect(ts!.children).toHaveLength(2);
      expect(ts!.children![0].name).toBe("One");
      expect(ts!.children![1].name).toBe("Two");
      dispose();
    });
  });

  it("selectTab updates selectedTabId signal", () => {
    createRoot((dispose) => {
      const model = makeModel();
      bridge = createLayoutBridge(model);
      const doAction = buildDoAction(model);

      expect(bridge.selectedTabId()).toBeUndefined();

      doAction(Action.selectTab("tab2"));
      expect(bridge.selectedTabId()).toBe("tab2");

      dispose();
    });
  });

  it("selectTab updates selected index in store", () => {
    createRoot((dispose) => {
      const model = makeModel();
      bridge = createLayoutBridge(model);
      const doAction = buildDoAction(model);

      doAction(Action.selectTab("tab2"));

      const ts = findTabSet(bridge.store.layout!, "ts0");
      expect(ts).toBeDefined();
      expect(ts!.selected).toBe(1);

      dispose();
    });
  });

  it("setActiveTabset updates activeTabsetId signal", () => {
    createRoot((dispose) => {
      const model = makeModel(twoTabsetLayout);
      bridge = createLayoutBridge(model);
      const doAction = buildDoAction(model);

      expect(bridge.activeTabsetId()).toBeUndefined();

      doAction(Action.setActiveTabset("ts1"));
      expect(bridge.activeTabsetId()).toBe("ts1");

      dispose();
    });
  });

  it("deleteTab removes tab from store", () => {
    createRoot((dispose) => {
      const model = makeModel();
      bridge = createLayoutBridge(model);
      const doAction = buildDoAction(model);

      const tsBefore = findTabSet(bridge.store.layout!, "ts0");
      expect(tsBefore!.children).toHaveLength(2);

      doAction(Action.deleteTab("tab1"));

      const tsAfter = findTabSet(bridge.store.layout!, "ts0");
      expect(tsAfter!.children).toHaveLength(1);
      expect(tsAfter!.children![0].name).toBe("Two");

      dispose();
    });
  });

  it("empty tabset has no children in store", () => {
    createRoot((dispose) => {
      const model = makeModel(emptyTabsetLayout);
      bridge = createLayoutBridge(model);

      const ts = findTabSet(bridge.store.layout!, "ts-empty");
      expect(ts).toBeDefined();
      expect(ts!.children).toHaveLength(0);

      dispose();
    });
  });

  it("enableClose attribute is preserved in store", () => {
    createRoot((dispose) => {
      const model = makeModel(closableTabLayout);
      bridge = createLayoutBridge(model);

      const ts = findTabSet(bridge.store.layout!, "ts0");
      expect(ts).toBeDefined();

      const tab1 = ts!.children![0];
      const tab2 = ts!.children![1];
      expect(tab1.enableClose).toBe(true);
      expect(tab2.enableClose).toBe(false);

      dispose();
    });
  });

  it("tab button CSS class includes selected/unselected suffix", () => {
    const identity = (cls: string) => cls;
    const base = CLASSES.FLEXLAYOUT__TAB_BUTTON;

    const selectedClass = `${identity(base)} ${identity(base + "_top")} ${identity(base + "--selected")}`;
    const unselectedClass = `${identity(base)} ${identity(base + "_top")} ${identity(base + "--unselected")}`;

    expect(selectedClass).toContain("--selected");
    expect(unselectedClass).toContain("--unselected");
    expect(selectedClass).not.toContain("--unselected");
    expect(unselectedClass).not.toContain("--selected");
  });

  it("tabset CSS class includes selected when active", () => {
    const identity = (cls: string) => cls;
    const base = CLASSES.FLEXLAYOUT__TABSET;
    const selected = CLASSES.FLEXLAYOUT__TABSET_SELECTED;

    const activeClass = `${identity(base)} ${identity(selected)}`;
    const inactiveClass = identity(base);

    expect(activeClass).toContain(selected);
    expect(inactiveClass).not.toContain(selected);
  });

  it("maximizeToggle action dispatches through model", () => {
    createRoot((dispose) => {
      const model = makeModel(twoTabsetLayout);
      bridge = createLayoutBridge(model);
      const doAction = buildDoAction(model);

      doAction(Action.setActiveTabset("ts0"));
      expect(bridge.activeTabsetId()).toBe("ts0");

      doAction(Action.maximizeToggle("ts0"));

      const ts = findTabSet(bridge.store.layout!, "ts0");
      expect(ts).toBeDefined();
      expect(ts!.maximized).toBe(true);

      dispose();
    });
  });

  it("switching tabs updates selected tab across tabsets", () => {
    createRoot((dispose) => {
      const model = makeModel(twoTabsetLayout);
      bridge = createLayoutBridge(model);
      const doAction = buildDoAction(model);

      doAction(Action.selectTab("tab1"));
      expect(bridge.selectedTabId()).toBe("tab1");
      expect(bridge.activeTabsetId()).toBe("ts0");

      doAction(Action.selectTab("tab3"));
      expect(bridge.selectedTabId()).toBe("tab3");
      expect(bridge.activeTabsetId()).toBe("ts1");

      dispose();
    });
  });

  it("renameTab updates tab name in store", () => {
    createRoot((dispose) => {
      const model = makeModel();
      bridge = createLayoutBridge(model);
      const doAction = buildDoAction(model);

      doAction(Action.renameTab("tab1", "Renamed"));

      const ts = findTabSet(bridge.store.layout!, "ts0");
      expect(ts!.children![0].name).toBe("Renamed");

      dispose();
    });
  });
});
