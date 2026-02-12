/**
 * FlexLayout Upstream Model Tests
 *
 * Adapted from caplin/FlexLayout/tests/Model.test.ts
 * Tests the TypeScript model implementation against upstream assertions
 */

import { describe, it, expect, beforeEach } from "vitest";
import { Action, Model, Node, TabNode, TabSetNode, BorderNode, RowNode } from "../model";
import { DockLocation } from "../core/DockLocation";
import type { IJsonModel } from "../types";

/*
* The textRendered tabs: a representation of the model 'rendered' to a list of tab paths
* where /ts0/t1[One]* is tab index 1 in tabset 0 of the root row with name=One and its selected (ie. path + tabname and selected indicator))
*/
let tabsArray: string[] = []; // the rendered tabs as an array
let tabs = ""; // the rendered tabs array as a comma separated string
let pathMap: Record<string, Node> = {}; // maps tab path (e.g /ts1/t0) to the actual Node

let model: Model;

describe("Tree", function () {

    describe("Actions", () => {

        describe("Add", () => {

            it("empty tabset", function () {
                model = Model.fromJson(
                    {
                        global: {},
                        layout: {
                            type: "row",
                            children: [
                                {
                                    type: "tabset",
                                    id: "1",
                                    enableDeleteWhenEmpty: false,
                                    children: []
                                }
                            ]
                        }
                    }
                );

                doAction(Action.addNode({ id: "2", name: "newtab1", component: "grid" }, "1", DockLocation.CENTER.getName(), -1));

                expect(tabs).toBe("/ts0/t0[newtab1]*");
                expect(tab("/ts0/t0").getId()).toBe("2");
                expect(tab("/ts0/t0").getComponent()).toBe("grid");
            });

            describe("tabsets", () => {
                beforeEach(() => {
                    model = Model.fromJson(twoTabs);
                    textRender(model);
                    // two tabsets in a row, each with a single tab will textRender as:
                    expect(tabs).toBe("/ts0/t0[One]*,/ts1/t0[Two]*");
                });

                it("add to tabset center", () => {
                    const id0 = tabset("/ts0").getId();
                    doAction(Action.addNode({ name: "newtab1", component: "grid" }, id0, DockLocation.CENTER.getName(), -1));

                    expect(tabs).toBe("/ts0/t0[One],/ts0/t1[newtab1]*,/ts1/t0[Two]*");

                    const id1 = tabset("/ts1").getId();
                    doAction(Action.addNode({ name: "newtab2", component: "grid" }, id1, DockLocation.CENTER.getName(), -1));

                    expect(tabs).toBe("/ts0/t0[One],/ts0/t1[newtab1]*,/ts1/t0[Two],/ts1/t1[newtab2]*");
                });

                it("add to tabset at position", () => {
                    const id0 = tabset("/ts0").getId();
                    doAction(Action.addNode({ name: "newtab1", component: "grid" }, id0, DockLocation.CENTER.getName(), 0));

                    expect(tabs).toBe("/ts0/t0[newtab1]*,/ts0/t1[One],/ts1/t0[Two]*");

                    doAction(Action.addNode({ name: "newtab2", component: "grid" }, id0, DockLocation.CENTER.getName(), 1));

                    expect(tabs).toBe("/ts0/t0[newtab1],/ts0/t1[newtab2]*,/ts0/t2[One],/ts1/t0[Two]*");

                    doAction(Action.addNode({ name: "newtab3", component: "grid" }, id0, DockLocation.CENTER.getName(), 3));

                    expect(tabs).toBe("/ts0/t0[newtab1],/ts0/t1[newtab2],/ts0/t2[One],/ts0/t3[newtab3]*,/ts1/t0[Two]*");
                });

                it("add to tabset top", () => {
                    const id0 = tabset("/ts0").getId();
                    doAction(Action.addNode({ name: "newtab1", component: "grid" }, id0, DockLocation.TOP.getName(), -1));

                    expect(tabs).toBe("/r0/ts0/t0[newtab1]*,/r0/ts1/t0[One]*,/ts1/t0[Two]*");

                    const id1 = tabset("/ts1").getId();
                    doAction(Action.addNode({ name: "newtab2", component: "grid" }, id1, DockLocation.TOP.getName(), -1));

                    expect(tabs).toBe("/r0/ts0/t0[newtab1]*,/r0/ts1/t0[One]*,/r1/ts0/t0[newtab2]*,/r1/ts1/t0[Two]*");
                });

                it("add to tabset bottom", () => {
                    const id0 = tabset("/ts0").getId();
                    doAction(Action.addNode({ name: "newtab1", component: "grid" }, id0, DockLocation.BOTTOM.getName(), -1));

                    expect(tabs).toBe("/r0/ts0/t0[One]*,/r0/ts1/t0[newtab1]*,/ts1/t0[Two]*");

                    const id1 = tabset("/ts1").getId();
                    doAction(Action.addNode({ name: "newtab2", component: "grid" }, id1, DockLocation.BOTTOM.getName(), -1));

                    expect(tabs).toBe("/r0/ts0/t0[One]*,/r0/ts1/t0[newtab1]*,/r1/ts0/t0[Two]*,/r1/ts1/t0[newtab2]*");
                });

                it("add to tabset left", () => {
                    const id0 = tabset("/ts0").getId();
                    doAction(Action.addNode({ name: "newtab1", component: "grid" }, id0, DockLocation.LEFT.getName(), -1));

                    expect(tabs).toBe("/ts0/t0[newtab1]*,/ts1/t0[One]*,/ts2/t0[Two]*");

                    const id1 = tabset("/ts2").getId();
                    doAction(Action.addNode({ name: "newtab2", component: "grid" }, id1, DockLocation.LEFT.getName(), -1));

                    expect(tabs).toBe("/ts0/t0[newtab1]*,/ts1/t0[One]*,/ts2/t0[newtab2]*,/ts3/t0[Two]*");
                });

                it("add to tabset right", () => {
                    const id0 = tabset("/ts0").getId();
                    doAction(Action.addNode({ name: "newtab1", component: "grid" }, id0, DockLocation.RIGHT.getName(), -1));

                    expect(tabs).toBe("/ts0/t0[One]*,/ts1/t0[newtab1]*,/ts2/t0[Two]*");

                    const id1 = tabset("/ts2").getId();
                    doAction(Action.addNode({ name: "newtab2", component: "grid" }, id1, DockLocation.RIGHT.getName(), -1));

                    expect(tabs).toBe("/ts0/t0[One]*,/ts1/t0[newtab1]*,/ts2/t0[Two]*,/ts3/t0[newtab2]*");
                });
            });

            describe("borders", () => {
                beforeEach(() => {
                    model = Model.fromJson(withBorders);
                    textRender(model);

                    expect(tabs).toBe("/b/top/t0[top1],/b/bottom/t0[bottom1],/b/bottom/t1[bottom2],/b/left/t0[left1],/b/right/t0[right1],/ts0/t0[One]*");
                });

                it("add to top border", () => {
                    const path = "/b/top";
                    const others = tabsDontMatch(path);
                    const id0 = border(path).getId();
                    doAction(Action.addNode({ name: "newtab1", component: "grid" }, id0, DockLocation.CENTER.getName(), -1));

                    expect(tabsMatch(path)).toBe("/b/top/t0[top1],/b/top/t1[newtab1]");
                    expect(tabsDontMatch(path)).toBe(others);

                    // add tab at position 0
                    doAction(Action.addNode({ name: "newtab2", component: "grid" }, id0, DockLocation.CENTER.getName(), 0));

                    expect(tabsMatch(path)).toBe("/b/top/t0[newtab2],/b/top/t1[top1],/b/top/t2[newtab1]");
                    expect(tabsDontMatch(path)).toBe(others);

                    // add tab at position 1
                    doAction(Action.addNode({ name: "newtab3", component: "grid" }, id0, DockLocation.CENTER.getName(), 1));

                    expect(tabsMatch(path)).toBe("/b/top/t0[newtab2],/b/top/t1[newtab3],/b/top/t2[top1],/b/top/t3[newtab1]");
                    expect(tabsDontMatch(path)).toBe(others);
                });

                it("add to bottom border", () => {
                    const path = "/b/bottom";
                    const others = tabsDontMatch(path);
                    const id0 = border(path).getId();
                    doAction(Action.addNode({ name: "newtab1", component: "grid" }, id0, DockLocation.CENTER.getName(), -1));

                    expect(tabsMatch(path)).toBe("/b/bottom/t0[bottom1],/b/bottom/t1[bottom2],/b/bottom/t2[newtab1]");
                    expect(tabsDontMatch(path)).toBe(others);

                    // add tab at position 0
                    doAction(Action.addNode({ name: "newtab2", component: "grid" }, id0, DockLocation.CENTER.getName(), 0));

                    expect(tabsMatch(path)).toBe("/b/bottom/t0[newtab2],/b/bottom/t1[bottom1],/b/bottom/t2[bottom2],/b/bottom/t3[newtab1]");
                    expect(tabsDontMatch(path)).toBe(others);

                    // add tab at position 1
                    doAction(Action.addNode({ name: "newtab3", component: "grid" }, id0, DockLocation.CENTER.getName(), 1));

                    expect(tabsMatch(path)).toBe("/b/bottom/t0[newtab2],/b/bottom/t1[newtab3],/b/bottom/t2[bottom1],/b/bottom/t3[bottom2],/b/bottom/t4[newtab1]");
                    expect(tabsDontMatch(path)).toBe(others);
                });

                it("add to left border", () => {
                    const path = "/b/left";
                    const others = tabsDontMatch(path);
                    const id0 = border(path).getId();
                    doAction(Action.addNode({ name: "newtab1", component: "grid" }, id0, DockLocation.CENTER.getName(), -1));

                    expect(tabsMatch(path)).toBe("/b/left/t0[left1],/b/left/t1[newtab1]");
                    expect(tabsDontMatch(path)).toBe(others);

                    // add tab at position 0
                    doAction(Action.addNode({ name: "newtab2", component: "grid" }, id0, DockLocation.CENTER.getName(), 0));

                    expect(tabsMatch(path)).toBe("/b/left/t0[newtab2],/b/left/t1[left1],/b/left/t2[newtab1]");
                    expect(tabsDontMatch(path)).toBe(others);

                    // add tab at position 1
                    doAction(Action.addNode({ name: "newtab3", component: "grid" }, id0, DockLocation.CENTER.getName(), 1));

                    expect(tabsMatch(path)).toBe("/b/left/t0[newtab2],/b/left/t1[newtab3],/b/left/t2[left1],/b/left/t3[newtab1]");
                    expect(tabsDontMatch(path)).toBe(others);
                });

                it("add to right border", () => {
                    const path = "/b/right";
                    const others = tabsDontMatch(path);
                    const id0 = border(path).getId();
                    doAction(Action.addNode({ name: "newtab1", component: "grid" }, id0, DockLocation.CENTER.getName(), -1));

                    expect(tabsMatch(path)).toBe("/b/right/t0[right1],/b/right/t1[newtab1]");
                    expect(tabsDontMatch(path)).toBe(others);

                    // add tab at position 0
                    doAction(Action.addNode({ name: "newtab2", component: "grid" }, id0, DockLocation.CENTER.getName(), 0));

                    expect(tabsMatch(path)).toBe("/b/right/t0[newtab2],/b/right/t1[right1],/b/right/t2[newtab1]");
                    expect(tabsDontMatch(path)).toBe(others);

                    // add tab at position 1
                    doAction(Action.addNode({ name: "newtab3", component: "grid" }, id0, DockLocation.CENTER.getName(), 1));

                    expect(tabsMatch(path)).toBe("/b/right/t0[newtab2],/b/right/t1[newtab3],/b/right/t2[right1],/b/right/t3[newtab1]");
                    expect(tabsDontMatch(path)).toBe(others);
                });
            });
        });

        describe("Move", () => {
            beforeEach(() => {
                model = Model.fromJson(threeTabs);
                textRender(model);
                expect(tabs).toBe("/ts0/t0[One]*,/ts1/t0[Two]*,/ts2/t0[Three]*");
            });

            it("move to center", () => {
                const fromId = tab("/ts0/t0").getId();
                const toId = tab("/ts1").getId();
                doAction(Action.moveNode(fromId, toId, DockLocation.CENTER.getName(), -1));
                expect(tabs).toBe("/ts0/t0[Two],/ts0/t1[One]*,/ts1/t0[Three]*");
            });

            it("move to center position", () => {
                let fromId = tab("/ts0/t0").getId();
                const toId = tab("/ts1").getId();
                doAction(Action.moveNode(fromId, toId, DockLocation.CENTER.getName(), 0));
                expect(tabs).toBe("/ts0/t0[One]*,/ts0/t1[Two],/ts1/t0[Three]*");

                fromId = tab("/ts1/t0").getId();
                doAction(Action.moveNode(fromId, toId, DockLocation.CENTER.getName(), 1));
                expect(tabs).toBe("/ts0/t0[One],/ts0/t1[Three]*,/ts0/t2[Two]");
            });

            it("move to top", () => {
                const fromId = tab("/ts0/t0").getId();
                const toId = tab("/ts1").getId();
                doAction(Action.moveNode(fromId, toId, DockLocation.TOP.getName(), -1));
                expect(tabs).toBe("/r0/ts0/t0[One]*,/r0/ts1/t0[Two]*,/ts1/t0[Three]*");
            });

            it("move to bottom", () => {
                const fromId = tab("/ts0/t0").getId();
                const toId = tab("/ts1").getId();
                doAction(Action.moveNode(fromId, toId, DockLocation.BOTTOM.getName(), -1));
                expect(tabs).toBe("/r0/ts0/t0[Two]*,/r0/ts1/t0[One]*,/ts1/t0[Three]*");
            });

            it("move to left", () => {
                const fromId = tab("/ts0/t0").getId();
                const toId = tab("/ts1").getId();
                doAction(Action.moveNode(fromId, toId, DockLocation.LEFT.getName(), -1));
                expect(tabs).toBe("/ts0/t0[One]*,/ts1/t0[Two]*,/ts2/t0[Three]*");
            });

            it("move to right", () => {
                const fromId = tab("/ts0/t0").getId();
                const toId = tab("/ts1").getId();
                doAction(Action.moveNode(fromId, toId, DockLocation.RIGHT.getName(), -1));
                expect(tabs).toBe("/ts0/t0[Two]*,/ts1/t0[One]*,/ts2/t0[Three]*");
            });
        });

        describe("Move to/from borders", () => {
            beforeEach(() => {
                model = Model.fromJson(withBorders);
                textRender(model);
                expect(tabs).toBe("/b/top/t0[top1],/b/bottom/t0[bottom1],/b/bottom/t1[bottom2],/b/left/t0[left1],/b/right/t0[right1],/ts0/t0[One]*");
            });

            it("move to border top", () => {
                const fromId = tab("/ts0/t0").getId();
                const toId = tab("/b/top").getId();
                doAction(Action.moveNode(fromId, toId, DockLocation.CENTER.getName(), -1));
                expect(tabs).toBe("/b/top/t0[top1],/b/top/t1[One],/b/bottom/t0[bottom1],/b/bottom/t1[bottom2],/b/left/t0[left1],/b/right/t0[right1]");
            });

            it("move to border bottom", () => {
                const fromId = tab("/ts0/t0").getId();
                const toId = tab("/b/bottom").getId();
                doAction(Action.moveNode(fromId, toId, DockLocation.CENTER.getName(), -1));
                expect(tabs).toBe("/b/top/t0[top1],/b/bottom/t0[bottom1],/b/bottom/t1[bottom2],/b/bottom/t2[One],/b/left/t0[left1],/b/right/t0[right1]");
            });

            it("move to border left", () => {
                const fromId = tab("/ts0/t0").getId();
                const toId = tab("/b/left").getId();
                doAction(Action.moveNode(fromId, toId, DockLocation.CENTER.getName(), -1));
                expect(tabs).toBe("/b/top/t0[top1],/b/bottom/t0[bottom1],/b/bottom/t1[bottom2],/b/left/t0[left1],/b/left/t1[One],/b/right/t0[right1]");
            });

            it("move to border right", () => {
                const fromId = tab("/ts0/t0").getId();
                const toId = tab("/b/right").getId();
                doAction(Action.moveNode(fromId, toId, DockLocation.CENTER.getName(), -1));
                expect(tabs).toBe("/b/top/t0[top1],/b/bottom/t0[bottom1],/b/bottom/t1[bottom2],/b/left/t0[left1],/b/right/t0[right1],/b/right/t1[One]");
            });


            it("move from border top", () => {
                const fromId = tab("/b/top/t0").getId();
                const toId = tab("/ts0").getId();
                doAction(Action.moveNode(fromId, toId, DockLocation.CENTER.getName(), -1));
                expect(tabs).toBe("/b/bottom/t0[bottom1],/b/bottom/t1[bottom2],/b/left/t0[left1],/b/right/t0[right1],/ts0/t0[One],/ts0/t1[top1]*");
            });

            it("move from border bottom", () => {
                const fromId = tab("/b/bottom/t0").getId();
                const toId = tab("/ts0").getId();
                doAction(Action.moveNode(fromId, toId, DockLocation.CENTER.getName(), -1));
                expect(tabs).toBe("/b/top/t0[top1],/b/bottom/t0[bottom2],/b/left/t0[left1],/b/right/t0[right1],/ts0/t0[One],/ts0/t1[bottom1]*");
            });

            it("move from border left", () => {
                const fromId = tab("/b/left/t0").getId();
                const toId = tab("/ts0").getId();
                doAction(Action.moveNode(fromId, toId, DockLocation.CENTER.getName(), -1));
                expect(tabs).toBe("/b/top/t0[top1],/b/bottom/t0[bottom1],/b/bottom/t1[bottom2],/b/right/t0[right1],/ts0/t0[One],/ts0/t1[left1]*");
            });

            it("move from border right", () => {
                const fromId = tab("/b/right/t0").getId();
                const toId = tab("/ts0").getId();
                doAction(Action.moveNode(fromId, toId, DockLocation.CENTER.getName(), -1));
                expect(tabs).toBe("/b/top/t0[top1],/b/bottom/t0[bottom1],/b/bottom/t1[bottom2],/b/left/t0[left1],/ts0/t0[One],/ts0/t1[right1]*");
            });
        });

        describe("Delete", () => {
            beforeEach(() => {
            });

            it("delete from tabset with 1 tab", () => {
                model = Model.fromJson(threeTabs);
                textRender(model);
                expect(tabs).toBe("/ts0/t0[One]*,/ts1/t0[Two]*,/ts2/t0[Three]*");

                doAction(Action.deleteTab(tab("/ts0/t0").getId()));
                expect(tabs).toBe("/ts0/t0[Two]*,/ts1/t0[Three]*");
            });

            it("delete tab from tabset with 3 tabs", () => {
                model = Model.fromJson(threeTabs);
                textRender(model);
                expect(tabs).toBe("/ts0/t0[One]*,/ts1/t0[Two]*,/ts2/t0[Three]*");
                let fromId = tab("/ts0/t0").getId();
                let toId = tab("/ts1").getId();
                doAction(Action.moveNode(fromId, toId, DockLocation.CENTER.getName(), -1));
                fromId = tab("/ts1/t0").getId();
                toId = tab("/ts0").getId();
                doAction(Action.moveNode(fromId, toId, DockLocation.CENTER.getName(), -1));
                expect(tabs).toBe("/ts0/t0[Two],/ts0/t1[One],/ts0/t2[Three]*");
                // now had three tabs in /ts0

                doAction(Action.deleteTab(tab("/ts0/t1").getId()));
                expect(tabs).toBe("/ts0/t0[Two],/ts0/t1[Three]*");

                doAction(Action.deleteTab(tab("/ts0/t1").getId()));
                expect(tabs).toBe("/ts0/t0[Two]*");
            });

            it("delete tabset", () => {
                model = Model.fromJson(threeTabs);
                textRender(model);
                expect(tabs).toBe("/ts0/t0[One]*,/ts1/t0[Two]*,/ts2/t0[Three]*");
                const fromId = tab("/ts0/t0").getId();
                const toId = tab("/ts1").getId();
                doAction(Action.moveNode(fromId, toId, DockLocation.CENTER.getName(), -1));
                expect(tabs).toBe("/ts0/t0[Two],/ts0/t1[One]*,/ts1/t0[Three]*");

                doAction(Action.deleteTabset(tabset("/ts0").getId()));
                expect(tabs).toBe("/ts0/t0[Three]*");
            });

            it("delete tab from borders", () => {
                model = Model.fromJson(withBorders);
                textRender(model);

                expect(tabs).toBe("/b/top/t0[top1],/b/bottom/t0[bottom1],/b/bottom/t1[bottom2],/b/left/t0[left1],/b/right/t0[right1],/ts0/t0[One]*");

                doAction(Action.deleteTab(tab("/b/top/t0").getId()));
                expect(tabs).toBe("/b/bottom/t0[bottom1],/b/bottom/t1[bottom2],/b/left/t0[left1],/b/right/t0[right1],/ts0/t0[One]*");

                doAction(Action.deleteTab(tab("/b/bottom/t0").getId()));
                expect(tabs).toBe("/b/bottom/t0[bottom2],/b/left/t0[left1],/b/right/t0[right1],/ts0/t0[One]*");

                doAction(Action.deleteTab(tab("/b/bottom/t0").getId()));
                expect(tabs).toBe("/b/left/t0[left1],/b/right/t0[right1],/ts0/t0[One]*");

                doAction(Action.deleteTab(tab("/b/left/t0").getId()));
                expect(tabs).toBe("/b/right/t0[right1],/ts0/t0[One]*");

                doAction(Action.deleteTab(tab("/b/right/t0").getId()));
                expect(tabs).toBe("/ts0/t0[One]*");
            });

        });

        describe("Other Actions", () => {
            beforeEach(() => {
                model = Model.fromJson(twoTabs);
                textRender(model);
                expect(tabs).toBe("/ts0/t0[One]*,/ts1/t0[Two]*");
            });

            it("rename tab", () => {
                doAction(Action.renameTab(tab("/ts0/t0").getId(), "renamed"));
                expect(tabs).toBe("/ts0/t0[renamed]*,/ts1/t0[Two]*");
            });

            it("select tab", () => {
                doAction(Action.addNode({ name: "newtab1", component: "grid" }, tabset("/ts0").getId(), DockLocation.CENTER.getName(), -1));
                expect(tabs).toBe("/ts0/t0[One],/ts0/t1[newtab1]*,/ts1/t0[Two]*");

                doAction(Action.selectTab(tab("/ts0/t0").getId()));
                expect(tabs).toBe("/ts0/t0[One]*,/ts0/t1[newtab1],/ts1/t0[Two]*");
            });

            it("set active tabset", () => {
                const ts0 = tabset("/ts0");
                const ts1 = tabset("/ts1");
                expect(ts0.isActive()).toBe(false);
                expect(ts1.isActive()).toBe(false);
                doAction(Action.selectTab(tab("/ts0/t0").getId()));
                expect(ts0.isActive()).toBe(true);
                expect(ts1.isActive()).toBe(false);
                doAction(Action.selectTab(tab("/ts1/t0").getId()));
                expect(ts0.isActive()).toBe(false);
                expect(ts1.isActive()).toBe(true);

                doAction(Action.setActiveTabset(tabset("/ts0").getId()));
                expect(ts0.isActive()).toBe(true);
                expect(ts1.isActive()).toBe(false);
            });

            it("maximize tabset", () => {
                expect(tabset("/ts0").isMaximized()).toBe(false);
                expect(tabset("/ts1").isMaximized()).toBe(false);
                doAction(Action.maximizeToggle(tabset("/ts0").getId()));
                expect(tabset("/ts0").isMaximized()).toBe(true);
                expect(tabset("/ts1").isMaximized()).toBe(false);
                doAction(Action.maximizeToggle(tabset("/ts1").getId()));
                expect(tabset("/ts0").isMaximized()).toBe(false);
                expect(tabset("/ts1").isMaximized()).toBe(true);

                expect(model.getMaximizedTabset()).toBe(tabset("/ts1"));

                doAction(Action.maximizeToggle(tabset("/ts1").getId()));
                expect(tabset("/ts0").isMaximized()).toBe(false);
                expect(tabset("/ts1").isMaximized()).toBe(false);

                expect(model.getMaximizedTabset()).toBe(undefined);
            });

            it("set tab attributes", () => {
                expect(tab("/ts1/t0").getConfig()).toBe(undefined);
                doAction(Action.updateNodeAttributes(tab("/ts1/t0").getId(), { config: "newConfig" }));
                expect(tab("/ts1/t0").getConfig()).toBe("newConfig");
            });

            it("set model attributes", () => {
                expect(model.getSplitterSize()).toBe(4);
                doAction(Action.updateModelAttributes({ splitterSize: 10 }));
                expect(model.getSplitterSize()).toBe(10);
            });

        });
    });

    describe("Node events", () => {
        beforeEach(() => {
            model = Model.fromJson(twoTabs);
            textRender(model);
            expect(tabs).toBe("/ts0/t0[One]*,/ts1/t0[Two]*");
        });

        it("close tab", () => {
            let closed = false;
            tab("/ts0/t0").setEventListener("close", () => { closed = true; });
            doAction(Action.deleteTab(tab("/ts0/t0").getId()));
            expect(closed).toBe(true);
        });

        it("save tab", () => {
            let saved = false;
            tab("/ts0/t0").setEventListener("save", () => { saved = true; });
            model.toJson();
            expect(saved).toBe(true);
        });
    });
});


