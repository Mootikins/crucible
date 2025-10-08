import { App, TFile, getAllTags, parseFrontMatterTags } from "obsidian";
import { IncomingMessage, ServerResponse } from "http";
import { FileMetadata } from "../api-spec";

export class MetadataHandler {
  constructor(private app: App) {}

  async getMetadata(filePath: string, req: IncomingMessage, res: ServerResponse) {
    try {
      const file = this.app.vault.getAbstractFileByPath(filePath);

      if (!file || !(file instanceof TFile)) {
        this.sendJSON(res, 404, {
          error: "File not found",
          path: filePath,
        });
        return;
      }

      // Get cached metadata
      const cache = this.app.metadataCache.getFileCache(file);

      // Extract frontmatter properties
      const properties = cache?.frontmatter || {};

      // Extract tags from both frontmatter and inline
      const tags: string[] = [];

      // Tags from frontmatter
      if (cache?.frontmatter?.tags) {
        const frontmatterTags = parseFrontMatterTags(cache.frontmatter);
        if (frontmatterTags) {
          tags.push(...frontmatterTags);
        }
      }

      // Tags from inline content
      if (cache?.tags) {
        tags.push(...cache.tags.map(t => t.tag));
      }

      // Extract links
      const links = cache?.links?.map(l => l.link) || [];

      // Get backlinks (if method exists - not in all Obsidian versions)
      const backlinks: string[] = [];
      if ('getBacklinksForFile' in this.app.metadataCache) {
        const backlinksMap = (this.app.metadataCache as any).getBacklinksForFile(file);
        if (backlinksMap) {
          backlinksMap.forEach((_: any, key: any) => {
            if (key && key.path) {
              backlinks.push(key.path);
            }
          });
        }
      }

      // Read content for word count
      const content = await this.app.vault.read(file);
      const wordCount = this.countWords(content);

      const metadata: FileMetadata = {
        path: file.path,
        properties,
        tags,
        folder: file.parent?.path || "",
        links,
        backlinks,
        stats: {
          size: file.stat.size,
          created: file.stat.ctime,
          modified: file.stat.mtime,
          wordCount,
        },
      };

      this.sendJSON(res, 200, metadata);
    } catch (error) {
      this.sendJSON(res, 500, {
        error: "Failed to get metadata",
        message: error instanceof Error ? error.message : String(error),
      });
    }
  }

  private countWords(text: string): number {
    // Remove frontmatter
    const withoutFrontmatter = text.replace(/^---\n[\s\S]*?\n---\n/, '');
    // Remove markdown syntax
    const plainText = withoutFrontmatter
      .replace(/#+\s/g, '') // Headers
      .replace(/\*\*([^*]+)\*\*/g, '$1') // Bold
      .replace(/\*([^*]+)\*/g, '$1') // Italic
      .replace(/\[([^\]]+)\]\([^)]+\)/g, '$1') // Links
      .replace(/`([^`]+)`/g, '$1'); // Code
    // Count words
    return plainText.split(/\s+/).filter(word => word.length > 0).length;
  }

  private sendJSON(res: ServerResponse, status: number, data: any) {
    res.writeHead(status, { "Content-Type": "application/json" });
    res.end(JSON.stringify(data));
  }
}
