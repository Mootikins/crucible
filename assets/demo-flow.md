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
$ cru --version && cru stats -C assets/demo-config.toml --no-process
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
- Quick startup with `--no-process` flag

---

### Scene 2: Hero / Chat (`demo.tape` → `demo.gif`)

**Duration:** ~18.48s | **Size:** 1.76MB | **Config:** `demo-config.toml`

Interactive chat session with internal Rig agent asking about wikilinks and knowledge graphs.

```bash
$ cru chat -C assets/demo-config.toml --internal --local --no-process
> How does Crucible use wikilinks to build a knowledge graph?
```

**What it demonstrates:**
- Chat TUI launching with NORMAL mode
- Streaming markdown response from LLM
- Knowledge graph explanation with wikilink syntax examples
- Session persistence and response formatting

**Agent:** Internal Rig agent (Ollama `qwen3-4b-instruct-2507-q8_0`)

---

### Scene 3: ACP Agent (`acp-demo.tape` → `acp-demo.gif`)

**Duration:** ~19.8s | **Size:** 86KB | **Config:** `demo-acp-config.toml`

Crucible chat with Claude Code via Agent Context Protocol (ACP), demonstrating external agent integration.

```bash
$ cru chat -a claude --no-process --set perm.autoconfirm_session
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

**Duration:** ~18.6s | **Size:** 104KB | **Config:** `demo-acp-config.toml`

Claude delegating a task to Cursor via the `delegate_session` tool, demonstrating multi-agent orchestration.

```bash
$ cru chat -a claude -C assets/demo-acp-config.toml --no-process --set perm.autoconfirm_session
> Use the delegate_session tool to ask cursor to explain what the Agent Client Protocol is based on the codebase.
```

**What it demonstrates:**
- Multi-agent orchestration via delegation
- Claude using `delegate_session` tool to spawn Cursor
- Cross-agent knowledge graph access
- ACP delegation configuration in `demo-acp-config.toml`

**Agents:** Claude Code (primary) → Cursor (delegated)

---

## Configuration Files

### `demo-config.toml`

Used by Scene 1 and Scene 2 (internal chat).

- **Kiln:** `docs/` (155 markdown files)
- **LLM:** `qwen3-4b-instruct-2507-q8_0` via Ollama at `https://llm.example.com`
- **Storage:** Embedded (no daemon needed)
- **Flags:** `--internal --local --no-process` for self-contained recording

### `demo-acp-config.toml`

Used by Scene 3 and Scene 4 (ACP agents).

- **Kiln:** `docs/` (155 markdown files)
- **ACP Agents:** Claude Code with delegation settings
- **Delegation:** `[acp.agents.claude.delegation]` configured for Cursor
- **Flags:** `--no-process` for faster startup

---

## Regenerating Assets

```bash
# Generate all GIFs
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
