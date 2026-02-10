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

const emptyVerticalBordersFixture: IJsonModel = {
    global: { borderEnableDock: true, borderAutoHide: false },
    borders: [
        {
            type: "border",
            location: "left",
            selected: -1,
            dockState: "collapsed",
            children: [
                { type: "tab", name: "Explorer", component: "text" },
                { type: "tab", name: "Search", component: "text" },
            ],
        },
        {
            type: "border",
            location: "right",
            selected: -1,
            dockState: "collapsed",
            children: [
                { type: "tab", name: "Properties", component: "text" },
            ],
        },
        {
            type: "border",
            location: "top",
            selected: -1,
            dockState: "collapsed",
            children: [
                { type: "tab", name: "Toolbar", component: "text" },
            ],
        },
        {
            type: "border",
            location: "bottom",
            selected: -1,
            dockState: "collapsed",
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

describe("Bug #3: Empty vertical collapsed panes should have vertical text", () => {
    it("collapsed left border tab buttons have writing-mode: vertical-rl", () => {
        const model = Model.fromJson(emptyVerticalBordersFixture);
        const { host, renderer } = mountRenderer(model);

        try {
            const leftButtons = host.querySelectorAll<HTMLElement>(
                '.flexlayout__border_left[data-collapsed-strip="true"] button[data-collapsed-tab-item="true"]'
            );

            expect(leftButtons.length).toBeGreaterThan(0);

            for (const button of leftButtons) {
                expect(button.style.writingMode).toBe("vertical-rl");
            }
        } finally {
            renderer.unmount();
            host.remove();
        }
    });

    it("collapsed right border tab buttons have writing-mode: vertical-rl", () => {
        const model = Model.fromJson(emptyVerticalBordersFixture);
        const { host, renderer } = mountRenderer(model);

        try {
            const rightButtons = host.querySelectorAll<HTMLElement>(
                '.flexlayout__border_right[data-collapsed-strip="true"] button[data-collapsed-tab-item="true"]'
            );

            expect(rightButtons.length).toBeGreaterThan(0);

            for (const button of rightButtons) {
                expect(button.style.writingMode).toBe("vertical-rl");
            }
        } finally {
            renderer.unmount();
            host.remove();
        }
    });

    it("collapsed top border tab buttons do NOT have writing-mode set", () => {
        const model = Model.fromJson(emptyVerticalBordersFixture);
        const { host, renderer } = mountRenderer(model);

        try {
            const topButtons = host.querySelectorAll<HTMLElement>(
                '.flexlayout__border_top[data-collapsed-strip="true"] button[data-collapsed-tab-item="true"]'
            );

            expect(topButtons.length).toBeGreaterThan(0);

            for (const button of topButtons) {
                expect(button.style.writingMode).not.toBe("vertical-rl");
            }
        } finally {
            renderer.unmount();
            host.remove();
        }
    });

    it("collapsed bottom border tab buttons do NOT have writing-mode set", () => {
        const model = Model.fromJson(emptyVerticalBordersFixture);
        const { host, renderer } = mountRenderer(model);

        try {
            const bottomButtons = host.querySelectorAll<HTMLElement>(
                '.flexlayout__border_bottom[data-collapsed-strip="true"] button[data-collapsed-tab-item="true"]'
            );

            expect(bottomButtons.length).toBeGreaterThan(0);

            for (const button of bottomButtons) {
                expect(button.style.writingMode).not.toBe("vertical-rl");
            }
        } finally {
            renderer.unmount();
            host.remove();
        }
    });

    it("collapsed left border buttons have rotate(180deg) for bottom-to-top reading", () => {
        const model = Model.fromJson(emptyVerticalBordersFixture);
        const { host, renderer } = mountRenderer(model);

        try {
            const leftButtons = host.querySelectorAll<HTMLElement>(
                '.flexlayout__border_left[data-collapsed-strip="true"] button[data-collapsed-tab-item="true"]'
            );

            expect(leftButtons.length).toBeGreaterThan(0);

            for (const button of leftButtons) {
                expect(button.style.transform).toBe("rotate(180deg)");
            }
        } finally {
            renderer.unmount();
            host.remove();
        }
    });
});
