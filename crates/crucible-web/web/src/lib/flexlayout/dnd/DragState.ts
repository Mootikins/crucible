import { Node } from "../model/Node";
import { IJsonTabNode } from "../types";

/**
 * Enum for drag source type
 */
export enum DragSource {
  Internal = "internal",
  External = "external",
  Add = "add"
}

/**
 * Global drag state holder for HTML5 DnD operations
 * Maintains state across drag lifecycle (dragstart -> dragover -> drop)
 */
export class DragState {
  public readonly dragSource: DragSource;
  public readonly dragNode: Node | undefined;
  public readonly dragJson: IJsonTabNode | undefined;
  public readonly fnNewNodeDropped: ((node?: Node, event?: DragEvent) => void) | undefined;

  public constructor(
    dragSource: DragSource,
    dragNode: Node | undefined,
    dragJson: IJsonTabNode | undefined,
    fnNewNodeDropped: ((node?: Node, event?: DragEvent) => void) | undefined
  ) {
    this.dragSource = dragSource;
    this.dragNode = dragNode;
    this.dragJson = dragJson;
    this.fnNewNodeDropped = fnNewNodeDropped;
  }
}
