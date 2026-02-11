import { DockLocation } from "./DockLocation";
import { Rect } from "./Rect";

export const MIN_FLYOUT_SIZE = 100;
export const DEFAULT_SECONDARY_FRACTION = 0.25;
export const MAX_SECONDARY_FRACTION = 0.5;

export interface FlyoutRectParams {
    primarySize: number;
    location: DockLocation;
    layoutWidth: number;
    layoutHeight: number;
    insets: { top: number; right: number; bottom: number; left: number };
    tabButtonRect?: { x: number; y: number; width: number; height: number };
}

function clamp(value: number, min: number, max: number): number {
    return Math.max(min, Math.min(max, value));
}

export function computeFlyoutRect(params: FlyoutRectParams): Rect {
    const { primarySize, location, layoutWidth, layoutHeight, insets, tabButtonRect } = params;

    if (location === DockLocation.LEFT || location === DockLocation.RIGHT) {
        const availableHeight = layoutHeight - insets.top - insets.bottom;
        const secondarySize = Math.max(
            MIN_FLYOUT_SIZE,
            Math.min(availableHeight * MAX_SECONDARY_FRACTION, availableHeight * DEFAULT_SECONDARY_FRACTION),
        );
        const width = Math.max(MIN_FLYOUT_SIZE, primarySize);
        const height = secondarySize;

        let y: number;
        if (tabButtonRect) {
            y = clamp(tabButtonRect.y, insets.top, insets.top + availableHeight - height);
        } else {
            y = insets.top + (availableHeight - height) / 2;
        }

        if (location === DockLocation.LEFT) {
            return new Rect(insets.left, y, width, height);
        } else {
            return new Rect(layoutWidth - insets.right - width, y, width, height);
        }
    } else {
        const availableWidth = layoutWidth - insets.left - insets.right;
        const secondarySize = Math.max(
            MIN_FLYOUT_SIZE,
            Math.min(availableWidth * MAX_SECONDARY_FRACTION, availableWidth * DEFAULT_SECONDARY_FRACTION),
        );
        const width = secondarySize;
        const height = Math.max(MIN_FLYOUT_SIZE, primarySize);

        let x: number;
        if (tabButtonRect) {
            x = clamp(tabButtonRect.x, insets.left, insets.left + availableWidth - width);
        } else {
            x = insets.left + (availableWidth - width) / 2;
        }

        if (location === DockLocation.TOP) {
            return new Rect(x, insets.top, width, height);
        } else {
            return new Rect(x, layoutHeight - insets.bottom - height, width, height);
        }
    }
}
