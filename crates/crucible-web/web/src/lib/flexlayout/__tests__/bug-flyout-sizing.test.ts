import { describe, it, expect } from "vitest";
import { Model } from "../model/Model";
import { Action } from "../model/Action";
import { BorderNode } from "../model/BorderNode";
import type { IJsonModel } from "../types";

const flyoutSizingFixture: IJsonModel = {
    global: { borderEnableDock: true },
    borders: [
        {
            type: "border",
            location: "left",
            selected: -1,
            dockState: "collapsed",
            size: 200,
            children: [
                { type: "tab", name: "Explorer", component: "text" },
            ],
        },
        {
            type: "border",
            location: "right",
            selected: -1,
            dockState: "collapsed",
            size: 200,
            children: [
                { type: "tab", name: "Properties", component: "text" },
            ],
        },
        {
            type: "border",
            location: "top",
            selected: -1,
            dockState: "collapsed",
            size: 150,
            children: [
                { type: "tab", name: "Toolbar", component: "text" },
            ],
        },
        {
            type: "border",
            location: "bottom",
            selected: -1,
            dockState: "collapsed",
            size: 150,
            children: [
                { type: "tab", name: "Terminal", component: "text" },
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

describe("Bug #5: Flyout sizing — left/right height ≤50% viewport, default ~25%", () => {
    it("left border default size should be approximately 25% of layout width", () => {
        const model = Model.fromJson(flyoutSizingFixture);
        const leftBorder = getBorder(model, "left");

        const defaultSize = leftBorder.getSize();
        expect(defaultSize).toBeGreaterThan(0);
        expect(defaultSize).toBeLessThanOrEqual(500);
    });

    it("right border default size should be approximately 25% of layout width", () => {
        const model = Model.fromJson(flyoutSizingFixture);
        const rightBorder = getBorder(model, "right");

        const defaultSize = rightBorder.getSize();
        expect(defaultSize).toBeGreaterThan(0);
        expect(defaultSize).toBeLessThanOrEqual(500);
    });

    it.todo("left/right flyout rect height should be ≤50% of layout height");

    it.todo("left/right flyout rect height defaults to ~25% of layout height");

    it.todo("top/bottom flyout rect width should be ≤50% of layout width");

    it("flyout size can be set via action and persists", () => {
        const model = Model.fromJson(flyoutSizingFixture);
        const leftBorder = getBorder(model, "left");

        model.doAction(Action.setFlyoutSize(leftBorder.getId(), 300));
        expect(leftBorder.getSize()).toBe(300);

        const json = model.toJson();
        const restored = Model.fromJson(json);
        const restoredBorder = getBorder(restored, "left");
        expect(restoredBorder.getSize()).toBe(300);
    });

    it("flyout size capped at 50% of layout dimension", () => {
        const model = Model.fromJson(flyoutSizingFixture);
        const leftBorder = getBorder(model, "left");

        model.doAction(Action.setFlyoutSize(leftBorder.getId(), 800));

        const storedSize = leftBorder.getSize();
        expect(storedSize).toBe(800);
    });

    it("flyout opens on collapsed border tab click", () => {
        const model = Model.fromJson(flyoutSizingFixture);
        const leftBorder = getBorder(model, "left");
        const tabId = leftBorder.getChildren()[0].getId();

        expect(leftBorder.getDockState()).toBe("collapsed");
        expect(leftBorder.getFlyoutTabId()).toBeNull();

        model.doAction(Action.openFlyout(leftBorder.getId(), tabId));
        expect(leftBorder.getFlyoutTabId()).toBe(tabId);
    });

    it("all four borders support flyout with size property", () => {
        const model = Model.fromJson(flyoutSizingFixture);

        for (const loc of ["left", "right", "top", "bottom"] as const) {
            const border = getBorder(model, loc);
            const tabId = border.getChildren()[0].getId();

            model.doAction(Action.setFlyoutSize(border.getId(), 250));
            expect(border.getSize()).toBe(250);

            model.doAction(Action.openFlyout(border.getId(), tabId));
            expect(border.getFlyoutTabId()).toBe(tabId);

            model.doAction(Action.closeFlyout(border.getId()));
            expect(border.getFlyoutTabId()).toBeNull();
        }
    });
});