// ---------------------------- helpers ------------------------

function doAction(action: ReturnType<typeof Action[keyof typeof Action]>) {
    model.doAction(action);
    textRender(model);
}

// functions to save some inline casting
function tab(path: string) {
    return pathMap[path] as TabNode;
}

function tabset(path: string) {
    return pathMap[path] as TabSetNode;
}

function border(path: string) {
    return pathMap[path] as BorderNode;
}

function tabsMatch(regExStr: string) {
    const regex = new RegExp(regExStr);
    return tabsArray.filter(t => regex.test(t)).join(",");
}

function tabsDontMatch(regExStr: string) {
    const regex = new RegExp(regExStr);
    return tabsArray.filter(t => !regex.test(t)).join(",");
}

function textRender(model: Model) {
    pathMap = {};
    tabsArray = [];
    textRenderInner(pathMap, "", model.getBorderSet().getBorders());
    textRenderInner(pathMap, "", model.getRoot()!.getChildren());
    tabs = tabsArray.join(",");
}

function textRenderInner(pathMap: Record<string, Node>, path: string, children: Node[]) {
    let index = 0;
    children.forEach((c) => {
        if (c instanceof BorderNode) {
            const newpath = path + "/b/" + c.getLocation().getName();
            pathMap[newpath] = c;
            textRenderInner(pathMap, newpath, c.getChildren());
        } else if (c instanceof TabSetNode) {
            const newpath = path + "/ts" + index++;
            pathMap[newpath] = c;
            textRenderInner(pathMap, newpath, c.getChildren());
        } else if (c instanceof TabNode) {
            const newpath = path + "/t" + index++;
            pathMap[newpath] = c;
            const parent = c.getParent() as (BorderNode | TabSetNode);
            const selectedNode = (parent.getSelectedNode && parent.getSelectedNode()) === c;
            tabsArray.push(newpath + "[" + c.getName() + "]" + (selectedNode ? "*" : ""));
            textRenderInner(pathMap, newpath, c.getChildren());
        } else if (c instanceof RowNode) {
            const newpath = path + "/r" + index++;
            pathMap[newpath] = c;
            textRenderInner(pathMap, newpath, c.getChildren());
        }
    });
}

