# Browser UI for ACP Agent Chat

## Why

Users need a browser-based interface for chatting with ACP agents (Claude, Gemini, Codex, OpenCode). A web UI enables:

1. **Local network access** - Chat from any device on the network without installing CLI
2. **Rich rendering** - Proper markdown, syntax highlighting, and streaming token display
3. **Desktop foundation** - Same UI can power future Tauri desktop app
4. **Plugin architecture** - Web server as a plugin establishes patterns for future extensibility

## What Changes

**New crate `crucible-web`:**
- Actix-web server with actix actor system
- SSE endpoints for streaming chat responses
- Svelte 5 SPA with svelte-ai-elements components
- Asset serving: filesystem in debug, embedded in release

**Actor architecture:**
- `WebHostActor` - Serves SPA, manages SSE connections to browsers
- `ChatActor` - Handles conversation state, communicates with ACP agents
- Message-passing between actors (foundation for plugin event system)

**CLI integration:**
- New `cru serve` command starts web server
- `--port` flag (default 3000)
- `--web-dir` flag for development override

## Impact

### Affected Specs
- **apis** (new requirements) - HTTP endpoints, SSE streaming, web asset serving

### Affected Code

**New crate:**
- `crates/crucible-web/` - Web server crate
  - `src/lib.rs` - Crate root, actor system setup
  - `src/server.rs` - Actix-web configuration
  - `src/actors/mod.rs` - Actor module
  - `src/actors/web_host.rs` - WebHostActor implementation
  - `src/actors/chat.rs` - ChatActor implementation
  - `src/routes/mod.rs` - Route handlers
  - `src/routes/chat.rs` - `/api/chat` POST + SSE endpoints
  - `src/routes/static.rs` - Static asset serving
  - `web/` - Svelte SPA source
    - `package.json` - Bun/npm config
    - `src/App.svelte` - Main app component
    - `src/lib/Chat.svelte` - Chat interface using svelte-ai-elements

**Modified:**
- `crates/crucible-cli/src/commands/mod.rs` - Add `serve` subcommand
- `crates/crucible-cli/src/commands/serve.rs` - New file, invokes crucible-web
- `Cargo.toml` - Add crucible-web to workspace

### Dependencies (crucible-web)
- `actix-web = "4"` - Web framework
- `actix = "0.13"` - Actor framework
- `actix-web-lab = "0.22"` - SSE support
- `actix-files = "0.6"` - Static file serving (debug mode)
- `rust-embed = "8"` - Asset embedding (release mode)
- `serde = { version = "1", features = ["derive"] }`
- `serde_json = "1"`
- `tokio = { version = "1", features = ["sync"] }`

### Frontend Dependencies (packages/web-ui or crates/crucible-web/web)
- Bun runtime
- Svelte 5
- svelte-ai-elements (chat primitives, markdown rendering)
- Tailwind CSS

### User-Facing Impact
- **New command**: `cru serve` starts web UI on localhost:3000
- **Browser access**: Open http://localhost:3000 to chat
- **Same experience**: Streaming responses, markdown rendering, code highlighting
