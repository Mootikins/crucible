import { Component, createSignal, createEffect, onCleanup, ParentProps } from "solid-js";
import { CLASSES } from "../flexlayout/core/Types";
import type { ILayoutContext } from "./Layout";
import { LayoutWindow } from "../flexlayout/model/LayoutWindow";

export interface IPopoutWindowProps {
    title: string;
    layout: ILayoutContext;
    layoutWindow: LayoutWindow;
    url: string;
    onCloseWindow: (layoutWindow: LayoutWindow) => void;
    onSetWindow: (layoutWindow: LayoutWindow, window: Window) => void;
}

/** @internal */
export const PopoutWindow: Component<ParentProps<IPopoutWindowProps>> = (props) => {
    let popoutWindow: Window | null = null;
    const [content, setContent] = createSignal<HTMLElement | undefined>(undefined);
    // map from main docs style -> this docs equivalent style
    const styleMap = new Map<HTMLElement, HTMLElement>();

    createEffect(() => {
        if (!popoutWindow) { // only create window once, even in strict mode
            const windowId = props.layoutWindow.windowId;
            const rect = props.layoutWindow.rect;
            
            popoutWindow = window.open(props.url, windowId, `left=${rect.x},top=${rect.y},width=${rect.width},height=${rect.height}`);

            if (popoutWindow) {
                props.layoutWindow.window = popoutWindow;
                props.onSetWindow(props.layoutWindow, popoutWindow);

                // listen for parent unloading to remove all popouts
                const handleParentUnload = () => {
                    if (popoutWindow) {
                        const closedWindow = popoutWindow;
                        popoutWindow = null; // need to set to null before close, since this will trigger popup window before unload...
                        closedWindow.close();
                    }
                };
                window.addEventListener("beforeunload", handleParentUnload);

                const handlePopoutLoad = () => {
                    if (popoutWindow) {
                        popoutWindow.focus();

                        // note: resizeto must be before moveto in chrome otherwise the window will end up at 0,0
                        popoutWindow.resizeTo(rect.width, rect.height);
                        popoutWindow.moveTo(rect.x, rect.y);

                        const popoutDocument = popoutWindow.document;
                        popoutDocument.title = props.title;
                        const popoutContent = popoutDocument.createElement("div");
                        popoutContent.className = CLASSES.FLEXLAYOUT__FLOATING_WINDOW_CONTENT;
                        popoutDocument.body.appendChild(popoutContent);
                        copyStyles(popoutDocument, styleMap).then(() => {
                            setContent(popoutContent); // re-render once link styles loaded
                        });

                        // listen for style mutations
                        const observer = new MutationObserver((mutationsList: any) => handleStyleMutations(mutationsList, popoutDocument, styleMap));
                        observer.observe(document.head, { childList: true });

                        // listen for popout unloading (needs to be after load for safari)
                        const handlePopoutUnload = () => {
                            if (popoutWindow) {
                                props.onCloseWindow(props.layoutWindow); // remove the layoutWindow in the model
                                popoutWindow = null;
                                observer.disconnect();
                            }
                        };
                        popoutWindow.addEventListener("beforeunload", handlePopoutUnload);

                        onCleanup(() => {
                            popoutWindow?.removeEventListener("beforeunload", handlePopoutUnload);
                        });
                    }
                };
                popoutWindow.addEventListener("load", handlePopoutLoad);

                onCleanup(() => {
                    window.removeEventListener("beforeunload", handleParentUnload);
                    popoutWindow?.removeEventListener("load", handlePopoutLoad);
                });
            } else {
                console.warn(`Unable to open window ${props.url}`);
                props.onCloseWindow(props.layoutWindow); // remove the layoutWindow in the model
            }
        }

        onCleanup(() => {
            if (!props.layout.model.getwindowsMap().has(props.layoutWindow.windowId)) {
                popoutWindow?.close();
                popoutWindow = null;
            }
        });
    });

    return (() => {
        const c = content();
        if (c !== undefined) {
            // In SolidJS, we need to manually append children to the portal target
            // This is a simplified approach - we'll render children into the popout content
            return <>{props.children}</>;
        } else {
            return null;
        }
    })();
};

function handleStyleMutations(mutationsList: any, popoutDocument: Document, styleMap: Map<HTMLElement, HTMLElement>) {
    for (const mutation of mutationsList) {
        if (mutation.type === 'childList') {
            for (const addition of mutation.addedNodes) {
                if (addition instanceof HTMLLinkElement || addition instanceof HTMLStyleElement) {
                    copyStyle(popoutDocument, addition, styleMap);
                }
            }
            for (const removal of mutation.removedNodes) {
                if (removal instanceof HTMLLinkElement || removal instanceof HTMLStyleElement) {
                    const popoutStyle = styleMap.get(removal);
                    if (popoutStyle) {
                        popoutDocument.head.removeChild(popoutStyle);
                    }
                }
            }
        }
    }
}

/** @internal */
function copyStyles(popoutDoc: Document, styleMap: Map<HTMLElement, HTMLElement>): Promise<boolean[]> {
    const promises: Promise<boolean>[] = [];
    const styleElements = document.querySelectorAll('style, link[rel="stylesheet"]') as NodeListOf<HTMLElement>
    for (const element of styleElements) {
        copyStyle(popoutDoc, element, styleMap, promises);
    }
    return Promise.all(promises);
}

/** @internal */
function copyStyle(popoutDoc: Document, element: HTMLElement, styleMap: Map<HTMLElement, HTMLElement>, promises?: Promise<boolean>[]) {
    if (element instanceof HTMLLinkElement) {
        // prefer links since they will keep paths to images etc
        const linkElement = element.cloneNode(true) as HTMLLinkElement;
        popoutDoc.head.appendChild(linkElement);
        styleMap.set(element, linkElement);

        if (promises) {
            promises.push(new Promise((resolve) => {
                linkElement.onload = () => resolve(true);
            }));
        }
    } else if (element instanceof HTMLStyleElement) {
        try {
            const styleElement = element.cloneNode(true) as HTMLStyleElement;
            popoutDoc.head.appendChild(styleElement);
            styleMap.set(element, styleElement);
        } catch (e) {
            // can throw an exception
        }
    }
}
