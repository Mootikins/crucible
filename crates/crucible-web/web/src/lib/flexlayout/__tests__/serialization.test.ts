import { describe, it, expect } from "vitest";
import { Model } from "../model/Model";
import { twoTabs, withBorders, threeTabs } from "./fixtures";
import type { IJsonModel } from "../types";

describe("JSON Serialization/Deserialization", () => {
  describe("Round-trip tests with fixtures", () => {
    it("should round-trip twoTabs fixture: fromJson -> toJson produces semantically identical JSON", () => {
      // Load from JSON
      const model = Model.fromJson(twoTabs);
      
      // Serialize back to JSON
      const serialized = model.toJson();
      
      // Deep equality check
      expect(serialized).toEqual(twoTabs);
    });

    it("should round-trip withBorders fixture: fromJson -> toJson produces semantically identical JSON", () => {
      // Load from JSON
      const model = Model.fromJson(withBorders);
      
      // Serialize back to JSON
      const serialized = model.toJson();
      
      // Deep equality check
      expect(serialized).toEqual(withBorders);
    });

    it("should round-trip threeTabs fixture: fromJson -> toJson produces semantically identical JSON", () => {
      // Load from JSON
      const model = Model.fromJson(threeTabs);
      
      // Serialize back to JSON
      const serialized = model.toJson();
      
      // Deep equality check
      expect(serialized).toEqual(threeTabs);
    });

    it("should preserve all node attributes during round-trip", () => {
      const model = Model.fromJson(threeTabs);
      const serialized = model.toJson();
      
      // Check layout structure
      expect(serialized.layout).toBeDefined();
      expect(serialized.layout?.type).toBe("row");
      expect(serialized.layout?.children).toHaveLength(3);
      
      // Check each tabset
      const children = serialized.layout?.children || [];
      expect(children[0]?.type).toBe("tabset");
      expect(children[1]?.type).toBe("tabset");
      expect(children[2]?.type).toBe("tabset");
      
      // Check tabs
      expect((children[0] as any)?.children?.[0]?.name).toBe("One");
      expect((children[1] as any)?.children?.[0]?.name).toBe("Two");
      expect((children[2] as any)?.children?.[0]?.name).toBe("Three");
    });

    it("should preserve border nodes during round-trip", () => {
      const model = Model.fromJson(withBorders);
      const serialized = model.toJson();
      
      // Check borders are preserved
      expect(serialized.borders).toBeDefined();
      expect(serialized.borders?.length).toBeGreaterThan(0);
      
      // Check border locations
      const borderLocations = serialized.borders?.map((b) => b.location) || [];
      expect(borderLocations).toContain("top");
      expect(borderLocations).toContain("bottom");
      expect(borderLocations).toContain("left");
      expect(borderLocations).toContain("right");
    });

    it("should preserve global attributes during round-trip", () => {
      const customJson: IJsonModel = {
        global: { customAttr: "value", number: 42 },
        borders: [],
        layout: {
          type: "tabset",
          children: [
            {
              type: "tab",
              name: "Test",
              component: "test",
            },
          ],
        },
      };
      
      const model = Model.fromJson(customJson);
      const serialized = model.toJson();
      
      expect(serialized.global).toEqual(customJson.global);
    });

    it("should preserve tab component and icon attributes", () => {
      const model = Model.fromJson(threeTabs);
      const serialized = model.toJson();
      
      const children = serialized.layout?.children || [];
      const secondTabset = children[1] as any;
      const secondTab = secondTabset?.children?.[0];
      
      expect(secondTab?.component).toBe("text");
      expect(secondTab?.icon).toBe("/test/images/settings.svg");
    });

    it("should preserve tabset name attribute", () => {
      const model = Model.fromJson(threeTabs);
      const serialized = model.toJson();
      
      const children = serialized.layout?.children || [];
      const secondTabset = children[1] as any;
      
      expect(secondTabset?.name).toBe("TheHeader");
    });

    it("should preserve weight values during round-trip", () => {
      const model = Model.fromJson(twoTabs);
      const serialized = model.toJson();
      
      const children = serialized.layout?.children || [];
      expect(children[0]?.weight).toBe(50);
      expect(children[1]?.weight).toBe(50);
    });

    it("should preserve node IDs during round-trip", () => {
      const model = Model.fromJson(twoTabs);
      const serialized = model.toJson();
      
      const children = serialized.layout?.children || [];
      // Second tabset has id "ts1" in fixture
      expect(children[1]?.id).toBe("ts1");
    });
  });

  describe("Error handling for invalid JSON", () => {
    it("should handle missing layout gracefully", () => {
      const invalidJson: IJsonModel = {
        global: {},
        borders: [],
      };
      
      expect(() => {
        Model.fromJson(invalidJson);
      }).not.toThrow();
      
      const model = Model.fromJson(invalidJson);
      expect(model).toBeDefined();
    });

    it("should handle empty borders array", () => {
      const json: IJsonModel = {
        global: {},
        borders: [],
        layout: {
          type: "tabset",
          children: [
            {
              type: "tab",
              name: "Test",
              component: "test",
            },
          ],
        },
      };
      
      const model = Model.fromJson(json);
      const serialized = model.toJson();
      
      expect(serialized.borders).toBeDefined();
      expect(Array.isArray(serialized.borders)).toBe(true);
    });

    it("should handle missing global attributes", () => {
      const json: IJsonModel = {
        borders: [],
        layout: {
          type: "tabset",
          children: [
            {
              type: "tab",
              name: "Test",
              component: "test",
            },
          ],
        },
      };
      
      const model = Model.fromJson(json);
      const serialized = model.toJson();
      
      expect(serialized.global).toBeDefined();
    });

    it("should handle null/undefined values in JSON", () => {
      const json: IJsonModel = {
        global: { nullValue: null, undefinedValue: undefined },
        borders: [],
        layout: {
          type: "tabset",
          children: [
            {
              type: "tab",
              name: "Test",
              component: "test",
            },
          ],
        },
      };
      
      expect(() => {
        Model.fromJson(json);
      }).not.toThrow();
      
      const model = Model.fromJson(json);
      expect(model).toBeDefined();
    });

    it("should handle deeply nested structures", () => {
      const deepJson: IJsonModel = {
        global: {},
        borders: [],
        layout: {
          type: "row",
          children: [
            {
              type: "row",
              children: [
                {
                  type: "row",
                  children: [
                    {
                      type: "tabset",
                      children: [
                        {
                          type: "tab",
                          name: "Deep",
                          component: "test",
                        },
                      ],
                    },
                  ],
                },
              ],
            },
          ],
        },
      };
      
      expect(() => {
        Model.fromJson(deepJson);
      }).not.toThrow();
      
      const model = Model.fromJson(deepJson);
      expect(model).toBeDefined();
    });

    it("should handle multiple borders with same location", () => {
      const json: IJsonModel = {
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
            location: "top",
            children: [
              {
                type: "tab",
                name: "top2",
                component: "text",
              },
            ],
          },
        ],
        layout: {
          type: "tabset",
          children: [
            {
              type: "tab",
              name: "Main",
              component: "text",
            },
          ],
        },
      };
      
      expect(() => {
        Model.fromJson(json);
      }).not.toThrow();
      
      const model = Model.fromJson(json);
      expect(model).toBeDefined();
    });

    it("should handle empty tabset", () => {
      const json: IJsonModel = {
        global: {},
        borders: [],
        layout: {
          type: "tabset",
          children: [],
        },
      };
      
      expect(() => {
        Model.fromJson(json);
      }).not.toThrow();
      
      const model = Model.fromJson(json);
      expect(model).toBeDefined();
    });

    it("should handle missing component attribute on tab", () => {
      const json: IJsonModel = {
        global: {},
        borders: [],
        layout: {
          type: "tabset",
          children: [
            {
              type: "tab",
              name: "NoComponent",
            },
          ],
        },
      };
      
      expect(() => {
        Model.fromJson(json);
      }).not.toThrow();
      
      const model = Model.fromJson(json);
      expect(model).toBeDefined();
    });
  });

  describe("Multiple round-trips", () => {
    it("should maintain data integrity through multiple round-trips", () => {
      let model = Model.fromJson(twoTabs);
      let json1 = model.toJson();
      
      model = Model.fromJson(json1);
      let json2 = model.toJson();
      
      model = Model.fromJson(json2);
      let json3 = model.toJson();
      
      // All three serializations should be identical
      expect(json1).toEqual(json2);
      expect(json2).toEqual(json3);
    });

    it("should maintain data integrity through multiple round-trips with borders", () => {
      let model = Model.fromJson(withBorders);
      let json1 = model.toJson();
      
      model = Model.fromJson(json1);
      let json2 = model.toJson();
      
      model = Model.fromJson(json2);
      let json3 = model.toJson();
      
      // All three serializations should be identical
      expect(json1).toEqual(json2);
      expect(json2).toEqual(json3);
    });
  });
});
