import { describe, it, expect, afterEach } from "vitest";
import { createRoot } from "solid-js";
import { Model } from "../../flexlayout/model/Model";
import { Action, type LayoutAction } from "../../flexlayout/model/Action";
import type { IJsonModel } from "../../flexlayout/types";
import { createLayoutBridge, type LayoutBridge } from "../bridge";
import { LayoutContext, useLayoutContext, type LayoutContextValue } from "../context";

const simpleLayout: IJsonModel = {
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
        ],
      },
    ],
  },
};

function makeModel(json: IJsonModel = simpleLayout): Model {
  return Model.fromJson(json);
}

function tick(): Promise<void> {
  return new Promise((r) => setTimeout(r, 0));
}

function buildDoAction(
  model: Model,
  onAction?: (action: LayoutAction) => LayoutAction | undefined,
): (action: LayoutAction) => void {
  return (action: LayoutAction) => {
    const intercepted = onAction?.(action);
    const finalAction =
      intercepted === undefined && onAction ? undefined : (intercepted ?? action);
    if (finalAction) {
      model.doAction(finalAction);
    }
  };
}

describe("Layout context and logic", () => {
  let bridge: LayoutBridge | undefined;

  afterEach(() => {
    bridge?.dispose();
    bridge = undefined;
  });

  it("useLayoutContext throws outside a Layout provider", () => {
    expect(() => {
      createRoot((dispose) => {
        try {
          useLayoutContext();
        } finally {
          dispose();
        }
      });
    }).toThrow("useLayoutContext must be used within a <Layout> component");
  });

  it("LayoutContext is defined and can hold a value", () => {
    createRoot((dispose) => {
      expect(LayoutContext).toBeDefined();
      expect(LayoutContext.id).toBeDefined();
      dispose();
    });
  });

  it("bridge initializes from model and exposes store", () => {
    createRoot((dispose) => {
      const model = makeModel();
      bridge = createLayoutBridge(model);

      expect(bridge.store).toBeDefined();
      expect(bridge.store.layout).toBeDefined();
      expect((bridge.store.layout as any).type).toBe("row");
      dispose();
    });
  });

  it("doAction dispatches through model when no interceptor", () => {
    createRoot((dispose) => {
      const model = makeModel();
      bridge = createLayoutBridge(model);
      const doAction = buildDoAction(model);

      expect(bridge.selectedTabId()).toBeUndefined();

      doAction(Action.selectTab("tab1"));

      expect(bridge.selectedTabId()).toBe("tab1");
      dispose();
    });
  });

  it("doAction interceptor can replace the action", () => {
    createRoot((dispose) => {
      const model = makeModel();
      bridge = createLayoutBridge(model);

      const interceptor = (_action: LayoutAction): LayoutAction | undefined =>
        Action.renameTab("tab1", "Intercepted");

      const doAction = buildDoAction(model, interceptor);
      doAction(Action.selectTab("tab1"));

      const tab = (bridge.store.layout as any).children[0].children[0];
      expect(tab.name).toBe("Intercepted");
      dispose();
    });
  });

  it("doAction interceptor returning undefined suppresses the action", () => {
    createRoot((dispose) => {
      const model = makeModel();
      bridge = createLayoutBridge(model);

      const interceptor = (): LayoutAction | undefined => undefined;
      const doAction = buildDoAction(model, interceptor);

      const storeBefore = JSON.stringify(bridge.store);
      doAction(Action.selectTab("tab1"));
      const storeAfter = JSON.stringify(bridge.store);

      expect(storeAfter).toBe(storeBefore);
      dispose();
    });
  });

  it("onModelChange callback fires when model processes an action", () => {
    createRoot((dispose) => {
      const model = makeModel();
      bridge = createLayoutBridge(model);

      let callCount = 0;
      let lastAction: LayoutAction | undefined;

      const actionDisposable = model.onDidAction((action) => {
        callCount++;
        lastAction = action;
      });

      model.doAction(Action.selectTab("tab1"));

      expect(callCount).toBe(1);
      expect(lastAction).toBeDefined();
      expect(lastAction!.type).toBe("SELECT_TAB");

      actionDisposable.dispose();
      dispose();
    });
  });

  it("onModelChange stops firing after disposable is disposed", () => {
    createRoot((dispose) => {
      const model = makeModel();
      bridge = createLayoutBridge(model);

      let callCount = 0;
      const actionDisposable = model.onDidAction(() => {
        callCount++;
      });

      model.doAction(Action.selectTab("tab1"));
      expect(callCount).toBe(1);

      actionDisposable.dispose();

      model.doAction(Action.renameTab("tab1", "Renamed"));
      expect(callCount).toBe(1);

      dispose();
    });
  });

  it("bridge dispose stops store updates from model changes", async () => {
    let dispose!: () => void;
    try {
      await createRoot(async (d) => {
        dispose = d;
        const model = makeModel();
        bridge = createLayoutBridge(model);

        const storeBefore = JSON.stringify(bridge.store);
        bridge.dispose();

        model.doAction(Action.selectTab("tab1"));
        await tick();
        const storeAfter = JSON.stringify(bridge.store);

        expect(storeAfter).toBe(storeBefore);
      });
    } finally {
      dispose?.();
    }
  });

  it("classNameMapper transforms class names", () => {
    const mapper = (cls: string) => `custom-${cls}`;
    expect(mapper("flexlayout__layout")).toBe("custom-flexlayout__layout");

    const identityMapper = (cls: string) => cls;
    expect(identityMapper("flexlayout__layout")).toBe("flexlayout__layout");
  });

  it("LayoutContextValue interface is satisfied by constructed value", () => {
    createRoot((dispose) => {
      const model = makeModel();
      bridge = createLayoutBridge(model);
      const factory = () => "content";
      const doAction = buildDoAction(model);

      const ctxValue: LayoutContextValue = {
        bridge,
        model,
        doAction,
        classNameMapper: (cls: string) => cls,
        factory,
      };

      expect(ctxValue.bridge).toBe(bridge);
      expect(ctxValue.model).toBe(model);
      expect(typeof ctxValue.doAction).toBe("function");
      expect(typeof ctxValue.factory).toBe("function");
      expect(typeof ctxValue.classNameMapper).toBe("function");
      dispose();
    });
  });

  it("context value factory and doAction are callable", () => {
    createRoot((dispose) => {
      const model = makeModel();
      bridge = createLayoutBridge(model);
      const factory = () => "rendered";
      const doAction = buildDoAction(model);

      const ctxValue: LayoutContextValue = {
        bridge,
        model,
        doAction,
        factory,
      };

      expect(ctxValue.factory({} as any)).toBe("rendered");

      ctxValue.doAction(Action.selectTab("tab1"));
      expect(bridge.selectedTabId()).toBe("tab1");

      dispose();
    });
  });
});
