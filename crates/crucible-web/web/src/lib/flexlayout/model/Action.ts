/**
 * Action type definition for FlexLayout model mutations
 */
export interface IAction {
  type: string;
  data?: Record<string, any>;
}

/**
 * Action creators for all 24 FlexLayout action types
 */
export const Action = {
  addNode(
    json: any,
    toNodeId: string,
    location: string,
    index: number
  ): IAction {
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
  ): IAction {
    return {
      type: "MOVE_NODE",
      data: { fromNode: fromNodeId, toNode: toNodeId, location, index, select },
    };
  },

  deleteTab(node: string): IAction {
    return {
      type: "DELETE_TAB",
      data: { node },
    };
  },

  deleteTabset(node: string): IAction {
    return {
      type: "DELETE_TABSET",
      data: { node },
    };
  },

  renameTab(node: string, text: string): IAction {
    return {
      type: "RENAME_TAB",
      data: { node, text },
    };
  },

  selectTab(tabNode: string, windowId?: string): IAction {
    return {
      type: "SELECT_TAB",
      data: { tabNode, windowId },
    };
  },

  setActiveTabset(tabsetNode: string, windowId?: string): IAction {
    return {
      type: "SET_ACTIVE_TABSET",
      data: { tabsetNode, windowId },
    };
  },

  adjustWeights(
    nodeId: string,
    weights: number[],
    orientation: string
  ): IAction {
    return {
      type: "ADJUST_WEIGHTS",
      data: { nodeId, weights, orientation },
    };
  },

  adjustBorderSplit(
    borderId: string,
    size: number
  ): IAction {
    return {
      type: "ADJUST_BORDER_SPLIT",
      data: { borderId, size },
    };
  },

  maximizeToggle(tabsetId: string): IAction {
    return {
      type: "MAXIMIZE_TOGGLE",
      data: { tabsetId },
    };
  },

  updateModelAttributes(attributes: Record<string, any>): IAction {
    return {
      type: "UPDATE_MODEL_ATTRIBUTES",
      data: { attributes },
    };
  },

  updateNodeAttributes(
    nodeId: string,
    attributes: Record<string, any>
  ): IAction {
    return {
      type: "UPDATE_NODE_ATTRIBUTES",
      data: { nodeId, attributes },
    };
  },

  popoutTab(tabId: string, windowId?: string): IAction {
    return {
      type: "POPOUT_TAB",
      data: { tabId, windowId },
    };
  },

  popoutTabset(tabsetId: string, windowId?: string): IAction {
    return {
      type: "POPOUT_TABSET",
      data: { tabsetId, windowId },
    };
  },

  closeWindow(windowId: string): IAction {
    return {
      type: "CLOSE_WINDOW",
      data: { windowId },
    };
  },

  createWindow(): IAction {
    return {
      type: "CREATE_WINDOW",
      data: {},
    };
  },

  floatTab(tabId: string, x: number, y: number, width: number, height: number): IAction {
    return {
      type: "FLOAT_TAB",
      data: { tabId, x, y, width, height },
    };
  },

  floatTabset(tabsetId: string, x: number, y: number, width: number, height: number): IAction {
    return {
      type: "FLOAT_TABSET",
      data: { tabsetId, x, y, width, height },
    };
  },

  dockTab(tabId: string, location: string): IAction {
    return {
      type: "DOCK_TAB",
      data: { tabId, location },
    };
  },

  dockTabset(tabsetId: string, location: string): IAction {
    return {
      type: "DOCK_TABSET",
      data: { tabsetId, location },
    };
  },

  moveWindow(windowId: string, x: number, y: number, width: number, height: number): IAction {
    return {
      type: "MOVE_WINDOW",
      data: { windowId, x, y, width, height },
    };
  },

  undoAction(): IAction {
    return {
      type: "UNDO",
      data: {},
    };
  },

  redoAction(): IAction {
    return {
      type: "REDO",
      data: {},
    };
  },

  setTabIcon(tabId: string, icon: string): IAction {
    return {
      type: "SET_TAB_ICON",
      data: { tabId, icon },
    };
  },

  setTabComponent(tabId: string, component: string): IAction {
    return {
      type: "SET_TAB_COMPONENT",
      data: { tabId, component },
    };
  },

  setTabConfig(tabId: string, config: any): IAction {
    return {
      type: "SET_TAB_CONFIG",
      data: { tabId, config },
    };
  },

  setTabEnableClose(tabId: string, enableClose: boolean): IAction {
    return {
      type: "SET_TAB_ENABLE_CLOSE",
      data: { tabId, enableClose },
    };
  },

  setDockState(nodeId: string, state: "expanded" | "collapsed" | "minimized"): IAction {
    return {
      type: "SET_DOCK_STATE",
      data: { nodeId, state },
    };
  },

  setVisibleTabs(nodeId: string, tabs: number[]): IAction {
    return {
      type: "SET_VISIBLE_TABS",
      data: { nodeId, tabs },
    };
  },
};
