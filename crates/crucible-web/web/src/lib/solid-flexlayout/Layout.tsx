import type { Component } from "solid-js";
import { computeNestingOrder } from "../flexlayout/rendering/VanillaBorderLayoutEngine";
import { SolidBinding } from "./SolidBinding";
import type { ILayoutProps } from "./LayoutTypes";

export type {
    ITabRenderValues,
    ITabSetRenderValues,
} from "./LayoutTypes";

export { computeNestingOrder };

export const Layout: Component<ILayoutProps> = (props) => {
    return <SolidBinding {...props} />;
};
