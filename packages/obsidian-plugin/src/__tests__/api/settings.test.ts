/**
 * Tests for SettingsHandler.
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { SettingsHandler } from "../../api/settings";
import { App } from "obsidian";
import { IncomingMessage, ServerResponse } from "http";
import { EmbeddingSettings } from "../../api-spec";

describe("SettingsHandler", () => {
  let app: App;
  let handler: SettingsHandler;

  beforeEach(() => {
    app = new App();
    handler = new SettingsHandler(app);
  });

  it("should initialize with app", () => {
    expect(handler).toBeDefined();
  });

  describe("Ollama Models", () => {
    it("should fetch Ollama models successfully", async () => {
      const settings: EmbeddingSettings = {
        provider: "ollama",
        apiUrl: "http://localhost:11434",
        model: "nomic-embed-text",
      };

      // Mock successful Ollama API response
      const mockResponse = {
        models: [
          { name: "nomic-embed-text" },
          { name: "all-minilm" },
          { name: "llama2" }, // Non-embedding model
        ],
      };

      // Mock the httpGet method
      vi.spyOn(handler as any, "httpGet").mockResolvedValue({
        statusCode: 200,
        body: JSON.stringify(mockResponse),
      });

      const req = {} as IncomingMessage;
      const res = {
        writeHead: vi.fn(),
        end: vi.fn(),
      } as unknown as ServerResponse;

      await handler.listEmbeddingModels(settings, req, res);

      expect(res.writeHead).toHaveBeenCalledWith(200, expect.anything());
      const response = JSON.parse((res.end as any).mock.calls[0][0]);
      expect(response.models).toContain("nomic-embed-text");
      expect(response.models).toContain("all-minilm");
      expect(response.models).toContain("llama2");
    });

    it("should handle Ollama API errors", async () => {
      const settings: EmbeddingSettings = {
        provider: "ollama",
        apiUrl: "http://localhost:11434",
        model: "nomic-embed-text",
      };

      // Mock failed Ollama API response
      vi.spyOn(handler as any, "httpGet").mockResolvedValue({
        statusCode: 500,
        body: "Internal Server Error",
      });

      const req = {} as IncomingMessage;
      const res = {
        writeHead: vi.fn(),
        end: vi.fn(),
      } as unknown as ServerResponse;

      await handler.listEmbeddingModels(settings, req, res);

      expect(res.writeHead).toHaveBeenCalledWith(500, expect.anything());
      const response = JSON.parse((res.end as any).mock.calls[0][0]);
      expect(response.error).toBe("Failed to fetch models");
    });

    it("should handle invalid Ollama response format", async () => {
      const settings: EmbeddingSettings = {
        provider: "ollama",
        apiUrl: "http://localhost:11434",
        model: "nomic-embed-text",
      };

      // Mock invalid response format
      vi.spyOn(handler as any, "httpGet").mockResolvedValue({
        statusCode: 200,
        body: JSON.stringify({ invalid: "format" }),
      });

      const req = {} as IncomingMessage;
      const res = {
        writeHead: vi.fn(),
        end: vi.fn(),
      } as unknown as ServerResponse;

      await handler.listEmbeddingModels(settings, req, res);

      expect(res.writeHead).toHaveBeenCalledWith(500, expect.anything());
      const response = JSON.parse((res.end as any).mock.calls[0][0]);
      expect(response.error).toBe("Failed to fetch models");
    });
  });

  describe("OpenAI Models", () => {
    it("should fetch OpenAI models successfully", async () => {
      const settings: EmbeddingSettings = {
        provider: "openai",
        apiUrl: "https://api.openai.com",
        apiKey: "sk-test123",
        model: "text-embedding-3-small",
      };

      // Mock successful OpenAI API response
      const mockResponse = {
        data: [
          { id: "text-embedding-3-small" },
          { id: "text-embedding-3-large" },
          { id: "text-embedding-ada-002" },
          { id: "gpt-4" }, // Non-embedding model
        ],
      };

      // Mock the httpGet method
      vi.spyOn(handler as any, "httpGet").mockResolvedValue({
        statusCode: 200,
        body: JSON.stringify(mockResponse),
      });

      const req = {} as IncomingMessage;
      const res = {
        writeHead: vi.fn(),
        end: vi.fn(),
      } as unknown as ServerResponse;

      await handler.listEmbeddingModels(settings, req, res);

      expect(res.writeHead).toHaveBeenCalledWith(200, expect.anything());
      const response = JSON.parse((res.end as any).mock.calls[0][0]);
      expect(response.models).toContain("text-embedding-3-small");
      expect(response.models).toContain("text-embedding-3-large");
      expect(response.models).toContain("text-embedding-ada-002");
      expect(response.models).not.toContain("gpt-4"); // Filtered out
    });

    it("should require API key for OpenAI", async () => {
      const settings: EmbeddingSettings = {
        provider: "openai",
        apiUrl: "https://api.openai.com",
        model: "text-embedding-3-small",
        // No API key provided
      };

      const req = {} as IncomingMessage;
      const res = {
        writeHead: vi.fn(),
        end: vi.fn(),
      } as unknown as ServerResponse;

      await handler.listEmbeddingModels(settings, req, res);

      expect(res.writeHead).toHaveBeenCalledWith(401, expect.anything());
      const response = JSON.parse((res.end as any).mock.calls[0][0]);
      expect(response.error).toBe("API key required");
    });

    it("should handle invalid OpenAI API key", async () => {
      const settings: EmbeddingSettings = {
        provider: "openai",
        apiUrl: "https://api.openai.com",
        apiKey: "invalid-key",
        model: "text-embedding-3-small",
      };

      // Mock 401 response
      vi.spyOn(handler as any, "httpGet").mockResolvedValue({
        statusCode: 401,
        body: "Unauthorized",
      });

      const req = {} as IncomingMessage;
      const res = {
        writeHead: vi.fn(),
        end: vi.fn(),
      } as unknown as ServerResponse;

      await handler.listEmbeddingModels(settings, req, res);

      expect(res.writeHead).toHaveBeenCalledWith(500, expect.anything());
      const response = JSON.parse((res.end as any).mock.calls[0][0]);
      expect(response.message).toBe("Invalid OpenAI API key");
    });

    it("should handle invalid OpenAI response format", async () => {
      const settings: EmbeddingSettings = {
        provider: "openai",
        apiUrl: "https://api.openai.com",
        apiKey: "sk-test123",
        model: "text-embedding-3-small",
      };

      // Mock invalid response format
      vi.spyOn(handler as any, "httpGet").mockResolvedValue({
        statusCode: 200,
        body: JSON.stringify({ invalid: "format" }),
      });

      const req = {} as IncomingMessage;
      const res = {
        writeHead: vi.fn(),
        end: vi.fn(),
      } as unknown as ServerResponse;

      await handler.listEmbeddingModels(settings, req, res);

      expect(res.writeHead).toHaveBeenCalledWith(500, expect.anything());
      const response = JSON.parse((res.end as any).mock.calls[0][0]);
      expect(response.error).toBe("Failed to fetch models");
    });
  });

  describe("Error Handling", () => {
    it("should handle unknown provider", async () => {
      const settings: EmbeddingSettings = {
        provider: "unknown" as any,
        apiUrl: "http://localhost",
        model: "test",
      };

      const req = {} as IncomingMessage;
      const res = {
        writeHead: vi.fn(),
        end: vi.fn(),
      } as unknown as ServerResponse;

      await handler.listEmbeddingModels(settings, req, res);

      expect(res.writeHead).toHaveBeenCalledWith(400, expect.anything());
      const response = JSON.parse((res.end as any).mock.calls[0][0]);
      expect(response.error).toBe("Invalid provider");
    });

    it("should handle network errors", async () => {
      const settings: EmbeddingSettings = {
        provider: "ollama",
        apiUrl: "http://localhost:11434",
        model: "nomic-embed-text",
      };

      // Mock network error
      vi.spyOn(handler as any, "httpGet").mockRejectedValue(
        new Error("Network request failed: connect ECONNREFUSED")
      );

      const req = {} as IncomingMessage;
      const res = {
        writeHead: vi.fn(),
        end: vi.fn(),
      } as unknown as ServerResponse;

      await handler.listEmbeddingModels(settings, req, res);

      expect(res.writeHead).toHaveBeenCalledWith(500, expect.anything());
      const response = JSON.parse((res.end as any).mock.calls[0][0]);
      expect(response.error).toBe("Failed to fetch models");
      expect(response.message).toContain("Network request failed");
    });
  });
});
