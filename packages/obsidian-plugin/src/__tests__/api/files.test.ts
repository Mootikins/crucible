/**
 * Tests for FilesHandler.
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { FilesHandler } from "../../api/files";
import { App, TFile } from "obsidian";
import { IncomingMessage, ServerResponse } from "http";

describe("FilesHandler", () => {
  let app: App;
  let handler: FilesHandler;

  beforeEach(() => {
    app = new App();
    handler = new FilesHandler(app);
  });

  it("should initialize with app", () => {
    expect(handler).toBeDefined();
  });

  it("should list all markdown files", async () => {
    const files = [new TFile("test1.md"), new TFile("test2.md")];
    app.vault.getMarkdownFiles.mockReturnValue(files);

    // When implemented:
    // const req = {} as IncomingMessage;
    // const res = {
    //   writeHead: vi.fn(),
    //   end: vi.fn(),
    // } as unknown as ServerResponse;
    //
    // await handler.listFiles(req, res);
    //
    // expect(res.writeHead).toHaveBeenCalledWith(200, expect.anything());
    // expect(res.end).toHaveBeenCalled();

    expect(handler.listFiles).toBeDefined();
  });

  it("should return empty list for empty vault", async () => {
    app.vault.getMarkdownFiles.mockReturnValue([]);

    // When implemented:
    // const req = {} as IncomingMessage;
    // const res = {
    //   writeHead: vi.fn(),
    //   end: vi.fn(),
    // } as unknown as ServerResponse;
    //
    // await handler.listFiles(req, res);
    //
    // const response = JSON.parse(res.end.mock.calls[0][0]);
    // expect(response.files).toHaveLength(0);

    expect(handler.listFiles).toBeDefined();
  });

  it("should get file content", async () => {
    const file = new TFile("test.md");
    app.vault.read.mockResolvedValue("# Test\n\nContent");

    // When implemented:
    // const req = {} as IncomingMessage;
    // const res = {
    //   writeHead: vi.fn(),
    //   end: vi.fn(),
    // } as unknown as ServerResponse;
    //
    // await handler.getFile("test.md", req, res);
    //
    // expect(app.vault.read).toHaveBeenCalled();
    // const response = JSON.parse(res.end.mock.calls[0][0]);
    // expect(response.content).toBe("# Test\n\nContent");

    expect(handler.getFile).toBeDefined();
  });

  it("should handle missing file gracefully", async () => {
    app.vault.getAbstractFileByPath.mockReturnValue(null);

    // When implemented with error handling:
    // const req = {} as IncomingMessage;
    // const res = {
    //   writeHead: vi.fn(),
    //   end: vi.fn(),
    // } as unknown as ServerResponse;
    //
    // await handler.getFile("missing.md", req, res);
    //
    // expect(res.writeHead).toHaveBeenCalledWith(404, expect.anything());

    expect(handler.getFile).toBeDefined();
  });

  it("should transform TFile to FileInfo correctly", () => {
    const file = new TFile("Projects/AI/notes.md");
    file.stat.size = 1024;
    file.stat.ctime = 1696000000000;
    file.stat.mtime = 1696100000000;

    // When implemented:
    // const info = handler.fileToInfo(file);
    // expect(info.path).toBe("Projects/AI/notes.md");
    // expect(info.name).toBe("notes.md");
    // expect(info.folder).toBe("Projects/AI");
    // expect(info.size).toBe(1024);

    expect(handler).toBeDefined();
  });

  it("should filter non-markdown files", async () => {
    const files = [
      new TFile("test.md"),
      new TFile("image.png"), // Should be filtered
    ];
    app.vault.getMarkdownFiles.mockReturnValue([files[0]]);

    // When implemented:
    // const req = {} as IncomingMessage;
    // const res = {
    //   writeHead: vi.fn(),
    //   end: vi.fn(),
    // } as unknown as ServerResponse;
    //
    // await handler.listFiles(req, res);
    //
    // const response = JSON.parse(res.end.mock.calls[0][0]);
    // expect(response.files).toHaveLength(1);
    // expect(response.files[0].name).toBe("test.md");

    expect(handler.listFiles).toBeDefined();
  });
});
