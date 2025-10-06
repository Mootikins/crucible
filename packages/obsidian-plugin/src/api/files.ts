import { App, TFile } from "obsidian";
import { IncomingMessage, ServerResponse } from "http";
import { FileInfo } from "../api-spec";

export class FilesHandler {
  constructor(private app: App) {}

  async listFiles(req: IncomingMessage, res: ServerResponse) {
    // TODO: Implement file listing
    pass;
  }

  async getFile(filePath: string, req: IncomingMessage, res: ServerResponse) {
    // TODO: Implement file content retrieval
    pass;
  }

  private fileToInfo(file: TFile): FileInfo {
    return {
      path: file.path,
      name: file.name,
      folder: file.parent?.path || "",
      extension: file.extension,
      size: file.stat.size,
      created: file.stat.ctime,
      modified: file.stat.mtime,
    };
  }

  private sendJSON(res: ServerResponse, status: number, data: any) {
    res.writeHead(status, { "Content-Type": "application/json" });
    res.end(JSON.stringify(data));
  }
}

function pass() {}
