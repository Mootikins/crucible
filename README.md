# 🔥 Crucible

> Where ideas transform through linked thinking

A knowledge management system that combines hierarchical organization, real-time collaboration, and AI agent integration to create a platform for organizing and connecting ideas. Crucible promotes **linked thinking** - the seamless connection and evolution of ideas across time and context.

## Features

- 🔍 **Infinite Zoom**: Navigate your knowledge at any scale with smooth transitions
- 🧬 **CRDT-based**: Real-time sync without conflicts, enabling collaborative thinking
- 🎨 **Canvas Mode**: Spatial organization of ideas with visual connections
- 🤖 **AI Agent Integration**: Your knowledge becomes agentic through MCP and A2A protocols
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

## Tech Stack

- **Core**: Rust + Tauri
- **Frontend**: Svelte 5 + TypeScript
- **Database**: DuckDB with vss extension
- **CRDT**: Yrs
- **Scripting**: Rune

## Documentation

- **[Architecture](./ARCHITECTURE.md)** - Complete system architecture and design principles
- **[Specifications](./specs/)** - Technical specs organized by tech stack and sprint phases
- **[Agent System](./AGENTS.md)** - AI agent integration, code generation, and tooling
- **[Roadmap](./crucible-roadmap.md)** - Development phases and timeline
- **[Gap Analysis](./specs/GAP_ANALYSIS_COMPREHENSIVE.md)** - Comprehensive analysis of implementation gaps and context engineering needs

### Specification Structure

```
specs/
├── rust-core/         # Core business logic and CRDT operations
├── tauri-backend/     # Desktop application backend
├── svelte-frontend/   # UI components and user experience
├── database/          # Persistence and vector search
├── plugin-system/     # Extensibility and Rune runtime
├── mcp-integration/   # AI agent tools and protocols
├── code-generation/   # Agent code generation specifications
├── data-specs/        # Schemas and type definitions
└── sprint-{1,2,3,4}/  # Implementation phases
```

### Sprint Phases

- **Sprint 1**: Foundation (CRDT + Basic UI) - *[See detailed specs](./specs/sprint-1/)*
- **Sprint 2**: Persistence & UI Polish - *[See detailed specs](./specs/sprint-2/)*
- **Sprint 3**: Canvas & Properties - *[See detailed specs](./specs/sprint-3/)*
- **Sprint 4**: Intelligence Layer - *[See detailed specs](./specs/sprint-4/)*

### Key Specifications

- **[Agent Code Generation](./specs/code-generation/)** - AI agent specifications for automated code generation
- **[A2A Protocol Integration](./specs/sprint-4/a2a-protocol-feature.md)** - Agent-to-agent communication protocols
- **[Gap Analysis](./specs/GAP_ANALYSIS_COMPREHENSIVE.md)** - Comprehensive analysis of implementation gaps and context engineering needs

## License

MIT OR Apache-2.0

