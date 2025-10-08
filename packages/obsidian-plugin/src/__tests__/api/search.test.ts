/**
 * Tests for SearchHandler.
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { SearchHandler } from "../../api/search";
import { App, TFile } from "obsidian";
import { IncomingMessage, ServerResponse } from "http";

describe("SearchHandler", () => {
  let app: App;
  let handler: SearchHandler;

  beforeEach(() => {
    app = new App();
    handler = new SearchHandler(app);
  });

  describe("Tag Search", () => {
    it("should find files with matching tags", async () => {
      const file1 = new TFile("note1.md");
      const file2 = new TFile("note2.md");

      app.vault.getMarkdownFiles.mockReturnValue([file1, file2]);

      app.metadataCache.getFileCache = vi.fn((file) => {
        if (file === file1) {
          return {
            frontmatter: { tags: ["project", "ai"] },
          };
        }
        return { frontmatter: { tags: ["research"] } };
      });

      const req = {} as IncomingMessage;
      const res = {
        writeHead: vi.fn(),
        end: vi.fn(),
      } as unknown as ServerResponse;

      await handler.searchByTags(["project"], req, res);

      const response = JSON.parse(res.end.mock.calls[0][0]);
      expect(response.files).toHaveLength(1);
      expect(response.files[0].path).toBe("note1.md");
    });

    it("should require all tags to match", async () => {
      const file1 = new TFile("note1.md");
      const file2 = new TFile("note2.md");

      app.vault.getMarkdownFiles.mockReturnValue([file1, file2]);

      app.metadataCache.getFileCache = vi.fn((file) => {
        if (file === file1) {
          return {
            frontmatter: { tags: ["project", "ai"] },
          };
        }
        return { frontmatter: { tags: ["project"] } };
      });

      const req = {} as IncomingMessage;
      const res = {
        writeHead: vi.fn(),
        end: vi.fn(),
      } as unknown as ServerResponse;

      await handler.searchByTags(["project", "ai"], req, res);

      const response = JSON.parse(res.end.mock.calls[0][0]);
      expect(response.files).toHaveLength(1);
      expect(response.files[0].path).toBe("note1.md");
    });

    it("should return empty array when no matches", async () => {
      const file1 = new TFile("note1.md");

      app.vault.getMarkdownFiles.mockReturnValue([file1]);
      app.metadataCache.getFileCache.mockReturnValue({
        frontmatter: { tags: ["project"] },
      });

      const req = {} as IncomingMessage;
      const res = {
        writeHead: vi.fn(),
        end: vi.fn(),
      } as unknown as ServerResponse;

      await handler.searchByTags(["nonexistent"], req, res);

      const response = JSON.parse(res.end.mock.calls[0][0]);
      expect(response.files).toHaveLength(0);
    });
  });

  describe("Folder Search", () => {
    it("should find files in folder (non-recursive)", async () => {
      const file1 = new TFile("Projects/note1.md");
      const file2 = new TFile("Projects/AI/note2.md");
      const file3 = new TFile("Archive/note3.md");

      app.vault.getMarkdownFiles.mockReturnValue([file1, file2, file3]);

      const req = {} as IncomingMessage;
      const res = {
        writeHead: vi.fn(),
        end: vi.fn(),
      } as unknown as ServerResponse;

      await handler.searchByFolder("Projects", false, req, res);

      const response = JSON.parse(res.end.mock.calls[0][0]);
      expect(response.files).toHaveLength(1);
      expect(response.files[0].path).toBe("Projects/note1.md");
    });

    it("should find files recursively", async () => {
      const file1 = new TFile("Projects/note1.md");
      const file2 = new TFile("Projects/AI/note2.md");
      const file3 = new TFile("Archive/note3.md");

      app.vault.getMarkdownFiles.mockReturnValue([file1, file2, file3]);

      const req = {} as IncomingMessage;
      const res = {
        writeHead: vi.fn(),
        end: vi.fn(),
      } as unknown as ServerResponse;

      await handler.searchByFolder("Projects", true, req, res);

      const response = JSON.parse(res.end.mock.calls[0][0]);
      expect(response.files).toHaveLength(2);
      expect(response.files.map((f: any) => f.path)).toContain("Projects/note1.md");
      expect(response.files.map((f: any) => f.path)).toContain("Projects/AI/note2.md");
    });

    it("should return empty array for empty folder", async () => {
      const file1 = new TFile("Projects/note1.md");

      app.vault.getMarkdownFiles.mockReturnValue([file1]);

      const req = {} as IncomingMessage;
      const res = {
        writeHead: vi.fn(),
        end: vi.fn(),
      } as unknown as ServerResponse;

      await handler.searchByFolder("Archive", false, req, res);

      const response = JSON.parse(res.end.mock.calls[0][0]);
      expect(response.files).toHaveLength(0);
    });
  });

  describe("Property Search", () => {
    it("should find files with matching properties", async () => {
      const file1 = new TFile("note1.md");
      const file2 = new TFile("note2.md");

      app.vault.getMarkdownFiles.mockReturnValue([file1, file2]);

      app.metadataCache.getFileCache = vi.fn((file) => {
        if (file === file1) {
          return {
            frontmatter: { status: "active", priority: "high" },
          };
        }
        return { frontmatter: { status: "draft" } };
      });

      const req = {} as IncomingMessage;
      const res = {
        writeHead: vi.fn(),
        end: vi.fn(),
      } as unknown as ServerResponse;

      await handler.searchByProperties({ status: "active" }, req, res);

      const response = JSON.parse(res.end.mock.calls[0][0]);
      expect(response.files).toHaveLength(1);
      expect(response.files[0].path).toBe("note1.md");
    });

    it("should match exact property values", async () => {
      const file1 = new TFile("note1.md");
      const file2 = new TFile("note2.md");

      app.vault.getMarkdownFiles.mockReturnValue([file1, file2]);

      app.metadataCache.getFileCache = vi.fn((file) => {
        if (file === file1) {
          return {
            frontmatter: { status: "active", priority: "high" },
          };
        }
        return { frontmatter: { status: "active", priority: "low" } };
      });

      const req = {} as IncomingMessage;
      const res = {
        writeHead: vi.fn(),
        end: vi.fn(),
      } as unknown as ServerResponse;

      await handler.searchByProperties({ status: "active", priority: "high" }, req, res);

      const response = JSON.parse(res.end.mock.calls[0][0]);
      expect(response.files).toHaveLength(1);
      expect(response.files[0].path).toBe("note1.md");
    });

    it("should handle missing properties", async () => {
      const file1 = new TFile("note1.md");

      app.vault.getMarkdownFiles.mockReturnValue([file1]);
      app.metadataCache.getFileCache.mockReturnValue({
        frontmatter: { status: "draft" },
      });

      const req = {} as IncomingMessage;
      const res = {
        writeHead: vi.fn(),
        end: vi.fn(),
      } as unknown as ServerResponse;

      await handler.searchByProperties({ priority: "high" }, req, res);

      const response = JSON.parse(res.end.mock.calls[0][0]);
      expect(response.files).toHaveLength(0);
    });
  });

  describe("Content Search", () => {
    it("should find files containing query text", async () => {
      const file1 = new TFile("note1.md");
      const file2 = new TFile("note2.md");

      app.vault.getMarkdownFiles.mockReturnValue([file1, file2]);
      app.vault.read = vi.fn((file) => {
        if (file === file1) {
          return Promise.resolve("# Research\n\nThis is about AI research.");
        }
        return Promise.resolve("# Notes\n\nGeneral notes here.");
      });

      const req = {} as IncomingMessage;
      const res = {
        writeHead: vi.fn(),
        end: vi.fn(),
      } as unknown as ServerResponse;

      await handler.searchByContent("research", req, res);

      const response = JSON.parse(res.end.mock.calls[0][0]);
      expect(response.files).toHaveLength(1);
      expect(response.files[0].path).toBe("note1.md");
    });

    it("should be case-insensitive", async () => {
      const file1 = new TFile("note1.md");

      app.vault.getMarkdownFiles.mockReturnValue([file1]);
      app.vault.read.mockResolvedValue("# Research\n\nAI RESEARCH project.");

      const req = {} as IncomingMessage;
      const res = {
        writeHead: vi.fn(),
        end: vi.fn(),
      } as unknown as ServerResponse;

      await handler.searchByContent("research", req, res);

      const response = JSON.parse(res.end.mock.calls[0][0]);
      expect(response.files).toHaveLength(1);
      expect(response.files[0].path).toBe("note1.md");
    });

    it("should return empty array when no matches", async () => {
      const file1 = new TFile("note1.md");

      app.vault.getMarkdownFiles.mockReturnValue([file1]);
      app.vault.read.mockResolvedValue("# Notes\n\nGeneral content.");

      const req = {} as IncomingMessage;
      const res = {
        writeHead: vi.fn(),
        end: vi.fn(),
      } as unknown as ServerResponse;

      await handler.searchByContent("nonexistent", req, res);

      const response = JSON.parse(res.end.mock.calls[0][0]);
      expect(response.files).toHaveLength(0);
    });
  });
});
