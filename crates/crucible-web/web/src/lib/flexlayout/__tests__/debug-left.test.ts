import { describe, it } from "vitest";
import { Model } from "../model/Model";
import { Action } from "../model/Action";
import { twoTabs } from "./fixtures";

describe("Debug Left Add", () => {
  it("shows what happens", () => {
    const model = Model.fromJson(twoTabs);
    const ts0 = model.getNodeById("ts0");

    model.doAction(
      Action.addNode(
        { type: "tab", name: "newtab1", component: "grid" },
        ts0?.getId() || "",
        "left",
        -1
      )
    );

    console.log("\nAfter first add:");
    console.log("ts0:", model.getNodeById("ts0")?.getName());
    console.log("ts1:", model.getNodeById("ts1")?.getName());
    console.log("ts2:", model.getNodeById("ts2")?.getName());
    console.log("ts3:", model.getNodeById("ts3")?.getName());
  });
});
