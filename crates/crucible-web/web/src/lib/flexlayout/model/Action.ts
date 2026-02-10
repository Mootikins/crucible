/**
 * Discriminated union type for all 29 FlexLayout action types
 * Each action type has a specific payload structure enforced by TypeScript
 */
export type LayoutAction =
  | { type: "ADD_NODE"; data: { json: any; toNodeId: string; location: string; index: number } }
  | { type: "MOVE_NODE"; data: { fromNode: string; toNode: string; location: string; index: number; select?: boolean } }
  | { type: "DELETE_TAB"; data: { node: string } }
  | { type: "DELETE_TABSET"; data: { node: string } }
  | { type: "RENAME_TAB"; data: { node: string; text: string } }
  | { type: "SELECT_TAB"; data: { tabNode: string; windowId?: string } }
  | { type: "SET_ACTIVE_TABSET"; data: { tabsetNode: string; windowId?: string } }
  | { type: "ADJUST_WEIGHTS"; data: { nodeId: string; weights: number[]; orientation: string } }
  | { type: "ADJUST_BORDER_SPLIT"; data: { borderId: string; size: number } }
  | { type: "MAXIMIZE_TOGGLE"; data: { tabsetId: string } }
  | { type: "UPDATE_MODEL_ATTRIBUTES"; data: { attributes: Record<string, any> } }
  | { type: "UPDATE_NODE_ATTRIBUTES"; data: { nodeId: string; attributes: Record<string, any> } }
  | { type: "POPOUT_TAB"; data: { tabId: string; windowId?: string } }
  | { type: "POPOUT_TABSET"; data: { tabsetId: string; windowId?: string } }
  | { type: "CLOSE_WINDOW"; data: { windowId: string } }
  | { type: "CREATE_WINDOW"; data: Record<string, never> }
  | { type: "FLOAT_TAB"; data: { tabId: string; x: number; y: number; width: number; height: number } }
  | { type: "FLOAT_TABSET"; data: { tabsetId: string; x: number; y: number; width: number; height: number } }
  | { type: "DOCK_TAB"; data: { tabId: string; location: string } }
  | { type: "DOCK_TABSET"; data: { tabsetId: string; location: string } }
  | { type: "MOVE_WINDOW"; data: { windowId: string; x: number; y: number; width: number; height: number } }
  | { type: "UNDO"; data: Record<string, never> }
  | { type: "REDO"; data: Record<string, never> }
  | { type: "SET_TAB_ICON"; data: { tabId: string; icon: string } }
  | { type: "SET_TAB_COMPONENT"; data: { tabId: string; component: string } }
  | { type: "SET_TAB_CONFIG"; data: { tabId: string; config: any } }
  | { type: "SET_TAB_ENABLE_CLOSE"; data: { tabId: string; enableClose: boolean } }
  | { type: "SET_DOCK_STATE"; data: { nodeId: string; state: "expanded" | "collapsed" | "hidden" } }
  | { type: "SET_VISIBLE_TABS"; data: { nodeId: string; tabs: number[] } }
  | { type: "PIN_TAB"; data: { tabId: string } }
  | { type: "UNPIN_TAB"; data: { tabId: string } }
  | { type: "PIN_BORDER"; data: { borderId: string } }
  | { type: "UNPIN_BORDER"; data: { borderId: string } }
  | { type: "OPEN_FLYOUT"; data: { borderId: string; tabId: string } }
  | { type: "CLOSE_FLYOUT"; data: { borderId: string } }
  | { type: "SET_FLYOUT_SIZE"; data: { borderId: string; size: number } }
  | { type: "SET_TABSET_MODE"; data: { tabsetId: string; mode: "tabs" | "paneview" } }
  | { type: string; [key: string]: unknown };  // Escape hatch for extensibility

/**
 * Backward compatibility alias
 */
export type IAction = LayoutAction;

/**
 * Action creators for all 29 FlexLayout action types
 */
