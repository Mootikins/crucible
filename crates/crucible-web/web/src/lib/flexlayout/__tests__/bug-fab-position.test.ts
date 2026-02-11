import { describe, it, expect } from "vitest";
import { Model } from "../model/Model";
import { Action } from "../model/Action";
import { TabNode } from "../model/TabNode";
import { BorderNode } from "../model/BorderNode";
import type { IContentRenderer, IRenderParams } from "../rendering/IContentRenderer";
import { VanillaLayoutRenderer } from "../rendering/VanillaLayoutRenderer";
import type { IJsonModel } from "../types";

class StubContentRenderer implements IContentRenderer {
    init(_container: HTMLElement, _params: IRenderParams): void {}
    update(_params: Partial<IRenderParams>): void {}
    dispose(): void {}
}

const fabPositionFixture: IJsonModel = {
    global: { borderEnableDock: true },
    borders: [
        {
            type: "border",
            location: "left",
            selected: 0,
            children: [
                { type: "tab", name: "Explorer", component: "text" },
                { type: "tab", name: "Search", component: "text" },
            ],
        },
        {
            type: "border",
            location: "right",
            selected: 0,
            children: [
                { type: "tab", name: "Properties", component: "text" },
            ],
        },
        {
            type: "border",
            location: "top",
            selected: -1,
            children: [
                { type: "tab", name: "Toolbar", component: "text" },
            ],
        },
        {
            type: "border",
            location: "bottom",
            selected: 0,
            children: [
                { type: "tab", name: "Terminal", component: "text" },
                { type: "tab", name: "Output", component: "text" },
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

function mountRenderer(model: Model): { host: HTMLElement; renderer: VanillaLayoutRenderer } {
    const host = document.createElement("div");
    host.style.width = "1000px";
    host.style.height = "600px";
    document.body.appendChild(host);

    const renderer = new VanillaLayoutRenderer({
        model,
        getClassName: (className) => className,
        doAction: (action) => model.doAction(action),
        createContentRenderer: (_node: TabNode) => new StubContentRenderer(),
    });

    renderer.mount(host);
    return { host, renderer };
}

function getBorder(model: Model, location: string): BorderNode {
    const border = model.getBorderSet().getBorder(location as any);
    if (!border) throw new Error(`No border at ${location}`);
    return border;
}

describe("Bug #4: FAB dock button position consistency across border states", () => {
    it("collapsed strip has exactly one dock button per border", () => {
        const model = Model.fromJson(fabPositionFixture);

        for (const loc of ["left", "right", "top", "bottom"] as const) {
            const border = getBorder(model, loc);
            model.doAction(Action.setDockState(border.getId(), "collapsed"));
        }

        const { host, renderer } = mountRenderer(model);

        try {
            for (const loc of ["left", "right", "top", "bottom"]) {
                const strip = host.querySelector(
                    `.flexlayout__border_${loc}[data-collapsed-strip="true"]`
                );
                expect(strip).toBeTruthy();

                const fabs = strip!.querySelectorAll('button[data-collapsed-fab="true"]');
                expect(fabs.length).toBe(1);
            }
        } finally {
            renderer.unmount();
            host.remove();
        }
    });

    it("collapsed strip FAB is the first child (leading position, matching expanded toolbar)", () => {
        const model = Model.fromJson(fabPositionFixture);

        for (const loc of ["left", "right", "top", "bottom"] as const) {
            const border = getBorder(model, loc);
            model.doAction(Action.setDockState(border.getId(), "collapsed"));
        }

        const { host, renderer } = mountRenderer(model);

        try {
            for (const loc of ["left", "right", "top", "bottom"]) {
                const strip = host.querySelector(
                    `.flexlayout__border_${loc}[data-collapsed-strip="true"]`
                );
                expect(strip).toBeTruthy();

                const firstChild = strip!.children[0] as HTMLElement;
                expect(firstChild?.dataset.collapsedFab).toBe("true");
            }
        } finally {
            renderer.unmount();
            host.remove();
        }
    });

    it("expanded border toolbar has dock button with FLEXLAYOUT__BORDER_DOCK_BUTTON class", () => {
        const model = Model.fromJson(fabPositionFixture);
        const { host, renderer } = mountRenderer(model);

        try {
            const dockButtons = host.querySelectorAll(
                ".flexlayout__border_dock_button"
            );
            expect(dockButtons.length).toBeGreaterThan(0);

            for (const button of dockButtons) {
                expect(button.tagName.toLowerCase()).toBe("button");
            }
        } finally {
            renderer.unmount();
            host.remove();
        }
    });

    it("collapsed FAB and expanded FAB use the same CSS class for consistent styling", () => {
        const model = Model.fromJson(fabPositionFixture);

        // Expanded state
        const { host: expandedHost, renderer: expandedRenderer } = mountRenderer(model);
        const expandedDockButton = expandedHost.querySelector(
            '.flexlayout__border_dock_button[data-dock-context="expanded-toolbar"]'
        );
        expect(expandedDockButton).toBeTruthy();
        const expandedClassName = expandedDockButton!.className;
        expandedRenderer.unmount();
        expandedHost.remove();

        // Collapsed state
        for (const loc of ["left", "right", "top", "bottom"] as const) {
            model.doAction(Action.setDockState(getBorder(model, loc).getId(), "collapsed"));
        }
        const { host: collapsedHost, renderer: collapsedRenderer } = mountRenderer(model);
        const collapsedDockButton = collapsedHost.querySelector(
            '.flexlayout__border_dock_button[data-dock-context="collapsed-strip"]'
        );
        expect(collapsedDockButton).toBeTruthy();
        expect(collapsedDockButton!.className).toBe(expandedClassName);
        collapsedRenderer.unmount();
        collapsedHost.remove();
    });

    it("vertical collapsed strip FAB has margin-bottom for spacing from tab buttons", () => {
        const model = Model.fromJson(fabPositionFixture);

        for (const loc of ["left", "right"] as const) {
            const border = getBorder(model, loc);
            model.doAction(Action.setDockState(border.getId(), "collapsed"));
        }

        const { host, renderer } = mountRenderer(model);

        try {
            const leftFab = host.querySelector<HTMLElement>(
                '.flexlayout__border_left[data-collapsed-strip="true"] button[data-collapsed-fab="true"]'
            );
            expect(leftFab).toBeTruthy();
            expect(leftFab!.style.marginBottom).toBe("4px");

            const rightFab = host.querySelector<HTMLElement>(
                '.flexlayout__border_right[data-collapsed-strip="true"] button[data-collapsed-fab="true"]'
            );
            expect(rightFab).toBeTruthy();
            expect(rightFab!.style.marginBottom).toBe("4px");
        } finally {
            renderer.unmount();
            host.remove();
        }
    });

    it("horizontal collapsed strip FAB has margin-right for spacing from tab buttons", () => {
        const model = Model.fromJson(fabPositionFixture);

        for (const loc of ["top", "bottom"] as const) {
            const border = getBorder(model, loc);
            model.doAction(Action.setDockState(border.getId(), "collapsed"));
        }

        const { host, renderer } = mountRenderer(model);

        try {
            const topFab = host.querySelector<HTMLElement>(
                '.flexlayout__border_top[data-collapsed-strip="true"] button[data-collapsed-fab="true"]'
            );
            expect(topFab).toBeTruthy();
            expect(topFab!.style.marginRight).toBe("4px");

            const bottomFab = host.querySelector<HTMLElement>(
                '.flexlayout__border_bottom[data-collapsed-strip="true"] button[data-collapsed-fab="true"]'
            );
            expect(bottomFab).toBeTruthy();
            expect(bottomFab!.style.marginRight).toBe("4px");
        } finally {
            renderer.unmount();
            host.remove();
        }
    });
});
