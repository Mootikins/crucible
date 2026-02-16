# Crucible Demo Flow

> Reproducible demo scenario for README screenshots and GIF.
> All assets generated with [VHS](https://github.com/charmbracelet/vhs) v0.10.0.

## Terminal Settings

| Setting | Value |
|---------|-------|
| Emulator | VHS v0.10.0 (headless ttyd) |
| Width | 1200px (overview: 960px) |
| Height | 700px (overview: 540px) |
| Font Size | 16px (overview: 18px) |
| Theme | Catppuccin Mocha |
| Window Bar | Colorful (macOS-style traffic lights) |
| Shell | bash |

## Assets

| File | Type | Description |
|------|------|-------------|
| `cru-overview.png` | Screenshot | Version + kiln stats (155 markdown files) |
| `cru-overview.gif` | GIF | Animated version of overview |
| `chat-demo.png` | Screenshot | Chat TUI mid-stream: question + partial response |
| `chat-response.png` | Screenshot | Chat TUI with completed response |
| `demo.gif` | GIF (~29s) | Full flow: stats → chat → question → streaming response |

## Demo Scenario

### Scene 1: Overview (`overview.tape` → `cru-overview.gif`, `cru-overview.png`)

Show `cru --version` and `cru stats` to establish what Crucible is.

```
$ cru --version && cru stats -C assets/demo-config.toml --no-process
cru 0.1.0
📊 Kiln Statistics
📁 Total files: 189
📝 Markdown files: 155
💾 Total size: 1799 KB
🗂️  Kiln path: docs
✅ Kiln scan completed successfully.
```

### Scene 2: Chat Interface (`chat.tape` / `demo.tape` → `chat-demo.png`, `chat-response.png`)

Launch `cru chat` with the internal Rig agent and ask about wikilinks.

```
$ cru chat -C assets/demo-config.toml --internal --local --no-process
> What are wikilinks and how does the knowledge graph work?
```

The agent:
1. Describes `[[wikilink]]` syntax with examples (e.g., `[[Help/Getting Started]]`)
2. Explains knowledge graph structure (Parsing → Indexing → Querying → Search)
3. Streams a formatted markdown response with headings and bullet points

### Scene 3: Full Demo (`demo.tape` → `demo.gif`)

~29 second animated GIF (3x playback) showing the complete flow:
1. `cru stats` → kiln overview (155 markdown files)
2. `cru chat` → TUI launches with NORMAL mode, model name in status bar
3. User types question → agent streams response about wikilinks and knowledge graph
4. Response completes → Ready status shown

## Regenerating Assets

```bash
vhs assets/demo.tape
vhs assets/overview.tape
vhs assets/chat.tape

# Extract PNG screenshots from GIF frames (frame numbers may vary with LLM response timing):
# chat-demo.png: mid-stream frame showing question + partial response
# chat-response.png: completed response frame
ffmpeg -i assets/demo.gif -vf "select=eq(n\,300)" -frames:v 1 -update 1 assets/chat-demo.png -y
ffmpeg -i assets/demo.gif -vf "select=eq(n\,500)" -frames:v 1 -update 1 assets/chat-response.png -y
```

## Configuration

Demo uses `assets/demo-config.toml`:
- Kiln: `docs/` (ships with repo, 155 notes)
- LLM: `qwen3-4b-instruct-2507-q8_0` via Ollama at `https://llm.example.com`
- Storage: embedded (no daemon needed)
- Flags: `--internal --local --no-process` for self-contained recording

## Notes

- LLM responses vary between recordings — frame numbers for screenshot extraction may need adjustment
- If the Ollama endpoint is unavailable, overview screenshots still work (`--no-process`)
- The `--no-process` flag skips file embedding on startup (faster, but no Precognition/auto-RAG)
- To enable Precognition context injection, remove `--no-process` (adds ~20s startup for 155 files)
