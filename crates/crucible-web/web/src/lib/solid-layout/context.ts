/**
 * Layout context: provides reactive bridge, model, actions, and rendering
 * config to the entire component tree.
 *
 * Every layout child accesses this via `useLayoutContext()`.
 */

import { createContext, useContext, type JSX } from "solid-js";
import type { Model } from "../flexlayout/model/Model";
import type { TabNode } from "../flexlayout/model/TabNode";
import type { LayoutAction } from "../flexlayout/model/Action";
import type { LayoutBridge } from "./bridge";

export interface LayoutContextValue {
  /** Reactive bridge: store, signals, drag state */
  bridge: LayoutBridge;
  /** The mutable FlexLayout model instance */
  model: Model;
  /** Dispatch an action through the model */
  doAction: (action: LayoutAction) => void;
  /** Map default CSS class names to custom ones */
  classNameMapper?: (defaultClassName: string) => string;
  /** Render content for a given tab node */
  factory: (node: TabNode) => JSX.Element;
}

export const LayoutContext = createContext<LayoutContextValue>();

/**
 * Access the layout context from any descendant of `<Layout>`.
 * Throws if called outside a `<Layout>` provider.
 */
export function useLayoutContext(): LayoutContextValue {
  const ctx = useContext(LayoutContext);
  if (!ctx) {
    throw new Error(
      "useLayoutContext must be used within a <Layout> component",
    );
  }
  return ctx;
}
