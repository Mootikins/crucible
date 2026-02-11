import { describe, it, expect, afterEach } from "vitest";
import { createRoot } from "solid-js";
import { Model } from "../../flexlayout/model/Model";
import { Action } from "../../flexlayout/model/Action";
import type { IJsonModel } from "../../flexlayout/types";
import {
  createLayoutBridge,
  useLayoutStore,
  useSelectedTab,
  useDragState,
  useActiveTabset,
  type LayoutBridge,
} from "../bridge";

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

const threeTabsets: IJsonModel = {
  global: {},
  borders: [],
  layout: {
    type: "row",
    weight: 100,
    children: [
      {
        type: "tabset",
        id: "ts0",
        weight: 33,
        children: [
          { type: "tab", id: "t0", name: "A", component: "text" },
        ],
      },
      {
        type: "tabset",
        id: "ts1",
        weight: 33,
        children: [
          { type: "tab", id: "t1", name: "B", component: "text" },
        ],
      },
      {
        type: "tabset",
        id: "ts2",
        weight: 34,
        children: [
          { type: "tab", id: "t2", name: "C", component: "text" },
        ],
      },
    ],
  },
};

function makeModel(json: IJsonModel = twoTabsets): Model {
  return Model.fromJson(json);
}

function tick(): Promise<void> {
  return new Promise((r) => setTimeout(r, 0));
}

