# Browser UI Design

## Context

Crucible needs a web-based chat interface for ACP agents. This establishes the foundation for:
- Local network access to chat functionality
- Future Tauri desktop app (same UI)
- Plugin architecture patterns (actors, events)

**Constraints:**
- Must work as single binary distribution (embedded assets)
- Must support fast dev iteration (filesystem serving)
- Must stream tokens in real-time (SSE)
- Claude-only for MVP, but provider-agnostic design

## Goals / Non-Goals

**Goals:**
- Streaming chat with Claude via ACP
- Markdown rendering with syntax highlighting
- Single binary distribution
- Actor-based architecture for future extensibility
- Self-contained crate (portable for plugin extraction)

**Non-Goals (MVP):**
- Multiple simultaneous agents
- Conversation persistence
- Authentication/authorization
- Multi-user support
- WebSocket (SSE is sufficient for server→client streaming)

## Decisions

### Web Framework: Actix-web + Actix Actors

**Decision:** Use actix-web with the actix actor framework.

**Rationale:**
- Already a transitive dependency (minimal binary size impact)
- Actor model aligns with event-driven architecture vision
- Built-in SSE support via `actix-web-lab`
- Same paradigm from data layer to presentation (SurrealDB → Actors → SSE → Svelte)

**Alternatives considered:**
- Axum: Lighter, but no actor framework - would need separate eventing solution
- Rocket: Less mature async story
- Pure tokio channels: Works but actors provide better structure for complex interactions

### Frontend: Svelte 5 + svelte-ai-elements

**Decision:** Svelte 5 SPA with svelte-ai-elements component library.

**Rationale:**
- svelte-ai-elements provides chat primitives (Message, Prompt, Markdown, CodeBlock)
- Built on shadcn-svelte (consistent design system)
- Includes Chain of Thought / Tool components for Phase 2
- Small bundle size, excellent performance
- Bun for fast builds

**Alternatives considered:**
- shadcn-svelte only: Would need to build chat components from scratch
- Server-rendered (HTMX): Poor fit for streaming token display
- React: Larger bundle, more complexity for same result

### Asset Serving: Conditional Compilation

**Decision:** Use `#[cfg(debug_assertions)]` to switch between filesystem and embedded assets.

```rust
#[cfg(not(debug_assertions))]
static ASSETS: include_dir::Dir = include_dir::include_dir!("web/dist");

#[cfg(debug_assertions)]
fn serve_assets() -> actix_files::Files {
    actix_files::Files::new("/", "./web/dist")
}
```

**Rationale:**
- Release builds are self-contained (single binary)
- Debug builds allow hot-reload of frontend
- `--web-dir` flag provides escape hatch for both modes

### Actor Topology

**Decision:** Two actors for MVP - WebHostActor and ChatActor.

```
Browser ←─SSE─→ WebHostActor ←─messages─→ ChatActor ←─ACP─→ Claude
```

**WebHostActor responsibilities:**
- Accept HTTP requests
- Manage SSE connections (multiple browsers)
- Route chat requests to ChatActor
- Transform ChatActor messages to SSE events

**ChatActor responsibilities:**
- Manage conversation state
- Communicate with ACP agent
- Emit events (token received, message complete, error)

**Rationale:**
- Clean separation of concerns
- ChatActor is reusable (CLI could use same actor)
- WebHostActor handles web-specific concerns
- Foundation for adding more actors (Router, per-Agent actors)

### SSE Event Format

**Decision:** JSON events with type discrimination.

```
event: token
data: {"content": "Hello"}

event: message_complete
data: {"id": "msg_123", "content": "Hello, world!"}

event: error
data: {"code": "provider_error", "message": "Rate limited"}
```

**Rationale:**
- Typed events allow frontend to handle each case
- JSON payload is extensible
- Standard SSE format, easy to debug

### Crate Location

**Decision:** `crates/crucible-web/` with `web/` subfolder for Svelte source.

```
crates/crucible-web/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── server.rs
│   ├── actors/
│   └── routes/
└── web/
    ├── package.json
    ├── bun.lock
    ├── src/
    │   ├── App.svelte
    │   └── lib/
    └── dist/        # Built assets (gitignored except for release)
```

**Rationale:**
- Self-contained for future plugin extraction
- Co-location makes embedding straightforward
- Clear ownership of frontend assets

## Risks / Trade-offs

**Risk:** Actix actor framework adds learning curve.
- **Mitigation:** Start with two simple actors, document patterns.

**Risk:** Frontend build step complicates release process.
- **Mitigation:** CI builds frontend first, then Rust. Or commit dist/ for releases.

**Risk:** SSE limitations (server→client only).
- **Mitigation:** Sufficient for MVP. POST for client→server, SSE for responses.

**Trade-off:** Embedding assets increases binary size.
- **Accepted:** Svelte bundle is ~50-100KB gzipped. Negligible vs 69MB binary.

## Migration Plan

N/A - New feature, no existing functionality to migrate.

## Open Questions

1. **Event bus scope:** Should ChatActor events be the foundation for a broader plugin event system, or keep them internal for now?
   - **Tentative answer:** Keep internal for MVP. Design messages to be wrappable later.

2. **Conversation persistence:** Should we store chat history in SurrealDB?
   - **Tentative answer:** Not for MVP. Add in Phase 2.

3. **Authentication:** How do we secure the web UI for network access?
   - **Tentative answer:** Not for MVP. Localhost-only default. Add token auth later.
