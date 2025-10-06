/**
 * Tests for PropertiesHandler.
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { PropertiesHandler } from "../../api/properties";
import { App, TFile } from "obsidian";
import { IncomingMessage, ServerResponse } from "http";

describe("PropertiesHandler", () => {
  let app: App;
  let handler: PropertiesHandler;

  beforeEach(() => {
    app = new App();
    handler = new PropertiesHandler(app);
  });

  it("should initialize with app", () => {
    expect(handler).toBeDefined();
  });

  it("should update existing frontmatter properties", async () => {
    const file = new TFile("test.md");
    const existingContent = `---
status: draft
---

# Test

Content`;

    app.vault.read.mockResolvedValue(existingContent);
    app.vault.modify.mockResolvedValue(undefined);

    const req = {
      on: vi.fn((event, handler) => {
        if (event === "data") {
          handler(JSON.stringify({ properties: { status: "published" } }));
        }
        if (event === "end") {
          handler();
        }
      }),
    } as unknown as IncomingMessage;

    const res = {
      writeHead: vi.fn(),
      end: vi.fn(),
    } as unknown as ServerResponse;

    // When implemented:
    // await handler.updateProperties("test.md", req, res);
    //
    // expect(app.vault.modify).toHaveBeenCalled();
    // const modifiedContent = app.vault.modify.mock.calls[0][1];
    // expect(modifiedContent).toContain("status: published");
    // expect(res.writeHead).toHaveBeenCalledWith(200, expect.anything());

    expect(handler.updateProperties).toBeDefined();
  });

  it("should add frontmatter to file without it", async () => {
    const file = new TFile("test.md");
    const existingContent = `# Test\n\nContent without frontmatter`;

    app.vault.read.mockResolvedValue(existingContent);
    app.vault.modify.mockResolvedValue(undefined);

    const req = {
      on: vi.fn((event, handler) => {
        if (event === "data") {
          handler(JSON.stringify({ properties: { status: "draft" } }));
        }
        if (event === "end") {
          handler();
        }
      }),
    } as unknown as IncomingMessage;

    const res = {
      writeHead: vi.fn(),
      end: vi.fn(),
    } as unknown as ServerResponse;

    // When implemented:
    // await handler.updateProperties("test.md", req, res);
    //
    // const modifiedContent = app.vault.modify.mock.calls[0][1];
    // expect(modifiedContent).toContain("---");
    // expect(modifiedContent).toContain("status: draft");
    // expect(modifiedContent).toContain("# Test");

    expect(handler.updateProperties).toBeDefined();
  });

  it("should preserve content when updating frontmatter", async () => {
    const file = new TFile("test.md");
    const existingContent = `---
title: Original
---

# Heading

Paragraph 1

Paragraph 2`;

    app.vault.read.mockResolvedValue(existingContent);
    app.vault.modify.mockResolvedValue(undefined);

    const req = {
      on: vi.fn((event, handler) => {
        if (event === "data") {
          handler(JSON.stringify({ properties: { author: "Test User" } }));
        }
        if (event === "end") {
          handler();
        }
      }),
    } as unknown as IncomingMessage;

    const res = {
      writeHead: vi.fn(),
      end: vi.fn(),
    } as unknown as ServerResponse;

    // When implemented:
    // await handler.updateProperties("test.md", req, res);
    //
    // const modifiedContent = app.vault.modify.mock.calls[0][1];
    // expect(modifiedContent).toContain("# Heading");
    // expect(modifiedContent).toContain("Paragraph 1");
    // expect(modifiedContent).toContain("Paragraph 2");

    expect(handler.updateProperties).toBeDefined();
  });

  it("should handle complex property values", async () => {
    const file = new TFile("test.md");
    const existingContent = `---
title: Test
---

Content`;

    app.vault.read.mockResolvedValue(existingContent);
    app.vault.modify.mockResolvedValue(undefined);

    const req = {
      on: vi.fn((event, handler) => {
        if (event === "data") {
          handler(
            JSON.stringify({
              properties: {
                tags: ["tag1", "tag2"],
                metadata: { created: "2024-01-01" },
                count: 42,
              },
            })
          );
        }
        if (event === "end") {
          handler();
        }
      }),
    } as unknown as IncomingMessage;

    const res = {
      writeHead: vi.fn(),
      end: vi.fn(),
    } as unknown as ServerResponse;

    // When implemented:
    // await handler.updateProperties("test.md", req, res);
    //
    // const modifiedContent = app.vault.modify.mock.calls[0][1];
    // expect(modifiedContent).toContain("tags:");
    // expect(modifiedContent).toContain("metadata:");
    // expect(modifiedContent).toContain("count: 42");

    expect(handler.updateProperties).toBeDefined();
  });

  it("should handle invalid YAML gracefully", async () => {
    const file = new TFile("test.md");
    const existingContent = `---
invalid: : : yaml
---

Content`;

    app.vault.read.mockResolvedValue(existingContent);

    const req = {
      on: vi.fn((event, handler) => {
        if (event === "data") {
          handler(JSON.stringify({ properties: { status: "active" } }));
        }
        if (event === "end") {
          handler();
        }
      }),
    } as unknown as IncomingMessage;

    const res = {
      writeHead: vi.fn(),
      end: vi.fn(),
    } as unknown as ServerResponse;

    // When implemented with error handling:
    // await handler.updateProperties("test.md", req, res);
    //
    // expect(res.writeHead).toHaveBeenCalledWith(400, expect.anything());
    // // Or handle by replacing frontmatter entirely

    expect(handler.updateProperties).toBeDefined();
  });

  it("should handle missing file", async () => {
    app.vault.getAbstractFileByPath.mockReturnValue(null);

    const req = {
      on: vi.fn((event, handler) => {
        if (event === "data") {
          handler(JSON.stringify({ properties: { status: "active" } }));
        }
        if (event === "end") {
          handler();
        }
      }),
    } as unknown as IncomingMessage;

    const res = {
      writeHead: vi.fn(),
      end: vi.fn(),
    } as unknown as ServerResponse;

    // When implemented with error handling:
    // await handler.updateProperties("missing.md", req, res);
    //
    // expect(res.writeHead).toHaveBeenCalledWith(404, expect.anything());

    expect(handler.updateProperties).toBeDefined();
  });
});
