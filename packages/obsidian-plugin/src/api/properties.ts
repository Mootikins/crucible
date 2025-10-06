import { App, TFile } from "obsidian";
import { IncomingMessage, ServerResponse } from "http";

export class PropertiesHandler {
  constructor(private app: App) {}

  async updateProperties(filePath: string, req: IncomingMessage, res: ServerResponse) {
    // TODO: Implement frontmatter property updates
    pass;
  }

  private async readBody(req: IncomingMessage): Promise<string> {
    return new Promise((resolve, reject) => {
      let body = "";
      req.on("data", (chunk) => (body += chunk));
      req.on("end", () => resolve(body));
      req.on("error", reject);
    });
  }

  private sendJSON(res: ServerResponse, status: number, data: any) {
    res.writeHead(status, { "Content-Type": "application/json" });
    res.end(JSON.stringify(data));
  }
}

function pass() {}
