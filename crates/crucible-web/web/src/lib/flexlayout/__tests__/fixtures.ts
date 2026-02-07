import type { IJsonModel } from "../types";

/**
 * Two tabs fixture: two tabsets in a row, each with a single tab
 * textRender output: "/ts0/t0[One]*,/ts1/t0[Two]*"
 */
export const twoTabs: IJsonModel = {
  global: {},
  borders: [],
  layout: {
    type: "row",
    weight: 100,
    children: [
      {
        type: "tabset",
        id: "ts0",
        weight: 50,
        children: [
          {
            type: "tab",
            name: "One",
            component: "text",
          },
        ],
      },
      {
        type: "tabset",
        id: "ts1",
        weight: 50,
        children: [
          {
            type: "tab",
            name: "Two",
            component: "text",
          },
        ],
      },
    ],
  },
};

/**
 * With borders fixture: layout with border tabs on all four sides
 * textRender output: "/b/top/t0[top1],/b/bottom/t0[bottom1],/b/bottom/t1[bottom2],/b/left/t0[left1],/b/right/t0[right1],/ts0/t0[One]*"
 */
export const withBorders: IJsonModel = {
  global: {},
  borders: [
    {
      type: "border",
      location: "top",
      children: [
        {
          type: "tab",
          name: "top1",
          component: "text",
        },
      ],
    },
    {
      type: "border",
      location: "bottom",
      children: [
        {
          type: "tab",
          name: "bottom1",
          component: "text",
        },
        {
          type: "tab",
          name: "bottom2",
          component: "text",
        },
      ],
    },
    {
      type: "border",
      location: "left",
      children: [
        {
          type: "tab",
          name: "left1",
          component: "text",
        },
      ],
    },
    {
      type: "border",
      location: "right",
      children: [
        {
          type: "tab",
          name: "right1",
          component: "text",
        },
      ],
    },
  ],
  layout: {
    type: "row",
    weight: 100,
    children: [
      {
        type: "tabset",
        id: "ts0",
        weight: 50,
        children: [
          {
            type: "tab",
            name: "One",
            component: "text",
          },
        ],
      },
    ],
  },
};

/**
 * Three tabs fixture: three tabsets in a row, each with a single tab
 * textRender output: "/ts0/t0[One]*,/ts1/t0[Two]*,/ts2/t0[Three]*"
 */
export const threeTabs: IJsonModel = {
  global: {},
  borders: [],
  layout: {
    type: "row",
    weight: 100,
    children: [
      {
        type: "tabset",
        id: "ts0",
        weight: 50,
        children: [
          {
            type: "tab",
            name: "One",
            component: "text",
          },
        ],
      },
      {
        type: "tabset",
        id: "ts1",
        weight: 50,
        name: "TheHeader",
        children: [
          {
            type: "tab",
            name: "Two",
            icon: "/test/images/settings.svg",
            component: "text",
          },
        ],
      },
      {
        type: "tabset",
        id: "ts2",
        weight: 50,
        children: [
          {
            type: "tab",
            name: "Three",
            component: "text",
          },
        ],
      },
    ],
  },
};
