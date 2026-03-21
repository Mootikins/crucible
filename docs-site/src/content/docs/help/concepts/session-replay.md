---
title: "Session Replay"
description: "Play back recorded chat sessions without needing LLM API calls"
---

Session replay lets you play back a recorded Crucible chat session exactly as it happened, without making any LLM API calls. The recording captures every event from the original conversation: user messages, streaming text, tool calls, tool results. Replay reconstructs the experience with original timing.

## Why Replay?

**Demos without dependencies.** Show off Crucible to someone without needing a running LLM provider. Recordings are self-contained, so they work offline, on any machine, every time.

**Deterministic testing.** Run the same conversation repeatedly with identical output. No network latency variance, no model drift, no API rate limits. CI pipelines can validate TUI behavior against known-good recordings.

**Workflow documentation.** Record a complex multi-tool session once, then replay it to show teammates how you solved a problem. The recording preserves the full interaction, including tool calls and their results.

**GIF generation.** Crucible's demo GIFs are generated from replay fixtures using [VHS](https://github.com/charmbracelet/vhs). This keeps demos consistent across regenerations.

## The Replay Command

```bash
cru session replay <path-to-recording.jsonl>
```

This plays back the recording in your terminal with formatted output, streaming text at the original pace.

### Options

| Flag | Description |
|------|-------------|
| `--speed <multiplier>` | Playback speed. Default `1.0` (real-time). Use `2.0` for double speed, `0` for instant. |
| `--raw` | Show raw JSON events instead of formatted output. Useful for debugging. |

### Examples

```bash
# Play back at normal speed
cru session replay assets/fixtures/demo.jsonl

# Watch at double speed
cru session replay assets/fixtures/demo.jsonl --speed 2

# Instant playback, raw JSON output
cru session replay assets/fixtures/demo.jsonl --speed 0 --raw
```

## TUI Replay

You can also replay recordings inside the full chat TUI:

```bash
cru chat --replay assets/fixtures/demo.jsonl
```

This renders the session in the interactive interface, complete with streaming markdown, tool call panels, and all the visual polish of a live session. Add `--replay-auto-exit` to close the TUI automatically when playback finishes.

## Recording Format

Recordings use JSONL (JSON Lines), where each line is one event. A recording file has three parts:

1. **Header** (first line): version, session ID, recording mode, start timestamp
2. **Events** (middle lines): timestamped, sequenced events from the session
3. **Footer** (last line): end timestamp, total event count, duration

Event types include `user_message`, `text_delta`, `tool_call`, `tool_result`, and others. You don't need to understand the format to use replay. It's there if you want to inspect or edit recordings.

## Recording Your Own Sessions

Start any chat session with the `--record` flag:

```bash
# Record with the default agent
cru chat --record my-session.jsonl

# Record an ACP session with Claude
cru chat --record demo.jsonl -a claude
```

Chat normally. When you exit, the recording saves to the path you specified. You can replay it later, share it with others, or use it to generate GIFs.

## Built-in Fixtures

Crucible ships with pre-recorded fixtures in `assets/fixtures/`:

| Fixture | What it shows |
|---------|---------------|
| `demo.jsonl` | Internal agent explaining wikilinks and the knowledge graph |
| `acp-demo.jsonl` | Claude Code via ACP discussing protocol differences |
| `delegation-demo.jsonl` | Cross-agent delegation between Claude and Cursor |

These fixtures power the demo GIFs in the README and serve as test data for TUI development.

Try one out:

```bash
cru session replay assets/fixtures/demo.jsonl --speed 2
```

## Tips

- **Speed 0** is great for quick validation. It dumps the entire session instantly.
- **Raw mode** (`--raw`) pipes well into `jq` for filtering specific event types.
- Recordings are version-controlled. Commit them alongside your code for reproducible demos.
- Fixture files are typically small (tens of KB) since they store text, not binary data.

## See Also

- [Kilns](./kilns/) - Where sessions and notes live
- [Plaintext First](./plaintext-first/) - Why recordings use a text-based format
- [Agent Client Protocol](./agent-client-protocol/) - ACP sessions can be recorded too
