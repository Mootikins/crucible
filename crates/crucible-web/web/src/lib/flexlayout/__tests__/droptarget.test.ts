import { describe, it, expect } from "vitest";
import { DockLocation } from "../core/DockLocation";
import { Rect } from "../core/Rect";

describe("Drop Target Detection", () => {
  it("should detect center zone with DockLocation.getLocation", () => {
    const rect = new Rect(0, 0, 1000, 1000);
    const location = DockLocation.getLocation(rect, 500, 500);
    expect(location).toBe(DockLocation.CENTER);
  });

  it("should detect top zone with DockLocation.getLocation", () => {
    const rect = new Rect(0, 0, 1000, 1000);
    const location = DockLocation.getLocation(rect, 500, 100);
    expect(location).toBe(DockLocation.TOP);
  });

  it("should detect bottom zone with DockLocation.getLocation", () => {
    const rect = new Rect(0, 0, 1000, 1000);
    const location = DockLocation.getLocation(rect, 500, 900);
    expect(location).toBe(DockLocation.BOTTOM);
  });

  it("should detect left zone with DockLocation.getLocation", () => {
    const rect = new Rect(0, 0, 1000, 1000);
    const location = DockLocation.getLocation(rect, 100, 500);
    expect(location).toBe(DockLocation.LEFT);
  });

  it("should detect right zone with DockLocation.getLocation", () => {
    const rect = new Rect(0, 0, 1000, 1000);
    const location = DockLocation.getLocation(rect, 900, 500);
    expect(location).toBe(DockLocation.RIGHT);
  });

  it("should handle offset rects", () => {
    const rect = new Rect(100, 100, 800, 800);
    const location = DockLocation.getLocation(rect, 500, 500);
    expect(location).toBe(DockLocation.CENTER);
  });

  it("should calculate quadrant as percentage", () => {
    const rect = new Rect(0, 0, 100, 100);
    const location = DockLocation.getLocation(rect, 25, 25);
    expect(location).toBe(DockLocation.CENTER);
  });

  it("should handle edge cases at boundaries", () => {
    const rect = new Rect(0, 0, 1000, 1000);
    const location = DockLocation.getLocation(rect, 250, 250);
    expect(location).toBe(DockLocation.CENTER);
  });

  it("should reflect dock locations correctly", () => {
    expect(DockLocation.TOP.reflect()).toBe(DockLocation.BOTTOM);
    expect(DockLocation.BOTTOM.reflect()).toBe(DockLocation.TOP);
    expect(DockLocation.LEFT.reflect()).toBe(DockLocation.RIGHT);
    expect(DockLocation.RIGHT.reflect()).toBe(DockLocation.LEFT);
    expect(DockLocation.CENTER.reflect()).toBe(DockLocation.TOP);
  });

  it("should split rects correctly for TOP location", () => {
    const rect = new Rect(0, 0, 1000, 1000);
    const split = DockLocation.TOP.split(rect, 200);
    expect(split.start.height).toBe(200);
    expect(split.end.height).toBe(800);
  });

  it("should split rects correctly for LEFT location", () => {
    const rect = new Rect(0, 0, 1000, 1000);
    const split = DockLocation.LEFT.split(rect, 300);
    expect(split.start.width).toBe(300);
    expect(split.end.width).toBe(700);
  });
});
