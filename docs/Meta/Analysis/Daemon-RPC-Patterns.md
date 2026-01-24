# Daemon/RPC Architecture Patterns

Research on daemon architectures and RPC patterns from production terminal applications.

## Overview

Crucible uses a multi-binary architecture with `cru` (CLI/TUI) and `cru-server` (daemon) communicating over Unix sockets via JSON-RPC. This document compares patterns from zellij, neovim, and helix to identify improvements.

## Zellij: Single Binary with Crate Separation

### Architecture

Zellij uses a **single binary** containing both client and server, organized into separate crates:

```
zellij/
├── src/main.rs          # Entry point, dispatches to client or server
├── zellij-client/       # Terminal UI, input handling, TTY management
├── zellij-server/       # PTY management, screen rendering, pane orchestration
└── zellij-utils/        # Shared: IPC protocol, data types, session serialization
```

**Key insight**: No explicit `--server` flag. `zellij attach` spawns server daemon if not running, then attaches client.

### RPC Protocol

**Protobuf over Unix socket** with code generation:

```protobuf
// client_to_server.proto - 16 message types
message DetachSession { ... }
message TerminalResize { ... }
message Action { ... }

// server_to_client.proto - 13 message types  
message Render { ... }
message Exit { ... }
message Connected { ... }
```

Generated via `prost` at build time into `generated_client_server_api.rs`.

### Version Handling

**No explicit version handshake** - relies on same-binary assumption:

- Client and server always same version (ship together)
- Breaking protobuf changes → old binaries fail to decode → connection drops
- New enum variants: old binaries ignore unknown `oneof` fields (forward compatible)

**Upgrade path**: User upgrades binary → daemon detects incompatibility on decode error → dies → new binary starts new daemon.

### Error Handling

Three-layer approach:

1. **Protobuf decode errors** - silently dropped, logged as warning
2. **Conversion errors** - `TryFrom` with `anyhow::Error`
3. **Application errors** - `ExitReason::Error(String)` sent to client

```rust
pub enum ExitReason {
    Normal,
    ForceDetached,
    Disconnect,      // Socket error
    Error(String),   // Server-side error
}
```

### Dispatch Pattern

Enum-based pattern matching, no trait-based RPC:

```rust
// route.rs
match msg {
    ClientToServerMsg::DetachSession => handle_detach(),
    ClientToServerMsg::TerminalResize(size) => handle_resize(size),
    ClientToServerMsg::Action(action) => handle_action(action),
    // ...
}
```

---

## Neovim: Code-Generated RPC

### Architecture

Neovim uses **heavy code generation** - Lua scripts generate C at build time.

### Method Registration

API functions marked with macros, processed by code generator:

```c
// src/nvim/api/buffer.c
Integer nvim_buf_line_count(Buffer buffer, Error *err)
  FUNC_API_SINCE(1)  // Mark for export
{
  buf_T *buf = find_buffer_by_handle(buffer, err);
  if (!buf) { return 0; }
  return buf->b_ml.ml_line_count;
}
```

`gen_api_dispatch.lua` generates:
- Wrapper function `handle_nvim_buf_line_count()` with validation
- `method_handlers[]` array for dispatch
- Hash function for O(1) method lookup

### Dispatch

**Hash-based lookup** - generated hash function maps method name → array index:

```c
MsgpackRpcRequestHandler msgpack_rpc_get_handler_for(
    const char *name, size_t name_len, Error *error)
{
  int hash = msgpack_rpc_get_handler_for_hash(name, name_len);
  if (hash < 0) {
    api_set_error(error, kErrorTypeException, 
                "Invalid method: %.*s", name_len, name);
    return (MsgpackRpcRequestHandler){ 0 };
  }
  return method_handlers[hash];
}
```

### Validation

Multi-level validation in generated wrappers:

```c
// Generated wrapper
if (args.size != 4) {
  api_set_error(error, kErrorTypeException, 
              "Wrong number of arguments: expecting 4 but got %zu", args.size);
  goto cleanup;
}
```

Type checking via macros:

```c
#define VALIDATE_T(name, expected_t, actual_t, code) \
  do { \
    if (expected_t != actual_t) { \
      api_err_exp(err, name, api_typename(expected_t), api_typename(actual_t)); \
      code; \
    } \
  } while (0)
```

### Introspection

`nvim_get_api_info()` returns full API metadata:

```lua
{
  version = {...},
  functions = [
    {
      name = "nvim_buf_get_lines",
      parameters = [["Buffer", "buffer"], ["Integer", "start"], ...],
      return_type = "Array",
      since = 1,
    },
    ...
  ],
  types = {
    Buffer = { id = 0, prefix = "nvim_buf_" },
    Window = { id = 1, prefix = "nvim_win_" },
  },
}
```

