# Crucible Demo Fixtures

Pre-recorded JSONL event streams from real Crucible chat sessions. These fixtures enable reproducible, deterministic demo GIF generation without requiring live LLM calls.

## What Are Fixtures?

Fixtures are `.jsonl` files containing serialized session events (user messages, LLM responses, tool calls, etc.) captured from actual chat sessions. When you run `cru chat --replay <fixture.jsonl>`, the TUI:

1. Auto-injects user messages from the fixture
2. Streams pre-recorded LLM responses
3. Replays tool calls and state changes
4. Automatically exits after the final response (with `--replay-auto-exit`)

This makes demo recording **deterministic** — no waiting for LLM latency, no variable response times, no network failures.

## Available Fixtures

| Fixture | Demo | Description |
|---------|------|-------------|
| `demo.jsonl` | `demo.gif` | Internal Rig agent explaining wikilinks and knowledge graphs |
| `acp-demo.jsonl` | `acp-demo.gif` | Claude Code via ACP discussing ACP vs MCP |
| `delegation-demo.jsonl` | `delegation-demo.gif` | Claude delegating to Cursor via `delegate_session` tool |

## Recording New Fixtures

To capture a new fixture from a real chat session:

```bash
# Start a chat session with recording enabled
cru chat --record assets/fixtures/<name>.jsonl [agent flags]

# Example: Record with Claude Code
cru chat --record assets/fixtures/my-demo.jsonl -a claude

# Example: Record with internal Rig agent
cru chat --record assets/fixtures/my-demo.jsonl --internal --local
```

The `--record` flag captures all session events to the JSONL file. Interact normally — type queries, use tools, etc. When you exit the session, the fixture is saved.

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

Fixtures are JSONL (JSON Lines) — one JSON object per line, each representing a session event:

```json
{"type": "user_message", "content": "How does Crucible work?", "timestamp": "2025-02-18T10:30:00Z"}
{"type": "llm_response", "content": "Crucible is a local-first AI assistant...", "timestamp": "2025-02-18T10:30:05Z"}
{"type": "tool_call", "tool": "search", "args": {"query": "knowledge graph"}, "timestamp": "2025-02-18T10:30:10Z"}
```

See `crucible-core/src/session/types.rs` for the canonical `SessionEvent` type.

## Updating Fixtures

If you need to update a fixture (e.g., to fix a response or add new content):

1. **Delete the old fixture:**
   ```bash
   rm assets/fixtures/<name>.jsonl
   ```

2. **Record a new one:**
   ```bash
   cru chat --record assets/fixtures/<name>.jsonl [agent flags]
   ```

3. **Regenerate the GIF:**
   ```bash
   just demo <name>
   ```

## Notes

- Fixtures must exist **before** running VHS to generate GIFs
- Recording requires a working LLM provider (Ollama, Claude, etc.)
- Fixture file sizes vary based on response length and tool usage
- Fixtures are version-controlled in git (they're deterministic snapshots)
- To enable Precognition (auto-RAG), remove `--no-process` from the tape's launch command
