import { Component } from "solid-js";
import { render } from "solid-js/web";
import { Layout } from "@/lib/solid-flexlayout";
import { Model } from "@/lib/flexlayout/model/Model";
import { TabNode } from "@/lib/flexlayout/model/TabNode";


/**
 * FlexLayout smoke test harness.
 *
 * Renders a minimal 2-tabset layout so Playwright can verify
 * that the SolidJS FlexLayout port renders and responds to clicks.
 */
const FlexLayoutTest: Component = () => {
  const model = Model.fromJson({
    global: {},
    borders: [],
    layout: {
      type: "row",
      children: [
        {
          type: "tabset",
          children: [
            { type: "tab", name: "Tab 1", component: "panel" },
            { type: "tab", name: "Tab 1b", component: "panel" },
          ],
        },
        {
          type: "tabset",
          children: [
            { type: "tab", name: "Tab 2", component: "panel" },
          ],
        },
      ],
    },
  });

  const factory = (node: TabNode) => {
    return (
      <div
        data-testid={`panel-${node.getName()}`}
        style={{
          padding: "16px",
          height: "100%",
          "box-sizing": "border-box",
          background: "#1a1a2e",
          color: "#e0e0e0",
          "font-family": "monospace",
        }}
      >
        Content for {node.getName()}
      </div>
    );
  };

  const onAction = (action: any) => {
    return action;
  };

  return (
    <div
      id="flexlayout-test-root"
      style={{
        width: "100vw",
        height: "100vh",
        position: "absolute",
        top: 0,
        left: 0,
        background: "#0f0f23",
      }}
    >
      <Layout model={model} factory={factory} onAction={onAction} />
    </div>
  );
};

// Mount directly — this is a standalone test page
const root = document.getElementById("root");
if (root) {
  render(() => <FlexLayoutTest />, root);
}

export default FlexLayoutTest;
