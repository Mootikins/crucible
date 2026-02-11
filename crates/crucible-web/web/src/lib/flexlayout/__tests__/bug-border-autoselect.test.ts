import { describe, it, expect } from "vitest";
import { Model } from "../model/Model";
import { Action } from "../model/Action";
import { BorderNode } from "../model/BorderNode";
import { TabNode } from "../model/TabNode";
import type { IJsonModel } from "../types";

/**
 * Regression: moveNode into collapsed empty border didn't auto-select the tab.
 *
 * BorderNode.drop() called isAutoSelectTab() without the whenOpen hint.
 * For an empty collapsed border (selected === -1), it fell through to
 * autoSelectTabWhenClosed (false) instead of autoSelectTabWhenOpen (true).
 * Fix: pass willBeOpen so collapsed→expanded transition uses the correct setting.
 */

const fixtureWithAutoSelect: IJsonModel = {
  global: {
    borderAutoSelectTabWhenOpen: true,
    borderAutoSelectTabWhenClosed: false,
  },
  borders: [
    {
      type: "border",
      location: "right",
      size: 200,
      children: [],
    },
  ],
  layout: {
    type: "row",
    weight: 100,
    children: [
      {
        type: "tabset",
        weight: 100,
        children: [
          { type: "tab", name: "Main", component: "text" },
          { type: "tab", name: "Side Panel", component: "text" },
        ],
      },
    ],
  },
};

const fixtureNoAutoSelect: IJsonModel = {
  global: {
    borderAutoSelectTabWhenOpen: false,
    borderAutoSelectTabWhenClosed: false,
  },
  borders: [
    {
      type: "border",
      location: "right",
      size: 200,
      children: [],
    },
  ],
  layout: {
    type: "row",
    weight: 100,
    children: [
      {
        type: "tabset",
        weight: 100,
        children: [
          { type: "tab", name: "Main", component: "text" },
          { type: "tab", name: "Side Panel", component: "text" },
        ],
      },
    ],
  },
};

function getBorder(model: Model, location: string): BorderNode {
  const border = model.getBorderSet().getBorder(location as any);
  if (!border) throw new Error(`No border at ${location}`);
  return border;
}

function getTabByName(model: Model, name: string): TabNode {
  const root = model.getRoot()!;
  const tabset = root.getChildren()[0];
  for (const child of tabset.getChildren()) {
    if (child instanceof TabNode && child.getName() === name) {
      return child;
    }
  }
  throw new Error(`Tab "${name}" not found in first tabset`);
}

describe("bug: moveNode into collapsed empty border should auto-select tab", () => {
  it("tab is selected after moving into a collapsed empty border (autoSelectTabWhenOpen: true)", () => {
    const model = Model.fromJson(fixtureWithAutoSelect);
    const right = getBorder(model, "right");

    expect(right.getChildren().length).toBe(0);
    expect(right.getSelected()).toBe(-1);

    model.doAction(Action.setDockState(right.getId(), "collapsed"));
    expect(right.getDockState()).toBe("collapsed");

    const sidePanel = getTabByName(model, "Side Panel");
    model.doAction(Action.moveNode(sidePanel.getId(), right.getId(), "center", -1));

    expect(right.getChildren().length).toBe(1);
    expect((right.getChildren()[0] as TabNode).getName()).toBe("Side Panel");
    expect(right.getSelected()).toBe(0);
    expect(right.getDockState()).toBe("expanded");
  });

  it("tab is NOT selected when autoSelectTabWhenOpen is false", () => {
    const model = Model.fromJson(fixtureNoAutoSelect);
    const right = getBorder(model, "right");

    model.doAction(Action.setDockState(right.getId(), "collapsed"));

    const sidePanel = getTabByName(model, "Side Panel");
    model.doAction(Action.moveNode(sidePanel.getId(), right.getId(), "center", -1));

    expect(right.getChildren().length).toBe(1);
    expect(right.getSelected()).toBe(-1);
    expect(right.getDockState()).toBe("expanded");
  });

  it("explicit select=true overrides autoSelectTabWhenOpen: false", () => {
    const model = Model.fromJson(fixtureNoAutoSelect);
    const right = getBorder(model, "right");

    model.doAction(Action.setDockState(right.getId(), "collapsed"));

    const sidePanel = getTabByName(model, "Side Panel");
    model.doAction(Action.moveNode(sidePanel.getId(), right.getId(), "center", -1, true));

    expect(right.getSelected()).toBe(0);
    expect(right.getDockState()).toBe("expanded");
  });

  it("second tab into expanded border with existing selection is auto-selected", () => {
    const model = Model.fromJson(fixtureWithAutoSelect);
    const right = getBorder(model, "right");

    // First: collapse then move — triggers the collapsed→expanded transition fix
    model.doAction(Action.setDockState(right.getId(), "collapsed"));
    const sidePanel = getTabByName(model, "Side Panel");
    model.doAction(Action.moveNode(sidePanel.getId(), right.getId(), "center", -1));

    expect(right.getSelected()).toBe(0);
    expect(right.getDockState()).toBe("expanded");
    expect(right.getChildren().length).toBe(1);

    // Second move: border is expanded AND has selection → willBeOpen=true → auto-selects
    const main = getTabByName(model, "Main");
    model.doAction(Action.moveNode(main.getId(), right.getId(), "center", -1));

    expect(right.getChildren().length).toBe(2);
    expect(right.getSelected()).toBe(1);
    expect((right.getChildren()[1] as TabNode).getName()).toBe("Main");
  });
});
