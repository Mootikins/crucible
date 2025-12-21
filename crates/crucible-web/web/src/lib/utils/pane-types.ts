export type PaneType = 'chat' | 'document-tree' | 'document-view' | 'tabs';

export interface PaneConfig {
	id: string;
	type: PaneType;
	size: number; // width or height in pixels
	visible: boolean;
	order: number;
}

export interface LayoutState {
	sidebarWidth: number;
	sidebarVisible: boolean;
	panes: PaneConfig[];
	activePane: string | null;
}

export const DEFAULT_LAYOUT: LayoutState = {
	sidebarWidth: 250,
	sidebarVisible: true,
	panes: [
		{
			id: 'document-tree',
			type: 'document-tree',
			size: 250,
			visible: true,
			order: 0
		},
		{
			id: 'chat',
			type: 'chat',
			size: 0, // Takes remaining space
			visible: true,
			order: 1
		}
	],
	activePane: 'chat'
};

