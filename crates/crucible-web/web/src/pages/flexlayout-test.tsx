import { Component, createSignal, JSX } from "solid-js";
import { render } from "solid-js/web";
import { Layout } from "@/lib/solid-flexlayout";
import type { ITabRenderValues, ITabSetRenderValues } from "@/lib/solid-flexlayout";
import { Model } from "@/lib/flexlayout/model/Model";
import { TabNode } from "@/lib/flexlayout/model/TabNode";
import { TabSetNode } from "@/lib/flexlayout/model/TabSetNode";
import { BorderNode } from "@/lib/flexlayout/model/BorderNode";
import { Action } from "@/lib/flexlayout/model/Action";

const layouts: Record<string, any> = {
  test_two_tabs: {
    global: {},
    borders: [],
    layout: {
      type: "row",
      weight: 100,
      children: [
        {
          type: "tabset",
          weight: 50,
          children: [{ type: "tab", name: "One", component: "testing" }],
        },
        {
          type: "tabset",
          id: "#1",
          weight: 50,
          children: [{ type: "tab", name: "Two", component: "testing" }],
        },
      ],
    },
  },

  test_three_tabs: {
    global: {},
    borders: [],
    layout: {
      type: "row",
      weight: 100,
      children: [
        {
          type: "tabset",
          weight: 50,
          children: [{ type: "tab", name: "One", component: "testing" }],
        },
        {
          type: "tabset",
          weight: 50,
          name: "TheHeader",
          children: [
            {
              type: "tab",
              name: "Two",
              icon: "/test/images/settings.svg",
              component: "testing",
            },
          ],
        },
        {
          type: "tabset",
          weight: 50,
          children: [{ type: "tab", name: "Three", component: "testing" }],
        },
      ],
    },
  },

  test_with_borders: {
    global: {},
    borders: [
      {
        type: "border",
        location: "top",
        children: [{ type: "tab", name: "top1", component: "testing" }],
      },
      {
        type: "border",
        location: "bottom",
        children: [{ type: "tab", name: "bottom1", component: "testing" }],
      },
      {
        type: "border",
        location: "left",
        children: [{ type: "tab", name: "left1", component: "testing" }],
      },
      {
        type: "border",
        location: "right",
        children: [{ type: "tab", name: "right1", component: "testing" }],
      },
    ],
    layout: {
      type: "row",
      weight: 100,
      children: [
        {
          type: "tabset",
          weight: 50,
          children: [{ type: "tab", name: "One", component: "testing" }],
        },
        {
          type: "tabset",
          weight: 50,
          id: "#1",
          children: [{ type: "tab", name: "Two", component: "testing" }],
        },
        {
          type: "tabset",
          weight: 50,
          children: [{ type: "tab", name: "Three", component: "testing" }],
        },
      ],
    },
  },

  test_with_onRenderTab: {
    global: {},
    borders: [
      {
        type: "border",
        location: "top",
        children: [
          {
            type: "tab",
            id: "onRenderTab2",
            name: "top1",
            component: "testing",
          },
        ],
      },
      {
        type: "border",
        location: "bottom",
        children: [
          { type: "tab", name: "bottom1", component: "testing" },
          { type: "tab", name: "bottom2", component: "testing" },
        ],
      },
      {
        type: "border",
        location: "left",
        children: [{ type: "tab", name: "left1", component: "testing" }],
      },
      {
        type: "border",
        location: "right",
        children: [{ type: "tab", name: "right1", component: "testing" }],
      },
    ],
    layout: {
      type: "row",
      weight: 100,
      children: [
        {
          type: "tabset",
          id: "onRenderTabSet1",
          weight: 50,
          children: [
            { type: "tab", id: "345", name: "One", component: "testing" },
          ],
        },
        {
          type: "tabset",
          id: "onRenderTabSet2",
          name: "will be replaced",
          weight: 50,
          children: [
            {
              type: "tab",
              id: "onRenderTab1",
              name: "Two",
              component: "testing",
            },
          ],
        },
        {
          type: "tabset",
          id: "onRenderTabSet3",
          weight: 50,
          children: [
            { type: "tab", id: "123", name: "Three", component: "testing" },
          ],
        },
      ],
    },
  },

  test_with_min_size: {
    global: {
      tabSetMinHeight: 100,
      tabSetMinWidth: 100,
      borderMinSize: 100,
      borderEnableAutoHide: true,
      tabSetEnableClose: true,
    },
    borders: [
      {
        type: "border",
        location: "top",
        children: [{ type: "tab", name: "top1", component: "testing" }],
      },
      {
        type: "border",
        location: "bottom",
        children: [
          { type: "tab", name: "bottom1", component: "testing" },
          { type: "tab", name: "bottom2", component: "testing" },
        ],
      },
      {
        type: "border",
        location: "left",
        children: [{ type: "tab", name: "left1", component: "testing" }],
      },
      {
        type: "border",
        location: "right",
        children: [{ type: "tab", name: "right1", component: "testing" }],
      },
    ],
    layout: {
      type: "row",
      weight: 100,
      children: [
        {
          type: "tabset",
          weight: 50,
          children: [{ type: "tab", name: "One", component: "testing" }],
        },
        {
          type: "tabset",
          weight: 50,
          id: "#1",
          children: [{ type: "tab", name: "Two", component: "testing" }],
        },
        {
          type: "row",
          weight: 100,
          children: [
            {
              type: "tabset",
              weight: 50,
              children: [
                { type: "tab", name: "Three", component: "testing" },
                { type: "tab", name: "Four", component: "testing" },
                { type: "tab", name: "Five", component: "testing" },
              ],
            },
            {
              type: "tabset",
              weight: 50,
              children: [
                { type: "tab", name: "Six", component: "testing" },
                { type: "tab", name: "Seven", component: "testing" },
              ],
            },
          ],
        },
      ],
    },
  },
};