// -------------------- layouts --------------------

const twoTabs: IJsonModel = {
    global: {},
    borders: [],
    layout: {
        type: "row",
        weight: 100,
        children: [
            {
                type: "tabset",
                weight: 50,
                children: [
                    {
                        type: "tab",
                        name: "One",
                        component: "text",
                    }
                ]
            },
            {
                type: "tabset",
                id: "#1",
                weight: 50,
                children: [
                    {
                        type: "tab",
                        name: "Two",
                        component: "text",
                    }
                ]
            }
        ]
    }
};

const withBorders: IJsonModel = {
    global: {},
    borders: [
        {
            "type": "border",
            "location": "top",
            "children": [
                {
                    "type": "tab",
                    "name": "top1",
                    "component": "text"
                }
            ]
        },
        {
            "type": "border",
            "location": "bottom",
            "children": [
                {
                    "type": "tab",
                    "name": "bottom1",
                    "component": "text"
                },
                {
                    "type": "tab",
                    "name": "bottom2",
                    "component": "text"
                }
            ]
        },
        {
            "type": "border",
            "location": "left",
            "children": [
                {
                    "type": "tab",
                    "name": "left1",
                    "component": "text"
                }
            ]
        },
        {
            "type": "border",
            "location": "right",
            "children": [
                {
                    "type": "tab",
                    "name": "right1",
                    "component": "text"
                }
            ]
        }
    ],
    layout: {
        type: "row",
        weight: 100,
        children: [
            {
                type: "tabset",
                weight: 50,
                children: [
                    {
                        type: "tab",
                        name: "One",
                        component: "text",
                    }
                ]
            }
        ]
    }
};

