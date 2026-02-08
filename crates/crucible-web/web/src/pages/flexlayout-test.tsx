import { Component, createSignal } from "solid-js";
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

  tabset_tab_wrap: {
    global: {
      ...defaultGlobal,
      tabSetEnableTabWrap: true,
    },
    borders: [],
    layout: {
      type: "row",
      weight: 100,
      children: [
        {
          type: "tabset",
          weight: 50,
          children: [
            { type: "tab", name: "Alpha", component: "info", config: { description: "Tab wrapping demo — with enough tabs, the tab strip wraps to multiple lines instead of scrolling." } },
            { type: "tab", name: "Beta", component: "info", config: { description: "Second tab in wrap demo." } },
            { type: "tab", name: "Gamma", component: "info", config: { description: "Third tab in wrap demo." } },
            { type: "tab", name: "Delta", component: "info", config: { description: "Fourth tab in wrap demo." } },
            { type: "tab", name: "Epsilon", component: "info", config: { description: "Fifth tab in wrap demo." } },
            { type: "tab", name: "Zeta", component: "info", config: { description: "Sixth tab in wrap demo." } },
            { type: "tab", name: "Eta", component: "info", config: { description: "Seventh tab in wrap demo." } },
            { type: "tab", name: "Theta", component: "info", config: { description: "Eighth tab in wrap demo." } },
            { type: "tab", name: "Iota", component: "info", config: { description: "Ninth tab — should force wrapping." } },
          ],
        },
        {
          type: "tabset",
          weight: 50,
          children: [
            { type: "tab", name: "Sidebar", component: "info", config: { description: "Companion panel for reference." } },
          ],
        },
      ],
    },
  },

  tabset_closeable: {
    global: {
      ...defaultGlobal,
      tabSetEnableClose: true,
    },
    borders: [],
    layout: {
      type: "row",
      weight: 100,
      children: [
        {
          type: "tabset",
          weight: 33,
          children: [
            { type: "tab", name: "Panel A", component: "info", config: { description: "Closeable tabset demo — the entire tabset has a close button. Close this tabset to remove it." } },
          ],
        },
        {
          type: "tabset",
          weight: 34,
          children: [
            { type: "tab", name: "Panel B", component: "info", config: { description: "Second closeable tabset. Close it and the remaining panels resize." } },
            { type: "tab", name: "Panel B2", component: "info", config: { description: "Extra tab in second closeable tabset." } },
          ],
        },
        {
          type: "tabset",
          weight: 33,
          children: [
            { type: "tab", name: "Panel C", component: "info", config: { description: "Third closeable tabset." } },
          ],
        },
      ],
    },
  },

  tabset_tab_scrollbar: {
    global: {
      ...defaultGlobal,
      tabSetEnableTabScrollbar: true,
    },
    borders: [],
    layout: {
      type: "row",
      weight: 100,
      children: [
        {
          type: "tabset",
          weight: 60,
          children: [
            { type: "tab", name: "Scroll-1", component: "info", config: { description: "Tab scrollbar demo — a horizontal mini scrollbar appears in the tab strip when tabs overflow." } },
            { type: "tab", name: "Scroll-2", component: "info", config: { description: "Second scrollbar tab." } },
            { type: "tab", name: "Scroll-3", component: "info", config: { description: "Third scrollbar tab." } },
            { type: "tab", name: "Scroll-4", component: "info", config: { description: "Fourth scrollbar tab." } },
            { type: "tab", name: "Scroll-5", component: "info", config: { description: "Fifth scrollbar tab." } },
            { type: "tab", name: "Scroll-6", component: "info", config: { description: "Sixth scrollbar tab." } },
            { type: "tab", name: "Scroll-7", component: "info", config: { description: "Seventh scrollbar tab." } },
            { type: "tab", name: "Scroll-8", component: "info", config: { description: "Eighth scrollbar tab." } },
            { type: "tab", name: "Scroll-9", component: "info", config: { description: "Ninth scrollbar tab — should trigger scrollbar." } },
          ],
        },
        {
          type: "tabset",
          weight: 40,
          children: [
            { type: "tab", name: "Reference", component: "info", config: { description: "Companion panel without scrollbar overflow." } },
          ],
        },
      ],
    },
  },

  tabset_active_icon: {
    global: {
      ...defaultGlobal,
      tabSetEnableActiveIcon: true,
    },
    borders: [],
    layout: {
      type: "row",
      weight: 100,
      children: [
        {
          type: "tabset",
          weight: 33,
          children: [
            { type: "tab", name: "Editor", component: "info", config: { description: "Active icon demo — the active tabset shows a visual indicator distinguishing it from inactive tabsets." } },
            { type: "tab", name: "Settings", component: "info", config: { description: "Second tab in first tabset." } },
          ],
        },
        {
          type: "tabset",
          weight: 34,
          children: [
            { type: "tab", name: "Preview", component: "info", config: { description: "Click this tabset to see the active icon move here." } },
          ],
        },
        {
          type: "tabset",
          weight: 33,
          children: [
            { type: "tab", name: "Console", component: "info", config: { description: "Third tabset — click to activate and observe the icon indicator." } },
          ],
        },
      ],
    },
  },

  tabset_bottom_tabs: {
    global: { ...defaultGlobal },
    borders: [],
    layout: {
      type: "row",
      weight: 100,
      children: [
        {
          type: "tabset",
          weight: 50,
          tabSetTabLocation: "top",
          children: [
            { type: "tab", name: "Top Tabs", component: "info", config: { description: "Standard top tab strip — tabs at the top (default position)." } },
            { type: "tab", name: "Also Top", component: "info", config: { description: "Another tab at the top position." } },
          ],
        },
        {
          type: "tabset",
          weight: 50,
          tabSetTabLocation: "bottom",
          children: [
            { type: "tab", name: "Bottom Tabs", component: "info", config: { description: "Bottom tab strip — tabs rendered below the content area, like a terminal panel." } },
            { type: "tab", name: "Also Bottom", component: "info", config: { description: "Another tab at the bottom position." } },
          ],
        },
      ],
    },
  },

  tabset_hidden_strip: {
    global: { ...defaultGlobal },
    borders: [],
    layout: {
      type: "row",
      weight: 100,
      children: [
        {
          type: "tabset",
          weight: 50,
          enableTabStrip: false,
          children: [
            { type: "tab", name: "Left Pane", component: "info", config: { description: "Hidden tab strip — no tab strip visible. This creates a split-pane layout where content fills the entire area." } },
          ],
        },
        {
          type: "tabset",
          weight: 50,
          enableTabStrip: false,
          children: [
            { type: "tab", name: "Right Pane", component: "info", config: { description: "Second pane with hidden strip — clean split-pane mode with no tab chrome." } },
          ],
        },
      ],
    },
  },

  tabset_custom_class: {
    global: { ...defaultGlobal },
    borders: [],
    layout: {
      type: "row",
      weight: 100,
      children: [
        {
          type: "tabset",
          weight: 50,
          classNameTabStrip: "custom-strip-primary",
          children: [
            { type: "tab", name: "Primary", component: "info", config: { description: "Custom CSS class on tabset — classNameTabStrip allows styling individual tabsets differently." } },
            { type: "tab", name: "Primary B", component: "info", config: { description: "Another tab in the primary-styled tabset." } },
          ],
        },
        {
          type: "tabset",
          weight: 50,
          classNameTabStrip: "custom-strip-secondary",
          children: [
            { type: "tab", name: "Secondary", component: "info", config: { description: "Different CSS class — this tabset uses a secondary custom class for distinct styling." } },
            { type: "tab", name: "Secondary B", component: "info", config: { description: "Another tab in the secondary-styled tabset." } },
          ],
        },
      ],
    },
  },

  tabset_min_max: {
    global: {
      ...defaultGlobal,
      tabSetMinWidth: 150,
      tabSetMaxWidth: 600,
      tabSetMinHeight: 100,
      tabSetMaxHeight: 400,
    },
    borders: [],
    layout: {
      type: "row",
      weight: 100,
      children: [
        {
          type: "tabset",
          weight: 40,
          children: [
            { type: "tab", name: "Constrained A", component: "info", config: { description: "Min/max size demo — tabSetMinWidth: 150, tabSetMaxWidth: 600. Drag the splitter to see constraints enforced." } },
          ],
        },
        {
          type: "row",
          weight: 60,
          children: [
            {
              type: "tabset",
              weight: 50,
              children: [
                { type: "tab", name: "Constrained B", component: "info", config: { description: "tabSetMinHeight: 100, tabSetMaxHeight: 400. Vertical resizing is constrained." } },
              ],
            },
            {
              type: "tabset",
              weight: 50,
              children: [
                { type: "tab", name: "Constrained C", component: "info", config: { description: "Both width and height constraints apply. Resize to test limits." } },
              ],
            },
          ],
        },
      ],
    },
  },

  tab_border_size: {
    global: { ...defaultGlobal },
    borders: [
      {
        type: "border",
        location: "left",
        size: 250,
        children: [
          { type: "tab", name: "Wide Border", component: "info", tabBorderWidth: 300, tabBorderHeight: 200, config: { description: "Per-tab border width/height — tabBorderWidth: 300, tabBorderHeight: 200. This tab overrides the default border size." } },
        ],
      },
      {
        type: "border",
        location: "bottom",
        size: 150,
        children: [
          { type: "tab", name: "Tall Border", component: "info", tabBorderWidth: 400, tabBorderHeight: 250, config: { description: "Bottom border tab — tabBorderWidth: 400, tabBorderHeight: 250. Different dimensions per tab." } },
          { type: "tab", name: "Default Border", component: "info", config: { description: "Default border size — no per-tab overrides, uses the border's default size." } },
        ],
      },
    ],
    layout: {
      type: "row",
      weight: 100,
      children: [
        {
          type: "tabset",
          weight: 100,
          children: [
            { type: "tab", name: "Main Content", component: "info", config: { description: "Main content area — open border tabs to see per-tab border sizing in action." } },
          ],
        },
      ],
    },
  },

  tab_close_types: {
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
            { type: "tab", name: "Close Type 0", component: "info", tabCloseType: 0, config: { description: "tabCloseType: 0 — close button always visible on the tab." } },
            { type: "tab", name: "Close Type 1", component: "info", tabCloseType: 1, config: { description: "tabCloseType: 1 — close button visible on hover (default behavior)." } },
            { type: "tab", name: "Close Type 2", component: "info", tabCloseType: 2, config: { description: "tabCloseType: 2 — no close button, tab cannot be closed by clicking." } },
            { type: "tab", name: "Also Type 0", component: "info", tabCloseType: 0, config: { description: "Another tab with close type 0 for comparison." } },
            { type: "tab", name: "Also Type 2", component: "info", tabCloseType: 2, config: { description: "Another non-closeable tab (type 2)." } },
          ],
        },
      ],
    },
  },

  border_autohide: {
    global: {
      ...defaultGlobal,
      borderEnableAutoHide: true,
    },
    borders: [
      {
        type: "border",
        location: "top",
        children: [{ type: "tab", name: "Top Panel", component: "info", config: { description: "Top border — collapses when no tab is selected (borderEnableAutoHide: true)" } }],
      },
      {
        type: "border",
        location: "bottom",
        children: [{ type: "tab", name: "Bottom Panel", component: "info", config: { description: "Bottom border — collapses when deselected" } }],
      },
      {
        type: "border",
        location: "left",
        children: [{ type: "tab", name: "Left Panel", component: "info", config: { description: "Left border — auto-hides when closed" } }],
      },
      {
        type: "border",
        location: "right",
        children: [{ type: "tab", name: "Right Panel", component: "info", config: { description: "Right border — auto-hides when closed" } }],
      },
    ],
    layout: {
      type: "row",
      weight: 100,
      children: [
        {
          type: "tabset",
          weight: 100,
          children: [{ type: "tab", name: "Main Content", component: "info", config: { description: "Click border tabs to expand, deselect to auto-hide" } }],
        },
      ],
    },
  },

  border_scrollbar: {
    global: {
      ...defaultGlobal,
      borderEnableTabScrollbar: true,
    },
    borders: [
      {
        type: "border",
        location: "bottom",
        children: [
          { type: "tab", name: "Console", component: "info", config: { description: "Console output" } },
          { type: "tab", name: "Problems", component: "info", config: { description: "Problem list" } },
          { type: "tab", name: "Output", component: "info", config: { description: "Build output" } },
          { type: "tab", name: "Debug", component: "info", config: { description: "Debug console" } },
          { type: "tab", name: "Terminal", component: "info", config: { description: "Integrated terminal" } },
          { type: "tab", name: "Ports", component: "info", config: { description: "Forwarded ports" } },
          { type: "tab", name: "Tasks", component: "info", config: { description: "Running tasks" } },
          { type: "tab", name: "Comments", component: "info", config: { description: "Code comments" } },
          { type: "tab", name: "Timeline", component: "info", config: { description: "File timeline" } },
          { type: "tab", name: "Notifications", component: "info", config: { description: "Notification log" } },
        ],
      },
      {
        type: "border",
        location: "left",
        children: [
          { type: "tab", name: "Explorer", component: "info", config: { description: "File explorer" } },
          { type: "tab", name: "Search", component: "info", config: { description: "Search panel" } },
          { type: "tab", name: "Git", component: "info", config: { description: "Source control" } },
          { type: "tab", name: "Extensions", component: "info", config: { description: "Extension manager" } },
          { type: "tab", name: "Bookmarks", component: "info", config: { description: "Bookmarks" } },
          { type: "tab", name: "Outline", component: "info", config: { description: "Symbol outline" } },
        ],
      },
    ],
    layout: {
      type: "row",
      weight: 100,
      children: [
        {
          type: "tabset",
          weight: 100,
          children: [{ type: "tab", name: "Editor", component: "info", config: { description: "Many border tabs — scroll the tab strip with borderEnableTabScrollbar: true" } }],
        },
      ],
    },
  },

  border_sizing: {
    global: {
      ...defaultGlobal,
      borderSize: 300,
      borderMinSize: 100,
      borderMaxSize: 500,
    },
    borders: [
      {
        type: "border",
        location: "bottom",
        children: [{ type: "tab", name: "Sized Panel", component: "info", config: { description: "borderSize: 300, borderMinSize: 100, borderMaxSize: 500 — drag to resize within constraints" } }],
      },
      {
        type: "border",
        location: "left",
        children: [{ type: "tab", name: "Left Sized", component: "info", config: { description: "Left border with same sizing constraints" } }],
      },
      {
        type: "border",
        location: "right",
        children: [{ type: "tab", name: "Right Sized", component: "info", config: { description: "Right border with same sizing constraints" } }],
      },
    ],
    layout: {
      type: "row",
      weight: 100,
      children: [
        {
          type: "tabset",
          weight: 100,
          children: [{ type: "tab", name: "Main", component: "info", config: { description: "Borders open at 300px, can be resized between 100px and 500px" } }],
        },
      ],
    },
  },

  border_config: {
    global: { ...defaultGlobal },
    borders: [
      {
        type: "border",
        location: "top",
        className: "border-highlight",
        enableDrop: false,
        children: [{ type: "tab", name: "No Drop Zone", component: "info", config: { description: "This top border has enableDrop: false — tabs cannot be dragged into it" } }],
      },
      {
        type: "border",
        location: "bottom",
        className: "border-accent",
        children: [{ type: "tab", name: "Styled Bottom", component: "info", config: { description: "Bottom border with custom borderClassName" } }],
      },
      {
        type: "border",
        location: "left",
        enableDrop: true,
        children: [{ type: "tab", name: "Drop Enabled", component: "info", config: { description: "Left border with enableDrop: true (default)" } }],
      },
      {
        type: "border",
        location: "right",
        enableDrop: false,
        className: "border-readonly",
        children: [{ type: "tab", name: "Read-Only", component: "info", config: { description: "Right border: no drop, custom className" } }],
      },
    ],
    layout: {
      type: "row",
      weight: 100,
      children: [
        {
          type: "tabset",
          weight: 100,
          children: [{ type: "tab", name: "Center", component: "info", config: { description: "Per-border className and enableDrop control — try dragging tabs to different borders" } }],
        },
      ],
    },
  },

  splitter_handle: {
    global: {
      ...defaultGlobal,
      splitterEnableHandle: true,
      splitterSize: 12,
    },
    borders: [],
    layout: {
      type: "row",
      weight: 100,
      children: [
        {
          type: "tabset",
          weight: 33,
          children: [{ type: "tab", name: "Left", component: "info", config: { description: "Drag the visible grip handle between panels" } }],
        },
        {
          type: "tabset",
          weight: 34,
          children: [{ type: "tab", name: "Center", component: "info", config: { description: "splitterEnableHandle: true, splitterSize: 12" } }],
        },
        {
          type: "tabset",
          weight: 33,
          children: [{ type: "tab", name: "Right", component: "info", config: { description: "Wide splitters with visible grip handles" } }],
        },
      ],
    },
  },

  splitter_extra: {
    global: {
      ...defaultGlobal,
      splitterSize: 3,
      splitterExtra: 8,
    },
    borders: [],
    layout: {
      type: "row",
      weight: 100,
      children: [
        {
          type: "tabset",
          weight: 50,
          children: [{ type: "tab", name: "Panel A", component: "info", config: { description: "splitterSize: 3 (thin line), splitterExtra: 8 (expanded hit area)" } }],
        },
        {
          type: "row",
          weight: 50,
          children: [
            {
              type: "tabset",
              weight: 50,
              children: [{ type: "tab", name: "Panel B", component: "info", config: { description: "The splitter looks thin but is easy to grab" } }],
            },
            {
              type: "tabset",
              weight: 50,
              children: [{ type: "tab", name: "Panel C", component: "info", config: { description: "Large invisible hit area around thin splitter" } }],
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

  action_add_remove: {
    global: { ...defaultGlobal },
    borders: [],
    layout: {
      type: "row",
      id: "root-row",
      weight: 100,
      children: [
        {
          type: "tabset",
          id: "tabset-main",
          weight: 50,
          children: [
            { type: "tab", id: "tab-alpha", name: "Alpha", component: "testing" },
            { type: "tab", id: "tab-beta", name: "Beta", component: "testing" },
          ],
        },
        {
          type: "tabset",
          id: "tabset-side",
          weight: 50,
          children: [
            { type: "tab", id: "tab-gamma", name: "Gamma", component: "testing" },
          ],
        },
      ],
    },
  },

  action_model_update: {
    global: { ...defaultGlobal, enableEdgeDock: true, rootOrientationVertical: false },
    borders: [],
    layout: {
      type: "row",
      id: "root-row",
      weight: 100,
      children: [
        {
          type: "tabset",
          id: "tabset-left",
          weight: 50,
          children: [
            { type: "tab", name: "Panel A", component: "testing" },
          ],
        },
        {
          type: "tabset",
          id: "tabset-right",
          weight: 50,
          children: [
            { type: "tab", name: "Panel B", component: "testing" },
          ],
        },
      ],
    },
  },

  action_external_drag: {
    global: { ...defaultGlobal },
    borders: [],
    layout: {
      type: "row",
      id: "root-row",
      weight: 100,
      children: [
        {
          type: "tabset",
          id: "tabset-drop-target",
          weight: 60,
          children: [
            { type: "tab", name: "Drop Target", component: "testing" },
          ],
        },
        {
          type: "tabset",
          id: "tabset-existing",
          weight: 40,
          children: [
            { type: "tab", name: "Existing Tab", component: "testing" },
          ],
        },
      ],
    },
  },

  action_weights: {
    global: { ...defaultGlobal },
    borders: [],
    layout: {
      type: "row",
      id: "root-row",
      weight: 100,
      children: [
        {
          type: "tabset",
          id: "tabset-w1",
          weight: 50,
          children: [
            { type: "tab", name: "Left", component: "testing" },
          ],
        },
        {
          type: "tabset",
          id: "tabset-w2",
          weight: 50,
          children: [
            { type: "tab", name: "Right", component: "testing" },
          ],
        },
      ],
    },
  },

  stress_complex: {
    global: {
      ...defaultGlobal,
      tabSetMinHeight: 50,
      tabSetMinWidth: 50,
      tabSetEnableClose: true,
    },
    borders: [
      {
        type: "border",
        location: "top",
        children: [
          { type: "tab", name: "TopA", component: "heavy" },
          { type: "tab", name: "TopB", component: "heavy" },
        ],
      },
      {
        type: "border",
        location: "bottom",
        children: [
          { type: "tab", name: "BottomA", component: "heavy" },
          { type: "tab", name: "BottomB", component: "heavy" },
        ],
      },
      {
        type: "border",
        location: "left",
        children: [
          { type: "tab", name: "LeftA", component: "heavy" },
        ],
      },
      {
        type: "border",
        location: "right",
        children: [
          { type: "tab", name: "RightA", component: "heavy" },
        ],
      },
    ],
    layout: {
      type: "row",
      weight: 100,
      children: [
        {
          type: "tabset",
          weight: 30,
          children: [
            { type: "tab", name: "Nav1", component: "heavy" },
            { type: "tab", name: "Nav2", component: "heavy" },
            { type: "tab", name: "Nav3", component: "heavy" },
            { type: "tab", name: "Nav4", component: "heavy" },
          ],
        },
        {
          type: "row",
          weight: 40,
          children: [
            {
              type: "tabset",
              weight: 60,
              children: [
                { type: "tab", name: "Editor1", component: "heavy" },
                { type: "tab", name: "Editor2", component: "heavy" },
                { type: "tab", name: "Editor3", component: "heavy" },
                { type: "tab", name: "Editor4", component: "heavy" },
              ],
            },
            {
              type: "row",
              weight: 40,
              children: [
                {
                  type: "tabset",
                  weight: 50,
                  children: [
                    { type: "tab", name: "DeepA", component: "heavy" },
                    { type: "tab", name: "DeepB", component: "heavy" },
                    { type: "tab", name: "DeepC", component: "heavy" },
                  ],
                },
                {
                  type: "tabset",
                  weight: 50,
                  children: [
                    { type: "tab", name: "DeepD", component: "heavy" },
                    { type: "tab", name: "DeepE", component: "heavy" },
                    { type: "tab", name: "DeepF", component: "heavy" },
                  ],
                },
              ],
            },
          ],
        },
        {
          type: "row",
          weight: 30,
          children: [
            {
              type: "tabset",
              weight: 50,
              children: [
                { type: "tab", name: "Panel1", component: "heavy" },
                { type: "tab", name: "Panel2", component: "heavy" },
                { type: "tab", name: "Panel3", component: "heavy" },
              ],
            },
            {
              type: "tabset",
              weight: 50,
              children: [
                { type: "tab", name: "Console1", component: "heavy" },
                { type: "tab", name: "Console2", component: "heavy" },
                { type: "tab", name: "Console3", component: "heavy" },
                { type: "tab", name: "Console4", component: "heavy" },
                { type: "tab", name: "Console5", component: "heavy" },
              ],
            },
          ],
        },
      ],
    },
    windows: {
      "stress_float": {
        windowType: "float",
        rect: { x: 80, y: 80, width: 350, height: 250 },
        layout: {
          type: "row",
          weight: 100,
          children: [
            {
              type: "tabset",
              weight: 100,
              children: [
                { type: "tab", name: "Float1", component: "heavy" },
                { type: "tab", name: "Float2", component: "heavy" },
              ],
            },
          ],
        },
      },
    },
  },

  stress_sub_layout: {
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
            { type: "tab", name: "NestedLeft", component: "nested" },
            { type: "tab", name: "InfoLeft", component: "testing" },
          ],
        },
        {
          type: "tabset",
          weight: 50,
          children: [
            { type: "tab", name: "NestedRight", component: "nested" },
            { type: "tab", name: "InfoRight", component: "testing" },
          ],
        },
      ],
    },
  },

  stress_state_preservation: {
    global: { ...defaultGlobal },
    borders: [],
    layout: {
      type: "row",
      weight: 100,
      children: [
        {
          type: "tabset",
          weight: 33,
          children: [
            { type: "tab", name: "CounterA", component: "counter" },
          ],
        },
        {
          type: "tabset",
          weight: 34,
          id: "target_tabset",
          children: [
            { type: "tab", name: "CounterB", component: "counter" },
          ],
        },
        {
          type: "tabset",
          weight: 33,
          children: [
            { type: "tab", name: "CounterC", component: "counter" },
          ],
        },
      ],
    },
  },

  // ── Basic & Layout Structure Demos ──────────────────────────────────

  basic_simple: {
    global: { ...defaultGlobal, tabSetEnableSingleTabStretch: true },
    borders: [],
    layout: {
      type: "row",
      weight: 100,
      children: [
        {
          type: "tabset",
          weight: 50,
          children: [
            {
              type: "tab",
              name: "Left",
              component: "info",
              config: { description: "Minimal 2-pane layout with single tab stretch enabled. Each tabset stretches its sole tab to fill the header." },
            },
          ],
        },
        {
          type: "tabset",
          weight: 50,
          children: [
            {
              type: "tab",
              name: "Right",
              component: "info",
              config: { description: "Second pane. With tabSetEnableSingleTabStretch, the tab header fills the entire tabset header bar." },
            },
          ],
        },
      ],
    },
  },

  basic_vertical_root: {
    global: { ...defaultGlobal, rootOrientationVertical: true },
    borders: [],
    layout: {
      type: "row",
      weight: 100,
      children: [
        {
          type: "tabset",
          weight: 33,
          children: [
            {
              type: "tab",
              name: "Top",
              component: "info",
              config: { description: "Vertical root orientation — tabsets stack top-to-bottom instead of left-to-right." },
            },
          ],
        },
        {
          type: "tabset",
          weight: 34,
          children: [
            {
              type: "tab",
              name: "Middle",
              component: "info",
              config: { description: "Middle pane in vertical stack. The root row flows vertically due to rootOrientationVertical: true." },
            },
          ],
        },
        {
          type: "tabset",
          weight: 33,
          children: [
            {
              type: "tab",
              name: "Bottom",
              component: "info",
              config: { description: "Bottom pane. All three tabsets share vertical space equally." },
            },
          ],
        },
      ],
    },
  },

  basic_edge_dock: {
    global: { ...defaultGlobal, enableEdgeDock: true },
    borders: [],
    layout: {
      type: "row",
      weight: 100,
      children: [
        {
          type: "tabset",
          weight: 50,
          children: [
            {
              type: "tab",
              name: "Main",
              component: "info",
              config: { description: "Edge docking enabled. Drag a tab to the window edges to dock it as a border panel." },
            },
          ],
        },
        {
          type: "tabset",
          weight: 50,
          children: [
            {
              type: "tab",
              name: "Sidebar",
              component: "info",
              config: { description: "Try dragging this tab to the left, right, top, or bottom edge of the layout to create a docked border." },
            },
          ],
        },
      ],
    },
  },

  basic_maximize: {
    global: { ...defaultGlobal, tabSetEnableMaximize: true },
    borders: [],
    layout: {
      type: "row",
      weight: 100,
      children: [
        {
          type: "tabset",
          weight: 33,
          children: [
            {
              type: "tab",
              name: "Panel A",
              component: "info",
              config: { description: "Double-click this tabset's header bar to maximize it, filling the entire layout area. Double-click again to restore." },
            },
          ],
        },
        {
          type: "tabset",
          weight: 34,
          children: [
            {
              type: "tab",
              name: "Panel B",
              component: "info",
              config: { description: "tabSetEnableMaximize: true adds a maximize button to each tabset header. Click it or double-click the header." },
            },
          ],
        },
        {
          type: "tabset",
          weight: 33,
          children: [
            {
              type: "tab",
              name: "Panel C",
              component: "info",
              config: { description: "When one panel is maximized, others are hidden. Restore by clicking the restore button or double-clicking." },
            },
          ],
        },
      ],
    },
  },

  basic_realtime_resize: {
    global: { ...defaultGlobal, realtimeResize: true },
    borders: [],
    layout: {
      type: "row",
      weight: 100,
      children: [
        {
          type: "tabset",
          weight: 33,
          children: [
            {
              type: "tab",
              name: "Left Pane",
              component: "info",
              config: { description: "Realtime resize enabled. Drag the splitter between panes — content resizes live as you drag, not just on release." },
            },
          ],
        },
        {
          type: "tabset",
          weight: 34,
          children: [
            {
              type: "tab",
              name: "Center Pane",
              component: "info",
              config: { description: "With realtimeResize: true, the layout recalculates during drag for smooth visual feedback." },
            },
          ],
        },
        {
          type: "tabset",
          weight: 33,
          children: [
            {
              type: "tab",
              name: "Right Pane",
              component: "info",
              config: { description: "Compare with realtimeResize: false (default) where only a ghost splitter moves during drag." },
            },
          ],
        },
      ],
    },
  },

  basic_serialization: {
    global: { ...defaultGlobal },
    borders: [],
    layout: {
      type: "row",
      weight: 100,
      children: [
        {
          type: "tabset",
          weight: 33,
          children: [
            {
              type: "tab",
              name: "Layout Info",
              component: "info",
              config: { description: "This layout demonstrates save/load capability. Use model.toJson() to serialize the current state to JSON." },
            },
          ],
        },
        {
          type: "tabset",
          weight: 34,
          children: [
            {
              type: "tab",
              name: "Serialize",
              component: "info",
              config: { description: "Call model.toJson() to get a JSON snapshot. Save it to localStorage or a file. Rearrange tabs first to see state changes." },
            },
          ],
        },
        {
          type: "tabset",
          weight: 33,
          children: [
            {
              type: "tab",
              name: "Restore",
              component: "info",
              config: { description: "To restore, pass saved JSON to Model.fromJson(). The layout, tab positions, and sizes are all restored." },
            },
          ],
        },
      ],
    },
  },

  basic_overflow: {
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
            { type: "tab", name: "Tab 1", component: "info", config: { description: "Tab overflow test. This tabset has 12 tabs — when the header can't fit them all, an overflow menu appears." } },
            { type: "tab", name: "Tab 2", component: "info", config: { description: "Overflow menu shows hidden tabs as a dropdown list. Click a hidden tab name to select it." } },
            { type: "tab", name: "Tab 3", component: "info", config: { description: "Resize the window narrower to trigger the overflow menu showing extra tabs." } },
            { type: "tab", name: "Tab 4", component: "info", config: { description: "Tab 4 content — part of the overflow test group." } },
            { type: "tab", name: "Tab 5", component: "info", config: { description: "Tab 5 content — part of the overflow test group." } },
            { type: "tab", name: "Tab 6", component: "info", config: { description: "Tab 6 content — part of the overflow test group." } },
            { type: "tab", name: "Tab 7", component: "info", config: { description: "Tab 7 content — part of the overflow test group." } },
            { type: "tab", name: "Tab 8", component: "info", config: { description: "Tab 8 content — part of the overflow test group." } },
            { type: "tab", name: "Tab 9", component: "info", config: { description: "Tab 9 content — part of the overflow test group." } },
            { type: "tab", name: "Tab 10", component: "info", config: { description: "Tab 10 content — part of the overflow test group." } },
            { type: "tab", name: "Tab 11", component: "info", config: { description: "Tab 11 content — part of the overflow test group." } },
            { type: "tab", name: "Tab 12", component: "info", config: { description: "Tab 12 — the last tab. With 12 tabs, overflow is guaranteed at most viewport widths." } },
          ],
        },
      ],
    },
  },

  basic_drop_disabled: {
    global: { ...defaultGlobal },
    borders: [],
    layout: {
      type: "row",
      weight: 100,
      children: [
        {
          type: "tabset",
          weight: 33,
          children: [
            {
              type: "tab",
              name: "Source A",
              component: "info",
              config: { description: "Drag this tab to the locked tabset (middle) — it will reject the drop. Drag to the right tabset instead." },
            },
          ],
        },
        {
          type: "tabset",
          weight: 34,
          enableDrop: false,
          children: [
            {
              type: "tab",
              name: "No Drop Zone",
              component: "info",
              config: { description: "This tabset has enableDrop: false. Tabs cannot be dropped here. The drop indicator won't appear." },
            },
          ],
        },
        {
          type: "tabset",
          weight: 33,
          children: [
            {
              type: "tab",
              name: "Source B",
              component: "info",
              config: { description: "This tabset accepts drops normally. Try dragging tabs from the left pane here." },
            },
          ],
        },
      ],
    },
  },

  basic_delete_when_empty: {
    global: { ...defaultGlobal, tabSetEnableDeleteWhenEmpty: true },
    borders: [],
    layout: {
      type: "row",
      weight: 100,
      children: [
        {
          type: "tabset",
          weight: 33,
          children: [
            {
              type: "tab",
              name: "Closeable A",
              enableClose: true,
              component: "info",
              config: { description: "Close this tab (click X) and the empty tabset will be automatically removed from the layout." },
            },
          ],
        },
        {
          type: "tabset",
          weight: 34,
          children: [
            {
              type: "tab",
              name: "Closeable B",
              enableClose: true,
              component: "info",
              config: { description: "tabSetEnableDeleteWhenEmpty: true removes empty tabsets. Close this tab to see the tabset disappear." },
            },
          ],
        },
        {
          type: "tabset",
          weight: 33,
          children: [
            {
              type: "tab",
              name: "Closeable C",
              enableClose: true,
              component: "info",
              config: { description: "Last closeable tab. Closing all three tabs will remove all tabsets, leaving an empty layout." },
            },
          ],
        },
      ],
    },
  },

  basic_divide: {
    global: { ...defaultGlobal, tabSetEnableDivide: true },
    borders: [],
    layout: {
      type: "row",
      weight: 100,
      children: [
        {
          type: "tabset",
          weight: 50,
          children: [
            {
              type: "tab",
              name: "Alpha",
              component: "info",
              config: { description: "tabSetEnableDivide: true allows splitting. Drag a tab to the edge of a tabset to split it into two." },
            },
            {
              type: "tab",
              name: "Beta",
              component: "info",
              config: { description: "Drag this tab to the left/right/top/bottom edge of the other tabset to create a new split pane." },
            },
            {
              type: "tab",
              name: "Gamma",
              component: "info",
              config: { description: "With 3+ tabs per tabset, you have plenty to drag around and create complex split arrangements." },
            },
          ],
        },
        {
          type: "tabset",
          weight: 50,
          children: [
            {
              type: "tab",
              name: "Delta",
              component: "info",
              config: { description: "Drop target tabset. Drag tabs from the left pane to this pane's edges to divide it." },
            },
            {
              type: "tab",
              name: "Epsilon",
              component: "info",
              config: { description: "Each edge drop creates a new tabset adjacent to this one, splitting the space." },
            },
            {
              type: "tab",
              name: "Zeta",
              component: "info",
              config: { description: "The divider drops create either horizontal or vertical splits depending on the edge targeted." },
            },
          ],
        },
      ],
    },
  },

  basic_drag_disabled: {
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
            {
              type: "tab",
              name: "Draggable",
              component: "info",
              config: { description: "This tab can be dragged normally. Try dragging it to rearrange." },
            },
            {
              type: "tab",
              name: "Locked",
              enableDrag: false,
              component: "info",
              config: { description: "This tab has enableDrag: false — it cannot be dragged or moved. It stays pinned in place." },
            },
            {
              type: "tab",
              name: "Also Draggable",
              component: "info",
              config: { description: "This tab is draggable. Compare with the 'Locked' tab which resists drag attempts." },
            },
          ],
        },
      ],
    },
  },

  basic_close_disabled: {
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
            {
              type: "tab",
              name: "Closeable",
              enableClose: true,
              component: "info",
              config: { description: "This tab has enableClose: true — click the X to close it." },
            },
            {
              type: "tab",
              name: "Permanent",
              enableClose: false,
              component: "info",
              config: { description: "This tab has enableClose: false — no close button appears. It cannot be removed by the user." },
            },
            {
              type: "tab",
              name: "Also Closeable",
              enableClose: true,
              component: "info",
              config: { description: "Another closeable tab. Notice the 'Permanent' tab has no X button while closeable tabs do." },
            },
          ],
        },
      ],
    },
  },

  basic_tab_rename: {
    global: { ...defaultGlobal, tabEnableRename: true },
    borders: [],
    layout: {
      type: "row",
      weight: 100,
      children: [
        {
          type: "tabset",
          weight: 100,
          children: [
            {
              type: "tab",
              name: "Rename Me",
              component: "info",
              config: { description: "Double-click this tab's name in the header to edit it inline. Press Enter to confirm, Escape to cancel." },
            },
            {
              type: "tab",
              name: "Try Double-Click",
              component: "info",
              config: { description: "tabEnableRename: true enables inline tab renaming. Double-click any tab header text to start editing." },
            },
            {
              type: "tab",
              name: "Editable Name",
              component: "info",
              config: { description: "After renaming, the new name is stored in the model. Serialize with model.toJson() to persist renamed tabs." },
            },
          ],
        },
      ],
    },
  },

  basic_icons: {
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
            {
              type: "tab",
              name: "Home",
              icon: "🏠",
              component: "info",
              config: { description: "Tab with a 🏠 icon. Icons appear before the tab name in the header." },
            },
            {
              type: "tab",
              name: "Settings",
              icon: "⚙️",
              component: "info",
              config: { description: "Tab with a ⚙️ icon. Icons can be emoji strings or image paths." },
            },
            {
              type: "tab",
              name: "Search",
              icon: "🔍",
              component: "info",
              config: { description: "Tab with a 🔍 icon. The icon attribute accepts any string rendered as the icon." },
            },
            {
              type: "tab",
              name: "Star",
              icon: "⭐",
              component: "info",
              config: { description: "Tab with a ⭐ icon. Mix of icon styles to show versatility." },
            },
            {
              type: "tab",
              name: "Warning",
              icon: "⚠️",
              component: "info",
              config: { description: "Tab with a ⚠️ icon. Five tabs with different emoji icons demonstrating the icon feature." },
            },
          ],
        },
      ],
    },
  },

  basic_help_text: {
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
            {
              type: "tab",
              name: "Overview",
              helpText: "This tab shows a project overview. Hover to see this tooltip.",
              component: "info",
              config: { description: "Hover over this tab's header to see a tooltip. The helpText attribute provides contextual information on hover." },
            },
            {
              type: "tab",
              name: "Details",
              helpText: "Detailed information panel. Contains in-depth content and analysis.",
              component: "info",
              config: { description: "This tab has helpText set. Hover the tab header to see the tooltip with additional context." },
            },
            {
              type: "tab",
              name: "Actions",
              helpText: "Action buttons and controls. Use this panel to perform operations.",
              component: "info",
              config: { description: "Third tab with helpText tooltip. Tooltips help users understand tab purpose without switching to it." },
            },
          ],
        },
      ],
    },
  },

  // ── Callback & Render Demos ──────────────────────────────────────────

  render_custom_tab: {
    global: { ...defaultGlobal },
    borders: [
      {
        type: "border",
        location: "bottom",
        children: [
          { type: "tab", id: "render_border_tab", name: "Border Tab", component: "info", config: { description: "Border tab with custom rendering" } },
        ],
      },
    ],
    layout: {
      type: "row",
      weight: 100,
      children: [
        {
          type: "tabset",
          weight: 50,
          children: [
            { type: "tab", id: "render_tab_a", name: "Custom Leading", component: "info", config: { description: "This tab has a custom leading icon, custom content text, and extra buttons injected via onRenderTab." } },
            { type: "tab", id: "render_tab_b", name: "Custom Buttons", component: "info", config: { description: "This tab has extra action buttons injected into the tab header." } },
          ],
        },
        {
          type: "tabset",
          weight: 50,
          children: [
            { type: "tab", name: "Normal Tab", component: "info", config: { description: "This tab is NOT customized — verifies selective rendering." } },
          ],
        },
      ],
    },
  },

  render_custom_tabset: {
    global: { ...defaultGlobal },
    borders: [],
    layout: {
      type: "row",
      weight: 100,
      children: [
        {
          type: "tabset",
          id: "render_ts_buttons",
          weight: 50,
          children: [
            { type: "tab", name: "Panel A", component: "info", config: { description: "The tabset header above has custom action buttons injected via onRenderTabSet." } },
          ],
        },
        {
          type: "tabset",
          id: "render_ts_sticky",
          weight: 50,
          children: [
            { type: "tab", name: "Panel B", component: "info", config: { description: "The tabset header above has a sticky '+' add button via onRenderTabSet stickyButtons." } },
          ],
        },
      ],
    },
  },

  render_drag_rect: {
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
            { type: "tab", name: "Drag Me", component: "info", config: { description: "Drag this tab to another tabset. The drag preview rectangle shows custom styled content instead of the default outline." } },
            { type: "tab", name: "Also Drag Me", component: "info", config: { description: "Another tab to test custom drag rectangles." } },
          ],
        },
        {
          type: "tabset",
          weight: 50,
          children: [
            { type: "tab", name: "Drop Target", component: "info", config: { description: "Drop tabs here to test the custom drag rect preview." } },
          ],
        },
      ],
    },
  },

  render_tab_placeholder: {
    global: { ...defaultGlobal, tabSetEnableClose: true },
    borders: [],
    layout: {
      type: "row",
      weight: 100,
      children: [
        {
          type: "tabset",
          weight: 50,
          children: [
            { type: "tab", name: "Close Me", component: "info", config: { description: "Close all tabs in a tabset to see the placeholder content rendered by onTabSetPlaceHolder." } },
          ],
        },
        {
          type: "tabset",
          weight: 50,
          children: [
            { type: "tab", name: "Keep Open", component: "info", config: { description: "Keep this tab so you can compare empty vs non-empty tabsets." } },
          ],
        },
      ],
    },
  },

  render_context_menu: {
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
            { type: "tab", name: "Right-Click Me", component: "info", config: { description: "Right-click this tab header to see a custom context menu (logged to console)." } },
            { type: "tab", name: "Also Right-Click", component: "info", config: { description: "Another tab for context menu testing." } },
          ],
        },
      ],
    },
  },

  render_class_mapper: {
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
            { type: "tab", name: "Mapped Classes", component: "info", config: { description: "The classNameMapper adds a 'demo-mapped' prefix to all CSS class names. Inspect the DOM to verify." } },
          ],
        },
        {
          type: "tabset",
          weight: 50,
          children: [
            { type: "tab", name: "Check DOM", component: "info", config: { description: "Open DevTools and inspect class names — they should include both the original and mapped class." } },
          ],
        },
      ],
    },
  },

  render_action_intercept: {
    global: { ...defaultGlobal, tabSetEnableClose: true },
    borders: [],
    layout: {
      type: "row",
      weight: 100,
      children: [
        {
          type: "tabset",
          weight: 50,
          children: [
            { type: "tab", name: "Try Closing", component: "info", config: { description: "Try closing this tab — the onAction callback blocks FlexLayout_DeleteTab actions and logs them to the console instead." } },
            { type: "tab", name: "Moveable", component: "info", config: { description: "Moving and selecting tabs still works — only delete is intercepted." } },
          ],
        },
        {
          type: "tabset",
          weight: 50,
          children: [
            { type: "tab", name: "Action Log", component: "info", config: { description: "Open the browser console to see intercepted actions logged in real time." } },
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

  const onActionAddTab = () => {
    const m = model();
    const activeTabset = m.getActiveTabset();
    if (activeTabset) {
      m.doAction(
        Action.addNode(
          { type: "tab", name: "New " + nextIndex++, component: "testing" },
          activeTabset.getId(),
          "center",
          -1,
        ),
      );
      setModel(m);
    }
  };

  const onActionDeleteActive = () => {
    const m = model();
    const activeTabset = m.getActiveTabset();
    if (activeTabset) {
      const children = activeTabset.getChildren();
      const selected = activeTabset.getSelected();
      if (selected >= 0 && selected < children.length) {
        const tabId = children[selected].getId();
        m.doAction(Action.deleteTab(tabId));
        setModel(m);
      }
    }
  };

  const [edgeDockEnabled, setEdgeDockEnabled] = createSignal(true);
  const [verticalOrientation, setVerticalOrientation] = createSignal(false);

  const onToggleEdgeDock = () => {
    const m = model();
    const newVal = !edgeDockEnabled();
    m.doAction(Action.updateModelAttributes({ enableEdgeDock: newVal }));
    setEdgeDockEnabled(newVal);
    setModel(m);
  };

  const onToggleVertical = () => {
    const m = model();
    const newVal = !verticalOrientation();
    m.doAction(Action.updateModelAttributes({ rootOrientationVertical: newVal }));
    setVerticalOrientation(newVal);
    setModel(m);
  };

  const onExternalDragStart = (event: DragEvent) => {
    const tabJson = {
      type: "tab",
      name: "External " + nextIndex++,
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

  const onEqualWeights = () => {
    const m = model();
    const root = m.getRoot();
    if (root) {
      const children = root.getChildren();
      const equalWeights = children.map(() => 50);
      m.doAction(Action.adjustWeights(root.getId(), equalWeights, "row"));
      setModel(m);
    }
  };

  const onWeights8020 = () => {
    const m = model();
    const root = m.getRoot();
    if (root) {
      m.doAction(Action.adjustWeights(root.getId(), [80, 20], "row"));
      setModel(m);
    }
  };

  const onRenderTab = (node: TabNode, renderValues: ITabRenderValues) => {
    if (["onRenderTab1", "onRenderTab2"].includes(node.getId())) {
      renderValues.leading = (
        <img
          src="images/settings.svg"
          style={{ width: "1em", height: "1em" }}
        />
      );
      renderValues.content = <span>{node.getId()}</span>;
      renderValues.buttons.push(
        <img
          src="images/folder.svg"
          style={{ width: "1em", height: "1em" }}
        />,
      );
    } else if (layoutName === "render_custom_tab" && node.getId() === "render_tab_a") {
      renderValues.leading = <span data-testid="custom-leading" style={{ "font-size": "1.1em" }}>★</span>;
      renderValues.content = <span>Custom Tab A</span>;
      renderValues.buttons.push(<span data-testid="custom-btn" style={{ cursor: "pointer" }}>✎</span>);
    } else if (layoutName === "render_custom_tab" && node.getId() === "render_tab_b") {
      renderValues.buttons.push(<span data-testid="extra-btn-1" style={{ cursor: "pointer" }}>📌</span>);
      renderValues.buttons.push(<span data-testid="extra-btn-2" style={{ cursor: "pointer" }}>🔒</span>);
    }
  };

  const onRenderTabSet = (
    node: TabSetNode | BorderNode,
    renderValues: ITabSetRenderValues,
  ) => {
    if (["onRenderTabSet1", "onRenderTabSet2"].includes(node.getId())) {
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
    } else if (layoutName === "render_custom_tabset" && node.getId() === "render_ts_buttons") {
      renderValues.buttons.push(<span data-testid="ts-btn-save" style={{ cursor: "pointer" }}>💾</span>);
      renderValues.buttons.push(<span data-testid="ts-btn-gear" style={{ cursor: "pointer" }}>⚙</span>);
    } else if (layoutName === "render_custom_tabset" && node.getId() === "render_ts_sticky") {
      renderValues.stickyButtons.push(
        <span data-testid="ts-sticky-add" style={{ cursor: "pointer", "margin-left": "4px" }} title="Add tab">＋</span>,
      );
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

  const onAction = (action: any) => {
    if (layoutName === "render_action_intercept" && action.type === "FlexLayout_DeleteTab") {
      console.log("[render_action_intercept] Blocked action:", action);
      return undefined;
    }
    return action;
  };

  const classNameMapper = layoutName === "render_class_mapper"
    ? (defaultClassName: string) => `demo-mapped ${defaultClassName}`
    : undefined;

  const needsCustomTab = ["render_custom_tab", "test_with_min_size"].includes(layoutName);
  const needsCustomTabSet = ["render_custom_tabset", "test_with_min_size"].includes(layoutName);

  return (
    <div
      style={{
        width: "100vw",
        height: "100vh",
        display: "flex",
        "flex-direction": "column",
      }}
    >
      <div style={{ padding: "4px", display: "flex", gap: "4px", "flex-wrap": "wrap", "align-items": "center" }}>
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

        {layoutName === "action_add_remove" && (
          <>
            <span style={{ "border-left": "1px solid #666", height: "20px", margin: "0 4px" }} />
            <button data-id="action-add-tab" onClick={onActionAddTab}>
              Add Tab
            </button>
            <button data-id="action-delete-active" onClick={onActionDeleteActive}>
              Delete Active
            </button>
          </>
        )}

        {layoutName === "action_model_update" && (
          <>
            <span style={{ "border-left": "1px solid #666", height: "20px", margin: "0 4px" }} />
            <button data-id="action-toggle-edge-dock" onClick={onToggleEdgeDock}>
              Edge Dock: {edgeDockEnabled() ? "ON" : "OFF"}
            </button>
            <button data-id="action-toggle-vertical" onClick={onToggleVertical}>
              Vertical: {verticalOrientation() ? "ON" : "OFF"}
            </button>
          </>
        )}

        {layoutName === "action_weights" && (
          <>
            <span style={{ "border-left": "1px solid #666", height: "20px", margin: "0 4px" }} />
            <button data-id="action-equal-weights" onClick={onEqualWeights}>
              Equal Weights
            </button>
            <button data-id="action-weights-8020" onClick={onWeights8020}>
              80/20
            </button>
          </>
        )}
      </div>

      {layoutName === "action_external_drag" && (
        <div style={{ padding: "4px", display: "flex", gap: "4px", "align-items": "center" }}>
          <div
            data-id="external-drag-source"
            draggable={true}
            onDragStart={onExternalDragStart}
            style={{
              padding: "6px 12px",
              background: "#4a90d9",
              color: "#fff",
              "border-radius": "4px",
              cursor: "grab",
              "user-select": "none",
              "font-size": "13px",
            }}
          >
            Drag me into the layout
          </div>
        </div>
      )}

      <div style={{ flex: 1, position: "relative" }}>
        <Layout
          model={model()}
          factory={factory}
          onAction={onAction}
          onRenderTab={needsCustomTab ? onRenderTab : undefined}
          onRenderTabSet={needsCustomTabSet ? onRenderTabSet : undefined}
          classNameMapper={classNameMapper}
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
