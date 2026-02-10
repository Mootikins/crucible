import { describe, it, expect } from "bun:test";
import { Action, LayoutAction, IAction } from "../model/Action";

describe("Action type system", () => {
  it("should create typed MOVE_NODE actions", () => {
    const action = Action.moveNode("node1", "node2", "center", 0);
    expect(action.type).toBe("MOVE_NODE");
    expect(action.data.fromNode).toBe("node1");
    expect(action.data.toNode).toBe("node2");
  });

  it("should create typed ADD_NODE actions", () => {
    const action = Action.addNode({ type: "tab" }, "node1", "center", 0);
    expect(action.type).toBe("ADD_NODE");
    expect(action.data.toNodeId).toBe("node1");
  });

  it("should support type narrowing in switch statements", () => {
    const moveAction = Action.moveNode("a", "b", "center", 0);
    
    switch (moveAction.type) {
      case "MOVE_NODE":
        // TypeScript should narrow to MOVE_NODE type
        expect(moveAction.data.fromNode).toBe("a");
        expect(moveAction.data.toNode).toBe("b");
        break;
      default:
        throw new Error("Should not reach here");
    }
  });

  it("should maintain backward compatibility with IAction alias", () => {
    const action: IAction = Action.moveNode("a", "b", "center", 0);
    expect(action.type).toBe("MOVE_NODE");
  });

  it("should support all 29 action types", () => {
    const actions: LayoutAction[] = [
      Action.addNode({}, "n", "c", 0),
      Action.moveNode("a", "b", "c", 0),
      Action.deleteTab("n"),
      Action.deleteTabset("n"),
      Action.renameTab("n", "t"),
      Action.selectTab("n"),
      Action.setActiveTabset("n"),
      Action.adjustWeights("n", [1, 2], "h"),
      Action.adjustBorderSplit("b", 100),
      Action.maximizeToggle("n"),
      Action.updateModelAttributes({}),
      Action.updateNodeAttributes("n", {}),
      Action.popoutTab("n"),
      Action.popoutTabset("n"),
      Action.closeWindow("w"),
      Action.createWindow(),
      Action.floatTab("n", 0, 0, 100, 100),
      Action.floatTabset("n", 0, 0, 100, 100),
      Action.dockTab("n", "c"),
      Action.dockTabset("n", "c"),
      Action.moveWindow("w", 0, 0, 100, 100),
      Action.undoAction(),
      Action.redoAction(),
      Action.setTabIcon("n", "i"),
      Action.setTabComponent("n", "c"),
      Action.setTabConfig("n", {}),
      Action.setTabEnableClose("n", true),
      Action.setDockState("n", "expanded"),
      Action.setVisibleTabs("n", [1, 2]),
    ];
    
    expect(actions.length).toBe(29);
    expect(actions.every(a => a.type && a.data !== undefined)).toBe(true);
  });
});