Clients discover available methods after connecting.

---

## Helix: Static Registration

### Architecture

**Single binary** with workspace crates:

```
helix/
├── helix-term/     # TUI and CLI
├── helix-view/     # Editor state
├── helix-lsp/      # LSP client
├── helix-core/     # Core editing engine
└── helix-tui/      # Terminal UI primitives
```

### Command Registration

**Compile-time static array** with lazy HashMap:

```rust
pub struct TypableCommand {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub doc: &'static str,
    pub fun: fn(&mut Context, Args, PromptEvent) -> anyhow::Result<()>,
    pub signature: Signature,
}

pub static TYPABLE_COMMAND_LIST: &[TypableCommand] = &[
    // All commands defined here at compile time
];

pub static TYPABLE_COMMAND_MAP: Lazy<HashMap<&'static str, &'static TypableCommand>> =
    Lazy::new(|| {
        TYPABLE_COMMAND_LIST
            .iter()
            .flat_map(|cmd| {
                std::iter::once((cmd.name, cmd))
                    .chain(cmd.aliases.iter().map(move |&alias| (alias, cmd)))
            })
            .collect()
    });
```

### Dispatch

HashMap lookup with signature validation:

```rust
fn execute_command_line(cx: &mut Context, input: &str, event: PromptEvent) -> Result<()> {
    let (command, rest, _) = command_line::split(input);
    
    match typed::TYPABLE_COMMAND_MAP.get(command) {
        Some(cmd) => execute_command(cx, cmd, rest, event),
        None if event == PromptEvent::Validate => Err(anyhow!("no such command")),
        None => Ok(()),  // Preview mode - don't error while typing
    }
}
```

### Signature-Based Parsing

Commands declare parameter schema, args validated automatically:

```rust
let args = Args::parse(args, cmd.signature, true, |token| {
    expansion::expand(cx.editor, token)
}).map_err(|err| anyhow!("'{cmd.name}': {err}"))?;
```

### LSP Request/Response Correlation

Monotonic IDs with oneshot channels:

```rust
fn call_with_timeout<R: lsp::request::Request>(&self, params: &R::Params, timeout: u64)
    -> impl Future<Output = Result<R::Result>>
{
    let id = self.next_request_id();  // Atomic counter
    let (tx, rx) = channel::<Result<Value>>(1);
    
    server_tx.send(Payload::Request { chan: tx, value: request })?;
    
    async move {
        timeout(Duration::from_secs(timeout), rx?.recv())
            .await
            .map_err(|_| Error::Timeout(id))?
    }
}
```

---

## Comparison to Crucible

| Aspect | Zellij | Neovim | Helix | Crucible |
|--------|--------|--------|-------|----------|
| **Binaries** | Single | Single | Single | Multi (cru + cru-server) |
| **Protocol** | Protobuf | msgpack | JSON-RPC | JSON-RPC |
| **Dispatch** | Enum match | Hash lookup | HashMap | String match |
| **Registration** | Proto codegen | Lua codegen | Static array | Manual handlers |
| **Version check** | None (same binary) | None | None | Build hash check |
| **Introspection** | None | `nvim_get_api_info` | None | `daemon.capabilities` |

### What Crucible Does Well

1. **Version checking** - Daemon returns build hash, client can detect mismatch
2. **Introspection** - `daemon.capabilities` returns available methods
3. **Explicit errors** - JSON-RPC returns structured error responses
4. **Clean separation** - Daemon client is a separate crate

### Potential Improvements

1. **Static method registration** - Like Helix, define methods at compile time
2. **Signature validation** - Validate args against schema before dispatch
3. **Request/response correlation** - Already using JSON-RPC IDs
4. **Code generation** - Consider generating dispatch from schema (like Neovim)

---

## Recommendations

### Short Term

1. **Add method signatures** - Define expected params for each RPC method
2. **Validate on dispatch** - Check param types/counts before calling handler
3. **Better error context** - Include method name and param details in errors

### Medium Term

1. **Static registration macro** - Define handlers with compile-time checks:
   ```rust
   rpc_methods! {
       "session.create" => session_create(session_type: String, kiln: Path),
       "session.send_message" => session_send(session_id: String, content: String),
   }
   ```

2. **Schema introspection** - Extend `daemon.capabilities` with param types

### Long Term

1. **Consider protobuf/msgpack** - Binary protocols are more efficient
2. **Code generation** - Generate handlers from schema definition
3. **Single binary option** - Allow embedded daemon mode for simpler deployment
