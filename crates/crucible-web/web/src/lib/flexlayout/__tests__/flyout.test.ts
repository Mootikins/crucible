import { describe, it, expect } from "vitest";
import { withBorders } from "./fixtures";
import { Model } from "../model/Model";
import { Action } from "../model/Action";


describe("flyout > open/close", () => {
  it("open flyout sets flyoutTabId on border", () => {
    const model = Model.fromJson(withBorders);
    const topBorder = model.getBorderSet().getBorder("top")!;
    const tabId = topBorder.getChildren()[0].getId();

    model.doAction(Action.openFlyout(topBorder.getId(), tabId));

    expect(topBorder.getFlyoutTabId()).toBe(tabId);
  });

  it("close flyout clears flyoutTabId", () => {
    const model = Model.fromJson(withBorders);
    const topBorder = model.getBorderSet().getBorder("top")!;
    const tabId = topBorder.getChildren()[0].getId();

    model.doAction(Action.openFlyout(topBorder.getId(), tabId));
    expect(topBorder.getFlyoutTabId()).toBe(tabId);

    model.doAction(Action.closeFlyout(topBorder.getId()));

    expect(topBorder.getFlyoutTabId()).toBeNull();
  });

  it("swap flyout to different tab", () => {
    const model = Model.fromJson(withBorders);
    const bottomBorder = model.getBorderSet().getBorder("bottom")!;
    const tab1Id = bottomBorder.getChildren()[0].getId();
    const tab2Id = bottomBorder.getChildren()[1].getId();

    model.doAction(Action.openFlyout(bottomBorder.getId(), tab1Id));
    expect(bottomBorder.getFlyoutTabId()).toBe(tab1Id);

    model.doAction(Action.openFlyout(bottomBorder.getId(), tab2Id));
    expect(bottomBorder.getFlyoutTabId()).toBe(tab2Id);
  });

  it("flyout on nonexistent border is a no-op", () => {
    const model = Model.fromJson(withBorders);
    model.doAction(Action.openFlyout("nonexistent", "tab1"));
  });
});

describe("flyout > auto-close on expand", () => {
  it("setting dock state to expanded clears flyout", () => {
    const model = Model.fromJson(withBorders);
    const topBorder = model.getBorderSet().getBorder("top")!;
    const tabId = topBorder.getChildren()[0].getId();

    model.doAction(Action.openFlyout(topBorder.getId(), tabId));
    expect(topBorder.getFlyoutTabId()).toBe(tabId);

    model.doAction(Action.setDockState(topBorder.getId(), "expanded"));

    expect(topBorder.getFlyoutTabId()).toBeNull();
  });

  it("setting dock state to collapsed does NOT clear flyout", () => {
    const model = Model.fromJson(withBorders);
    const topBorder = model.getBorderSet().getBorder("top")!;
    const tabId = topBorder.getChildren()[0].getId();

    model.doAction(Action.openFlyout(topBorder.getId(), tabId));
    model.doAction(Action.setDockState(topBorder.getId(), "collapsed"));

    expect(topBorder.getFlyoutTabId()).toBe(tabId);
  });
});

describe("flyout > set size", () => {
  it("SET_FLYOUT_SIZE updates border size", () => {
    const model = Model.fromJson(withBorders);
    const topBorder = model.getBorderSet().getBorder("top")!;

    model.doAction(Action.setFlyoutSize(topBorder.getId(), 300));

    expect(topBorder.getSize()).toBe(300);
  });
});

describe("flyout > serialization", () => {
  it("flyoutTabId survives JSON round-trip", () => {
    const model = Model.fromJson(withBorders);
    const topBorder = model.getBorderSet().getBorder("top")!;
    const tabId = topBorder.getChildren()[0].getId();
    model.doAction(Action.openFlyout(topBorder.getId(), tabId));

    const json = model.toJson();
    const restored = Model.fromJson(json);
    const restoredBorder = restored.getBorderSet().getBorder("top")!;

    expect(restoredBorder.getFlyoutTabId()).toBe(tabId);
  });

  it("null flyoutTabId not written to JSON (backward compat)", () => {
    const model = Model.fromJson(withBorders);
    const json = model.toJson();
    const jsonStr = JSON.stringify(json);
    expect(jsonStr).not.toContain('"flyoutTabId"');
  });

  it("old JSON without flyoutTabId loads with null default", () => {
    const model = Model.fromJson(withBorders);
    const topBorder = model.getBorderSet().getBorder("top")!;
    expect(topBorder.getFlyoutTabId()).toBeNull();
  });
});
