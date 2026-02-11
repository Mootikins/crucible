import { describe, it, expect } from "vitest";
import { Rect } from "../core/Rect";
import { LayoutEngine } from "../layout/LayoutEngine";
import { Model } from "../model/Model";
import { RowNode } from "../model/RowNode";
import { computeNestingOrder } from "../model/Utils";
import type { IJsonModel } from "../types";

describe("LayoutEngine", () => {
    describe("weight distribution", () => {
        it("should distribute equal weights equally", () => {
            const model = Model.fromJson({
                global: { splitterSize: 0 },
                borders: [],
                layout: {
                    type: "row",
                    orientation: "HORZ",
                    weight: 100,
                    children: [
                        { type: "tabset", weight: 100, children: [] },
                        { type: "tabset", weight: 100, children: [] }
                    ]
                }
            });

            const root = model.getRoot()!;
            const ts1 = root.getChildren()[0];
            const ts2 = root.getChildren()[1];

            const containerRect = new Rect(0, 0, 400, 400);

            LayoutEngine.calculateLayout(root, containerRect);

            expect(ts1.getRect().width).toBe(200);
            expect(ts2.getRect().width).toBe(200);
            expect(ts1.getRect().height).toBe(400);
            expect(ts2.getRect().height).toBe(400);
        });

        it("should distribute unequal weights proportionally", () => {
            const model = Model.fromJson({
                global: { splitterSize: 0 },
                borders: [],
                layout: {
                    type: "row",
                    orientation: "HORZ",
                    weight: 100,
                    children: [
                        { type: "tabset", weight: 100, children: [] },
                        { type: "tabset", weight: 200, children: [] }
                    ]
                }
            });

            const root = model.getRoot()!;
            const ts1 = root.getChildren()[0];
            const ts2 = root.getChildren()[1];

            const containerRect = new Rect(0, 0, 300, 400);
            root.setRect(containerRect);

            LayoutEngine.calculateLayout(root, containerRect);

            expect(ts1.getRect().width).toBeCloseTo(100, 0);
            expect(ts2.getRect().width).toBeCloseTo(200, 0);
        });

        it("should respect min/max constraints", () => {
            const model = Model.fromJson({
                global: { splitterSize: 0 },
                borders: [],
                layout: {
                    type: "row",
                    orientation: "HORZ",
                    weight: 100,
                    children: [
                        { type: "tabset", weight: 100, minWidth: 150, children: [] },
                        { type: "tabset", weight: 100, minWidth: 150, children: [] }
                    ]
                }
            });

            const root = model.getRoot()!;
            const ts1 = root.getChildren()[0];
            const ts2 = root.getChildren()[1];

            const containerRect = new Rect(0, 0, 400, 400);
            root.setRect(containerRect);

            LayoutEngine.calculateLayout(root, containerRect);

            expect(ts1.getRect().width).toBeGreaterThanOrEqual(150);
            expect(ts2.getRect().width).toBeGreaterThanOrEqual(150);
        });

        it("should handle vertical orientation", () => {
            const model = Model.fromJson({
                global: { rootOrientationVertical: true, splitterSize: 0 },
                borders: [],
                layout: {
                    type: "row",
                    weight: 100,
                    children: [
                        { type: "tabset", weight: 100, children: [] },
                        { type: "tabset", weight: 100, children: [] }
                    ]
                }
            });

            const root = model.getRoot()!;
            const ts1 = root.getChildren()[0];
            const ts2 = root.getChildren()[1];

            const containerRect = new Rect(0, 0, 400, 400);
            root.setRect(containerRect);

            LayoutEngine.calculateLayout(root, containerRect);

            expect(ts1.getRect().height).toBe(200);
            expect(ts2.getRect().height).toBe(200);
            expect(ts1.getRect().width).toBe(400);
            expect(ts2.getRect().width).toBe(400);
        });

        it("should handle nested rows", () => {
            const model = Model.fromJson({
                global: { splitterSize: 0 },
                borders: [],
                layout: {
                    type: "row",
                    weight: 100,
                    children: [
                        { type: "tabset", weight: 100, children: [] },
                        {
                            type: "row",
                            weight: 100,
                            children: [
                                { type: "tabset", weight: 100, children: [] },
                                { type: "tabset", weight: 100, children: [] }
                            ]
                        }
                    ]
                }
            });

            const root = model.getRoot()!;
            const ts1 = root.getChildren()[0];
            const row2 = root.getChildren()[1] as RowNode;
            const ts2 = row2.getChildren()[0];
            const ts3 = row2.getChildren()[1];

            const containerRect = new Rect(0, 0, 400, 400);
            root.setRect(containerRect);

            LayoutEngine.calculateLayout(root, containerRect);

            expect(ts1.getRect().width).toBe(200);
            expect(row2.getRect().width).toBe(200);
            expect(ts2.getRect().height).toBe(200);
            expect(ts3.getRect().height).toBe(200);
        });

        it("should handle maximize state", () => {
            const model = Model.fromJson({
                global: { splitterSize: 0 },
                borders: [],
                layout: {
                    type: "row",
                    orientation: "HORZ",
                    weight: 100,
                    children: [
                        { type: "tabset", weight: 100, children: [] },
                        { type: "tabset", weight: 100, children: [] }
                    ]
                }
            });

            const root = model.getRoot()!;
            const ts1 = root.getChildren()[0];
            const ts2 = root.getChildren()[1];

            model.setMaximizedTabset(ts1 as any, Model.MAIN_WINDOW_ID);

            const containerRect = new Rect(0, 0, 400, 400);
            root.setRect(containerRect);

            LayoutEngine.calculateLayout(root, containerRect);

            expect(ts1.getRect().width).toBe(400);
            expect(ts1.getRect().height).toBe(400);
            expect(ts2.getRect().width).toBe(0);
        });
    });

    describe("rect positioning", () => {
        it("should position rects correctly in horizontal layout", () => {
            const model = Model.fromJson({
                global: { splitterSize: 0 },
                borders: [],
                layout: {
                    type: "row",
                    orientation: "HORZ",
                    weight: 100,
                    children: [
                        { type: "tabset", weight: 100, children: [] },
                        { type: "tabset", weight: 100, children: [] }
                    ]
                }
            });

            const root = model.getRoot()!;
            const ts1 = root.getChildren()[0];
            const ts2 = root.getChildren()[1];

            const containerRect = new Rect(10, 20, 400, 300);
            root.setRect(containerRect);

            LayoutEngine.calculateLayout(root, containerRect);

            expect(ts1.getRect().x).toBe(10);
            expect(ts1.getRect().y).toBe(20);
            expect(ts2.getRect().x).toBe(210);
            expect(ts2.getRect().y).toBe(20);
        });

        it("should position rects correctly in vertical layout", () => {
            const model = Model.fromJson({
                global: { rootOrientationVertical: true, splitterSize: 0 },
                borders: [],
                layout: {
                    type: "row",
                    weight: 100,
                    children: [
                        { type: "tabset", weight: 100, children: [] },
                        { type: "tabset", weight: 100, children: [] }
                    ]
                }
            });

            const root = model.getRoot()!;
            const ts1 = root.getChildren()[0];
            const ts2 = root.getChildren()[1];

            const containerRect = new Rect(10, 20, 400, 300);
            root.setRect(containerRect);

            LayoutEngine.calculateLayout(root, containerRect);

            expect(ts1.getRect().y).toBe(20);
            expect(ts1.getRect().x).toBe(10);
            expect(ts2.getRect().y).toBe(170);
            expect(ts2.getRect().x).toBe(10);
        });
    });

    describe("splitter size", () => {
        it("should use default splitter size of 4 when not specified", () => {
            const model = Model.fromJson({
                global: {},
                borders: [],
                layout: {
                    type: "row",
                    orientation: "HORZ",
                    weight: 100,
                    children: [
                        { type: "tabset", weight: 100, children: [] },
                        { type: "tabset", weight: 100, children: [] }
                    ]
                }
            });

            expect(model.getSplitterSize()).toBe(4);

            const root = model.getRoot()!;
            const ts1 = root.getChildren()[0];
            const ts2 = root.getChildren()[1];

            const containerRect = new Rect(0, 0, 400, 400);
            LayoutEngine.calculateLayout(root, containerRect);

            // 400 - 4 (one splitter) = 396 / 2 = 198 each
            expect(ts1.getRect().width).toBe(198);
            expect(ts2.getRect().width).toBe(198);
        });

        it("should respect custom splitter size", () => {
            const model = Model.fromJson({
                global: { splitterSize: 10 },
                borders: [],
                layout: {
                    type: "row",
                    orientation: "HORZ",
                    weight: 100,
                    children: [
                        { type: "tabset", weight: 100, children: [] },
                        { type: "tabset", weight: 100, children: [] }
                    ]
                }
            });

            expect(model.getSplitterSize()).toBe(10);

            const root = model.getRoot()!;
            const ts1 = root.getChildren()[0];
            const ts2 = root.getChildren()[1];

            const containerRect = new Rect(0, 0, 400, 400);
            LayoutEngine.calculateLayout(root, containerRect);

            // 400 - 10 (one splitter) = 390 / 2 = 195 each
            expect(ts1.getRect().width).toBe(195);
            expect(ts2.getRect().width).toBe(195);
        });
    });

    describe("computeNestingOrder", () => {
        const makeModel = (borders: IJsonModel["borders"]): Model =>
            new Model({
                global: { splitterSize: 0 },
                borders: borders ?? [],
                layout: {
                    type: "row",
                    weight: 100,
                    children: [{ type: "tabset", weight: 100, children: [{ type: "tab", name: "Main", component: "text" }] }],
                },
            });

        it("should return empty array for no borders", () => {
            const result = computeNestingOrder([]);
            expect(result).toEqual([]);
        });

        it("should return single border as-is", () => {
            const model = makeModel([
                { type: "border", location: "left", selected: 0, priority: 1, children: [{ type: "tab", name: "L", component: "text" }] },
            ]);
            const borders = model.getBorderSet().getBorders();
            const result = computeNestingOrder(borders);
            expect(result.length).toBe(1);
            expect(result[0].getLocation().getName()).toBe("left");
        });

        it("should sort by priority descending (highest = outermost first)", () => {
            const model = makeModel([
                { type: "border", location: "top", selected: 0, priority: 0, children: [{ type: "tab", name: "T", component: "text" }] },
                { type: "border", location: "bottom", selected: 0, priority: 0, children: [{ type: "tab", name: "B", component: "text" }] },
                { type: "border", location: "left", selected: 0, priority: 1, children: [{ type: "tab", name: "L", component: "text" }] },
                { type: "border", location: "right", selected: 0, priority: 1, children: [{ type: "tab", name: "R", component: "text" }] },
            ]);
            const borders = model.getBorderSet().getBorders();
            const result = computeNestingOrder(borders);

            // left(1) and right(1) before top(0) and bottom(0)
            expect(result[0].getPriority()).toBeGreaterThanOrEqual(result[1].getPriority());
            expect(result[1].getPriority()).toBeGreaterThanOrEqual(result[2].getPriority());
            expect(result[2].getPriority()).toBeGreaterThanOrEqual(result[3].getPriority());
        });

        it("should break ties by location order [top, bottom, left, right]", () => {
            const model = makeModel([
                { type: "border", location: "top", selected: 0, priority: 1, children: [{ type: "tab", name: "T", component: "text" }] },
                { type: "border", location: "bottom", selected: 0, priority: 1, children: [{ type: "tab", name: "B", component: "text" }] },
                { type: "border", location: "left", selected: 0, priority: 1, children: [{ type: "tab", name: "L", component: "text" }] },
                { type: "border", location: "right", selected: 0, priority: 1, children: [{ type: "tab", name: "R", component: "text" }] },
            ]);
            const borders = model.getBorderSet().getBorders();
            const result = computeNestingOrder(borders);

            expect(result[0].getLocation().getName()).toBe("top");
            expect(result[1].getLocation().getName()).toBe("bottom");
            expect(result[2].getLocation().getName()).toBe("left");
            expect(result[3].getLocation().getName()).toBe("right");
        });

        it("default priorities (left=1, right=1, top=0, bottom=0) preserve left/right outer nesting", () => {
            const model = makeModel([
                { type: "border", location: "top", selected: 0, priority: 0, children: [{ type: "tab", name: "T", component: "text" }] },
                { type: "border", location: "bottom", selected: 0, priority: 0, children: [{ type: "tab", name: "B", component: "text" }] },
                { type: "border", location: "left", selected: 0, priority: 1, children: [{ type: "tab", name: "L", component: "text" }] },
                { type: "border", location: "right", selected: 0, priority: 1, children: [{ type: "tab", name: "R", component: "text" }] },
            ]);
            const borders = model.getBorderSet().getBorders();
            const result = computeNestingOrder(borders);

            // left/right (priority 1) should be outermost
            const names = result.map(b => b.getLocation().getName());
            const leftIdx = names.indexOf("left");
            const rightIdx = names.indexOf("right");
            const topIdx = names.indexOf("top");
            const bottomIdx = names.indexOf("bottom");

            // left and right should come before top and bottom
            expect(leftIdx).toBeLessThan(topIdx);
            expect(leftIdx).toBeLessThan(bottomIdx);
            expect(rightIdx).toBeLessThan(topIdx);
            expect(rightIdx).toBeLessThan(bottomIdx);
        });

        it("swapped priorities (top=2, bottom=2) put horizontal borders outermost", () => {
            const model = makeModel([
                { type: "border", location: "top", selected: 0, priority: 2, children: [{ type: "tab", name: "T", component: "text" }] },
                { type: "border", location: "bottom", selected: 0, priority: 2, children: [{ type: "tab", name: "B", component: "text" }] },
                { type: "border", location: "left", selected: 0, priority: 1, children: [{ type: "tab", name: "L", component: "text" }] },
                { type: "border", location: "right", selected: 0, priority: 1, children: [{ type: "tab", name: "R", component: "text" }] },
            ]);
            const borders = model.getBorderSet().getBorders();
            const result = computeNestingOrder(borders);

            // top/bottom (priority 2) should be outermost
            const names = result.map(b => b.getLocation().getName());
            expect(names.indexOf("top")).toBeLessThan(names.indexOf("left"));
            expect(names.indexOf("top")).toBeLessThan(names.indexOf("right"));
            expect(names.indexOf("bottom")).toBeLessThan(names.indexOf("left"));
            expect(names.indexOf("bottom")).toBeLessThan(names.indexOf("right"));
        });

        it("mixed priorities: unique ordering per border", () => {
            const model = makeModel([
                { type: "border", location: "top", selected: 0, priority: 2, children: [{ type: "tab", name: "T", component: "text" }] },
                { type: "border", location: "bottom", selected: 0, priority: 0, children: [{ type: "tab", name: "B", component: "text" }] },
                { type: "border", location: "left", selected: 0, priority: 3, children: [{ type: "tab", name: "L", component: "text" }] },
                { type: "border", location: "right", selected: 0, priority: 1, children: [{ type: "tab", name: "R", component: "text" }] },
            ]);
            const borders = model.getBorderSet().getBorders();
            const result = computeNestingOrder(borders);

            expect(result[0].getLocation().getName()).toBe("left");   // priority 3
            expect(result[1].getLocation().getName()).toBe("top");    // priority 2
            expect(result[2].getLocation().getName()).toBe("right");  // priority 1
            expect(result[3].getLocation().getName()).toBe("bottom"); // priority 0
        });

        it("should not mutate the input array", () => {
            const model = makeModel([
                { type: "border", location: "top", selected: 0, priority: 0, children: [{ type: "tab", name: "T", component: "text" }] },
                { type: "border", location: "left", selected: 0, priority: 1, children: [{ type: "tab", name: "L", component: "text" }] },
            ]);
            const borders = model.getBorderSet().getBorders();
            const originalOrder = borders.map(b => b.getLocation().getName());
            computeNestingOrder(borders);
            const afterOrder = borders.map(b => b.getLocation().getName());
            expect(afterOrder).toEqual(originalOrder);
        });
    });

    describe("splitter adjustment", () => {
        it("should recalculate weights after splitter drag", () => {
            const model = Model.fromJson({
                global: { splitterSize: 0 },
                borders: [],
                layout: {
                    type: "row",
                    weight: 100,
                    children: [
                        { type: "tabset", weight: 100, children: [] },
                        { type: "tabset", weight: 100, children: [] }
                    ]
                }
            });

            const root = model.getRoot()! as RowNode;
            const ts1 = root.getChildren()[0];
            const ts2 = root.getChildren()[1];

            const containerRect = new Rect(0, 0, 400, 400);
            root.setRect(containerRect);

            LayoutEngine.calculateLayout(root, containerRect);

            expect(ts1.getRect().width).toBe(200);
            expect(ts2.getRect().width).toBe(200);

            const initialSizes = [200, 200];
            const weights = root.calculateSplit(1, 300, initialSizes, 400, 200);

            expect(weights[0]).toBeCloseTo(75, 0);
            expect(weights[1]).toBeCloseTo(25, 0);
        });
    });
});
