# Crucible Demo Flow

> Reproducible demo scenarios for README and documentation.
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

## Demo Scenes

### Scene 1: Overview (`overview.tape` → `cru-overview.gif`)

**Duration:** ~3.8s | **Size:** 42KB | **Config:** `demo-config.toml`

Shows Crucible version and kiln statistics to establish what Crucible is.

```bash
$ cru --version && cru stats -C assets/demo-config.toml
cru 0.1.0
📊 Kiln Statistics
📁 Total files: 189
📝 Markdown files: 155
💾 Total size: 1799 KB
🗂️  Kiln path: docs
✅ Kiln scan completed successfully.
```

**What it demonstrates:**
- Crucible CLI version and basic commands
- Kiln statistics (155 markdown files in docs/)
- Quick startup

---

### Scene 2: Hero / Chat (`demo.tape` → `demo.gif`)

**Duration:** ~16s | **Size:** 1.76MB | **Config:** `demo-config.toml`

Multi-turn feature showcase: 3 exchanges covering wikilinks/knowledge graph, semantic search, and Lua plugins.

```bash
$ cru chat -C assets/demo-config.toml
> How does Crucible use wikilinks to build a knowledge graph?
> Show me how semantic search works in Crucible
> What can Lua plugins do in Crucible?
```

**What it demonstrates:**
- Chat TUI launching with NORMAL mode
- Streaming markdown responses from LLM across 3 exchanges
- Knowledge graph, semantic search, and plugin explanations
- Session persistence and response formatting

**Agent:** Internal Rig agent (`glm-4.7-flash-iq4` via OpenAI-compatible endpoint) — the default when no `-a` flag is provided

---

### Scene 3: ACP Agent (`acp-demo.tape` → `acp-demo.gif`)

**Duration:** ~48s | **Size:** 86KB | **Config:** `demo-acp-config.toml`

Crucible chat with Claude Code via Agent Context Protocol (ACP), demonstrating external agent integration.

```bash
$ cru chat -a claude -C assets/demo-acp-config.toml --set perm.autoconfirm_session
> How does ACP differ from MCP, and how does Crucible use both?
```

**What it demonstrates:**
- ACP agent integration (Claude Code)
- External LLM with access to Crucible's knowledge base
- Streaming response from Claude
- Session auto-confirmation for unattended recording

**Agent:** Claude Code (via ACP)

---

### Scene 4: Cross-Agent Delegation (`delegation-demo.tape` → `delegation-demo.gif`)

**Duration:** ~48s | **Size:** 104KB | **Config:** `demo-acp-config.toml`

Claude delegating a task to OpenCode via the `delegate_session` tool, demonstrating multi-agent orchestration.

```bash
$ cru chat -a claude -C assets/demo-acp-config.toml --set perm.autoconfirm_session
> Use the delegate_session tool to ask opencode to explain what the Agent Client Protocol is based on the codebase.
```

**What it demonstrates:**
- Multi-agent orchestration via delegation
- Claude using `delegate_session` tool to spawn OpenCode
- Cross-agent knowledge graph access
- ACP delegation configuration in `demo-acp-config.toml`

**Agents:** Claude Code (primary) → OpenCode (delegated)

---

## Configuration Files

### `demo-config.toml`

Used by Scene 1 and Scene 2 (internal chat).

- **Kiln:** `docs/` (155 markdown files)
- **LLM:** `glm-4.7-flash-iq4` via `openai` provider type (OpenAI-compatible endpoint at `https://llama.krohnos.io/v1`)
- **Storage:** Embedded (no daemon needed)
- **Flags:** None (Precognition enabled by default)

### `demo-acp-config.toml`

Used by Scene 3 and Scene 4 (ACP agents).

- **Kiln:** `docs/` (155 markdown files)
- **ACP Agents:** Claude Code with delegation settings
- **Delegation:** `[acp.agents.claude.delegation]` configured for OpenCode
- **Flags:** None (Precognition enabled by default)

---

## Replay-Based Pipeline

The demo GIFs are generated deterministically from pre-recorded JSONL fixtures using VHS. This replaces the old workflow of running live LLM sessions during recording.

### Fixture Location

Fixtures are stored in `assets/fixtures/` as JSONL files (one JSON object per line, representing recorded chat events).

### Recording Fixtures

The `--record` flag accepts a path argument and records the session directly to that file.

To record a new fixture:

```bash
# Record demo fixture (internal Rig agent)
cru chat --record assets/fixtures/demo.jsonl -C assets/demo-config.toml
# Type your query, interact with the chat, and press Ctrl+C to stop recording
# Note: Precognition requires pre-processed kiln. Run `cru process -C assets/demo-config.toml` before recording if vectors don't exist.

# Record acp-demo fixture (Claude Code via ACP)
cru chat --record assets/fixtures/acp-demo.jsonl -a claude -C assets/demo-acp-config.toml --set perm.autoconfirm_session
# Type your query and press Ctrl+C

# Record delegation-demo fixture (Claude delegating to OpenCode)
cru chat --record assets/fixtures/delegation-demo.jsonl -a claude -C assets/demo-acp-config.toml --set perm.autoconfirm_session
# Type your delegation query and press Ctrl+C
```

The recording is saved directly to the specified path and ready for GIF generation.

**Notes on recording:**
- `--record <path>` records the session to the specified JSONL file
- `--set perm.autoconfirm_session` auto-confirms session creation for unattended recording
- Fixtures must exist before GIF generation (see below)

### Headless Replay

You can also replay fixtures without the TUI, useful for testing or CI:

```bash
# Replay at normal speed with text output
cru session replay assets/fixtures/demo.jsonl

# Instant replay with raw JSON events
cru session replay assets/fixtures/demo.jsonl --speed 0 --raw
```

### Regenerating GIFs

Once fixtures are recorded, regenerate GIFs deterministically:

```bash
# Generate a single GIF from its fixture
just demo demo

# Generate all demo GIFs
just demo-all
```

This runs VHS on each `.tape` file, which replays the corresponding JSONL fixture and captures the terminal output as a GIF.

**Notes:**
- GIF generation is deterministic once fixtures are recorded
- If the Ollama endpoint is unavailable, overview GIF still works
- Precognition is enabled by default (embeddings must be pre-processed via `cru process`)
- To skip Precognition context injection, omit the kiln config

### Hide/Show Pattern

VHS tapes use `Hide`/`Show` to mask the `--replay` flag. The viewer sees `cru chat` being typed; the actual replay command is hidden. This keeps the demo looking natural while ensuring deterministic playback.

## Validation

Run `just demo-validate` to check all fixtures for quality:
- Response completeness (message_complete events present)
- Expected keywords present (golden reference files in `assets/fixtures/golden/`)
- No factual negation patterns detected

## Regenerating Assets (Legacy)

For manual VHS recording without fixtures:

```bash
# Generate all GIFs (requires live LLM sessions)
vhs assets/overview.tape
vhs assets/demo.tape
vhs assets/acp-demo.tape
vhs assets/delegation-demo.tape
```

**Notes:**
- LLM responses vary between recordings — timing may need adjustment
- If the Ollama endpoint is unavailable, overview GIF still works (`--no-process`)
- The `--no-process` flag skips file embedding on startup (faster, but no Precognition/auto-RAG)
- To enable Precognition context injection, remove `--no-process` (adds ~20s startup for 155 files)