// ═════════════════════════════════════════════════════════════════════════════════════════════
// Docked Panel Behavior Tests
// ═══════════════════════════════════════════════════════════════════════════════════════

describe("Docked Panel", function () {
    describe("Dock State", () => {
        describe("Set dock state", () => {
            beforeEach(() => {
                model = Model.fromJson({
                    global: {},
                    borders: [
                        {
                            type: "border",
                            location: "top",
                            children: [{ type: "tab", name: "Tab1", component: "text" }]
                        },
                    ],
                    layout: { type: "row", weight: 100, children: [] }
                });
                textRender(model);
            });

            it("should expand collapsed border", () => {
                const borderId = border("/b/top").getId();
                doAction(Action.setDockState(borderId, "expanded"));

                expect(border("/b/top").getDockState()).toBe("expanded");
            });

            it("should collapse expanded border", () => {
                const borderId = border("/b/top").getId();
                model.doAction(Action.setDockState(borderId, "collapsed"));

                expect(border("/b/top").getDockState()).toBe("collapsed");
            });

            it("should serialize dockState to JSON", () => {
                const borderId = border("/b/top").getId();
                doAction(Action.setDockState(borderId, "collapsed"));

                const json = model.toJson();
                expect(json.borders?.[0]).toHaveProperty("dockState", "collapsed");
            });
        });

        describe("Auto-expand on drop", () => {
            it("should auto-expand when dropping on collapsed border", () => {
                model = Model.fromJson({
                    global: {},
                    borders: [
                        { type: "border", location: "left", dockState: "collapsed", children: [] },
                    ],
                    layout: {
                        type: "row",
                        weight: 100,
                        children: [
                            { type: "tabset", children: [{ type: "tab", name: "Source" }] }
                        ]
                    }
                });

                const tabId = tab("/ts0/t0").getId();
                const borderId = border("/b/left").getId();

                doAction(Action.moveNode(tabId, borderId, DockLocation.CENTER.getName(), -1));

                expect(border("/b/left").getDockState()).toBe("expanded");
                expect(border("/b/left").getChildren().length).toBe(1);
            });

            it("should not auto-expand if already expanded", () => {
                model = Model.fromJson({
                    global: {},
                    borders: [
                        { type: "border", location: "right", dockState: "expanded", children: [] },
                    ],
                    layout: {
                        type: "row",
                        weight: 100,
                        children: [
                            { type: "tabset", children: [{ type: "tab", name: "Source" }] }
                        ]
                    }
                });

                const tabId = tab("/ts0/t0").getId();
                const borderId = border("/b/right").getId();

                doAction(Action.moveNode(tabId, borderId, DockLocation.CENTER.getName(), -1));

                expect(border("/b/right").getDockState()).toBe("expanded"); // Should stay expanded
            });
        });

        describe("Auto-collapse on empty", () => {
            it("should auto-collapse when last tab is removed", () => {
                model = Model.fromJson({
                    global: {},
                    borders: [
                        {
                            type: "border",
                            location: "bottom",
                            dockState: "expanded",
                            selected: 0,
                            children: [
                                { type: "tab", name: "OnlyTab", component: "text" }
                            ]
                        }
                    ],
                    layout: { type: "row", weight: 100, children: [] }
                });

                const tabId = tab("/b/bottom/t0").getId();
                doAction(Action.deleteTab(tabId));

                expect(border("/b/bottom").getDockState()).toBe("collapsed");
                expect(border("/b/bottom").getChildren().length).toBe(0);
            });

            it("should not auto-collapse if tabs remain", () => {
                model = Model.fromJson({
                    global: {},
                    borders: [
                        {
                            type: "border",
                            location: "top",
                            dockState: "expanded",
                            children: [
                                { type: "tab", name: "Tab1", component: "text" },
                                { type: "tab", name: "Tab2", component: "text" }
                            ]
                        }
                    ],
                    layout: { type: "row", weight: 100, children: [] }
                });

                const tabId = tab("/b/top/t0").getId();
                doAction(Action.deleteTab(tabId));

                expect(border("/b/top").getDockState()).toBe("expanded"); // Should stay expanded
                expect(border("/b/top").getChildren().length).toBe(1);
            });
        });

        describe("Visible Tabs (Tiling)", () => {
            it("should set visibleTabs array", () => {
                model = Model.fromJson({
                    global: {},
                    borders: [
                        {
                            type: "border",
                            location: "bottom",
                            children: [
                                { type: "tab", name: "Tab1", component: "text" },
                                { type: "tab", name: "Tab2", component: "text" }
                            ]
                        }
                    ],
                    layout: { type: "row", weight: 100, children: [] }
                });

                const borderId = border("/b/bottom").getId();
                doAction(Action.setVisibleTabs(borderId, [0, 1]));

                const json = model.toJson();
                expect(json.borders?.[0]).toHaveProperty("visibleTabs", [0, 1]);
            });

            it("should clear visibleTabs with empty array", () => {
                model = Model.fromJson({
                    global: {},
                    borders: [
                        {
                            type: "border",
                            location: "top",
                            visibleTabs: [0, 1],
                            children: [
                                { type: "tab", name: "A", component: "text" },
                                { type: "tab", name: "B", component: "text" }
                            ]
                        }
                    ],
                    layout: { type: "row", weight: 100, children: [] }
                });

                const borderId = border("/b/top").getId();
                doAction(Action.setVisibleTabs(borderId, []));

                const json = model.toJson();
                expect(json.borders?.[0]).toHaveProperty("visibleTabs", []);
            });

            it("should preserve visibleTabs across model changes", () => {
                model = Model.fromJson({
                    global: {},
                    borders: [
                        {
                            type: "border",
                            location: "left",
                            visibleTabs: [0, 1],
                            children: [
                                { type: "tab", name: "A", component: "text" },
                                { type: "tab", name: "B", component: "text" },
                                { type: "tab", name: "C", component: "text" }
                            ]
                        }
                    ],
                    layout: { type: "row", weight: 100, children: [] }
                });

                // Serialize and reload
                const json1 = model.toJson();
                model = Model.fromJson(json1);

                expect(border("/b/left").getVisibleTabs()).toEqual([0, 1]);
            });
        });

        describe("Flyout Mode", () => {
            it("should set flyoutTabId", () => {
                model = Model.fromJson({
                    global: {},
                    borders: [
                        {
                            type: "border",
                            location: "top",
                            children: [
                                { type: "tab", id: "flyout_tab", name: "FlyoutTab", component: "text" }
                            ]
                        }
                    ],
                    layout: { type: "row", weight: 100, children: [] }
                });

                const borderId = border("/b/top").getId();
                const tabId = "flyout_tab";
                doAction(Action.openFlyout(borderId, tabId));

                expect(border("/b/top").getFlyoutTabId()).toBe(tabId);
            });

            it("should clear flyoutTabId on close", () => {
                model = Model.fromJson({
                    global: {},
                    borders: [
                        {
                            type: "border",
                            location: "bottom",
                            flyoutTabId: "active_flyout",
                            children: [
                                { type: "tab", id: "flyout_tab", name: "Tab", component: "text" }
                            ]
                        }
                    ],
                    layout: { type: "row", weight: 100, children: [] }
                });

                const borderId = border("/b/bottom").getId();
                doAction(Action.closeFlyout(borderId));

                expect(border("/b/bottom").getFlyoutTabId()).toBeNull();
            });

            it("should serialize flyoutTabId to JSON", () => {
                model = Model.fromJson({
                    global: {},
                    borders: [
                        {
                            type: "border",
                            location: "left",
                            flyoutTabId: "flyout_1",
                            children: [
                                { type: "tab", name: "Tab", component: "text" }
                            ]
                        }
                    ],
                    layout: { type: "row", weight: 100, children: [] }
                });

                const json = model.toJson();
                expect(json.borders?.[0]).toHaveProperty("flyoutTabId", "flyout_1");
            });
        });

        describe("Pinning Behavior", () => {
            it("should always allow dragging tabs (no pinning check)", () => {
                model = Model.fromJson({
                    global: {},
                    borders: [
                        {
                            type: "border",
                            location: "left",
                            children: [
                                { type: "tab", name: "PinnedTab", pinned: true, component: "text" }
                            ]
                        }
                    ],
                    layout: {
                        type: "row",
                        weight: 100,
                        children: [
                            { type: "tabset", children: [{ type: "tab", name: "Target" }] }
                        ]
                    }
                });

                const tabId = tab("/b/left/t0").getId();
                const targetId = tab("/ts0/t0").getId();

                // Even though tab has pinned: true, move should succeed
                doAction(Action.moveNode(tabId, targetId, DockLocation.CENTER.getName(), -1));

                expect(tab("/ts0/t0").getName()).toBe("PinnedTab");
                expect(tab("/ts0/t0").getParent()?.getType()).toBe("tabset");
            });

            it("should always allow deleting tabs (no pinning check)", () => {
                model = Model.fromJson({
                    global: {},
                    borders: [
                        {
                            type: "border",
                            location: "right",
                            children: [
                                { type: "tab", name: "PinnedTab", pinned: true, component: "text" }
                            ]
                        }
                    ],
                    layout: { type: "row", weight: 100, children: [] }
                });

                const tabId = tab("/b/right/t0").getId();

                // Even though tab has pinned: true, delete should succeed
                doAction(Action.deleteTab(tabId));

                expect(border("/b/right").getChildren().length).toBe(0);
            });
        });

        describe("Border Sizing", () => {
            it("should respect border size attribute", () => {
                model = Model.fromJson({
                    global: {},
                    borders: [
                        {
                            type: "border",
                            location: "top",
                            size: 300,
                            children: [
                                { type: "tab", name: "Tab", component: "text" }
                            ]
                        }
                    ],
                    layout: { type: "row", weight: 100, children: [] }
                });

                const borderNode = border("/b/top");
                expect(border.getSize()).toBe(300);
            });

            it("should respect minSize constraint", () => {
                model = Model.fromJson({
                    global: {},
                    borders: [
                        {
                            type: "border",
                            location: "bottom",
                            size: 200,
                            minSize: 150,
                            children: [
                                { type: "tab", name: "Tab", component: "text" }
                            ]
                        }
                    ],
                    layout: { type: "row", weight: 100, children: [] }
                });

                const borderNode = border("/b/bottom");
                expect(border.getMinSize()).toBe(150);
            });

            it("should respect maxSize constraint", () => {
                model = Model.fromJson({
                    global: {},
                    borders: [
                        {
                            type: "border",
                            location: "left",
                            size: 200,
                            maxSize: 250,
                            children: [
                                { type: "tab", name: "Tab", component: "text" }
                            ]
                        }
                    ],
                    layout: { type: "row", weight: 100, children: [] }
                });

                const borderNode = border("/b/left");
                expect(border.getMaxSize()).toBe(250);
            });
        });
    });
});

