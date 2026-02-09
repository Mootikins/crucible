import { describe, it, expect } from "vitest";
import { DragState, DragSource } from "../dnd/DragState";
import { DockLocation } from "../core/DockLocation";
import { Rect } from "../core/Rect";
import { Model } from "../model/Model";
import { Action } from "../model/Action";
import { BorderNode } from "../model/BorderNode";
import { TabNode } from "../model/TabNode";
import type { IJsonModel, IJsonTabNode } from "../types";

const dndFixture: IJsonModel = {
  global: { borderEnableDock: true },
  borders: [
    {
      type: "border",
      location: "bottom",
      selected: 0,
      dockState: "expanded",
      children: [
        { type: "tab", name: "Terminal", component: "text" },
        { type: "tab", name: "Output", component: "text" },
        { type: "tab", name: "Problems", component: "text" },
      ],
    },
    {
      type: "border",
      location: "left",
      selected: 0,
      dockState: "expanded",
      children: [
        { type: "tab", name: "Explorer", component: "text" },
        { type: "tab", name: "Search", component: "text" },
      ],
    },
    {
      type: "border",
      location: "right",
      selected: 0,
      dockState: "expanded",
      children: [
        { type: "tab", name: "Properties", component: "text" },
      ],
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

function getBorder(model: Model, location: string): BorderNode {
  const border = model.getBorderSet().getBorder(location as any);
  if (!border) throw new Error(`No border at ${location}`);
  return border;
}

describe("DragState", () => {
  it("should create drag state for internal drag", () => {
    const dragJson: IJsonTabNode = { type: "tab", name: "Tab1", component: "test" };
    const dragState = new DragState(
      DragSource.Internal,
      undefined,
      dragJson,
      undefined
    );

    expect(dragState.dragSource).toBe(DragSource.Internal);
    expect(dragState.dragJson).toBe(dragJson);
    expect(dragState.dragNode).toBeUndefined();
    expect(dragState.fnNewNodeDropped).toBeUndefined();
  });

  it("should create drag state for external drag", () => {
    const dragJson: IJsonTabNode = { type: "tab", name: "ExternalTab", component: "test" };
    const dragState = new DragState(
      DragSource.External,
      undefined,
      dragJson,
      undefined
    );

    expect(dragState.dragSource).toBe(DragSource.External);
    expect(dragState.dragNode).toBeUndefined();
    expect(dragState.dragJson).toBe(dragJson);
  });

  it("should create drag state for add drag", () => {
    const dragJson: IJsonTabNode = { type: "tab", name: "NewTab", component: "test" };
    const onDrop = () => {};
    const dragState = new DragState(
      DragSource.Add,
      undefined,
      dragJson,
      onDrop
    );

    expect(dragState.dragSource).toBe(DragSource.Add);
    expect(dragState.dragJson).toBe(dragJson);
    expect(dragState.fnNewNodeDropped).toBe(onDrop);
  });

  it("should handle drag state with callback", () => {
    let callbackCalled = false;
    const onDrop = () => {
      callbackCalled = true;
    };

    const dragState = new DragState(
      DragSource.Internal,
      undefined,
      undefined,
      onDrop
    );

    expect(dragState.fnNewNodeDropped).toBe(onDrop);
    dragState.fnNewNodeDropped?.(undefined, undefined);
    expect(callbackCalled).toBe(true);
  });

  it("should support all drag sources", () => {
    const sources = [DragSource.Internal, DragSource.External, DragSource.Add];
    
    sources.forEach(source => {
      const dragState = new DragState(source, undefined, undefined, undefined);
      expect(dragState.dragSource).toBe(source);
    });
  });
});

describe("BorderNode content-area canDrop", () => {
  function setupModelWithContentRect(location: string, contentRect: Rect): { model: Model; border: BorderNode } {
    const model = Model.fromJson(dndFixture);
    const border = getBorder(model, location);
    border.setTabHeaderRect(new Rect(0, 0, 100, 30));
    border.setContentRect(contentRect);
    return { model, border };
  }

  it("bottom border: left half of content area returns LEFT location", () => {
    const { border } = setupModelWithContentRect("bottom", new Rect(100, 400, 800, 200));
    const dragTab = border.getChildren()[1] as TabNode;
    const dropInfo = border.canDrop(dragTab, 300, 500);

    expect(dropInfo).toBeDefined();
    expect(dropInfo!.location).toBe(DockLocation.LEFT);
    expect(dropInfo!.rect.width).toBe(400);
    expect(dropInfo!.rect.x).toBe(100);
  });

  it("bottom border: right half of content area returns RIGHT location", () => {
    const { border } = setupModelWithContentRect("bottom", new Rect(100, 400, 800, 200));
    const dragTab = border.getChildren()[1] as TabNode;
    const dropInfo = border.canDrop(dragTab, 600, 500);

    expect(dropInfo).toBeDefined();
    expect(dropInfo!.location).toBe(DockLocation.RIGHT);
    expect(dropInfo!.rect.x).toBe(500);
    expect(dropInfo!.rect.width).toBe(400);
  });

  it("left border: top half of content area returns TOP location", () => {
    const { border } = setupModelWithContentRect("left", new Rect(0, 100, 200, 600));
    const dragTab = border.getChildren()[1] as TabNode;
    const dropInfo = border.canDrop(dragTab, 100, 250);

    expect(dropInfo).toBeDefined();
    expect(dropInfo!.location).toBe(DockLocation.TOP);
    expect(dropInfo!.rect.height).toBe(300);
  });

  it("left border: bottom half of content area returns BOTTOM location", () => {
    const { border } = setupModelWithContentRect("left", new Rect(0, 100, 200, 600));
    const dragTab = border.getChildren()[1] as TabNode;
    const dropInfo = border.canDrop(dragTab, 100, 550);

    expect(dropInfo).toBeDefined();
    expect(dropInfo!.location).toBe(DockLocation.BOTTOM);
    expect(dropInfo!.rect.y).toBe(400);
    expect(dropInfo!.rect.height).toBe(300);
  });

  it("content-area drop returns index -1", () => {
    const { border } = setupModelWithContentRect("bottom", new Rect(100, 400, 800, 200));
    const dragTab = border.getChildren()[1] as TabNode;
    const dropInfo = border.canDrop(dragTab, 300, 500);

    expect(dropInfo).toBeDefined();
    expect(dropInfo!.index).toBe(-1);
  });

  it("returns undefined when no tab is selected (border closed)", () => {
    const model = Model.fromJson(dndFixture);
    const border = getBorder(model, "bottom");
    border.setSelected(-1);
    border.setTabHeaderRect(new Rect(0, 0, 100, 30));
    border.setContentRect(new Rect(100, 400, 800, 200));
    const dragTab = border.getChildren()[1] as TabNode;

    const dropInfo = border.canDrop(dragTab, 300, 500);
    expect(dropInfo).toBeUndefined();
  });

  it("returns undefined for empty content rect", () => {
    const { border } = setupModelWithContentRect("bottom", Rect.empty());
    const dragTab = border.getChildren()[1] as TabNode;
    const dropInfo = border.canDrop(dragTab, 500, 500);
    expect(dropInfo).toBeUndefined();
  });
});

describe("BorderNode tiling drop", () => {
  it("drop with LEFT location tiles dragged tab first", () => {
    const model = Model.fromJson(dndFixture);
    const border = getBorder(model, "bottom");
    expect(border.getSelected()).toBe(0);

    const outputTab = border.getChildren()[1] as TabNode;
    model.doAction(
      Action.moveNode(outputTab.getId(), border.getId(), "left", -1)
    );

    const vis = border.getVisibleTabs();
    expect(vis.length).toBe(2);
    const names = vis.map((i) => (border.getChildren()[i] as TabNode).getName());
    expect(names[0]).toBe("Output");
    expect(names[1]).toBe("Terminal");
  });

  it("drop with RIGHT location tiles dragged tab second", () => {
    const model = Model.fromJson(dndFixture);
    const border = getBorder(model, "bottom");
    expect(border.getSelected()).toBe(0);

    const outputTab = border.getChildren()[1] as TabNode;
    model.doAction(
      Action.moveNode(outputTab.getId(), border.getId(), "right", -1)
    );

    const vis = border.getVisibleTabs();
    expect(vis.length).toBe(2);
    const names = vis.map((i) => (border.getChildren()[i] as TabNode).getName());
    expect(names[0]).toBe("Terminal");
    expect(names[1]).toBe("Output");
  });

  it("drop with TOP location tiles dragged tab first (left border)", () => {
    const model = Model.fromJson(dndFixture);
    const border = getBorder(model, "left");
    expect(border.getSelected()).toBe(0);

    const searchTab = border.getChildren()[1] as TabNode;
    model.doAction(
      Action.moveNode(searchTab.getId(), border.getId(), "top", -1)
    );

    const vis = border.getVisibleTabs();
    expect(vis.length).toBe(2);
    const names = vis.map((i) => (border.getChildren()[i] as TabNode).getName());
    expect(names[0]).toBe("Search");
    expect(names[1]).toBe("Explorer");
  });

  it("drop with BOTTOM location tiles dragged tab second (left border)", () => {
    const model = Model.fromJson(dndFixture);
    const border = getBorder(model, "left");

    const searchTab = border.getChildren()[1] as TabNode;
    model.doAction(
      Action.moveNode(searchTab.getId(), border.getId(), "bottom", -1)
    );

    const vis = border.getVisibleTabs();
    expect(vis.length).toBe(2);
    const names = vis.map((i) => (border.getChildren()[i] as TabNode).getName());
    expect(names[0]).toBe("Explorer");
    expect(names[1]).toBe("Search");
  });

  it("cross-border tiling: move tab from one border to another + tile", () => {
    const model = Model.fromJson(dndFixture);
    const bottom = getBorder(model, "bottom");
    const rightBorder = getBorder(model, "right");

    const propertiesTab = rightBorder.getChildren()[0] as TabNode;
    model.doAction(
      Action.moveNode(propertiesTab.getId(), bottom.getId(), "right", -1)
    );

    expect(rightBorder.getChildren().length).toBe(0);

    const vis = bottom.getVisibleTabs();
    expect(vis.length).toBe(2);
    const names = vis.map((i) => (bottom.getChildren()[i] as TabNode).getName());
    expect(names[0]).toBe("Terminal");
    expect(names[1]).toBe("Properties");
  });

  it("CENTER location drop does not trigger tiling", () => {
    const model = Model.fromJson(dndFixture);
    const border = getBorder(model, "bottom");

    const outputTab = border.getChildren()[1] as TabNode;
    model.doAction(
      Action.moveNode(outputTab.getId(), border.getId(), "center", -1)
    );

    expect(border.getVisibleTabs()).toEqual([]);
  });

  it("regular drop (positive index) does not trigger tiling", () => {
    const model = Model.fromJson(dndFixture);
    const border = getBorder(model, "bottom");

    const outputTab = border.getChildren()[1] as TabNode;
    model.doAction(
      Action.moveNode(outputTab.getId(), border.getId(), "left", 0)
    );

    expect(border.getVisibleTabs()).toEqual([]);
  });
});
