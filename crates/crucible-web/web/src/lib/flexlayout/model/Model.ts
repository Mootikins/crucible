// Stub Model class - full implementation to be ported from FlexLayout
// This is a placeholder to allow other node types to compile

export const DefaultMax = 100000;
export const DefaultMin = 0;

export class Model {
	static readonly MAIN_WINDOW_ID = "main";

	private windowsMap = new Map<string, any>();

	nextUniqueId(): string {
		return Math.random().toString(36).substr(2, 9);
	}

	addNode(_node: any): void {}

	getwindowsMap(): Map<string, any> {
		return this.windowsMap;
	}

	getMaximizedTabset(_windowId: string): any {
		return undefined;
	}

	getActiveTabset(_windowId: string): any {
		return undefined;
	}

	setActiveTabset(_tabset: any, _windowId: string): void {}

	getRoot(_windowId: string): any {
		return undefined;
	}

	getOnAllowDrop(): any {
		return undefined;
	}

	getOnCreateTabSet(): any {
		return undefined;
	}

	tidy(): void {}

	getAttribute(_name: string): any {
		return undefined;
	}

	isRootOrientationVertical(): boolean {
		return true;
	}
}
