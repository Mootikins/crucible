# Crucible Demo Flow

> Reproducible demo scenarios for README and documentation.
> All assets generated with the **programmatic session pipeline** and **asciinema+agg** (VHS deprecated).

## Recording Pipeline

### Programmatic Session Recording

Demos are recorded using a **programmatic pipeline** that creates sessions, sends messages, and captures recordings deterministically:

```bash
# Step 1: Create session with recording enabled
export OPENAI_API_KEY=dummy
SESSION_ID=$(cru session create --recording-mode granular -C assets/demo-config.toml 2>&1 | grep "Created session" | awk '{print $NF}')

# Step 2: Configure agent (if not auto-configured by -C flag)
cru session configure "$SESSION_ID" --provider openai --model qwen3-32b-ud-q4_k_xl --endpoint https://llm.example.com/v1

# Step 3: Send message (blocks until complete)
cru session send "$SESSION_ID" "How does the wikilink knowledge graph work in Crucible?" --raw

# Step 4: Extract recording
RECORDING=$(find docs/.crucible/sessions/$SESSION_ID -name "recording.jsonl")
cp "$RECORDING" assets/fixtures/demo.jsonl

# Step 5: Capture GIF from fixture
bash scripts/record-gif.sh assets/fixtures/demo.jsonl assets/demo.gif --speed 5
```

**Key points:**
- Sessions are created with `--recording-mode granular` to capture all events
- Recording file is written to `docs/.crucible/sessions/<session-id>/recording.jsonl` (inside the kiln directory)
- `cru session send` blocks until the message is complete, ensuring full response is captured
- Fixtures are copied to `assets/fixtures/` for version control and reproducible GIF generation
- GIF capture uses `scripts/record-gif.sh` which wraps tmux + asciinema + agg

### GIF Capture Tool

The `scripts/record-gif.sh` script replaces VHS (which hangs on Chrome 145 with go-rod). It uses:

- **asciinema**: Records terminal session to `.cast` format
- **agg**: Converts `.cast` to GIF with dracula theme
- **tmux**: Provides real PTY for TUI rendering

```bash
bash scripts/record-gif.sh <fixture.jsonl> <output.gif> [--speed N]
```

**Example:**
```bash
bash scripts/record-gif.sh assets/fixtures/demo.jsonl assets/demo.gif --speed 5
```

**Terminal settings:**
- Size: 120x35 characters
- Font size: 16px
- Theme: dracula (dark, matches docs-site)
- Idle timeout: 3 seconds

## Terminal Settings (Legacy VHS Reference)

| Setting | Value |
|---------|-------|
| Emulator | asciinema + agg (replaces VHS v0.10.0) |
| Width | 120 chars (1200px equivalent) |
| Height | 35 chars (700px equivalent) |
| Font Size | 16px |
| Theme | dracula |
| Shell | bash |
| Recording | Programmatic session pipeline |

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

**Duration:** ~30s | **Size:** ~2MB | **Config:** `demo-config.toml`

Single-turn feature showcase: wikilink knowledge graph with Precognition context injection.

```bash
$ cru chat -C assets/demo-config.toml
> How does the wikilink knowledge graph work in Crucible?
```

**What it demonstrates:**
- Chat TUI launching with NORMAL mode
- Streaming markdown response from LLM
- **Precognition context injection** — top-5 relevant notes from knowledge graph automatically injected before LLM call
- Knowledge graph structure and semantic search capabilities
- Session persistence and response formatting

**Agent:** Internal Rig agent (`qwen3-32b-ud-q4_k_xl` via OpenAI-compatible endpoint) — the default when no `-a` flag is provided

**Recording:** Programmatic session pipeline (see above). Fixture: `assets/fixtures/demo.jsonl` (39KB, 1 exchange, 1 precognition_complete, 71 text_delta, 188 thinking events). Recorded 2026-03-15.

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
- Tool calls (Claude can read files and search knowledge graph)
- Streaming response from Claude
- Session auto-confirmation for unattended recording

**Agent:** Claude Code (via ACP)

**Recording:** Programmatic session pipeline. Fixture: `assets/fixtures/acp-demo.jsonl` (65KB, 1 exchange, 4 tool_call, 1 precognition_complete, 227 text_delta).

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

**Recording:** Programmatic session pipeline. Fixture: `assets/fixtures/delegation-demo.jsonl` (4.2KB, 1 exchange, 1 tool_call, 18 text_delta).

---

## Configuration Files

### `demo-config.toml`

Used by Scene 1 and Scene 2 (internal chat).

