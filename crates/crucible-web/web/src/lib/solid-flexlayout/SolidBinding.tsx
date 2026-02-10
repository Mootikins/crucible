import { Component, onCleanup, onMount } from "solid-js";
import { render } from "solid-js/web";
import { Layout as LegacyLayout } from "../solid-flexlayout-legacy/Layout";
import type { ILayoutProps } from "./LayoutTypes";
import { VanillaLayoutRenderer } from "../flexlayout/rendering/VanillaLayoutRenderer";
import type { IContentRenderer, IRenderParams } from "../flexlayout/rendering/IContentRenderer";
import { CLASSES } from "../flexlayout/core/Types";

class SolidContentRenderer implements IContentRenderer {
    private container: HTMLElement | undefined;
    private params: IRenderParams | undefined;
    private disposeFn: (() => void) | undefined;
    private error: Error | undefined;

    constructor(private readonly factory: ILayoutProps["factory"]) {}

    init(container: HTMLElement, params: IRenderParams): void {
        this.container = container;
        this.params = params;
        this.error = undefined;
        this.mount();
    }

    update(params: Partial<IRenderParams>): void {
        if (!this.params) {
            return;
        }
        this.params = { ...this.params, ...params };
        this.mount();
    }

    dispose(): void {
        if (this.disposeFn) {
            this.disposeFn();
            this.disposeFn = undefined;
        }
        this.container = undefined;
        this.params = undefined;
        this.error = undefined;
    }

    private mount(): void {
        if (!this.container || !this.params) {
            return;
        }
        if (this.disposeFn) {
            this.disposeFn();
            this.disposeFn = undefined;
        }
        
        try {
            this.error = undefined;
            this.disposeFn = render(() => this.factory(this.params!.node), this.container!);
        } catch (err) {
            this.error = err instanceof Error ? err : new Error(String(err));
            console.error("Content render error:", this.error);
            this.showErrorFallback();
        }
    }

    private showErrorFallback(): void {
        if (!this.container) {
            return;
        }
        
        // Clear container
        this.container.innerHTML = "";
        
        // Create fallback UI
        const errorDiv = document.createElement("div");
        errorDiv.className = CLASSES.FLEXLAYOUT__ERROR_BOUNDARY_CONTAINER;
        
        const contentDiv = document.createElement("div");
        contentDiv.className = CLASSES.FLEXLAYOUT__ERROR_BOUNDARY_CONTENT;
        
        const messageDiv = document.createElement("div");
        messageDiv.style.display = "flex";
        messageDiv.style.flexDirection = "column";
        messageDiv.style.alignItems = "center";
        messageDiv.style.gap = "1rem";
        
        const errorMessage = document.createElement("p");
        errorMessage.textContent = "Content failed to render";
        errorMessage.style.margin = "0";
        errorMessage.style.fontWeight = "bold";
        
        const errorDetails = document.createElement("p");
        errorDetails.textContent = this.error?.message || "Unknown error";
        errorDetails.style.margin = "0";
        errorDetails.style.fontSize = "0.875rem";
        errorDetails.style.opacity = "0.7";
        
        const retryButton = document.createElement("button");
        retryButton.textContent = "Retry";
        retryButton.style.padding = "0.5rem 1rem";
        retryButton.style.cursor = "pointer";
        retryButton.onclick = () => {
            this.mount();
        };
        
        messageDiv.appendChild(errorMessage);
        messageDiv.appendChild(errorDetails);
        messageDiv.appendChild(retryButton);
        contentDiv.appendChild(messageDiv);
        errorDiv.appendChild(contentDiv);
        this.container.appendChild(errorDiv);
    }
}

export const SolidBinding: Component<ILayoutProps> = (props) => {
    let containerRef: HTMLDivElement | undefined;
    let renderer: VanillaLayoutRenderer | undefined;

    const useVanilla = () => {
        const config = window as Window & { __FLEXLAYOUT_VANILLA__?: string };
        return config.__FLEXLAYOUT_VANILLA__ === "1";
    };

    onMount(() => {
        if (!useVanilla() || !containerRef) {
            return;
        }

        renderer = new VanillaLayoutRenderer({
            model: props.model,
            getClassName: (className) => props.classNameMapper?.(className) ?? className,
            doAction: (action) => {
                props.model.doAction(action);
            },
            onModelChange: props.onModelChange,
            onAction: props.onAction,
            onContextMenu: props.onContextMenu,
            onAllowDrop: props.onAllowDrop,
            createContentRenderer: () => new SolidContentRenderer(props.factory),
        });

        renderer.mount(containerRef);
    });

    onCleanup(() => {
        renderer?.unmount();
        renderer = undefined;
    });

    if (!useVanilla()) {
        return <LegacyLayout {...props} />;
    }

    return <div ref={containerRef} style={{ width: "100%", height: "100%", position: "relative" }} />;
};
