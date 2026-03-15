# Crucible Demo Fixtures

Pre-recorded JSONL event streams from real Crucible chat sessions. These fixtures enable reproducible, deterministic demo GIF generation without requiring live LLM calls.

## What Are Fixtures?

Fixtures are `.jsonl` files containing recorded session events (user messages, LLM responses, tool calls, etc.) captured from actual chat sessions. When you run `cru chat --replay <fixture.jsonl>`, the daemon:

1. Creates a replay session and parses the recording
2. Emits events with original timing (adjustable via `--replay-speed`)
3. Streams events to the TUI, which auto-injects user messages and renders responses
4. Automatically exits after the final event (with `--replay-auto-exit`)

This makes demo recording **deterministic** — no waiting for LLM latency, no variable response times, no network failures.

## Available Fixtures

| Fixture | Demo | Description |
|---------|------|-------------|
| `demo.jsonl` | `demo.gif` | Multi-turn feature showcase: wikilinks/knowledge graph, semantic search, and Lua plugins (3 exchanges) |
| `acp-demo.jsonl` | `acp-demo.gif` | Claude Code via ACP discussing ACP vs MCP |
| `delegation-demo.jsonl` | `delegation-demo.gif` | Claude delegating to OpenCode via `delegate_session` tool |

## Recording New Fixtures

Recording is daemon-managed. The `--record` flag accepts a path argument and records the session to that file.

```bash
# Start a chat session with recording enabled
cru chat --record assets/fixtures/<name>.jsonl [agent flags]

# Example: Record with Claude Code
cru chat --record assets/fixtures/demo.jsonl -a claude

# Example: Record with internal Rig agent
cru chat --record assets/fixtures/demo.jsonl -C assets/demo-config.toml
```

Interact normally, type queries, use tools, etc. When you exit, the recording is saved to the specified path. You can then use it to regenerate GIFs:

```bash
# Regenerate GIF from the fixture
just demo <name>
```

## Regenerating GIFs

Once fixtures exist, regenerate GIFs deterministically:

```bash
# Regenerate all demo GIFs
just demo-all

# Regenerate a single demo
just demo demo
just demo acp-demo
just demo delegation-demo

# Or use VHS directly
vhs assets/demo.tape
vhs assets/acp-demo.tape
vhs assets/delegation-demo.tape
```

The VHS tapes reference fixtures via `--replay` and `--replay-auto-exit`, ensuring consistent output across runs.

## Fixture Format

Fixtures are JSONL (JSON Lines) with three sections: a header, recorded events, and a footer.

```json
{"version":1,"session_id":"abc123","recording_mode":"granular","started_at":"2026-01-01T00:00:00Z"}
{"ts":"2026-01-01T00:00:01Z","seq":1,"event":"user_message","session_id":"abc123","data":{"content":"How does Crucible work?"}}
{"ts":"2026-01-01T00:00:05Z","seq":2,"event":"text_delta","session_id":"abc123","data":{"content":"Crucible is..."}}
{"ts":"2026-01-01T00:00:10Z","seq":3,"event":"tool_call","session_id":"abc123","data":{"tool":"search","args":{"query":"knowledge graph"}}}
{"ended_at":"2026-01-01T00:01:00Z","total_events":42,"duration_ms":60000}
```

- **Line 1 (RecordingHeader):** version, session_id, recording_mode, started_at
- **Lines 2..N (RecordedEvent):** ts, seq (monotonic), event type, session_id, data payload
- **Last line (RecordingFooter):** ended_at, total_events, duration_ms

See `crucible-daemon/src/recording.rs` for the canonical types.

## Headless Replay

You can replay fixtures without the TUI using `cru session replay`:

```bash
# Replay at normal speed with text output
cru session replay assets/fixtures/demo.jsonl

# Replay at 2x speed
cru session replay assets/fixtures/demo.jsonl --speed 2

# Instant replay with raw JSON events
cru session replay assets/fixtures/demo.jsonl --speed 0 --raw
```

This is useful for testing, CI, or piping replay output to other tools.

## Updating Fixtures

If you need to update a fixture (e.g., to fix a response or add new content):

1. **Delete the old fixture:**
   ```bash
   rm assets/fixtures/<name>.jsonl
   ```

2. **Record a new one:**
   ```bash
   cru chat --record [agent flags]
   # interact, then exit
   cru session list
   cp ~/.crucible/sessions/<session-id>/recording.jsonl assets/fixtures/<name>.jsonl
   ```

3. **Regenerate the GIF:**
   ```bash
   just demo <name>
   ```

## Validation

Use `just demo-validate` to verify fixture quality:

```bash
just demo-validate
```

This checks all fixtures for:
- `message_complete` events (response completeness)
- Expected keywords (from `assets/fixtures/golden/*.keywords`)
- No factual negation patterns

## Notes

- Fixtures must exist **before** running VHS to generate GIFs
- Recording requires a working LLM provider (Ollama, Claude, etc.)
- Fixture file sizes vary based on response length and tool usage
- Fixtures are version-controlled in git (they're deterministic snapshots)
- Precognition is enabled by default (embeddings must be pre-processed via `cru process`)
