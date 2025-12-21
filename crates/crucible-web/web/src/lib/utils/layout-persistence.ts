import type { LayoutState } from './pane-types';
import { DEFAULT_LAYOUT } from './pane-types';

const STORAGE_KEY = 'crucible-layout';

export function saveLayout(layout: LayoutState): void {
	if (typeof window === 'undefined') return;
	try {
		localStorage.setItem(STORAGE_KEY, JSON.stringify(layout));
	} catch (error) {
		console.error('Failed to save layout:', error);
	}
}

export function loadLayout(): LayoutState | null {
	if (typeof window === 'undefined') return null;
	try {
		const stored = localStorage.getItem(STORAGE_KEY);
		if (!stored) return null;

		const layout = JSON.parse(stored) as LayoutState;
		
		// Validate layout structure
		if (
			typeof layout.sidebarWidth === 'number' &&
			typeof layout.sidebarVisible === 'boolean' &&
			Array.isArray(layout.panes) &&
			layout.panes.every(
				(p) =>
					typeof p.id === 'string' &&
					typeof p.type === 'string' &&
					typeof p.size === 'number' &&
					typeof p.visible === 'boolean' &&
					typeof p.order === 'number'
			)
		) {
			return layout;
		}
	} catch (error) {
		console.error('Failed to load layout:', error);
	}
	
	return null;
}

export function getDefaultLayout(): LayoutState {
	return { ...DEFAULT_LAYOUT };
}

