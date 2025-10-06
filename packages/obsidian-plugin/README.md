# Obsidian MCP Integration Plugin

Obsidian plugin that provides an HTTP API for MCP (Model Context Protocol) server integration with semantic search capabilities.

## Overview

This plugin enables AI assistants (like Claude) to interact with your Obsidian vault through an MCP server. The plugin runs a local HTTP server that exposes vault operations via a REST API.

## Features

- **Local HTTP API**: Runs on localhost:27123 (configurable)
- **File Operations**: List, read files
- **Metadata Access**: Extract properties, tags, links, backlinks
- **Search**: By tags, folders, properties, content
- **Property Management**: Update frontmatter properties
- **Embedding Configuration**: Settings UI for OpenAI/Ollama integration

## Installation

### Manual Installation

1. Copy the plugin files to your vault's plugins directory:

```bash
mkdir -p /path/to/vault/.obsidian/plugins/obsidian-mcp-plugin
cd packages/obsidian-plugin
npm install
npm run build
cp main.js manifest.json /path/to/vault/.obsidian/plugins/obsidian-mcp-plugin/
```

2. Enable the plugin in Obsidian → Settings → Community Plugins

### Development

```bash
cd packages/obsidian-plugin
npm install
npm run dev  # Watch mode for development
```

## Configuration

### Plugin Settings

Access via Obsidian → Settings → MCP Integration:

#### Server Configuration

- **HTTP Server Port**: Port for the local API server (default: 27123)
  - Requires restart after changing

#### Embedding Configuration

- **Embedding Provider**: Choose between OpenAI or Ollama
- **API URL**:
  - Ollama: `http://localhost:11434`
  - OpenAI: `https://api.openai.com/v1`
- **API Key**: Required for OpenAI
- **Model**: Embedding model name
  - Ollama: `nomic-embed-text`, `mxbai-embed-large`, etc.
  - OpenAI: `text-embedding-3-small`, `text-embedding-3-large`

## API Reference

The plugin exposes the following HTTP endpoints on `localhost:27123`:

### File Operations

#### `GET /api/files`

List all markdown files in the vault.

**Response:**
```json
{
  "files": [
    {
      "path": "Projects/AI/notes.md",
      "name": "notes.md",
      "folder": "Projects/AI",
      "extension": "md",
      "size": 1024,
      "created": 1696000000000,
      "modified": 1696100000000
    }
  ]
}
```

#### `GET /api/file/:path`

Get file content.

**Response:**
```json
{
  "content": "# My Note\n\nContent here...",
  "path": "Projects/AI/notes.md"
}
```

#### `GET /api/file/:path/metadata`

Get file metadata (properties, tags, links, etc.).

**Response:**
```json
{
  "path": "Projects/AI/notes.md",
  "properties": {
    "status": "active",
    "priority": "high"
  },
  "tags": ["project", "ai"],
  "folder": "Projects/AI",
  "links": ["other-note.md"],
  "backlinks": ["index.md"],
  "stats": {
    "size": 1024,
    "created": 1696000000000,
    "modified": 1696100000000,
    "wordCount": 150
  }
}
```

#### `PUT /api/file/:path/properties`

Update frontmatter properties.

**Request:**
```json
{
  "properties": {
    "status": "completed",
    "updated": "2025-10-03"
  }
}
```

**Response:**
```json
{
  "success": true
}
```

### Search Operations

#### `GET /api/search/tags?tags[]=tag1&tags[]=tag2`

Search files by tags.

#### `GET /api/search/folder?path=Projects&recursive=true`

Search files in a folder.

#### `GET /api/search/properties?properties[key]=value`

Search files by frontmatter properties.

#### `GET /api/search/content?query=search+term`

Full-text search in file contents.

### Settings Operations

#### `GET /api/settings/embeddings`

Get embedding provider configuration.

**Response:**
```json
{
  "provider": "ollama",
  "apiUrl": "http://localhost:11434",
  "model": "nomic-embed-text"
}
```

#### `PUT /api/settings/embeddings`

Update embedding provider configuration.

#### `GET /api/settings/embeddings/models`

List available models from the embedding provider.

## Security

- The HTTP server only accepts connections from `localhost` (127.0.0.1)
- No authentication is required since the server is local-only
- Desktop-only plugin (will not work on mobile)

## Architecture

```
Obsidian Plugin
├── HTTP Server (localhost:27123)
│   ├── File endpoints
│   ├── Search endpoints
│   └── Settings endpoints
├── Settings UI
└── Obsidian API Integration
    ├── File I/O
    ├── Metadata parsing
    └── Frontmatter updates
```

## Implementation Status

Core plugin structure is in place. API endpoints are stubbed and need implementation:

- [ ] File listing and reading
- [ ] Metadata extraction (properties, tags, links)
- [ ] Frontmatter property updates
- [ ] Tag-based search
- [ ] Folder-based search
- [ ] Property-based search
- [ ] Content search
- [ ] Model listing from embedding providers

## Development

### Building

```bash
npm run build  # Production build
npm run dev    # Development with watch
```

### Project Structure

```
packages/obsidian-plugin/
├── src/
│   ├── main.ts          # Plugin entry + HTTP server
│   ├── settings.ts      # Settings UI
│   ├── api-spec.ts      # API type definitions
│   └── api/
│       ├── files.ts     # File operations
│       ├── metadata.ts  # Metadata extraction
│       └── properties.ts # Property updates
├── manifest.json
├── package.json
├── tsconfig.json
└── esbuild.config.mjs
```

## Troubleshooting

### Server won't start

- Check if port 27123 is already in use
- Try changing the port in settings
- Check Obsidian developer console (Ctrl+Shift+I) for errors

### MCP server can't connect

- Verify the plugin is enabled
- Check the port matches in both plugin settings and MCP server config
- Restart Obsidian

## License

MIT
