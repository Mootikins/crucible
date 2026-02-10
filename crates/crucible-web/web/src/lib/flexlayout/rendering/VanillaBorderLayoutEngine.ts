import { DockLocation } from "../core/DockLocation";
import { BorderNode } from "../model/BorderNode";

const LOCATION_TIE_ORDER: Record<string, number> = {
    top: 0,
    right: 1,
    bottom: 2,
    left: 3,
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
