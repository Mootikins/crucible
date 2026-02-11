import { TabSetNode } from "./TabSetNode";
import { BorderNode } from "./BorderNode";
import { RowNode } from "./RowNode";
import { TabNode } from "./TabNode";

/** @internal */
export function adjustSelectedIndexAfterDock(node: TabNode) {
	const parent = node.getParent();
	if (parent !== null && (parent instanceof TabSetNode || parent instanceof BorderNode)) {
		const children = parent.getChildren();
		for (let i = 0; i < children.length; i++) {
			const child = children[i] as TabNode;
			if (child === node) {
				parent.setSelected(i);
				return;
			}
		}
	}
}

/** @internal */
export function adjustSelectedIndex(parent: TabSetNode | BorderNode | RowNode, removedIndex: number) {
	if (parent !== undefined && (parent instanceof TabSetNode || parent instanceof BorderNode)) {
		const selectedIndex = (parent as TabSetNode | BorderNode).getSelected();
		if (selectedIndex !== -1) {
			if (removedIndex === selectedIndex && parent.getChildren().length > 0) {
				if (removedIndex >= parent.getChildren().length) {
					parent.setSelected(parent.getChildren().length - 1);
				}
			} else if (removedIndex < selectedIndex) {
				parent.setSelected(selectedIndex - 1);
			} else if (removedIndex > selectedIndex) {
				// leave selected index as is
			} else {
				parent.setSelected(-1);
			}
		}
	}
}

export function randomUUID(): string {
	const template = "xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx";
	return template.replace(/[xy]/g, (c) => {
		const r = (Math.random() * 16) | 0;
		const v = c === "x" ? r : (r & 0x3) | 0x8;
		return v.toString(16);
	});
}

export function canDockToWindow(node: any): boolean {
	if (node instanceof TabNode) {
		return node.isEnablePopout();
	} else if (node instanceof TabSetNode) {
		for (const child of node.getChildren()) {
			if ((child as TabNode).isEnablePopout() === false) {
				return false;
			}
		}
		return true;
	}
	return true;
}

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
