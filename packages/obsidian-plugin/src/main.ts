import { Plugin, Notice } from "obsidian";
import { createServer, IncomingMessage, ServerResponse } from "http";
import { DEFAULT_PORT, DEFAULT_SETTINGS, EmbeddingSettings } from "./api-spec";
import { FilesHandler } from "./api/files";
import { MetadataHandler } from "./api/metadata";
import { PropertiesHandler } from "./api/properties";
import { SettingsTab } from "./settings";

interface MCPPluginSettings {
  port: number;
  embeddings: EmbeddingSettings;
}

const DEFAULT_PLUGIN_SETTINGS: MCPPluginSettings = {
  port: DEFAULT_PORT,
  embeddings: DEFAULT_SETTINGS,
};

export default class MCPPlugin extends Plugin {
  settings: MCPPluginSettings;
  server: any;
  filesHandler: FilesHandler;
  metadataHandler: MetadataHandler;
  propertiesHandler: PropertiesHandler;

  async onload() {
    await this.loadSettings();

    this.filesHandler = new FilesHandler(this.app);
    this.metadataHandler = new MetadataHandler(this.app);
    this.propertiesHandler = new PropertiesHandler(this.app);

    // Start HTTP server
    this.startServer();

    // Add settings tab
    this.addSettingTab(new SettingsTab(this.app, this));

    new Notice(`MCP Plugin: Server started on port ${this.settings.port}`);
  }

  onunload() {
    if (this.server) {
      this.server.close();
      new Notice("MCP Plugin: Server stopped");
    }
  }

  async loadSettings() {
    this.settings = Object.assign({}, DEFAULT_PLUGIN_SETTINGS, await this.loadData());
  }

  async saveSettings() {
    await this.saveData(this.settings);
  }

  startServer() {
    this.server = createServer(async (req: IncomingMessage, res: ServerResponse) => {
      // CORS headers
      res.setHeader("Access-Control-Allow-Origin", "*");
      res.setHeader("Access-Control-Allow-Methods", "GET, POST, PUT, DELETE, OPTIONS");
      res.setHeader("Access-Control-Allow-Headers", "Content-Type");

      if (req.method === "OPTIONS") {
        res.writeHead(200);
        res.end();
        return;
      }

      try {
        await this.handleRequest(req, res);
      } catch (error) {
        console.error("MCP Plugin Error:", error);
        res.writeHead(500, { "Content-Type": "application/json" });
        res.end(JSON.stringify({ error: error.message }));
      }
    });

    this.server.listen(this.settings.port, "127.0.0.1", () => {
      console.log(`MCP Plugin HTTP server listening on port ${this.settings.port}`);
    });
  }

  async handleRequest(req: IncomingMessage, res: ServerResponse) {
    const url = new URL(req.url || "", `http://localhost:${this.settings.port}`);
    const path = url.pathname;
    const method = req.method || "GET";

    // Route requests
    if (path === "/api/files" && method === "GET") {
      await this.filesHandler.listFiles(req, res);
    } else if (path.startsWith("/api/file/")) {
      const filePath = decodeURIComponent(path.replace("/api/file/", ""));

      if (path.endsWith("/metadata") && method === "GET") {
        await this.metadataHandler.getMetadata(filePath, req, res);
      } else if (path.endsWith("/properties") && method === "PUT") {
        await this.propertiesHandler.updateProperties(filePath, req, res);
      } else if (method === "GET") {
        await this.filesHandler.getFile(filePath, req, res);
      } else {
        this.sendNotFound(res);
      }
    } else if (path === "/api/search/tags" && method === "GET") {
      // TODO: Implement tag search
      pass;
    } else if (path === "/api/search/folder" && method === "GET") {
      // TODO: Implement folder search
      pass;
    } else if (path === "/api/search/properties" && method === "GET") {
      // TODO: Implement property search
      pass;
    } else if (path === "/api/search/content" && method === "GET") {
      // TODO: Implement content search
      pass;
    } else if (path === "/api/settings/embeddings" && method === "GET") {
      this.sendJSON(res, 200, this.settings.embeddings);
    } else if (path === "/api/settings/embeddings" && method === "PUT") {
      const body = await this.readBody(req);
      this.settings.embeddings = JSON.parse(body);
      await this.saveSettings();
      this.sendJSON(res, 200, { success: true });
    } else if (path === "/api/settings/embeddings/models" && method === "GET") {
      // TODO: Implement model listing
      pass;
    } else {
      this.sendNotFound(res);
    }
  }

  sendJSON(res: ServerResponse, status: number, data: any) {
    res.writeHead(status, { "Content-Type": "application/json" });
    res.end(JSON.stringify(data));
  }

  sendNotFound(res: ServerResponse) {
    res.writeHead(404, { "Content-Type": "application/json" });
    res.end(JSON.stringify({ error: "Not found" }));
  }

  async readBody(req: IncomingMessage): Promise<string> {
    return new Promise((resolve, reject) => {
      let body = "";
      req.on("data", (chunk) => (body += chunk));
      req.on("end", () => resolve(body));
      req.on("error", reject);
    });
  }
}

// TypeScript doesn't have 'pass', use a no-op
function pass() {}
