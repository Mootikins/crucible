import { App, TFile, parseFrontMatterTags } from "obsidian";
import { IncomingMessage, ServerResponse } from "http";
import { FileInfo } from "../api-spec";

export class SearchHandler {
  constructor(private app: App) {}

  /**
   * Search for files that have ALL specified tags
   */
  async searchByTags(tags: string[], req: IncomingMessage, res: ServerResponse) {
    try {
      if (!Array.isArray(tags) || tags.length === 0) {
        this.sendJSON(res, 400, {
          error: "Invalid tags parameter",
          message: "tags must be a non-empty array",
        });
        return;
      }

      const files = this.app.kiln.getMarkdownFiles();
      const matchingFiles: FileInfo[] = [];

      for (const file of files) {
        const cache = this.app.metadataCache.getFileCache(file);
        if (!cache) continue;

        // Collect all tags from the file
        const fileTags: string[] = [];

        // Tags from frontmatter
        if (cache.frontmatter?.tags) {
          const frontmatterTags = parseFrontMatterTags(cache.frontmatter);
          if (frontmatterTags) {
            fileTags.push(...frontmatterTags);
          }
        }

        // Tags from inline content
        if (cache.tags) {
          fileTags.push(...cache.tags.map(t => t.tag));
        }

        // Normalize tags (remove # prefix if present)
        const normalizedFileTags = fileTags.map(tag =>
          tag.startsWith('#') ? tag.slice(1) : tag
        );
        const normalizedSearchTags = tags.map(tag =>
          tag.startsWith('#') ? tag.slice(1) : tag
        );

        // Check if file has ALL specified tags
        const hasAllTags = normalizedSearchTags.every(searchTag =>
          normalizedFileTags.includes(searchTag)
        );

        if (hasAllTags) {
          matchingFiles.push(this.fileToInfo(file));
        }
      }

      this.sendJSON(res, 200, { files: matchingFiles });
    } catch (error) {
      this.sendJSON(res, 500, {
        error: "Failed to search by tags",
        message: error instanceof Error ? error.message : String(error),
      });
    }
  }

  /**
   * Search for files in a specific folder
   */
  async searchByFolder(
    folderPath: string,
    recursive: boolean,
    req: IncomingMessage,
    res: ServerResponse
  ) {
    try {
      if (typeof folderPath !== 'string') {
        this.sendJSON(res, 400, {
          error: "Invalid folderPath parameter",
          message: "folderPath must be a string",
        });
        return;
      }

      const files = this.app.kiln.getMarkdownFiles();
      const matchingFiles: FileInfo[] = [];

      // Normalize folder path (remove leading/trailing slashes)
      const normalizedFolder = folderPath.replace(/^\/+|\/+$/g, '');

      for (const file of files) {
        const fileFolder = file.parent?.path || "";

        if (recursive) {
          // Check if file is in folder or any subfolder
          if (normalizedFolder === "" || fileFolder === normalizedFolder || fileFolder.startsWith(normalizedFolder + "/")) {
            matchingFiles.push(this.fileToInfo(file));
          }
        } else {
          // Check if file is directly in the folder (not in subfolders)
          if (fileFolder === normalizedFolder) {
            matchingFiles.push(this.fileToInfo(file));
          }
        }
      }

      this.sendJSON(res, 200, { files: matchingFiles });
    } catch (error) {
      this.sendJSON(res, 500, {
        error: "Failed to search by folder",
        message: error instanceof Error ? error.message : String(error),
      });
    }
  }

  /**
   * Search for files matching frontmatter properties
   */
  async searchByProperties(
    properties: Record<string, any>,
    req: IncomingMessage,
    res: ServerResponse
  ) {
    try {
      if (typeof properties !== 'object' || properties === null || Array.isArray(properties)) {
        this.sendJSON(res, 400, {
          error: "Invalid properties parameter",
          message: "properties must be a non-null object",
        });
        return;
      }

      if (Object.keys(properties).length === 0) {
        this.sendJSON(res, 400, {
          error: "Invalid properties parameter",
          message: "properties must have at least one key",
        });
        return;
      }

      const files = this.app.kiln.getMarkdownFiles();
      const matchingFiles: FileInfo[] = [];

      for (const file of files) {
        const cache = this.app.metadataCache.getFileCache(file);
        if (!cache?.frontmatter) continue;

        const frontmatter = cache.frontmatter;

        // Check if all specified properties match
        const allPropertiesMatch = Object.entries(properties).every(([key, value]) => {
          if (!(key in frontmatter)) return false;

          const fileValue = frontmatter[key];

          // Deep equality check for arrays and objects
          if (Array.isArray(value) && Array.isArray(fileValue)) {
            return JSON.stringify(value) === JSON.stringify(fileValue);
          }

          if (typeof value === 'object' && value !== null &&
              typeof fileValue === 'object' && fileValue !== null) {
            return JSON.stringify(value) === JSON.stringify(fileValue);
          }

          // Simple equality for primitives
          return fileValue === value;
        });

        if (allPropertiesMatch) {
          matchingFiles.push(this.fileToInfo(file));
        }
      }

      this.sendJSON(res, 200, { files: matchingFiles });
    } catch (error) {
      this.sendJSON(res, 500, {
        error: "Failed to search by properties",
        message: error instanceof Error ? error.message : String(error),
      });
    }
  }

  /**
   * Full-text search in file content (case-insensitive)
   */
  async searchByContent(
    query: string,
    req: IncomingMessage,
    res: ServerResponse
  ) {
    try {
      if (typeof query !== 'string' || query.trim().length === 0) {
        this.sendJSON(res, 400, {
          error: "Invalid query parameter",
          message: "query must be a non-empty string",
        });
        return;
      }

      const files = this.app.kiln.getMarkdownFiles();
      const matchingFiles: FileInfo[] = [];
      const lowerQuery = query.toLowerCase();

      for (const file of files) {
        try {
          const content = await this.app.kiln.read(file);
          const lowerContent = content.toLowerCase();

          if (lowerContent.includes(lowerQuery)) {
            matchingFiles.push(this.fileToInfo(file));
          }
        } catch (readError) {
          // Skip files that can't be read
          console.error(`Failed to read file ${file.path}:`, readError);
        }
      }

      this.sendJSON(res, 200, { files: matchingFiles });
    } catch (error) {
      this.sendJSON(res, 500, {
        error: "Failed to search by content",
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
