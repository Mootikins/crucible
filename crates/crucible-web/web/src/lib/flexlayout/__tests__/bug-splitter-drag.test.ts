import { describe, it, expect } from "vitest";
import { Model } from "../model/Model";
import { Action } from "../model/Action";
import { BorderNode } from "../model/BorderNode";
import type { IJsonModel } from "../types";

const splitterDragFixture: IJsonModel = {
    global: {
        splitterSize: 4,
        borderEnableDock: true,
    },
    borders: [
        {
            type: "border",
            location: "left",
            selected: 0,
            size: 200,
            children: [
                { type: "tab", name: "Explorer", component: "text" },
            ],
        },
        {
            type: "border",
            location: "bottom",
            selected: 0,
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

describe("Bug #1: Splitter drag indicator vs applied position", () => {
    it.todo("border edge splitter bounds should be in layout-relative coordinates");

    it.todo("adjustBorderSplit with position X should resize border to match position X");

    it.todo("row splitter and border splitter use same coordinate system for outline placement");

    it("calculateSplit produces consistent round-trip values", () => {
        const model = Model.fromJson(splitterDragFixture);
        const leftBorder = getBorder(model, "left");

        expect(leftBorder.getDockState()).toBe("expanded");
        expect(leftBorder.getSelected()).toBe(0);
        expect(leftBorder.getSize()).toBe(200);

        const newSize = leftBorder.calculateSplit(leftBorder, 250);
        model.doAction(Action.adjustBorderSplit(leftBorder.getId(), newSize));

        const updatedSize = leftBorder.getSize();
        expect(updatedSize).toBe(newSize);
    });

    it.todo("splitter bounds are valid for both horizontal and vertical borders (requires DOM layout)");
});