export const Action = {
  addNode(
    json: any,
    toNodeId: string,
    location: string,
    index: number
  ): Extract<LayoutAction, { type: "ADD_NODE" }> {
    return {
      type: "ADD_NODE",
      data: { json, toNodeId, location, index },
    };
  },

  moveNode(
    fromNodeId: string,
    toNodeId: string,
    location: string,
    index: number,
    select?: boolean
  ): Extract<LayoutAction, { type: "MOVE_NODE" }> {
    return {
      type: "MOVE_NODE",
      data: { fromNode: fromNodeId, toNode: toNodeId, location, index, select },
    };
  },

  deleteTab(node: string): Extract<LayoutAction, { type: "DELETE_TAB" }> {
    return {
      type: "DELETE_TAB",
      data: { node },
    };
  },

  deleteTabset(node: string): Extract<LayoutAction, { type: "DELETE_TABSET" }> {
    return {
      type: "DELETE_TABSET",
      data: { node },
    };
  },

  renameTab(node: string, text: string): Extract<LayoutAction, { type: "RENAME_TAB" }> {
    return {
      type: "RENAME_TAB",
      data: { node, text },
    };
  },

  selectTab(tabNode: string, windowId?: string): Extract<LayoutAction, { type: "SELECT_TAB" }> {
    return {
      type: "SELECT_TAB",
      data: { tabNode, windowId },
    };
  },

  setActiveTabset(tabsetNode: string, windowId?: string): Extract<LayoutAction, { type: "SET_ACTIVE_TABSET" }> {
    return {
      type: "SET_ACTIVE_TABSET",
      data: { tabsetNode, windowId },
    };
  },

  adjustWeights(
    nodeId: string,
    weights: number[],
    orientation: string
  ): Extract<LayoutAction, { type: "ADJUST_WEIGHTS" }> {
    return {
      type: "ADJUST_WEIGHTS",
      data: { nodeId, weights, orientation },
    };
  },

  adjustBorderSplit(
    borderId: string,
    size: number
  ): Extract<LayoutAction, { type: "ADJUST_BORDER_SPLIT" }> {
    return {
      type: "ADJUST_BORDER_SPLIT",
      data: { borderId, size },
    };
  },

  maximizeToggle(tabsetId: string): Extract<LayoutAction, { type: "MAXIMIZE_TOGGLE" }> {
    return {
      type: "MAXIMIZE_TOGGLE",
      data: { tabsetId },
    };
  },

  updateModelAttributes(attributes: Record<string, any>): Extract<LayoutAction, { type: "UPDATE_MODEL_ATTRIBUTES" }> {
    return {
      type: "UPDATE_MODEL_ATTRIBUTES",
      data: { attributes },
    };
  },

  updateNodeAttributes(
    nodeId: string,
    attributes: Record<string, any>
  ): Extract<LayoutAction, { type: "UPDATE_NODE_ATTRIBUTES" }> {
    return {
      type: "UPDATE_NODE_ATTRIBUTES",
      data: { nodeId, attributes },
    };
  },

  popoutTab(tabId: string, windowId?: string): Extract<LayoutAction, { type: "POPOUT_TAB" }> {
    return {
      type: "POPOUT_TAB",
      data: { tabId, windowId },
    };
  },

  popoutTabset(tabsetId: string, windowId?: string): Extract<LayoutAction, { type: "POPOUT_TABSET" }> {
    return {
      type: "POPOUT_TABSET",
      data: { tabsetId, windowId },
    };
  },

  closeWindow(windowId: string): Extract<LayoutAction, { type: "CLOSE_WINDOW" }> {
    return {
      type: "CLOSE_WINDOW",
      data: { windowId },
    };
  },

  createWindow(): Extract<LayoutAction, { type: "CREATE_WINDOW" }> {
    return {
      type: "CREATE_WINDOW",
      data: {},
    };
  },

  floatTab(tabId: string, x: number, y: number, width: number, height: number): Extract<LayoutAction, { type: "FLOAT_TAB" }> {
    return {
      type: "FLOAT_TAB",
      data: { tabId, x, y, width, height },
    };
  },

  floatTabset(tabsetId: string, x: number, y: number, width: number, height: number): Extract<LayoutAction, { type: "FLOAT_TABSET" }> {
    return {
      type: "FLOAT_TABSET",
      data: { tabsetId, x, y, width, height },
    };
  },

  dockTab(tabId: string, location: string): Extract<LayoutAction, { type: "DOCK_TAB" }> {
    return {
      type: "DOCK_TAB",
      data: { tabId, location },
    };
  },

  dockTabset(tabsetId: string, location: string): Extract<LayoutAction, { type: "DOCK_TABSET" }> {
    return {
      type: "DOCK_TABSET",
      data: { tabsetId, location },
    };
  },

  moveWindow(windowId: string, x: number, y: number, width: number, height: number): Extract<LayoutAction, { type: "MOVE_WINDOW" }> {
    return {
      type: "MOVE_WINDOW",
      data: { windowId, x, y, width, height },
    };
  },

  undoAction(): Extract<LayoutAction, { type: "UNDO" }> {
    return {
      type: "UNDO",
      data: {},
    };
  },

  redoAction(): Extract<LayoutAction, { type: "REDO" }> {
    return {
      type: "REDO",
      data: {},
    };
  },

  setTabIcon(tabId: string, icon: string): Extract<LayoutAction, { type: "SET_TAB_ICON" }> {
    return {
      type: "SET_TAB_ICON",
      data: { tabId, icon },
    };
  },

  setTabComponent(tabId: string, component: string): Extract<LayoutAction, { type: "SET_TAB_COMPONENT" }> {
    return {
      type: "SET_TAB_COMPONENT",
      data: { tabId, component },
    };
  },

  setTabConfig(tabId: string, config: any): Extract<LayoutAction, { type: "SET_TAB_CONFIG" }> {
    return {
      type: "SET_TAB_CONFIG",
      data: { tabId, config },
    };
  },

  setTabEnableClose(tabId: string, enableClose: boolean): Extract<LayoutAction, { type: "SET_TAB_ENABLE_CLOSE" }> {
    return {
      type: "SET_TAB_ENABLE_CLOSE",
      data: { tabId, enableClose },
    };
  },

  setDockState(nodeId: string, state: "expanded" | "collapsed" | "hidden"): Extract<LayoutAction, { type: "SET_DOCK_STATE" }> {
    return {
      type: "SET_DOCK_STATE",
      data: { nodeId, state },
    };
  },

  setVisibleTabs(nodeId: string, tabs: number[]): Extract<LayoutAction, { type: "SET_VISIBLE_TABS" }> {
    return {
      type: "SET_VISIBLE_TABS",
      data: { nodeId, tabs },
    };
  },

  pinTab(tabId: string): Extract<LayoutAction, { type: "PIN_TAB" }> {
    return {
      type: "PIN_TAB",
      data: { tabId },
    };
  },

  unpinTab(tabId: string): Extract<LayoutAction, { type: "UNPIN_TAB" }> {
    return {
      type: "UNPIN_TAB",
      data: { tabId },
    };
  },

  pinBorder(borderId: string): Extract<LayoutAction, { type: "PIN_BORDER" }> {
    return {
      type: "PIN_BORDER",
      data: { borderId },
    };
  },

  unpinBorder(borderId: string): Extract<LayoutAction, { type: "UNPIN_BORDER" }> {
    return {
      type: "UNPIN_BORDER",
      data: { borderId },
    };
  },

  openFlyout(borderId: string, tabId: string): Extract<LayoutAction, { type: "OPEN_FLYOUT" }> {
    return {
      type: "OPEN_FLYOUT",
      data: { borderId, tabId },
    };
  },

  closeFlyout(borderId: string): Extract<LayoutAction, { type: "CLOSE_FLYOUT" }> {
    return {
      type: "CLOSE_FLYOUT",
      data: { borderId },
    };
  },

  setFlyoutSize(borderId: string, size: number): Extract<LayoutAction, { type: "SET_FLYOUT_SIZE" }> {
    return {
      type: "SET_FLYOUT_SIZE",
      data: { borderId, size },
    };
  },

  setTabsetMode(tabsetId: string, mode: "tabs" | "paneview"): Extract<LayoutAction, { type: "SET_TABSET_MODE" }> {
    return {
      type: "SET_TABSET_MODE",
      data: { tabsetId, mode },
    };
  },
};
