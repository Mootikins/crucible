/**
 * Tests for MetadataHandler.
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { MetadataHandler } from "../../api/metadata";
import { App, TFile } from "obsidian";
import { IncomingMessage, ServerResponse } from "http";

describe("MetadataHandler", () => {
  let app: App;
  let handler: MetadataHandler;

  beforeEach(() => {
    app = new App();
    handler = new MetadataHandler(app);
  });

  it("should initialize with app", () => {
    expect(handler).toBeDefined();
  });

  it("should extract frontmatter properties", async () => {
    const file = new TFile("test.md");
    app.vault.getAbstractFileByPath.mockReturnValue(file);
    app.metadataCache.getFileCache.mockReturnValue({
      frontmatter: {
        status: "active",
        priority: "high",
        tags: ["project", "ai"],
      },
    });
    app.metadataCache.getBacklinksForFile = vi.fn(() => new Map());

    app.vault.read.mockResolvedValue("# Test\n\nContent");

    const req = {} as IncomingMessage;
    const res = {
      writeHead: vi.fn(),
      end: vi.fn(),
    } as unknown as ServerResponse;

    await handler.getMetadata("test.md", req, res);

    const response = JSON.parse(res.end.mock.calls[0][0]);
    expect(response.properties.status).toBe("active");
    expect(response.properties.priority).toBe("high");
  });

  it("should extract tags from frontmatter and content", async () => {
    const file = new TFile("test.md");
    app.vault.getAbstractFileByPath.mockReturnValue(file);
    app.metadataCache.getFileCache.mockReturnValue({
      frontmatter: { tags: ["yaml-tag"] },
      tags: [{ tag: "#inline-tag" }],
    });
    app.metadataCache.getBacklinksForFile = vi.fn(() => new Map());

    app.vault.read.mockResolvedValue("# Test\n\nContent");

    const req = {} as IncomingMessage;
    const res = {
      writeHead: vi.fn(),
      end: vi.fn(),
    } as unknown as ServerResponse;

    await handler.getMetadata("test.md", req, res);

    const response = JSON.parse(res.end.mock.calls[0][0]);
    expect(response.tags).toContain("yaml-tag");
    expect(response.tags).toContain("#inline-tag");
  });

  it("should handle files without frontmatter", async () => {
    const file = new TFile("simple.md");
    app.vault.getAbstractFileByPath.mockReturnValue(file);
    app.metadataCache.getFileCache.mockReturnValue({
      // No frontmatter
    });
    app.metadataCache.getBacklinksForFile = vi.fn(() => new Map());

    app.vault.read.mockResolvedValue("# Simple\n\nNo frontmatter");

    const req = {} as IncomingMessage;
    const res = {
      writeHead: vi.fn(),
      end: vi.fn(),
    } as unknown as ServerResponse;

    await handler.getMetadata("simple.md", req, res);

    const response = JSON.parse(res.end.mock.calls[0][0]);
    expect(response.properties).toEqual({});
    expect(response.tags).toEqual([]);
  });

  it("should extract links", async () => {
    const file = new TFile("test.md");
    app.vault.getAbstractFileByPath.mockReturnValue(file);
    app.metadataCache.getFileCache.mockReturnValue({
      links: [{ link: "other-note.md" }, { link: "another.md" }],
    });
    app.metadataCache.getBacklinksForFile = vi.fn(() => new Map());

    app.vault.read.mockResolvedValue("# Test\n\nContent");

    const req = {} as IncomingMessage;
    const res = {
      writeHead: vi.fn(),
      end: vi.fn(),
    } as unknown as ServerResponse;

    await handler.getMetadata("test.md", req, res);

    const response = JSON.parse(res.end.mock.calls[0][0]);
    expect(response.links).toHaveLength(2);
    expect(response.links).toContain("other-note.md");
  });

  it("should calculate file stats", async () => {
    const file = new TFile("test.md");
    file.stat.size = 2048;
    file.stat.ctime = 1696000000000;
    file.stat.mtime = 1696100000000;

    app.vault.getAbstractFileByPath.mockReturnValue(file);
    app.metadataCache.getFileCache.mockReturnValue({});
    app.metadataCache.getBacklinksForFile = vi.fn(() => new Map());

    app.vault.read.mockResolvedValue("# Test\n\nThis has five words.");

    const req = {} as IncomingMessage;
    const res = {
      writeHead: vi.fn(),
      end: vi.fn(),
    } as unknown as ServerResponse;

    await handler.getMetadata("test.md", req, res);

    const response = JSON.parse(res.end.mock.calls[0][0]);
    expect(response.stats.size).toBe(2048);
    expect(response.stats.wordCount).toBe(5);
  });

  it("should handle missing file", async () => {
    app.vault.getAbstractFileByPath.mockReturnValue(null);

    const req = {} as IncomingMessage;
    const res = {
      writeHead: vi.fn(),
      end: vi.fn(),
    } as unknown as ServerResponse;

    await handler.getMetadata("missing.md", req, res);

    expect(res.writeHead).toHaveBeenCalledWith(404, expect.anything());
  });
});