describe("createLayoutBridge", () => {
  let bridge: LayoutBridge | undefined;

  afterEach(() => {
    bridge?.dispose();
    bridge = undefined;
  });

  it("initializes store from model.toJson()", () => {
    createRoot((dispose) => {
      const model = makeModel();
      bridge = createLayoutBridge(model);
      const store = useLayoutStore(bridge);

      expect(store.layout).toBeDefined();
      expect(store.layout!.type).toBe("row");

      const children = (store.layout as any).children;
      expect(children).toHaveLength(2);
      expect(children[0].id).toBe("ts0");
      expect(children[1].id).toBe("ts1");
      dispose();
    });
  });

  it("updates selectedTabId on SELECT_TAB action", async () => {
    let dispose!: () => void;
    try {
      await createRoot(async (d) => {
        dispose = d;
        const model = makeModel();
        bridge = createLayoutBridge(model);

        expect(bridge.selectedTabId()).toBeUndefined();

        model.doAction(Action.selectTab("tab2"));
        await tick();

        expect(bridge.selectedTabId()).toBe("tab2");
      });
    } finally {
      dispose?.();
    }
  });

  it("selectedTabId signal updates across multiple selections", async () => {
    let dispose!: () => void;
    try {
      await createRoot(async (d) => {
        dispose = d;
        const model = makeModel();
        bridge = createLayoutBridge(model);

        expect(bridge.selectedTabId()).toBeUndefined();

        model.doAction(Action.selectTab("tab1"));
        expect(bridge.selectedTabId()).toBe("tab1");

        model.doAction(Action.selectTab("tab2"));
        expect(bridge.selectedTabId()).toBe("tab2");

        model.doAction(Action.selectTab("tab3"));
        expect(bridge.selectedTabId()).toBe("tab3");
      });
    } finally {
      dispose?.();
    }
  });

  it("updates activeTabsetId on SET_ACTIVE_TABSET", async () => {
    let dispose!: () => void;
    try {
      await createRoot(async (d) => {
        dispose = d;
        const model = makeModel();
        bridge = createLayoutBridge(model);

        expect(bridge.activeTabsetId()).toBeUndefined();

        model.doAction(Action.setActiveTabset("ts1"));
        await tick();

        expect(bridge.activeTabsetId()).toBe("ts1");
      });
    } finally {
      dispose?.();
    }
  });

  it("updates store on MOVE_NODE action", async () => {
    let dispose!: () => void;
    try {
      await createRoot(async (d) => {
        dispose = d;
        const model = makeModel();
        bridge = createLayoutBridge(model);

        const ts1Before = (bridge.store.layout as any).children[1].children.length;
        expect(ts1Before).toBe(1);

        model.doAction(Action.moveNode("tab2", "ts1", "center", 0));
        await tick();

        const ts1After = (bridge.store.layout as any).children[1].children;
        expect(ts1After.length).toBe(2);
      });
    } finally {
      dispose?.();
    }
  });

  it("updates store on ADD_NODE action", async () => {
    let dispose!: () => void;
    try {
      await createRoot(async (d) => {
        dispose = d;
        const model = makeModel();
        bridge = createLayoutBridge(model);

        model.doAction(
          Action.addNode(
            { type: "tab", id: "tab-new", name: "New", component: "text" },
            "ts0",
            "center",
            -1,
          ),
        );
        await tick();

        const ts0Children = (bridge.store.layout as any).children[0].children;
        expect(ts0Children.length).toBe(3);
        expect(ts0Children[2].name).toBe("New");
      });
    } finally {
      dispose?.();
    }
  });

  it("updates store on DELETE_TAB action", async () => {
    let dispose!: () => void;
    try {
      await createRoot(async (d) => {
        dispose = d;
        const model = makeModel();
        bridge = createLayoutBridge(model);

        expect((bridge.store.layout as any).children[0].children.length).toBe(2);

        model.doAction(Action.deleteTab("tab1"));
        await tick();

        const ts0Children = (bridge.store.layout as any).children[0].children;
        expect(ts0Children.length).toBe(1);
        expect(ts0Children[0].name).toBe("Two");
      });
    } finally {
      dispose?.();
    }
  });

  it("updates store on ADJUST_WEIGHTS action", async () => {
    let dispose!: () => void;
    try {
      await createRoot(async (d) => {
        dispose = d;
        const model = makeModel(threeTabsets);
        bridge = createLayoutBridge(model);
        const rootId = model.getRoot()!.getId();

        model.doAction(Action.adjustWeights(rootId, [20, 30, 50], "horizontal"));
        await tick();

        const children = (bridge.store.layout as any).children;
        expect(children[0].weight).toBe(20);
        expect(children[1].weight).toBe(30);
        expect(children[2].weight).toBe(50);
      });
    } finally {
      dispose?.();
    }
  });

  it("dragState signal updates independently via setDragState", () => {
    createRoot((dispose) => {
      const model = makeModel();
      bridge = createLayoutBridge(model);

      const getDrag = useDragState(bridge);
      expect(getDrag()).toBeNull();

      bridge.setDragState({ draggingNodeId: "tab1", dropTargetId: "ts1" });
      expect(getDrag()).toEqual({ draggingNodeId: "tab1", dropTargetId: "ts1" });

      bridge.setDragState(null);
      expect(getDrag()).toBeNull();
      dispose();
    });
  });

  it("batch updates store and signals atomically on single action", async () => {
    let dispose!: () => void;
    try {
      await createRoot(async (d) => {
        dispose = d;
        const model = makeModel();
        bridge = createLayoutBridge(model);

        model.doAction(Action.selectTab("tab2"));

        expect(bridge.selectedTabId()).toBe("tab2");
        expect(bridge.activeTabsetId()).toBe("ts0");

        const storeChildren = (bridge.store.layout as any).children[0].children;
        expect(storeChildren[0].id).toBe("tab1");
        expect(storeChildren[1].id).toBe("tab2");
      });
    } finally {
      dispose?.();
    }
  });

  it("reconcile preserves unchanged node data after unrelated action", async () => {
    let dispose!: () => void;
    try {
      await createRoot(async (d) => {
        dispose = d;
        const model = makeModel();
        bridge = createLayoutBridge(model);

        const ts1ChildIdBefore = (bridge.store.layout as any).children[1].children[0].id;
        const ts1ChildNameBefore = (bridge.store.layout as any).children[1].children[0].name;

        model.doAction(Action.selectTab("tab2"));

        const ts1After = (bridge.store.layout as any).children[1];
        expect(ts1After.id).toBe("ts1");
        expect(ts1After.children[0].id).toBe(ts1ChildIdBefore);
        expect(ts1After.children[0].name).toBe(ts1ChildNameBefore);
        expect(ts1After.children.length).toBe(1);
      });
    } finally {
      dispose?.();
    }
  });

  it("handles multiple sequential actions correctly", async () => {
    let dispose!: () => void;
    try {
      await createRoot(async (d) => {
        dispose = d;
        const model = makeModel();
        bridge = createLayoutBridge(model);

        model.doAction(Action.selectTab("tab2"));
        model.doAction(Action.renameTab("tab2", "Renamed"));
        model.doAction(Action.setActiveTabset("ts1"));
        await tick();

        expect(bridge.selectedTabId()).toBe("tab3");
        expect(bridge.activeTabsetId()).toBe("ts1");

        const ts0Children = (bridge.store.layout as any).children[0].children;
        const renamedTab = ts0Children.find((c: any) => c.id === "tab2");
        expect(renamedTab?.name).toBe("Renamed");
      });
    } finally {
      dispose?.();
    }
  });

  it("typed accessor functions return correct values", () => {
    createRoot((dispose) => {
      const model = makeModel();
      bridge = createLayoutBridge(model);

      expect(useLayoutStore(bridge)).toBe(bridge.store);
      expect(useSelectedTab(bridge)).toBe(bridge.selectedTabId);
      expect(useDragState(bridge)).toBe(bridge.dragState);
      expect(useActiveTabset(bridge)).toBe(bridge.activeTabsetId);
      dispose();
    });
  });

  it("dispose unsubscribes from model changes", async () => {
    let dispose!: () => void;
    try {
      await createRoot(async (d) => {
        dispose = d;
        const model = makeModel();
        bridge = createLayoutBridge(model);

        const storeBefore = JSON.stringify(bridge.store);
        bridge.dispose();

        model.doAction(Action.selectTab("tab2"));
        await tick();
        const storeAfter = JSON.stringify(bridge.store);

        expect(storeAfter).toBe(storeBefore);
      });
    } finally {
      dispose?.();
    }
  });
});
