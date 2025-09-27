# 🔥 Crucible

> Where ideas transform

A next-generation knowledge management system that grows with you. Crucible combines the zooming interface of Workflowy, the extensibility of Obsidian, and the power of AI agents to create a living knowledge system.

## Features

- 🔍 **Infinite Zoom**: Navigate your knowledge at any scale
- 🧬 **CRDT-based**: Real-time sync without conflicts
- 🎨 **Canvas Mode**: Spatial organization of ideas
- 🤖 **MCP Integration**: Your knowledge becomes agentic
- 🔌 **Plugin System**: Extend with Rune scripts
- ⚡ **Blazing Fast**: Rust core with GPU acceleration

## Quick Start

```bash
# Clone the repository
git clone https://github.com/matthewkrohn/crucible.git
cd crucible

# Run setup script
./scripts/setup.sh

# Start development
pnpm dev
```

## Tech Stack

- **Core**: Rust + Tauri
- **Frontend**: Svelte 5 + TypeScript
- **Database**: PGlite with pgvector
- **CRDT**: Yrs
- **Scripting**: Rune

## Documentation

- **[Architecture](./ARCHITECTURE.md)** - Complete system architecture and design
- **[Specifications](./specs/)** - Technical specs organized by tech stack and sprint phases
- **[Agent System](./AGENTS.md)** - AI agent integration and tooling
- **[Roadmap](./crucible-roadmap.md)** - Development phases and timeline

### Specification Structure

```
specs/
├── rust-core/         # Core business logic and CRDT operations
├── tauri-backend/     # Desktop application backend
├── svelte-frontend/   # UI components and user experience
├── database/          # Persistence and vector search
├── plugin-system/     # Extensibility and Rune runtime
├── mcp-integration/   # AI agent tools and protocols
├── data-specs/        # Schemas and type definitions
└── sprint-{1,2,3,4}/  # Implementation phases
```

### Sprint Phases

- **Sprint 1**: Foundation (CRDT + Basic UI)
- **Sprint 2**: Persistence & UI Polish  
- **Sprint 3**: Canvas & Properties
- **Sprint 4**: Intelligence Layer

## License

MIT OR Apache-2.0

