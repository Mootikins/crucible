import { Rect } from "../core/Rect";
import { Orientation } from "../core/Orientation";
import { Node } from "../model/Node";
import { RowNode } from "../model/RowNode";
import { TabSetNode } from "../model/TabSetNode";
import { BorderNode } from "../model/BorderNode";

/**
 * Pure TypeScript layout engine for FlexLayout.
 * Calculates rects for all nodes in the tree based on weights and constraints.
 */
export class LayoutEngine {
    /**
     * Calculate layout for a node tree, assigning rects to all nodes.
     * @param root The root node (typically a RowNode)
     * @param containerRect The available space for layout
     */
    static calculateLayout(root: Node, containerRect: Rect): void {
        if (root instanceof RowNode) {
            root.calcMinMaxSize();
        }
        root.setRect(containerRect);
        this.layoutNode(root, containerRect);
    }

    private static layoutNode(node: Node, rect: Rect): void {
        if (node instanceof RowNode) {
            this.layoutRow(node, rect);
        } else if (node instanceof TabSetNode) {
            this.layoutTabSet(node, rect);
        } else if (node instanceof BorderNode) {
            this.layoutBorder(node, rect);
        }
    }

    private static layoutRow(row: RowNode, rect: Rect): void {
        const children = row.getChildren();
        if (children.length === 0) {
            row.setRect(rect);
            return;
        }

        const isHorizontal = row.getOrientation() === Orientation.HORZ;
        const splitterSize = 0;

        const availableSpace = isHorizontal ? rect.width : rect.height;
        const totalSplitterSpace = Math.max(0, (children.length - 1) * splitterSize);
        const layoutSpace = availableSpace - totalSplitterSpace;

        const sizes = this.calculateChildSizes(row, children, layoutSpace);

        let position = isHorizontal ? rect.x : rect.y;
        for (let i = 0; i < children.length; i++) {
            const child = children[i] as RowNode | TabSetNode;
            let size = sizes[i];

            // Check if this child is maximized
            if (child instanceof TabSetNode) {
                const model = child.getModel();
                const maximizedTabset = model.getMaximizedTabset(child.getWindowId());
                if (maximizedTabset === child) {
                    // Maximized tabset fills entire space
                    size = availableSpace;
                } else if (maximizedTabset !== undefined) {
                    // Another tabset is maximized, hide this one
                    size = 0;
                }
            }

            let childRect: Rect;
            if (isHorizontal) {
                childRect = new Rect(position, rect.y, size, rect.height);
            } else {
                childRect = new Rect(rect.x, position, rect.width, size);
            }

            child.setRect(childRect);
            this.layoutNode(child, childRect);

            position += size + splitterSize;
        }
    }

    private static layoutTabSet(tabset: TabSetNode, rect: Rect): void {
        tabset.setRect(rect);
    }

    private static layoutBorder(border: BorderNode, rect: Rect): void {
        border.setRect(rect);
    }

    private static calculateChildSizes(
        row: RowNode,
        children: Node[],
        availableSpace: number
    ): number[] {
        const sizes: number[] = [];
        const isHorizontal = row.getOrientation() === Orientation.HORZ;

        const weights: number[] = [];
        const minSizes: number[] = [];
        const maxSizes: number[] = [];

        for (const child of children) {
            const c = child as RowNode | TabSetNode;
            weights.push(c.getWeight());
            if (isHorizontal) {
                minSizes.push(c.getMinWidth());
                maxSizes.push(c.getMaxWidth());
            } else {
                minSizes.push(c.getMinHeight());
                maxSizes.push(c.getMaxHeight());
            }
        }

        const totalWeight = weights.reduce((a, b) => a + b, 0);

        for (let i = 0; i < children.length; i++) {
            const proportion = weights[i] / totalWeight;
            let size = Math.round(availableSpace * proportion);

            size = Math.max(minSizes[i], Math.min(maxSizes[i], size));
            sizes.push(size);
        }

        const totalSize = sizes.reduce((a, b) => a + b, 0);
        if (totalSize !== availableSpace) {
            const diff = availableSpace - totalSize;
            sizes[sizes.length - 1] += diff;
        }

        return sizes;
    }
}
