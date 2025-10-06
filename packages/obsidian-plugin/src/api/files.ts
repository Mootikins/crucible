import { App, TFile } from "obsidian";
import { IncomingMessage, ServerResponse } from "http";
import { FileInfo } from "../api-spec";

export class FilesHandler {
  constructor(private app: App) {}

  async listFiles(req: IncomingMessage, res: ServerResponse) {
    try {
      const files = this.app.vault.getMarkdownFiles();
      const fileInfos = files.map((file) => this.fileToInfo(file));
      this.sendJSON(res, 200, { files: fileInfos });
    } catch (error) {
      this.sendJSON(res, 500, {
        error: "Failed to list files",
        message: error instanceof Error ? error.message : String(error),
      });
    }
  }

  async getFile(filePath: string, req: IncomingMessage, res: ServerResponse) {
    try {
      const file = this.app.vault.getAbstractFileByPath(filePath);

      if (!file || !(file instanceof TFile)) {
        this.sendJSON(res, 404, {
          error: "File not found",
          path: filePath,
        });
        return;
      }

      const content = await this.app.vault.read(file);
      this.sendJSON(res, 200, {
        content,
        path: filePath,
      });
    } catch (error) {
      this.sendJSON(res, 500, {
        error: "Failed to read file",
        message: error instanceof Error ? error.message : String(error),
      });
    }
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