const FlexLayoutTest: Component = () => {
  const params = new URLSearchParams(window.location.search);
  const layoutName = params.get("layout") || "test_two_tabs";

  let nextIndex = 1;

  const currentLayout = () => layouts[layoutName] || layouts.test_two_tabs;
  const [model, setModel] = createSignal(Model.fromJson(currentLayout()), { equals: false });

  const reload = () => {
    const newModel = Model.fromJson(currentLayout());
    const root = newModel.getRoot();
    if (root) {
      root.setPaths("");
      newModel.getBorderSet().setPaths();
    }
    setModel(newModel);
    nextIndex = 1;
  };

  const onDragStart = (event: DragEvent) => {
    const tabJson = {
      type: "tab",
      name: "Text" + nextIndex++,
      component: "testing",
    };
    const tempNode = TabNode.fromJson(tabJson, model(), false);
    const layoutDiv = document.querySelector(".flexlayout__layout");
    if (layoutDiv) {
      (layoutDiv as any).__dragNode = tempNode;
    }
    event.dataTransfer!.setData("text/plain", "--flexlayout--");
    event.dataTransfer!.effectAllowed = "copyMove";
    event.dataTransfer!.dropEffect = "move";
  };

  const onAddActive = () => {
    const m = model();
    const activeTabset = m.getActiveTabset();
    if (activeTabset) {
      m.doAction(
        Action.addNode(
          { type: "tab", name: "Text" + nextIndex++, component: "testing" },
          activeTabset.getId(),
          "center",
          -1,
        ),
      );
      const newModel = Model.fromJson(m.toJson());
      const root = newModel.getRoot();
      if (root) {
        root.setPaths("");
        newModel.getBorderSet().setPaths();
      }
      setModel(newModel);
    }
  };

  const onRenderTab = (node: TabNode, renderValues: ITabRenderValues) => {
    if (node.getId() === "onRenderTab1") {
      renderValues.leading = (
        <img
          src="images/settings.svg"
          style={{ width: "1em", height: "1em" }}
        />
      ) as JSX.Element;
      renderValues.content = "onRenderTab1" as unknown as JSX.Element;
      renderValues.buttons.push(
        <img
          src="images/folder.svg"
          style={{ width: "1em", height: "1em" }}
        />,
      );
    } else if (node.getId() === "onRenderTab2") {
      renderValues.leading = (
        <img
          src="images/settings.svg"
          style={{ width: "1em", height: "1em" }}
        />
      ) as JSX.Element;
      renderValues.content = "onRenderTab2" as unknown as JSX.Element;
      renderValues.buttons.push(
        <img
          src="images/folder.svg"
          style={{ width: "1em", height: "1em" }}
        />,
      );
    }
  };

  const onRenderTabSet = (
    node: TabSetNode | BorderNode,
    renderValues: ITabSetRenderValues,
  ) => {
    if (node.getId() === "onRenderTabSet1") {
      renderValues.buttons.push(<img src="images/folder.svg" />);
      renderValues.buttons.push(<img src="images/settings.svg" />);
    } else if (node.getId() === "onRenderTabSet2") {
      renderValues.buttons.push(<img src="images/folder.svg" />);
      renderValues.buttons.push(<img src="images/settings.svg" />);
    } else if (node.getId() === "onRenderTabSet3") {
      renderValues.stickyButtons.push(
        <img
          src="images/add.svg"
          alt="Add"
          title="Add Tab (using onRenderTabSet callback, see Demo)"
          style={{
            "margin-left": "5px",
            width: "24px",
            height: "24px",
          }}
        />,
      );
    } else if (node instanceof BorderNode) {
      renderValues.buttons.push(<img src="images/folder.svg" />);
      renderValues.buttons.push(<img src="images/settings.svg" />);
    }
  };

  const factory = (node: TabNode) => (
    <div
      style={{
        padding: "16px",
        height: "100%",
        "box-sizing": "border-box",
      }}
    >
      {node.getName()}
    </div>
  );

  const onAction = (action: any) => action;

  return (
    <div
      style={{
        width: "100vw",
        height: "100vh",
        display: "flex",
        "flex-direction": "column",
      }}
    >
      <div style={{ padding: "4px", display: "flex", gap: "4px" }}>
        <button data-id="reload" onClick={reload}>
          Reload
        </button>
        <button
          data-id="add-drag"
          draggable={true}
          onDragStart={onDragStart}
        >
          Add Drag
        </button>
        <button data-id="add-active" onClick={onAddActive}>
          Add Active
        </button>
      </div>
      <div style={{ flex: 1, position: "relative" }}>
        <Layout
          model={model()}
          factory={factory}
          onAction={onAction}
          onRenderTab={onRenderTab}
          onRenderTabSet={onRenderTabSet}
        />
      </div>
    </div>
  );
};

// Mount directly — this is a standalone test page
const root = document.getElementById("root");
if (root) {
  render(() => <FlexLayoutTest />, root);
}

export default FlexLayoutTest;
