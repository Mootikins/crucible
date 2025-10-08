import { Plugin, Notice } from "obsidian";
import { createServer, IncomingMessage, ServerResponse } from "http";
import { DEFAULT_PORT, DEFAULT_SETTINGS, EmbeddingSettings } from "./api-spec";
import { FilesHandler } from "./api/files";
import { MetadataHandler } from "./api/metadata";
import { PropertiesHandler } from "./api/properties";
import { SearchHandler } from "./api/search";
import { SettingsHandler } from "./api/settings";
import { SettingsTab } from "./settings";
import { McpClient } from "./mcp";
import type { InitializeResponse } from "./mcp";

interface MCPPluginSettings {
  port: number;
  embeddings: EmbeddingSettings;
  mcp: {
    enabled: boolean;
    serverPath: string;
    serverArgs: string[];
    debug: boolean;
  };
}

const DEFAULT_PLUGIN_SETTINGS: MCPPluginSettings = {
  port: DEFAULT_PORT,
  embeddings: DEFAULT_SETTINGS,
  mcp: {
    enabled: false,
    serverPath: "",
    serverArgs: [],
    debug: false,
  },
};

export default class MCPPlugin extends Plugin {
  settings: MCPPluginSettings;
  server: any;
  mcpClient: McpClient | null = null;
  filesHandler: FilesHandler;
  metadataHandler: MetadataHandler;
  propertiesHandler: PropertiesHandler;
  searchHandler: SearchHandler;
  settingsHandler: SettingsHandler;

  async onload() {
    await this.loadSettings();

    this.filesHandler = new FilesHandler(this.app);
    this.metadataHandler = new MetadataHandler(this.app);
    this.propertiesHandler = new PropertiesHandler(this.app);
    this.searchHandler = new SearchHandler(this.app);
    this.settingsHandler = new SettingsHandler(this.app);

    // Start HTTP server
    this.startServer();

    // Start MCP client if enabled
    if (this.settings.mcp.enabled && this.settings.mcp.serverPath) {
      await this.startMcpClient();
    }

    // Add settings tab
    this.addSettingTab(new SettingsTab(this.app, this));

    new Notice(`MCP Plugin: Server started on port ${this.settings.port}`);
  }

