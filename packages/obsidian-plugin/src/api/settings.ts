import { App } from "obsidian";
import { IncomingMessage, ServerResponse } from "http";
import { request as httpRequest } from "http";
import { request as httpsRequest } from "https";
import { EmbeddingSettings } from "../api-spec";

export class SettingsHandler {
  constructor(private app: App) {}

  async listEmbeddingModels(
    embeddings: EmbeddingSettings,
    req: IncomingMessage,
    res: ServerResponse
  ) {
    try {
      let models: string[] = [];

      if (embeddings.provider === "ollama") {
        models = await this.fetchOllamaModels(embeddings.apiUrl);
      } else if (embeddings.provider === "openai") {
        if (!embeddings.apiKey) {
          this.sendJSON(res, 401, {
            error: "API key required",
            message: "OpenAI API key is required to list models",
          });
          return;
        }
        models = await this.fetchOpenAIModels(embeddings.apiUrl, embeddings.apiKey);
      } else {
        this.sendJSON(res, 400, {
          error: "Invalid provider",
          message: `Unknown provider: ${embeddings.provider}`,
        });
        return;
      }

      this.sendJSON(res, 200, { models });
    } catch (error) {
      if (error instanceof Error) {
        // Network or parsing errors
        this.sendJSON(res, 500, {
          error: "Failed to fetch models",
          message: error.message,
        });
      } else {
        this.sendJSON(res, 500, {
          error: "Failed to fetch models",
          message: String(error),
        });
      }
    }
  }

  private async fetchOllamaModels(apiUrl: string): Promise<string[]> {
    const url = new URL("/api/tags", apiUrl);
    const response = await this.httpGet(url.toString());

    if (response.statusCode !== 200) {
      throw new Error(
        `Ollama API returned status ${response.statusCode}: ${response.body}`
      );
    }

    const data = JSON.parse(response.body);

    // Ollama returns { models: [ { name: "...", ... }, ... ] }
    if (!data.models || !Array.isArray(data.models)) {
      throw new Error("Invalid response format from Ollama API");
    }

    return data.models.map((model: any) => model.name).filter(Boolean);
  }

  private async fetchOpenAIModels(apiUrl: string, apiKey: string): Promise<string[]> {
    const url = new URL("/v1/models", apiUrl);
    const response = await this.httpGet(url.toString(), {
      Authorization: `Bearer ${apiKey}`,
    });

    if (response.statusCode === 401) {
      throw new Error("Invalid OpenAI API key");
    }

    if (response.statusCode !== 200) {
      throw new Error(
        `OpenAI API returned status ${response.statusCode}: ${response.body}`
      );
    }

    const data = JSON.parse(response.body);

    // OpenAI returns { data: [ { id: "...", ... }, ... ] }
    if (!data.data || !Array.isArray(data.data)) {
      throw new Error("Invalid response format from OpenAI API");
    }

    // Filter only embedding models
    return data.data
      .map((model: any) => model.id)
      .filter((id: string) => id.includes("embedding"))
      .sort();
  }

  private async httpGet(
    url: string,
    headers: Record<string, string> = {}
  ): Promise<{ statusCode: number; body: string }> {
    return new Promise((resolve, reject) => {
      const urlObj = new URL(url);
      const isHttps = urlObj.protocol === "https:";
      const request = isHttps ? httpsRequest : httpRequest;

      const options = {
        method: "GET",
        headers: {
          "Content-Type": "application/json",
          ...headers,
        },
      };

      const req = request(url, options, (res) => {
        let body = "";

        res.on("data", (chunk) => {
          body += chunk;
        });

        res.on("end", () => {
          resolve({
            statusCode: res.statusCode || 500,
            body,
          });
        });
      });

      req.on("error", (error) => {
        reject(new Error(`Network request failed: ${error.message}`));
      });

      req.on("timeout", () => {
        req.destroy();
        reject(new Error("Request timeout"));
      });

      req.setTimeout(10000); // 10 second timeout
      req.end();
    });
  }

  private sendJSON(res: ServerResponse, status: number, data: any) {
    res.writeHead(status, { "Content-Type": "application/json" });
    res.end(JSON.stringify(data));
  }
}
