import { describe, it, expect } from "vitest";
import { Rect } from "../core/Rect";
import { LayoutEngine } from "../layout/LayoutEngine";
import { Model } from "../model/Model";
import { RowNode } from "../model/RowNode";

describe("LayoutEngine", () => {
    describe("weight distribution", () => {
        it("should distribute equal weights equally", () => {
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
                global: {},
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
                global: {},
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
                global: { rootOrientationVertical: true },
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
                global: {},
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
                global: { rootOrientationVertical: true },
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

    describe("splitter adjustment", () => {
        it("should recalculate weights after splitter drag", () => {
            const model = Model.fromJson({
                global: {},
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
