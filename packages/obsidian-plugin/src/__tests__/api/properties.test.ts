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
    app.kiln.getAbstractFileByPath.mockReturnValue(file);

    const existingContent = `---
status: draft
---

# Test

Content`;

    app.kiln.read.mockResolvedValue(existingContent);
    app.kiln.modify.mockResolvedValue(undefined);

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

    await handler.updateProperties("test.md", req, res);

    expect(app.kiln.modify).toHaveBeenCalled();
    const modifiedContent = app.kiln.modify.mock.calls[0][1];
    expect(modifiedContent).toContain("status: published");
    expect(res.writeHead).toHaveBeenCalledWith(200, expect.anything());
  });

  it("should add frontmatter to file without it", async () => {
    const file = new TFile("test.md");
    app.kiln.getAbstractFileByPath.mockReturnValue(file);

    const existingContent = `# Test\n\nContent without frontmatter`;

    app.kiln.read.mockResolvedValue(existingContent);
    app.kiln.modify.mockResolvedValue(undefined);

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

    await handler.updateProperties("test.md", req, res);

    const modifiedContent = app.kiln.modify.mock.calls[0][1];
    expect(modifiedContent).toContain("---");
    expect(modifiedContent).toContain("status: draft");
    expect(modifiedContent).toContain("# Test");
  });

  it("should preserve content when updating frontmatter", async () => {
    const file = new TFile("test.md");
    app.kiln.getAbstractFileByPath.mockReturnValue(file);

    const existingContent = `---
title: Original
---

# Heading

Paragraph 1

Paragraph 2`;

    app.kiln.read.mockResolvedValue(existingContent);
    app.kiln.modify.mockResolvedValue(undefined);

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

    await handler.updateProperties("test.md", req, res);

    const modifiedContent = app.kiln.modify.mock.calls[0][1];
    expect(modifiedContent).toContain("# Heading");
    expect(modifiedContent).toContain("Paragraph 1");
    expect(modifiedContent).toContain("Paragraph 2");
  });

  it("should handle complex property values", async () => {
    const file = new TFile("test.md");
    app.kiln.getAbstractFileByPath.mockReturnValue(file);

    const existingContent = `---
title: Test
---

Content`;

    app.kiln.read.mockResolvedValue(existingContent);
    app.kiln.modify.mockResolvedValue(undefined);

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

    await handler.updateProperties("test.md", req, res);

    const modifiedContent = app.kiln.modify.mock.calls[0][1];
    expect(modifiedContent).toContain("tags:");
    expect(modifiedContent).toContain("metadata:");
    expect(modifiedContent).toContain("count: 42");
  });

  it("should handle invalid YAML gracefully", async () => {
    const file = new TFile("test.md");
    app.kiln.getAbstractFileByPath.mockReturnValue(file);

    const existingContent = `---
invalid: : : yaml
---

Content`;

    app.kiln.read.mockResolvedValue(existingContent);
    app.kiln.modify.mockResolvedValue(undefined);

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

    // Invalid YAML is handled by replacing frontmatter entirely
    await handler.updateProperties("test.md", req, res);

    expect(app.kiln.modify).toHaveBeenCalled();
    const modifiedContent = app.kiln.modify.mock.calls[0][1];
    expect(modifiedContent).toContain("status: active");
  });

  it("should handle missing file", async () => {
    app.kiln.getAbstractFileByPath.mockReturnValue(null);

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

    await handler.updateProperties("missing.md", req, res);

    expect(res.writeHead).toHaveBeenCalledWith(404, expect.anything());
  });
});