// ═══════════════════════════════════════════════════════════════════════════════════════════
// Docked Panel Behavior Tests
// ═══════════════════════════════════════════════════════════════════════════════════════

describe("Docked Panel", function () {
    describe("Dock State", () => {
        describe("Set dock state", () => {
            beforeEach(() => {
                model = Model.fromJson({
                    global: {},
                    borders: [
                        { type: "border", location: "top", children: [{ type: "tab", name: "Tab1", component: "text" }] }
                    ],
                    layout: { type: "row", weight: 100, children: [] }
                });
                textRender(model);
            });

            it("should expand collapsed border", () => {
                doAction(Action.setDockState(border("/b/top").getId(), "expanded"));
                expect(border("/b/top").getDockState()).toBe("expanded");
            });

            it("should collapse expanded border", () => {
                model.doAction(Action.setDockState(border("/b/top").getId(), "collapsed"));
                expect(border("/b/top").getDockState()).toBe("collapsed");
            });

            it("should serialize dockState to JSON", () => {
                doAction(Action.setDockState(border("/b/top").getId(), "collapsed"));
                const json = model.toJson();
                expect(json.borders?.[0]).toHaveProperty("dockState", "collapsed");
            });
        });

        describe("Auto-expand on drop", () => {
            it("should auto-expand when dropping on collapsed border", () => {
                model = Model.fromJson({
                    global: {},
                    borders: [
                        { type: "border", location: "left", dockState: "collapsed", children: [] },
                    ],
                    layout: { type: "row", weight: 100, children: [{ type: "tabset", children: [{ type: "tab", name: "Source" }] }]
                    }
                });

                doAction(Action.moveNode(tab("/ts0/t0").getId(), border("/b/left").getId(), DockLocation.CENTER.getName(), -1));

                expect(border("/b/left").getDockState()).toBe("expanded");
            });
        });

        describe("Auto-collapse on empty", () => {
            it("should auto-collapse when last tab is removed", () => {
                model = Model.fromJson({
                    global: {},
                    borders: [
                        { type: "border", location: "bottom", dockState: "expanded", selected: 0, children: [{ type: "tab", name: "OnlyTab", component: "text" }] }
                    ],
                    layout: { type: "row", weight: 100, children: [] }
                });

                doAction(Action.deleteTab(tab("/b/bottom/t0").getId()));

                expect(border("/b/bottom").getDockState()).toBe("collapsed");
            });
        });

        describe("Visible Tabs (Tiling)", () => {
            it("should set visibleTabs array", () => {
                model = Model.fromJson({
                    global: {},
                    borders: [{ type: "border", location: "bottom", children: [{ type: "tab", name: "Tab1", component: "text" }, { type: "tab", name: "Tab2", component: "text" }] }]
                });

                doAction(Action.setVisibleTabs(border("/b/bottom").getId(), [0, 1]));

                const json = model.toJson();
                expect(json.borders?.[0]).toHaveProperty("visibleTabs", [0, 1]);
            });

            it("should clear visibleTabs with empty array", () => {
                model = Model.fromJson({
                    global: {},
                    borders: [{ type: "border", location: "top", visibleTabs: [0, 1], children: [{ type: "tab", name: "A", component: "text" }, { type: "tab", name: "B", component: "text" }] }]
                });

                doAction(Action.setVisibleTabs(border("/b/top").getId(), []));

                const json = model.toJson();
                expect(json.borders?.[0]).toHaveProperty("visibleTabs", []);
            });
        });

        describe("Flyout Mode", () => {
            it("should set flyoutTabId", () => {
                model = Model.fromJson({
                    global: {},
                    borders: [{ type: "border", location: "top", children: [{ type: "tab", id: "flyout_tab", name: "FlyoutTab", component: "text" }] }]
                });

                doAction(Action.openFlyout(border("/b/top").getId(), "flyout_tab"));

                expect(border("/b/top").getFlyoutTabId()).toBe("flyout_tab");
            });

            it("should clear flyoutTabId on close", () => {
                model = Model.fromJson({
                    global: {},
                    borders: [{ type: "border", location: "bottom", flyoutTabId: "active_flyout", children: [{ type: "tab", id: "flyout_tab", name: "Tab", component: "text" }] }]
                });

                doAction(Action.closeFlyout(border("/b/bottom").getId()));

                expect(border("/b/bottom").getFlyoutTabId()).toBeNull();
            });
        });

        describe("Pinning Behavior", () => {
            it("should always allow dragging tabs (no pinning check)", () => {
                model = Model.fromJson({
                    global: {},
                    borders: [{ type: "border", location: "left", children: [{ type: "tab", name: "PinnedTab", pinned: true, component: "text" }] }]
                });

                doAction(Action.moveNode(tab("/b/left/t0").getId(), tab("/ts0/t0").getId(), DockLocation.CENTER.getName(), -1));

                expect(tab("/ts0/t0").getParent()?.getType()).toBe("tabset");
            });

            it("should always allow deleting tabs (no pinning check)", () => {
                model = Model.fromJson({
                    global: {},
                    borders: [{ type: "border", location: "right", children: [{ type: "tab", name: "PinnedTab", pinned: true, component: "text" }] }]
                });

                doAction(Action.deleteTab(tab("/b/right/t0").getId()));

                expect(border("/b/right").getChildren().length).toBe(0);
            });
        });

        describe("Border Sizing", () => {
            it("should respect border size attribute", () => {
                model = Model.fromJson({
                    global: {},
                    borders: [{ type: "border", location: "top", size: 300, children: [{ type: "tab", name: "Tab", component: "text" }] }]
                });

                expect(border("/b/top").getSize()).toBe(300);
            });

            it("should respect minSize constraint", () => {
                model = Model.fromJson({
                    global: {},
                    borders: [{ type: "border", location: "bottom", size: 200, minSize: 150, children: [{ type: "tab", name: "Tab", component: "text" }] }]
                });

                expect(border("/b/bottom").getMinSize()).toBe(150);
            });

            it("should respect maxSize constraint", () => {
                model = Model.fromJson({
                    global: {},
                    borders: [{ type: "border", location: "left", size: 200, maxSize: 250, children: [{ type: "tab", name: "Tab", component: "text" }] }]
                });

                expect(border("/b/left").getMaxSize()).toBe(250);
            });
        });
    });
});

const threeTabs: IJsonModel = {
    global: {},
    borders: [],
    layout: {
        type: "row",
        weight: 100,
        children: [
            {
                type: "tabset",
                weight: 50,
                children: [
                    {
                        type: "tab",
                        name: "One",
                        component: "text",
                    }
                ]
            },
            {
                type: "tabset",
                weight: 50,
                name: "TheHeader",
                children: [
                    {
                        type: "tab",
                        name: "Two",
                        icon: "/test/images/settings.svg",
                        component: "text",
                    }
                ]
            },
            {
                type: "tabset",
                weight: 50,
                children: [
                    {
                        type: "tab",
                        name: "Three",
                        component: "text",
                    }
                ]
            }

        ]
    }
};
