/**
 * API Specification for Obsidian Plugin HTTP Server
 *
 * The plugin runs an HTTP server on localhost:27123 that provides
 * REST API access to vault operations for the MCP server.
 */

export interface APISpec {
  baseUrl: string;
  port: number;
  endpoints: {
    // File operations
    "GET /api/files": {
      description: "List all markdown files in the vault";
      response: { files: FileInfo[] };
    };
    "GET /api/file/:path": {
      description: "Get file content by path";
      params: { path: string };
      response: { content: string; path: string };
    };
    "GET /api/file/:path/metadata": {
      description: "Get file metadata (properties, tags, folder, stats)";
      params: { path: string };
      response: FileMetadata;
    };
    "PUT /api/file/:path/properties": {
      description: "Update frontmatter properties";
      params: { path: string };
      body: { properties: Record<string, any> };
      response: { success: boolean };
    };

    // Search operations
    "GET /api/search/tags": {
      description: "Search files by tags";
      query: { tags: string[] };
      response: { files: FileInfo[] };
    };
    "GET /api/search/folder": {
      description: "Search files in folder";
      query: { path: string; recursive?: boolean };
      response: { files: FileInfo[] };
    };
    "GET /api/search/properties": {
      description: "Search files by frontmatter properties";
      query: { properties: Record<string, any> };
      response: { files: FileInfo[] };
    };
    "GET /api/search/content": {
      description: "Full-text search in file contents";
      query: { query: string };
      response: { files: FileInfo[] };
    };

    // Settings operations
    "GET /api/settings/embeddings": {
      description: "Get embedding provider configuration";
      response: EmbeddingSettings;
    };
    "PUT /api/settings/embeddings": {
      description: "Update embedding provider configuration";
      body: EmbeddingSettings;
      response: { success: boolean };
    };
    "GET /api/settings/embeddings/models": {
      description: "List available models from embedding provider";
      response: { models: string[] };
    };
  };
}

export interface FileInfo {
  path: string;
  name: string;
  folder: string;
  extension: string;
  size: number;
  created: number;
  modified: number;
}

export interface FileMetadata {
  path: string;
  properties: Record<string, any>; // Frontmatter properties
  tags: string[];
  folder: string;
  links: string[];
  backlinks: string[];
  stats: {
    size: number;
    created: number;
    modified: number;
    wordCount: number;
  };
}

export interface EmbeddingSettings {
  provider: "openai" | "ollama";
  apiUrl: string;
  apiKey?: string; // For OpenAI
  model: string;
}

export const DEFAULT_PORT = 27123;
export const DEFAULT_SETTINGS: EmbeddingSettings = {
  provider: "ollama",
  apiUrl: "http://localhost:11434",
  model: "nomic-embed-text",
};
