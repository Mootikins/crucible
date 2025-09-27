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

See [docs/](./docs) for detailed documentation.

## License

MIT OR Apache-2.0

