import { describe, it, expect } from "vitest";
import { Model } from "../model/Model";
import { TabNode } from "../model/TabNode";
import type { IContentRenderer, IRenderParams } from "../rendering/IContentRenderer";
import { VanillaLayoutRenderer } from "../rendering/VanillaLayoutRenderer";
import type { IJsonModel } from "../types";

class StubContentRenderer implements IContentRenderer {
    init(_container: HTMLElement, _params: IRenderParams): void {}
    update(_params: Partial<IRenderParams>): void {}
    dispose(): void {}
}

const expandedBorderFixture: IJsonModel = {
    global: { borderEnableDock: true },
    borders: [
        {
            type: "border",
            location: "bottom",
            selected: 0,
            children: [
                { type: "tab", name: "Terminal", component: "text" },
                { type: "tab", name: "Output", component: "text" },
            ],
        },
        {
            type: "border",
            location: "left",
            selected: 0,
            children: [
                { type: "tab", name: "Explorer", component: "text" },
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

describe("Bug #2: Expanded border tab buttons should have same base classes as tabset tab buttons", () => {
    it.todo("expanded border toolbar dock button uses FLEXLAYOUT__BORDER_DOCK_BUTTON class");

    it.todo("expanded border tab buttons in per-tile tabbar include border location class");

    it("collapsed strip tab buttons include FLEXLAYOUT__BORDER_BUTTON base class", () => {
        const model = Model.fromJson(expandedBorderFixture);
        const { host, renderer } = mountRenderer(model);

        try {
            const collapsedButtons = host.querySelectorAll(
                ".flexlayout__border_button"
            );
            expect(collapsedButtons.length).toBeGreaterThan(0);

            for (const button of collapsedButtons) {
                expect(button.className).toContain("flexlayout__border_button");
            }
        } finally {
            renderer.unmount();
            host.remove();
        }
    });

    it("expanded border tab buttons have FLEXLAYOUT__BORDER_BUTTON class (not just tabset class)", () => {
        const model = Model.fromJson(expandedBorderFixture);
        const { host, renderer } = mountRenderer(model);

        try {
            const tabbarButtons = host.querySelectorAll(
                "[data-border-tabbar] .flexlayout__border_button"
            );

            expect(tabbarButtons.length).toBeGreaterThan(0);

            for (const button of tabbarButtons) {
                expect(button.className).toContain("flexlayout__border_button");
                expect(button.className).toMatch(
                    /flexlayout__border_button--(selected|unselected)/
                );
            }
        } finally {
            renderer.unmount();
            host.remove();
        }
    });

    it("expanded and collapsed border tab buttons share the same base class set", () => {
        const model = Model.fromJson(expandedBorderFixture);
        const { host, renderer } = mountRenderer(model);

        try {
            const expandedButtons = host.querySelectorAll(
                "[data-border-tabbar] .flexlayout__border_button"
            );
            const allBorderButtons = host.querySelectorAll(
                ".flexlayout__border_button"
            );

            expect(expandedButtons.length).toBeGreaterThan(0);
            expect(allBorderButtons.length).toBeGreaterThan(0);

            const expandedClassSets = Array.from(expandedButtons).map((el) =>
                el.className.split(/\s+/).filter((c) => c.startsWith("flexlayout__border_button")).sort()
            );
            const allClassSets = Array.from(allBorderButtons).map((el) =>
                el.className.split(/\s+/).filter((c) => c.startsWith("flexlayout__border_button")).sort()
            );

            for (const expandedClasses of expandedClassSets) {
                expect(expandedClasses).toContain("flexlayout__border_button");
            }
            for (const classes of allClassSets) {
                expect(classes).toContain("flexlayout__border_button");
            }
        } finally {
            renderer.unmount();
            host.remove();
        }
    });
});
