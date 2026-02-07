import { DockLocation } from "./DockLocation";
import { Rect } from "./Rect";

export interface IDropTarget {
  [key: string]: any;
}

export interface Node {
  [key: string]: any;
}

export class DropInfo {
  node: Node & IDropTarget;
  rect: Rect;
  location: DockLocation;
  index: number;
  className: string;

  constructor(
    node: Node & IDropTarget,
    rect: Rect,
    location: DockLocation,
    index: number,
    className: string
  ) {
    this.node = node;
    this.rect = rect;
    this.location = location;
    this.index = index;
    this.className = className;
  }
}
