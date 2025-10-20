# 🔥 Crucible

> Where ideas transform through linked thinking

A knowledge management system that combines hierarchical organization, real-time collaboration, and AI agent integration to create a platform for organizing and connecting ideas. Crucible promotes **linked thinking** - the seamless connection and evolution of ideas across time and context.

## Features

- 🔍 **Infinite Zoom**: Navigate your knowledge at any scale with smooth transitions
- 🧬 **CRDT-based**: Real-time sync without conflicts, enabling collaborative thinking
- 🎨 **Canvas Mode**: Spatial organization of ideas with visual connections
- 🤖 **AI Agent Integration**: Your knowledge becomes agentic through service architecture and A2A protocols
- 🔌 **Plugin System**: Extend with Rune scripts and custom behaviors
- 🔗 **Visual Programming**: Node-based workflow builder for agent orchestration
- ⚡ **High Performance**: Rust core with GPU acceleration for responsive interactions
- 🧠 **Linked Thinking**: Ideas connect, evolve, and generate new insights automatically

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

### CLI Usage

The CLI provides powerful command-line tools for knowledge management:

```bash
# Build and install CLI
cargo build -p crucible-cli

# Start interactive REPL (default behavior)
cargo run -p crucible-cli

# Show vault statistics
cargo run -p crucible-cli -- stats

# Interactive search
cargo run -p crucible-cli -- search "your query"

# Semantic search
cargo run -p crucible-cli -- semantic "conceptual query"

# Chat with AI agents
cargo run -p crucible-cli -- chat --agent researcher
```

**Default Behavior**: Running `crucible-cli` without arguments starts the interactive REPL with SurrealQL support, making it easy to explore your knowledge base immediately.

## Tech Stack

- **Core**: Rust + Tauri
- **Frontend**: Svelte 5 + TypeScript
- **Database**: DuckDB with vss extension
- **CRDT**: Yrs
- **Scripting**: Rune

## Documentation

- **[Architecture](./docs/ARCHITECTURE.md)** - Complete system architecture and design principles
- **[AI Agent Guide](./AGENTS.md)** - Instructions for AI agents working on the codebase

## License

Copyright (c) 2024 Crucible. All Rights Reserved.

This software is proprietary and may not be used, reproduced, or distributed without permission from Crucible.

