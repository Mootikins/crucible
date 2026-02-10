import { DockLocation } from "../core/DockLocation";
import { Action, type LayoutAction } from "../model/Action";
import { BorderNode } from "../model/BorderNode";
import { TabNode } from "../model/TabNode";

// Nesting order determines which borders "own" corner space.
// Horizontal borders (top/bottom) sort first → nest outermost → span full width.
// Vertical borders (left/right) sort later → nest inside → span between top and bottom.
// This produces the standard IDE layout where top/bottom bars are full-width
// and left/right sidebars fit between them.
const LOCATION_TIE_ORDER: Record<string, number> = {
    top: 0,
    bottom: 1,
    left: 2,
    right: 3,
};

export const BORDER_BAR_SIZE = 38;

export function computeNestingOrder(borders: BorderNode[]): BorderNode[] {
    return [...borders].sort((a, b) => {
        const priorityDiff = b.getPriority() - a.getPriority();
        if (priorityDiff !== 0) {
            return priorityDiff;
        }
        return (LOCATION_TIE_ORDER[a.getLocation().getName()] ?? 4)
            - (LOCATION_TIE_ORDER[b.getLocation().getName()] ?? 4);
    });
}

export interface IVisibleBorderStrip {
    border: BorderNode;
    show: boolean;
}

export function collectVisibleBorderStrips(
    borders: Map<DockLocation, BorderNode>,
    hiddenBorderLocation: DockLocation,
): Map<string, IVisibleBorderStrip> {
    const strips = new Map<string, IVisibleBorderStrip>();

    for (const [, location] of DockLocation.values) {
        const border = borders.get(location);
        if (!border) {
            continue;
        }
        if (!border.isShowing()) {
            continue;
        }

        const shouldShow = !border.isAutoHide()
            || border.getChildren().length > 0
            || hiddenBorderLocation === location;

        if (shouldShow) {
            strips.set(location.getName(), {
                border,
                show: border.getSelected() !== -1,
            });
        }
    }

    return strips;
}

export function handleCollapsedBorderTabClick(
    border: BorderNode,
    tab: TabNode,
    doAction: (action: LayoutAction) => void,
): void {
    for (const other of border.getModel().getBorderSet().getBorders()) {
        if (other.getId() !== border.getId() && other.getFlyoutTabId() !== null) {
            doAction(Action.closeFlyout(other.getId()));
        }
    }

    if (border.getFlyoutTabId() === tab.getId()) {
        doAction(Action.closeFlyout(border.getId()));
        return;
    }

    doAction(Action.openFlyout(border.getId(), tab.getId()));
}
