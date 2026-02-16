# Crucible Demo Flow

> Reproducible demo scenario for README screenshots and GIF.
> All assets generated with [VHS](https://github.com/charmbracelet/vhs).

## Terminal Settings

| Setting | Value |
|---------|-------|
| Emulator | VHS (headless ttyd) |
| Width | 120 columns |
| Height | 40 rows |
| Font Size | 16px |
| Theme | Catppuccin Mocha |
| Shell | bash |

## Demo Scenario

### Scene 1: Overview (screenshot — `cru-overview.png`)

Show `cru --help` and `cru stats` to establish what Crucible is.

```
$ cru --help          # Show available commands
$ cru stats           # Show kiln statistics (155 markdown files)
```

### Scene 2: Chat Interface (screenshot — `chat-demo.png`)

Launch `cru chat` and ask a knowledge question that triggers Precognition (context injection from vault notes).

```
$ cru chat
> What is the wikilink syntax and how does the knowledge graph work?
```

The agent should:
1. Find relevant notes via Precognition (auto-RAG)
2. Reference `[[Wikilinks]]`, `[[Knowledge Graph]]` etc.
3. Show streaming markdown response with code blocks

### Scene 3: Search & Commands (screenshot — `search-demo.png`)

Demonstrate `/search` and `:help` REPL commands.

```
/search plugin system     # Manual context injection
:help                     # Show available commands
```

### Scene 4: Full Demo (GIF — `demo.gif`)

20-30 second animated GIF showing the complete flow:
1. `cru stats` → kiln overview
2. `cru chat` → TUI launches
3. User types question → agent streams response with vault context
4. `/search` → results injected
5. `BackTab` → mode cycling (Normal → Plan → Auto)

## Regenerating Assets

```bash
# Regenerate all assets
vhs assets/demo.tape

# Regenerate individual screenshots
vhs assets/overview.tape
vhs assets/chat.tape
```

## Notes

- Use `docs/` as the demo kiln (ships with the repo, 155 notes)
- Config: `-C /tmp/crucible-demo-config.toml` overrides user config
- LLM backend: Ollama at `https://llama.krohnos.io` with devstral model
- If LLM is unavailable, screenshots can be generated from non-chat commands
