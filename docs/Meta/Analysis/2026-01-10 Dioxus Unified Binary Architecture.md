# Dioxus Unified Binary Architecture

Research into using [[https://github.com/dioxuslabs/dioxus|Dioxus]] for a unified desktop/web/CLI binary.

## Summary

Dioxus could enable a **single monolithic binary** that serves all three interfaces:
- CLI/TUI via Ratatui
- Desktop GUI via Dioxus (WebView-based)
- Web via embedded WASM assets served by Axum

The daemon pattern with fork-based auto-start makes this practical.

## Architecture

```
crucible (single binary, ~25-40MB)
│
├── crucible daemon        → Axum server + embedded web assets
├── crucible chat          → Ratatui TUI (default, auto-forks daemon)
├── crucible --gui         → Dioxus desktop window
├── crucible serve         → Daemon foreground mode
└── crucible [any command] → Auto-forks daemon if needed
```

### Component Interaction

```
┌─────────────────────────────────────────────────────┐
│  Daemon Mode (Axum)                                 │
│  ├── Web UI (embedded WASM + static assets)         │
│  ├── HTTP/SSE for browser clients                   │
│  ├── RPC/WebSocket for thick clients                │
│  └── All core: LLM providers, storage, plugins      │
└─────────────────────────────────────────────────────┘
         ▲              ▲              ▲
         │              │              │
    ┌────┴────┐   ┌─────┴─────┐   ┌────┴────┐
    │ Browser │   │ TUI/Desktop│   │ Mobile  │
    │  (web)  │   │(thick clnt)│   │ (future)│
    └─────────┘   └───────────┘   └─────────┘
```

### Entry Point

```rust
fn main() -> Result<()> {
    let args = Args::parse();

    // Auto-fork daemon if not running (for TUI/desktop modes)
    if args.needs_daemon() && !daemon_running() {
        Command::new(std::env::current_exe()?)
            .args(["daemon", "--background"])
            .spawn()?;
        wait_for_daemon()?;
    }

    match args.mode {
        Mode::Daemon => run_axum_server(),      // Axum + embedded web
        Mode::Desktop => dioxus::launch(App),   // Dioxus WebView → daemon
        Mode::Tui | _ => run_ratatui(),         // Ratatui → daemon
    }
}
```

## Binary Size Analysis

| Component | Size Impact |
|-----------|-------------|
| Core (LLM, storage, plugins) | ~15-20MB |
| Ratatui TUI | ~1-2MB |
| Dioxus desktop (WebView bindings) | ~2-3MB |
| Axum + tower | ~1-2MB |
| Embedded web assets (WASM+JS+CSS) | ~1-3MB |
| **Total** | **~25-35MB** |

### Feature Flags for Slim Builds

```toml
[features]
default = ["daemon", "tui", "desktop"]
daemon = ["axum", "tower", "embed-web"]
tui = ["ratatui", "crossterm"]
desktop = ["dioxus-desktop"]
minimal = ["tui"]  # CLI only, ~8MB
```

Users who only want CLI: `cargo install crucible --no-default-features -F minimal`

## Dioxus Findings

### What Dioxus Is

- Rust-native UI framework with React-like component model
- Uses signals-based reactivity (similar to SolidJS)
- Desktop rendering via system WebView (wry/Tauri stack)
- Current version: 0.7.2 (Dec 2025)

### What Dioxus Is NOT

- **Not truly native widgets** - uses WebView for desktop
- **Not a single binary for all platforms** - web WASM compiles separately
- **Does not have TUI support** - dioxus-tui/Rink is abandoned

### TUI Status: Dead

Dioxus TUI (`dioxus-tui`/Rink) is effectively abandoned:
- Last published: v0.5-alpha.2
- Removed from main branch
- Based on deprecated `tui-rs` (not ratatui)
- See: https://github.com/DioxusLabs/dioxus/issues/2620

**Implication**: Ratatui remains necessary for terminal UI.

### Desktop Rendering

Dioxus desktop uses `wry` (Tauri's WebView library):
- macOS: WKWebView (system, no bundling)
- Windows: Edge WebView2 (may need runtime)
- Linux: WebKitGTK (system dependency)

Binary sizes are small (~3-5MB) because they use system WebViews.

## Plugin Compatibility

Lua scripting is **unaffected** by this architecture:

```
crucible-lua   ──> Tool/Handler Registry ──> Daemon
                              │
                    ┌─────────┼─────────┐
                    ▼         ▼         ▼
                  Web       TUI     Desktop
```

Plugins load once in the daemon. All clients connect via RPC/WebSocket.

## Comparison: Current vs Proposed

| Aspect | Current | Proposed |
|--------|---------|----------|
| Web frontend | SolidJS + Vite (TypeScript) | Dioxus WASM or keep SolidJS |
| CLI | Ratatui | Ratatui (unchanged) |
| Desktop | None | Dioxus WebView → daemon |
| Daemon | Fork-based, Axum | Same, more prominent |
| Binaries | 2 (daemon + CLI) | 1 monolith or 2 (slim CLI option) |
| Frontend language | TypeScript + Rust | Rust only (if Dioxus web) |

## Alternatives Considered

### Keep SolidJS + Add Tauri Desktop

- Wrap existing SolidJS web UI in Tauri for desktop
- Minimal code changes
- Desktop binary separate from CLI
- Keeps TypeScript in the stack

### Dioxus Web + Desktop Only

- Replace SolidJS with Dioxus WASM for web
- Use Dioxus desktop for GUI
- Keep Ratatui for CLI
- Eliminates TypeScript entirely

### Status Quo + No Desktop

- Continue with SolidJS web + Ratatui CLI
- No desktop app
- Simplest path, no new dependencies

## Recommendation

The monolithic binary with fork-based daemon is architecturally sound:

1. **Daemon handles complexity** - LLM, storage, plugins load once
2. **Clients are thin** - TUI and desktop just render
3. **Web works standalone** - Browser needs no install
4. **One binary simplifies distribution** - Single download per platform

**Decision points:**

| Question | Options |
|----------|---------|
| Replace SolidJS with Dioxus web? | Rewrite cost vs TypeScript elimination |
| Ship monolith or split binaries? | Convenience vs download size |
| Desktop priority? | If low, skip Dioxus entirely |

## Relationship to DB Daemon Plan

The [[Meta/plans/2024-12-31 Single Binary DB Daemon|Single Binary DB Daemon Plan]] covers the **storage layer**:
- Fork-based daemon for SurrealDB
- Multi-session kiln access
- Embedded vs daemon storage modes

This Dioxus document covers the **UI layer**:
- Desktop GUI via Dioxus WebView
- How TUI/desktop/web share the daemon
- Binary bundling strategy

**Combined vision:**

```
┌────────────────────────────────────────────────────────┐
│  crucible (monolithic binary)                          │
├────────────────────────────────────────────────────────┤
│  Mode: daemon                                          │
│  ├── DB Daemon (from existing plan)                    │
│  │   └── SurrealDB, plugins, LLM providers             │
│  └── Web Server (Axum)                                 │
│      └── Embedded WASM + static assets                 │
├────────────────────────────────────────────────────────┤
│  Mode: tui (default)                                   │
│  └── Ratatui → connects to daemon                      │
├────────────────────────────────────────────────────────┤
│  Mode: desktop                                         │
│  └── Dioxus WebView → connects to daemon               │
└────────────────────────────────────────────────────────┘
```

The DB daemon plan is a prerequisite - it provides the RPC layer that thick clients connect to.

## References

- [Dioxus GitHub](https://github.com/DioxusLabs/dioxus)
- [Dioxus 0.6 Release](https://dioxuslabs.com/blog/release-060/)
- [Dioxus Desktop Guide](https://dioxuslabs.com/learn/0.7/guides/platforms/desktop/)
- [Blitz Native Renderer Discussion](https://github.com/DioxusLabs/dioxus/discussions/1519)
- [Rink TUI (abandoned)](https://github.com/DioxusLabs/rink)
- [[Meta/plans/2024-12-31 Single Binary DB Daemon|Single Binary DB Daemon Plan]]
