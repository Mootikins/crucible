import { Component, createSignal, JSX } from "solid-js";
import { render } from "solid-js/web";
import { Layout } from "@/lib/solid-flexlayout";
import type { ITabRenderValues, ITabSetRenderValues } from "@/lib/solid-flexlayout";
import { Model } from "@/lib/flexlayout/model/Model";
import { TabNode } from "@/lib/flexlayout/model/TabNode";
import { TabSetNode } from "@/lib/flexlayout/model/TabSetNode";
import { BorderNode } from "@/lib/flexlayout/model/BorderNode";
import { Action } from "@/lib/flexlayout/model/Action";

const defaultGlobal = {
  tabMinWidth: 0,
  tabMinHeight: 0,
  tabMaxWidth: 100000,
  tabMaxHeight: 100000,
  tabCloseType: 1,
  borderAutoSelectTabWhenOpen: true,
  borderAutoSelectTabWhenClosed: false,
  borderSize: 200,
  borderMinSize: 0,
  borderMaxSize: 99999,
  borderEnableDrop: true,
  borderEnableAutoHide: false,
};

const layouts: Record<string, any> = {
  test_two_tabs: {
    global: { ...defaultGlobal },
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
    global: { ...defaultGlobal },
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
    global: { ...defaultGlobal },
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
    global: { ...defaultGlobal },
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
      ...defaultGlobal,
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
  test_with_float: {
    global: { ...defaultGlobal },
    borders: [],
    layout: {
      type: "row",
      weight: 100,
      children: [
        {
          type: "tabset",
          weight: 50,
          children: [
            { type: "tab", name: "Main", component: "testing" },
            { type: "tab", name: "Editor", component: "testing" },
          ],
        },
        {
          type: "tabset",
          weight: 50,
          id: "#1",
          children: [{ type: "tab", name: "Preview", component: "testing" }],
        },
      ],
    },
    windows: {
      "float1": {
        windowType: "float",
        rect: { x: 100, y: 100, width: 300, height: 200 },
        layout: {
          type: "row",
          weight: 100,
          children: [
            {
              type: "tabset",
              weight: 100,
              children: [{ type: "tab", name: "Floating", component: "testing" }],
            },
          ],
        },
      },
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

  const onFloatActive = () => {
    const m = model();
    const activeTabset = m.getActiveTabset();
    if (activeTabset) {
      const r = activeTabset.getRect();
      m.doAction(
        Action.floatTabset(activeTabset.getId(), r.x + 20, r.y + 20, r.width, r.height),
      );
      setModel(m);
    }
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
      setModel(m);
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

   const factory = (node: TabNode) => {
     const componentType = node.getComponent();
     const config = node.getConfig();

     switch (componentType) {
       case "info": {
         const description = config?.description || "No description provided";
         return (
           <div
             data-testid={`panel-${node.getName()}`}
             style={{
               padding: "16px",
               height: "100%",
               "box-sizing": "border-box",
               "overflow-y": "auto",
             }}
           >
             <p style={{ margin: 0 }}>{description}</p>
           </div>
         );
       }

       case "counter": {
         const [count, setCount] = createSignal(0);
         return (
           <div
             data-testid={`panel-${node.getName()}`}
             style={{
               padding: "16px",
               height: "100%",
               "box-sizing": "border-box",
               display: "flex",
               "flex-direction": "column",
               gap: "8px",
             }}
           >
             <p>Count: {count()}</p>
             <button onClick={() => setCount(count() + 1)}>
               Increment
             </button>
           </div>
         );
       }

       case "color": {
         const bgColor = config?.color || "#f0f0f0";
         return (
           <div
             data-testid={`panel-${node.getName()}`}
             style={{
               padding: "16px",
               height: "100%",
               "box-sizing": "border-box",
               "background-color": bgColor,
             }}
           >
             <p>Color: {bgColor}</p>
           </div>
         );
       }

       case "form": {
         const [text, setText] = createSignal("");
         const [checked, setChecked] = createSignal(false);
         return (
           <div
             data-testid={`panel-${node.getName()}`}
             style={{
               padding: "16px",
               height: "100%",
               "box-sizing": "border-box",
               display: "flex",
               "flex-direction": "column",
               gap: "8px",
             }}
           >
             <input
               type="text"
               value={text()}
               onInput={(e) => setText(e.currentTarget.value)}
               placeholder="Enter text"
             />
             <label>
               <input
                 type="checkbox"
                 checked={checked()}
                 onChange={(e) => setChecked(e.currentTarget.checked)}
               />
               {" "}Agree
             </label>
             <p>Text: {text()}, Checked: {checked() ? "yes" : "no"}</p>
           </div>
         );
       }

       case "heavy": {
         return (
           <div
             data-testid={`panel-${node.getName()}`}
             style={{
               padding: "16px",
               height: "100%",
               "box-sizing": "border-box",
               "overflow-y": "auto",
             }}
           >
             {Array.from({ length: 50 }, (_, i) => (
               <div style={{ padding: "4px" }}>
                 Item {i + 1}
               </div>
             ))}
           </div>
         );
       }

       case "nested": {
         const nestedLayout: any = {
           global: { ...defaultGlobal },
           borders: [],
           layout: {
             type: "row",
             weight: 100,
             children: [
               {
                 type: "tabset",
                 weight: 100,
                 children: [
                   { type: "tab", name: "Nested Tab", component: "testing" },
                 ],
               },
             ],
           },
         };
         const nestedModel = Model.fromJson(nestedLayout);
         const root = nestedModel.getRoot();
         if (root) {
           root.setPaths("");
           nestedModel.getBorderSet().setPaths();
         }
         return (
           <div
             data-testid={`panel-${node.getName()}`}
             style={{
               padding: "16px",
               height: "100%",
               "box-sizing": "border-box",
               position: "relative",
             }}
           >
             <Layout
               model={nestedModel}
               factory={factory}
               onAction={onAction}
             />
           </div>
         );
       }

       default: {
         return (
           <div
             data-testid={`panel-${node.getName()}`}
             style={{
               padding: "16px",
               height: "100%",
               "box-sizing": "border-box",
             }}
           >
             {node.getName()}
           </div>
         );
       }
     }
   };

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
        <button data-id="float-active" onClick={onFloatActive}>
          Float Active
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
