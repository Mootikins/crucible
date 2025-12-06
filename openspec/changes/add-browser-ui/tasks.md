# Browser UI Implementation Tasks

## 1. Crate Setup

- [ ] 1.1 Create `crates/crucible-web/Cargo.toml` with dependencies
- [ ] 1.2 Add `crucible-web` to workspace `Cargo.toml`
- [ ] 1.3 Create `crates/crucible-web/src/lib.rs` with module structure
- [ ] 1.4 Verify crate compiles with `cargo check -p crucible-web`

## 2. Actor Infrastructure

- [ ] 2.1 Create `src/actors/mod.rs` with actor module exports
- [ ] 2.2 Implement `ChatActor` in `src/actors/chat.rs`
  - [ ] 2.2.1 Define `ChatMessage` enum (UserMessage, AgentToken, AgentComplete, Error)
  - [ ] 2.2.2 Implement Actor trait for ChatActor
  - [ ] 2.2.3 Add ACP client integration
  - [ ] 2.2.4 Handle streaming tokens from ACP
- [ ] 2.3 Implement `WebHostActor` in `src/actors/web_host.rs`
  - [ ] 2.3.1 Define SSE connection management
  - [ ] 2.3.2 Route incoming HTTP requests to ChatActor
  - [ ] 2.3.3 Transform ChatActor messages to SSE events

## 3. HTTP Routes

- [ ] 3.1 Create `src/routes/mod.rs` with route configuration
- [ ] 3.2 Implement `/api/chat` POST endpoint in `src/routes/chat.rs`
  - [ ] 3.2.1 Parse JSON request body
  - [ ] 3.2.2 Send message to ChatActor
  - [ ] 3.2.3 Return SSE stream response
- [ ] 3.3 Implement static asset serving in `src/routes/static.rs`
  - [ ] 3.3.1 Debug mode: serve from filesystem
  - [ ] 3.3.2 Release mode: serve embedded assets
  - [ ] 3.3.3 Support `--web-dir` override

## 4. Server Configuration

- [ ] 4.1 Create `src/server.rs` with Actix-web app factory
- [ ] 4.2 Configure CORS for local development
- [ ] 4.3 Add graceful shutdown handling
- [ ] 4.4 Expose `start_server(config)` public API

## 5. Frontend Setup

- [ ] 5.1 Initialize Bun project in `crates/crucible-web/web/`
- [ ] 5.2 Install dependencies: svelte, svelte-ai-elements, tailwindcss
- [ ] 5.3 Configure Vite for Svelte 5
- [ ] 5.4 Create `src/App.svelte` entry point
- [ ] 5.5 Create `src/lib/Chat.svelte` with svelte-ai-elements components
  - [ ] 5.5.1 Chat container layout
  - [ ] 5.5.2 Message list with streaming support
  - [ ] 5.5.3 Prompt input component
  - [ ] 5.5.4 Markdown rendering for responses
- [ ] 5.6 Implement SSE client in `src/lib/sse.ts`
- [ ] 5.7 Build and verify output in `dist/`

## 6. CLI Integration

- [ ] 6.1 Add `crucible-web` dependency to `crucible-cli`
- [ ] 6.2 Create `src/commands/serve.rs` subcommand
  - [ ] 6.2.1 Parse `--port` flag (default 3000)
  - [ ] 6.2.2 Parse `--web-dir` flag (optional)
  - [ ] 6.2.3 Start web server
  - [ ] 6.2.4 Print access URL
- [ ] 6.3 Register `serve` in `src/commands/mod.rs`
- [ ] 6.4 Test `cru serve` command

## 7. Asset Embedding

- [ ] 7.1 Add `rust-embed` or `include_dir` dependency
- [ ] 7.2 Configure conditional compilation for assets
- [ ] 7.3 Add build script to compile frontend before Rust (if needed)
- [ ] 7.4 Test release build with embedded assets

## 8. Testing

- [ ] 8.1 Unit tests for ChatActor message handling
- [ ] 8.2 Unit tests for SSE event formatting
- [ ] 8.3 Integration test: HTTP request → SSE response
- [ ] 8.4 Manual test: full chat flow in browser

## 9. Documentation

- [ ] 9.1 Add README to `crates/crucible-web/`
- [ ] 9.2 Document `cru serve` in CLI help
- [ ] 9.3 Add development setup instructions for frontend
