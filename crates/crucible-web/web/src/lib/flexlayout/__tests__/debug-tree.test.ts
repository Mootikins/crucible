import { describe, it } from "vitest";
import { Model } from "../model/Model";
import { Action } from "../model/Action";
import { twoTabs } from "./fixtures";

function printTree(node: any, indent = ""): void {
  const type = node.getType();
  const id = node.getId ? node.getId() : "no-id";
  const name = node.getName ? node.getName() : "";
  console.log(`${indent}${type} (${id}) ${name}`);
  
  if (node.getChildren) {
    for (const child of node.getChildren()) {
      printTree(child, indent + "  ");
    }
  }
}

describe("Debug Tree Structure", () => {
  it("shows tree before and after add to top", () => {
    const model = Model.fromJson(twoTabs);
    console.log("\n=== BEFORE ===");
    printTree(model.getRoot());

    const ts0 = model.getNodeById("ts0");
    model.doAction(
      Action.addNode(
        { type: "tab", name: "newtab1", component: "grid" },
        ts0?.getId() || "",
        "top",
        -1
      )
    );

    console.log("\n=== AFTER ===");
    printTree(model.getRoot());
  });
});
