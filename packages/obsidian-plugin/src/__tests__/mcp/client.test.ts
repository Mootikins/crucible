/**
 * MCP Client tests
 */

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { EventEmitter } from "events";
import { McpClient } from "../../mcp/client";
import type { InitializeResponse } from "../../mcp/types";

// Mock child_process
vi.mock("child_process", () => {
  return {
    spawn: vi.fn(),
  };
});

describe("McpClient", () => {
  let client: McpClient;
  let mockProcess: any;

  beforeEach(() => {
    mockProcess = new EventEmitter();
    mockProcess.stdin = {
      write: vi.fn((data: string, callback?: (error?: Error) => void) => {
        if (callback) callback();
        return true;
      }),
    };
    mockProcess.stdout = new EventEmitter();
    mockProcess.stderr = new EventEmitter();
    mockProcess.kill = vi.fn();
    mockProcess.killed = false;

    const { spawn } = require("child_process");
    spawn.mockReturnValue(mockProcess);

    client = new McpClient({
      serverPath: "/path/to/mcp-server",
      serverArgs: ["--db-path", "/path/to/db"],
      clientName: "test-client",
      clientVersion: "1.0.0",
      debug: false,
    });
  });

  afterEach(async () => {
    await client.stop();
    vi.clearAllMocks();
  });

  describe("start", () => {
    it("should spawn MCP server process", async () => {
      const { spawn } = require("child_process");

      setTimeout(() => {
        const response = {
          jsonrpc: "2.0",
          id: 1,
          result: {
            protocolVersion: "2024-11-05",
            capabilities: { tools: { listChanged: false } },
            serverInfo: { name: "crucible-mcp", version: "0.1.0" },
          },
        };
        mockProcess.stdout.emit("data", Buffer.from(JSON.stringify(response) + "
"));
      }, 10);

      await client.start();

      expect(spawn).toHaveBeenCalledWith("/path/to/mcp-server", ["--db-path", "/path/to/db"], {
        stdio: ["pipe", "pipe", "pipe"],
      });
    });

    it("should initialize with correct protocol version", async () => {
      setTimeout(() => {
        const response = {
          jsonrpc: "2.0",
          id: 1,
          result: {
            protocolVersion: "2024-11-05",
            capabilities: { tools: { listChanged: false } },
            serverInfo: { name: "crucible-mcp", version: "0.1.0" },
          },
        };
        mockProcess.stdout.emit("data", Buffer.from(JSON.stringify(response) + "
"));
      }, 10);

      const initResponse = await client.start();

      expect(initResponse.protocolVersion).toBe("2024-11-05");
      expect(initResponse.serverInfo.name).toBe("crucible-mcp");
    });

    it("should throw error if already started", async () => {
      setTimeout(() => {
        const response = {
          jsonrpc: "2.0",
          id: 1,
          result: {
            protocolVersion: "2024-11-05",
            capabilities: { tools: { listChanged: false } },
            serverInfo: { name: "crucible-mcp", version: "0.1.0" },
          },
        };
        mockProcess.stdout.emit("data", Buffer.from(JSON.stringify(response) + "
"));
      }, 10);

      await client.start();

      await expect(client.start()).rejects.toThrow("MCP server already started");
    });
  });

  describe("lifecycle", () => {
    it("should emit started event", async () => {
      const startedPromise = new Promise((resolve) => {
        client.on("started", resolve);
      });

      setTimeout(() => {
        const response = {
          jsonrpc: "2.0",
          id: 1,
          result: {
            protocolVersion: "2024-11-05",
            capabilities: { tools: { listChanged: false } },
            serverInfo: { name: "crucible-mcp", version: "0.1.0" },
          },
        };
        mockProcess.stdout.emit("data", Buffer.from(JSON.stringify(response) + "
"));
      }, 10);

      await Promise.all([client.start(), startedPromise]);
    });

    it("should emit stopped event on stop", async () => {
      setTimeout(() => {
        const response = {
          jsonrpc: "2.0",
          id: 1,
          result: {
            protocolVersion: "2024-11-05",
            capabilities: { tools: { listChanged: false } },
            serverInfo: { name: "crucible-mcp", version: "0.1.0" },
          },
        };
        mockProcess.stdout.emit("data", Buffer.from(JSON.stringify(response) + "
"));
      }, 10);

      await client.start();

      const stoppedPromise = new Promise((resolve) => {
        client.on("stopped", resolve);
      });

      await client.stop();
      await stoppedPromise;

      expect(mockProcess.kill).toHaveBeenCalled();
    });
  });
});