- **Kiln:** `docs/` (155 markdown files)
- **LLM:** `qwen3-32b-ud-q4_k_xl` via `openai` provider type (OpenAI-compatible endpoint at `https://llm.example.com/v1`)
- **Storage:** Embedded (no daemon needed)
- **Recording:** Sessions recorded to `docs/.crucible/sessions/<id>/recording.jsonl`
- **Flags:** None (Precognition enabled by default)

### `demo-acp-config.toml`

Used by Scene 3 and Scene 4 (ACP agents).

- **Kiln:** `docs/` (155 markdown files)
- **ACP Agents:** Claude Code with delegation settings
- **Delegation:** `[acp.agents.claude.delegation]` configured for OpenCode
- **Recording:** Sessions recorded to `docs/.crucible/sessions/<id>/recording.jsonl`
- **Flags:** None (Precognition enabled by default)

---

## Replay-Based Pipeline

The demo GIFs are generated deterministically from pre-recorded JSONL fixtures using the programmatic session pipeline and asciinema+agg. This replaces the old workflow of running live LLM sessions during recording.

### Fixture Location

Fixtures are stored in `assets/fixtures/` as JSONL files (one JSON object per line, representing recorded chat events).

### Recording Fixtures (Programmatic)

The programmatic pipeline creates sessions, sends messages, and captures recordings:

```bash
# Create session with recording enabled
export OPENAI_API_KEY=dummy
SESSION_ID=$(cru session create --recording-mode granular -C assets/demo-config.toml 2>&1 | grep "Created session" | awk '{print $NF}')

# Configure agent if needed
cru session configure "$SESSION_ID" --provider openai --model qwen3-32b-ud-q4_k_xl --endpoint https://llm.example.com/v1

# Send message (blocks until complete)
cru session send "$SESSION_ID" "Your query here" --raw

# Extract recording
RECORDING=$(find docs/.crucible/sessions/$SESSION_ID -name "recording.jsonl")
cp "$RECORDING" assets/fixtures/demo.jsonl
```

**Notes on recording:**
- `--recording-mode granular` captures all events (user_message, text_delta, tool_call, precognition_complete, etc.)
- Recording is written to `docs/.crucible/sessions/<id>/recording.jsonl` (inside the kiln directory)
- `cru session send` blocks until the message is complete
- `--raw` flag outputs raw JSON events (useful for debugging)
- Fixtures must exist before GIF generation (see below)

### Headless Replay

You can also replay fixtures without the TUI, useful for testing or CI:

```bash
# Replay at normal speed with text output
cru session replay assets/fixtures/demo.jsonl

# Instant replay with raw JSON events
cru session replay assets/fixtures/demo.jsonl --speed 0 --raw
```

### Replay Speed for GIF Generation

The `--replay-speed` flag on `cru chat --replay` controls how fast events are emitted relative to real-time. This is separate from agg's frame rate.

For long fixtures, use `--speed` parameter in `record-gif.sh`:
- `--speed 5` compresses a 60s fixture to ~12s of real-time playback
- agg then renders at normal frame rate

### Regenerating GIFs

Once fixtures are recorded, regenerate GIFs deterministically:

```bash
# Generate a single GIF from its fixture
bash scripts/record-gif.sh assets/fixtures/demo.jsonl assets/demo.gif --speed 5

# Generate all demo GIFs
just demo-all
```

This runs the programmatic pipeline: replay fixture → capture with asciinema → convert to GIF with agg.

**Notes:**
- GIF generation is deterministic once fixtures are recorded
- If the Ollama endpoint is unavailable, overview GIF still works
- Precognition is enabled by default (embeddings must be pre-processed via `cru process`)
- To skip Precognition context injection, omit the kiln config

### Hide/Show Pattern (Legacy)

VHS tapes use `Hide`/`Show` to mask the `--replay` flag. The viewer sees `cru chat` being typed; the actual replay command is hidden. This keeps the demo looking natural while ensuring deterministic playback.

## Validation

Run `scripts/validate-demos.sh` to check all fixtures for quality:

```bash
bash scripts/validate-demos.sh
```

- Response completeness (message_complete events present)
- Expected keywords present (golden reference files in `assets/fixtures/golden/`)
- No factual negation patterns detected

## Regenerating Assets (Legacy VHS)

For reference, the old VHS-based workflow is no longer used:

```bash
# OLD: Generate all GIFs with VHS (DEPRECATED — Chrome 145 hangs)
vhs assets/overview.tape
vhs assets/demo.tape
vhs assets/acp-demo.tape
vhs assets/delegation-demo.tape
```

**Why VHS was deprecated:**
- Chrome 145 + go-rod timing issues (no bundled Chromium)
- Unreliable terminal capture
- Replaced by asciinema+agg pipeline
