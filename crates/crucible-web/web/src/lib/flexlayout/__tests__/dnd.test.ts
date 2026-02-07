import { describe, it, expect } from "vitest";
import { DragState, DragSource } from "../dnd/DragState";
import { IJsonTabNode } from "../types";

describe("DragState", () => {
  it("should create drag state for internal drag", () => {
    const dragJson: IJsonTabNode = { type: "tab", name: "Tab1", component: "test" };
    const dragState = new DragState(
      DragSource.Internal,
      undefined,
      dragJson,
      undefined
    );

    expect(dragState.dragSource).toBe(DragSource.Internal);
    expect(dragState.dragJson).toBe(dragJson);
    expect(dragState.dragNode).toBeUndefined();
    expect(dragState.fnNewNodeDropped).toBeUndefined();
  });

  it("should create drag state for external drag", () => {
    const dragJson: IJsonTabNode = { type: "tab", name: "ExternalTab", component: "test" };
    const dragState = new DragState(
      DragSource.External,
      undefined,
      dragJson,
      undefined
    );

    expect(dragState.dragSource).toBe(DragSource.External);
    expect(dragState.dragNode).toBeUndefined();
    expect(dragState.dragJson).toBe(dragJson);
  });

  it("should create drag state for add drag", () => {
    const dragJson: IJsonTabNode = { type: "tab", name: "NewTab", component: "test" };
    const onDrop = () => {};
    const dragState = new DragState(
      DragSource.Add,
      undefined,
      dragJson,
      onDrop
    );

    expect(dragState.dragSource).toBe(DragSource.Add);
    expect(dragState.dragJson).toBe(dragJson);
    expect(dragState.fnNewNodeDropped).toBe(onDrop);
  });

  it("should handle drag state with callback", () => {
    let callbackCalled = false;
    const onDrop = () => {
      callbackCalled = true;
    };

    const dragState = new DragState(
      DragSource.Internal,
      undefined,
      undefined,
      onDrop
    );

    expect(dragState.fnNewNodeDropped).toBe(onDrop);
    dragState.fnNewNodeDropped?.(undefined, undefined);
    expect(callbackCalled).toBe(true);
  });

  it("should support all drag sources", () => {
    const sources = [DragSource.Internal, DragSource.External, DragSource.Add];
    
    sources.forEach(source => {
      const dragState = new DragState(source, undefined, undefined, undefined);
      expect(dragState.dragSource).toBe(source);
    });
  });
});
