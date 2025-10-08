import { spawn, ChildProcess } from "child_process";
import { EventEmitter } from "events";
import {
  JsonRpcRequest,
  JsonRpcResponse,
  JsonRpcNotification,
  InitializeRequest,
  InitializeResponse,
  ListToolsResponse,
  CallToolRequest,
  CallToolResponse,
  McpTool,
  McpClientConfig,
  McpClientEvents,
} from "./types";

export class McpClient extends EventEmitter {
  private config: Required<McpClientConfig>;
  private process: ChildProcess | null = null;
  private initialized = false;
  private requestId = 0;
  private pendingRequests: Map<number | string, any> = new Map();
  private buffer = "";
  private serverInfo: InitializeResponse | null = null;

  constructor(config: McpClientConfig) {
    super();
    this.config = {
      requestTimeout: 30000,
      debug: false,
      serverArgs: [],
      ...config,
    };
  }

  async start(): Promise<InitializeResponse> {
    if (this.process) {
      throw new Error("MCP server already started");
    }
    this.process = spawn(this.config.serverPath, this.config.serverArgs, {
      stdio: ["pipe", "pipe", "pipe"],
    });
    this.setupProcessHandlers();
    const initResponse = await this.initialize();
    this.initialized = true;
    this.serverInfo = initResponse;
    this.emit("initialized", initResponse);
    return initResponse;
  }

  async stop(): Promise<void> {
    if (this.process) {
      this.process.kill();
      this.process = null;
    }
    this.initialized = false;
    this.emit("stopped");
  }

  isReady(): boolean {
    return this.initialized && this.process !== null;
  }

  getServerInfo(): InitializeResponse | null {
    return this.serverInfo;
  }

  async listTools(): Promise<McpTool[]> {
    const response = await this.sendRequest<ListToolsResponse>("tools/list");
    return response.tools;
  }

  async callTool(name: string, args: any): Promise<CallToolResponse> {
    return await this.sendRequest<CallToolResponse>("tools/call", { name, arguments: args });
  }

  private async initialize(): Promise<InitializeResponse> {
    const params: InitializeRequest = {
      protocol_version: "2024-11-05",
      capabilities: {},
      client_info: {
        name: this.config.clientName,
        version: this.config.clientVersion,
      },
    };
    const response = await this.sendRequest<InitializeResponse>("initialize", params);

    if (this.config.debug) {
      console.log("[McpClient] Initialize response received, sending initialized notification");
    }

    // Send initialized notification and wait for server ready confirmation
    await this.sendNotification("initialized");

    if (this.config.debug) {
      console.log("[McpClient] Initialized notification sent, waiting for ready...");
    }

    await this.waitForReady();

    if (this.config.debug) {
      console.log("[McpClient] Ready notification received!");
    }

    return response;
  }

  private async waitForReady(): Promise<void> {
    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        this.removeListener("ready", readyHandler);
        reject(new Error("Timeout waiting for server ready notification"));
      }, 5000);

      const readyHandler = () => {
        clearTimeout(timeout);
        resolve();
      };

      this.once("ready", readyHandler);
    });
  }

  private async sendRequest<T>(method: string, params?: any): Promise<T> {
    const id = ++this.requestId;
    const request: JsonRpcRequest = { jsonrpc: "2.0", id, method, params };
    return new Promise<T>((resolve, reject) => {
      const timeout = setTimeout(() => {
        this.pendingRequests.delete(id);
        reject(new Error(`Request timeout: ${method}`));
      }, this.config.requestTimeout);
      this.pendingRequests.set(id, { resolve, reject, timeout });
      const message = JSON.stringify(request) + "\n";
      if (this.config.debug) console.log("[McpClient] Sending:", message.trim());
      this.process!.stdin!.write(message);
    });
  }

  private async sendNotification(method: string, params?: any): Promise<void> {
    const notification: JsonRpcNotification = { jsonrpc: "2.0", method, params };
    const message = JSON.stringify(notification) + "\n";
    if (this.config.debug) console.log("[McpClient] Sending notification:", message.trim());
    this.process!.stdin!.write(message);
  }

  private handleResponse(response: JsonRpcResponse): void {
    const pending = this.pendingRequests.get(response.id!);
    if (!pending) return;
    clearTimeout(pending.timeout);
    this.pendingRequests.delete(response.id!);
    if (response.error) {
      pending.reject(new Error(`JSON-RPC error: ${response.error.message}`));
    } else {
      pending.resolve(response.result);
    }
  }

  private handleNotification(notification: JsonRpcNotification): void {
    if (this.config.debug) {
      console.log("[McpClient] Received notification:", notification.method);
    }

    // Emit notification as event
    if (notification.method === "notifications/ready") {
      this.emit("ready");
    } else {
      this.emit("notification", notification);
    }
  }

  private handleStdout(data: Buffer): void {
    this.buffer += data.toString();
    let idx: number;
    while ((idx = this.buffer.indexOf("\n")) !== -1) {
      const line = this.buffer.slice(0, idx).trim();
      this.buffer = this.buffer.slice(idx + 1);
      if (line) {
        try {
          const parsed = JSON.parse(line);
          if ("id" in parsed) {
            this.handleResponse(parsed);
          } else if ("method" in parsed) {
            // Handle server notifications
            this.handleNotification(parsed);
          }
        } catch (e) {
          if (this.config.debug) console.log("[McpClient] Parse error:", e);
        }
      }
    }
  }

  private setupProcessHandlers(): void {
    this.process!.stdout!.on("data", (d: Buffer) => this.handleStdout(d));
    this.process!.stderr!.on("data", (d: Buffer) => {
      if (this.config.debug) console.log("[McpClient] stderr:", d.toString());
    });
    this.process!.on("exit", (code, signal) => {
      this.emit("exit", code, signal);
      Array.from(this.pendingRequests.values()).forEach((p) => {
        clearTimeout(p.timeout);
        p.reject(new Error("Process exited"));
      });
      this.pendingRequests.clear();
    });
    this.process!.on("error", (err) => this.emit("error", err));
    this.emit("started");
  }
}
