import { describe, it, expect } from "vitest";
import { twoTabs } from "./fixtures";
import { Model } from "../model/Model";
import { Action } from "../model/Action";
import { TabSetNode } from "../model/TabSetNode";
import type { IJsonModel } from "../types";

function getFirstTabSet(model: Model): TabSetNode | undefined {
  let found: TabSetNode | undefined;
  const root = model.getRoot()!;
  root.forEachNode((node) => {
    if (!found && node instanceof TabSetNode) found = node;
  }, 0);
  return found;
}

describe("paneview > mode attribute", () => {
  it("default mode is 'tabs'", () => {
    const model = Model.fromJson(twoTabs);
    const ts = getFirstTabSet(model)!;
    expect(ts.getMode()).toBe("tabs");
  });

  it("SET_TABSET_MODE changes mode to paneview", () => {
    const model = Model.fromJson(twoTabs);
    const ts = getFirstTabSet(model)!;

    model.doAction(Action.setTabsetMode(ts.getId(), "paneview"));

    expect(ts.getMode()).toBe("paneview");
  });

  it("SET_TABSET_MODE changes mode back to tabs", () => {
    const model = Model.fromJson(twoTabs);
    const ts = getFirstTabSet(model)!;

    model.doAction(Action.setTabsetMode(ts.getId(), "paneview"));
    model.doAction(Action.setTabsetMode(ts.getId(), "tabs"));

    expect(ts.getMode()).toBe("tabs");
  });

  it("SET_TABSET_MODE on nonexistent tabset is a no-op", () => {
    const model = Model.fromJson(twoTabs);
    model.doAction(Action.setTabsetMode("nonexistent", "paneview"));
  });
});

describe("paneview > serialization", () => {
  it("mode=paneview survives JSON round-trip", () => {
    const model = Model.fromJson(twoTabs);
    const ts = getFirstTabSet(model)!;
    model.doAction(Action.setTabsetMode(ts.getId(), "paneview"));

    const json = model.toJson();
    const restored = Model.fromJson(json);
    const restoredTs = getFirstTabSet(restored)!;

    expect(restoredTs.getMode()).toBe("paneview");
  });

  it("mode=tabs is NOT written to JSON (backward compat)", () => {
    const model = Model.fromJson(twoTabs);
    const json = model.toJson();
    const jsonStr = JSON.stringify(json);
    expect(jsonStr).not.toContain('"mode"');
  });

  it("old JSON without mode loads with default 'tabs'", () => {
    const model = Model.fromJson(twoTabs);
    const ts = getFirstTabSet(model)!;
    expect(ts.getMode()).toBe("tabs");
  });

  it("JSON with mode=paneview loads correctly", () => {
    const jsonWithMode: IJsonModel = {
      global: {},
      borders: [],
      layout: {
        type: "row",
        weight: 100,
        children: [
          {
            type: "tabset",
            weight: 100,
            mode: "paneview",
            children: [
              { type: "tab", name: "Test", component: "text" },
            ],
          } as any,
        ],
      } as any,
    };

    const model = Model.fromJson(jsonWithMode);
    const ts = getFirstTabSet(model)!;
    expect(ts.getMode()).toBe("paneview");
  });
});
