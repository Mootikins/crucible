import { App, TFile, getAllTags, parseFrontMatterTags } from "obsidian";
import { IncomingMessage, ServerResponse } from "http";
import { FileMetadata } from "../api-spec";

export class MetadataHandler {
  constructor(private app: App) {}

  async getMetadata(filePath: string, req: IncomingMessage, res: ServerResponse) {
    // TODO: Implement metadata retrieval
    pass;
  }

  private sendJSON(res: ServerResponse, status: number, data: any) {
    res.writeHead(status, { "Content-Type": "application/json" });
    res.end(JSON.stringify(data));
  }
}

function pass() {}