  async onunload() {
    if (this.server) {
      this.server.close();
      new Notice("MCP Plugin: Server stopped");
    }

    if (this.mcpClient) {
      await this.stopMcpClient();
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
      const tags = url.searchParams.getAll("tags[]");
      await this.searchHandler.searchByTags(tags, req, res);
    } else if (path === "/api/search/folder" && method === "GET") {
      const folderPath = url.searchParams.get("path") || "";
      const recursive = url.searchParams.get("recursive") === "true";
      await this.searchHandler.searchByFolder(folderPath, recursive, req, res);
    } else if (path === "/api/search/properties" && method === "GET") {
      const properties: Record<string, any> = {};
      url.searchParams.forEach((value, key) => {
        if (key.startsWith("properties[")) {
          const propKey = key.slice(11, -1); // Extract key from properties[key]
          properties[propKey] = value;
        }
      });
      await this.searchHandler.searchByProperties(properties, req, res);
    } else if (path === "/api/search/content" && method === "GET") {
      const query = url.searchParams.get("query") || "";
      await this.searchHandler.searchByContent(query, req, res);
    } else if (path === "/api/settings/embeddings" && method === "GET") {
      this.sendJSON(res, 200, this.settings.embeddings);
    } else if (path === "/api/settings/embeddings" && method === "PUT") {
      const body = await this.readBody(req);
      this.settings.embeddings = JSON.parse(body);
      await this.saveSettings();
      this.sendJSON(res, 200, { success: true });
    } else if (path === "/api/settings/embeddings/models" && method === "GET") {
      await this.settingsHandler.listEmbeddingModels(this.settings.embeddings, req, res);
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

  /**
   * Start the MCP client and connect to the Rust MCP server
   */
  async startMcpClient(): Promise<void> {
    if (this.mcpClient) {
      console.warn("MCP client already started");
      return;
    }

    try {
      console.log("Starting MCP client...");

      // Clean up any stuck MCP server processes before starting
      await this.cleanupStuckMcpProcesses();

      this.mcpClient = new McpClient({
        serverPath: this.settings.mcp.serverPath,
        serverArgs: this.settings.mcp.serverArgs,
        clientName: "obsidian-plugin",
        clientVersion: "1.0.0",
        debug: this.settings.mcp.debug,
      });

      // Set up event listeners
      this.mcpClient.on("initialized", (response: InitializeResponse) => {
        console.log("MCP server initialized:", response.server_info);
        new Notice(`MCP: Connected to ${response.server_info.name} v${response.server_info.version}`);
      });

      this.mcpClient.on("error", (error: Error) => {
        console.error("MCP client error:", error);
        new Notice(`MCP Error: ${error.message}`, 5000);
      });

      this.mcpClient.on("exit", (code: number | null, signal: string | null) => {
        console.log("MCP server exited:", { code, signal });
        new Notice("MCP: Server disconnected", 3000);
        this.mcpClient = null;
      });

      // Start the client (now waits for ready notification internally)
      await this.mcpClient.start();

      // List available tools for debugging
      if (this.settings.mcp.debug) {
        const tools = await this.mcpClient.listTools();
        console.log("Available MCP tools:", tools);
      }
    } catch (error) {
      console.error("Failed to start MCP client:", error);
      new Notice(`Failed to start MCP client: ${error.message}`, 5000);
      this.mcpClient = null;
    }
  }

  /**
   * Stop the MCP client
   */
  async stopMcpClient(): Promise<void> {
    if (!this.mcpClient) {
      return;
    }

    try {
      console.log("Stopping MCP client...");
      await this.mcpClient.stop();
      this.mcpClient = null;
    } catch (error) {
      console.error("Failed to stop MCP client:", error);
    }
  }

  /**
   * Restart the MCP client (useful when settings change)
   */
  async restartMcpClient(): Promise<void> {
    await this.stopMcpClient();

    if (this.settings.mcp.enabled && this.settings.mcp.serverPath) {
      await this.startMcpClient();
    }
  }

  /**
   * Clean up any stuck MCP server processes
   * This prevents database lock errors when restarting
   */
  async cleanupStuckMcpProcesses(): Promise<void> {
    try {
      const { exec } = require('child_process');
      const util = require('util');
      const execPromise = util.promisify(exec);

      // Get the server executable name
      const serverPath = this.settings.mcp.serverPath;
      if (!serverPath) return;

      const serverExe = serverPath.split(/[/\\]/).pop() || '';
      if (!serverExe) return;

      console.log(`Checking for stuck ${serverExe} processes...`);

      // Platform-specific process cleanup
      if (process.platform === 'win32') {
        // Windows: tasklist and taskkill
        try {
          const { stdout } = await execPromise(`tasklist | findstr ${serverExe}`);
          if (stdout && stdout.trim()) {
            const lines = stdout.trim().split('\n');
            console.log(`Found ${lines.length} stuck MCP process(es), cleaning up...`);

            // Kill all found processes
            await execPromise(`taskkill /F /IM ${serverExe}`);
            console.log('Stuck processes cleaned up successfully');

            // Wait a bit for processes to fully terminate
            await new Promise(resolve => setTimeout(resolve, 500));
          }
        } catch (err) {
          // No processes found or already cleaned up
          console.log('No stuck processes found');
        }
      } else {
        // Unix-like: pgrep and pkill
        try {
          const { stdout } = await execPromise(`pgrep -f ${serverExe}`);
          if (stdout && stdout.trim()) {
            const pids = stdout.trim().split('\n');
            console.log(`Found ${pids.length} stuck MCP process(es), cleaning up...`);

            // Kill all found processes
            await execPromise(`pkill -9 -f ${serverExe}`);
            console.log('Stuck processes cleaned up successfully');

            // Wait a bit for processes to fully terminate
            await new Promise(resolve => setTimeout(resolve, 500));
          }
        } catch (err) {
          // No processes found or already cleaned up
          console.log('No stuck processes found');
        }
      }
    } catch (error) {
      console.warn('Failed to cleanup stuck processes:', error);
      // Don't throw - this is a best-effort cleanup
    }
  }
}
