# Using GitHub Copilot Account with ACP Agents in Crucible

## 🎯 Goal

Use your **GitHub Copilot subscription** as the LLM provider for ACP-compatible agents (like OpenCode or Claude Code) to interact with Crucible, maintaining IP security while leveraging your existing Copilot license.

## ✅ TL;DR: Two Working Solutions

### Solution 1: OpenCode (Easiest) ✅ **RECOMMENDED**
**Native GitHub Copilot support - no proxy needed**

```bash
# Install OpenCode
npm install -g @opencode-ai/opencode

# Authenticate with GitHub Copilot
opencode auth login
# Select "GitHub Copilot" from the list
# Log in with your GitHub account

# Use with Crucible via ACP
opencode acp
```

**Pros:**
- ✅ Native Copilot support built-in
- ✅ No proxy server required
- ✅ Simple authentication
- ✅ Works with Crucible's ACP immediately

**Cons:**
- ⚠️ Some models require Pro+ subscription
- ⚠️ Must enable models in GitHub Copilot settings

---

### Solution 2: Claude Code + Proxy (More Complex)
**Requires proxy server to translate between Claude Code and Copilot**

**Option 2a: Using copilot-api (Recommended Proxy)**
```bash
# Install copilot-api proxy
npm install -g copilot-api

# Start with Claude Code wizard
copilot-api start --claude-code

# Install Claude Code
npm install -g @zed-industries/claude-code-acp

# Use with Crucible
npx @zed-industries/claude-code-acp
```

**Option 2b: Using LiteLLM Proxy**
```bash
# Install LiteLLM
pip install 'litellm[proxy]'

# Create config.yaml (see below)

# Start proxy
litellm --config config.yaml

# Configure Claude Code settings.json (see below)

# Use with Crucible
npx @zed-industries/claude-code-acp
```

**Pros:**
- ✅ Uses Claude Code interface (if you prefer it)
- ✅ Leverages Copilot subscription

**Cons:**
- ❌ Requires running proxy server
- ❌ Tool calling doesn't work properly
- ❌ More complex setup
- ❌ System messages can cause errors

---

## 📋 Detailed Setup Instructions

### Solution 1: OpenCode with GitHub Copilot (RECOMMENDED)

#### Step 1: Install OpenCode

```bash
npm install -g @opencode-ai/opencode
```

#### Step 2: Authenticate with GitHub Copilot

```bash
opencode auth login
```

This will:
1. Show you a list of supported providers
2. Select **"GitHub Copilot"**
3. Open browser for GitHub authentication
4. Grant permissions to OpenCode

Your credentials are stored in `~/.local/share/opencode/auth.json`

#### Step 3: Configure Provider (Optional)

If you need custom configuration, edit your OpenCode config:

**Location:** `~/.config/opencode/opencode.json` or project-specific `opencode.json`

```json
{
  "$schema": "https://opencode.ai/config.json",
  "provider": {
    "copilot": {
      "models": {
        "claude-sonnet-4.5": {
          "name": "Claude Sonnet 4.5 via Copilot"
        },
        "gpt-4o": {
          "name": "GPT-4o via Copilot"
        }
      }
    }
  }
}
```

**Note:** Some models require manual enablement in GitHub Copilot settings at https://github.com/settings/copilot

#### Step 4: Verify Crucible Recognizes OpenCode

Crucible automatically detects OpenCode as an ACP agent:

```rust
// From crucible-cli/src/acp/agent.rs
const KNOWN_AGENTS: &[(&str, &str, &[&str])] = &[
    ("opencode", "opencode", &["acp"]),
    // ...
];
```

#### Step 5: Use with Crucible

```bash
# Set your knowledge base path
export CRUCIBLE_KILN_PATH=/path/to/your/knowledge/base

# Start Crucible with chat (uses OpenCode automatically)
cru chat

# Or specify OpenCode explicitly if you have multiple agents
# (Crucible will auto-detect if opencode is installed)
```

Crucible will:
1. Discover OpenCode via parallel probing
2. Start OpenCode in ACP mode
3. Expose Crucible's MCP tools (search, notes, etc.)
4. Route LLM requests through your GitHub Copilot subscription

---

### Solution 2: Claude Code with copilot-api Proxy

This approach uses a proxy server to translate between Claude Code's Anthropic API format and GitHub Copilot's API.

#### Step 1: Install copilot-api

```bash
npm install -g copilot-api
```

#### Step 2: Start Proxy with Claude Code Wizard

```bash
copilot-api start --claude-code
```

This wizard will:
1. Authenticate with GitHub (OAuth)
2. Configure Claude Code settings automatically
3. Start the proxy server on `http://localhost:4141`

**Manual Configuration Alternative:**

If you want to configure manually, edit `~/.claude/settings.json`:

```json
{
  "env": {
    "ANTHROPIC_BASE_URL": "http://localhost:4141",
    "ANTHROPIC_AUTH_TOKEN": "sk-dummy",
    "ANTHROPIC_MODEL": "claude-sonnet-4.5",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "gpt-4o-mini"
  }
}
```

#### Step 3: Start copilot-api Proxy (if not using wizard)

```bash
# Basic start
copilot-api start

# With rate limiting (recommended)
copilot-api start --rate-limit 30 --wait

# Declare account type for better performance
copilot-api start --account-type business --rate-limit 30
```

Account types: `personal`, `business`, `enterprise`

#### Step 4: Install Claude Code

```bash
npm install -g @zed-industries/claude-code-acp
```

#### Step 5: Use with Crucible

```bash
# Set your knowledge base path
export CRUCIBLE_KILN_PATH=/path/to/your/knowledge/base

# Start Crucible chat (it will detect Claude Code)
cru chat

# Or use Claude Code directly
npx @zed-industries/claude-code-acp
```

#### Step 6: Verify Setup

Test the proxy is working:

```bash
# Test with curl
curl -X POST http://localhost:4141/v1/messages \
  -H "Content-Type: application/json" \
  -H "x-api-key: sk-dummy" \
  -d '{
    "model": "claude-sonnet-4.5",
    "max_tokens": 1024,
    "messages": [{"role": "user", "content": "Hello!"}]
  }'
```

---

### Solution 2b: Claude Code with LiteLLM Proxy

#### Step 1: Install LiteLLM

```bash
pip install 'litellm[proxy]'
```

#### Step 2: Create LiteLLM Configuration

Create `litellm-config.yaml`:

```yaml
model_list:
  - model_name: claude-sonnet-4.5
    litellm_params:
      model: github_copilot/claude-sonnet-4.5
      api_base: https://api.githubcopilot.com
      custom_llm_provider: github_copilot

  - model_name: gpt-4o
    litellm_params:
      model: github_copilot/gpt-4o
      api_base: https://api.githubcopilot.com
      custom_llm_provider: github_copilot

litellm_settings:
  drop_params: true
  success_callback: []
  failure_callback: []
```

#### Step 3: Start LiteLLM Proxy

```bash
litellm --config litellm-config.yaml --port 4000
```

#### Step 4: Configure Claude Code

Edit `~/.claude/settings.json`:

```json
{
  "env": {
    "ANTHROPIC_BASE_URL": "http://localhost:4000",
    "ANTHROPIC_AUTH_TOKEN": "sk-dummy",
    "ANTHROPIC_MODEL": "claude-sonnet-4.5"
  }
}
```

#### Step 5: Use with Crucible

Same as copilot-api setup - Crucible will detect Claude Code automatically.

---

## 🔧 Crucible Integration Details

### How Crucible Discovers ACP Agents

From `crucible-cli/src/acp/agent.rs`:

```rust
const KNOWN_AGENTS: &[(&str, &str, &[&str])] = &[
    ("opencode", "opencode", &["acp"]),
    ("claude", "npx", &["@zed-industries/claude-code-acp"]),
    ("gemini", "gemini-cli", &[]),
    ("codex", "npx", &["@zed-industries/codex-acp"]),
];
```

Crucible performs **parallel probing** to discover which agents are installed:

1. Checks all known agents concurrently
2. Caches the first discovered agent
3. Uses cached agent for subsequent sessions

### Forcing a Specific Agent

If you have multiple ACP agents installed and want to use a specific one:

```bash
# Set preferred agent via environment or config
# (Check crucible-cli documentation for exact flag)
cru chat --agent opencode
```

### What Crucible Exposes to ACP Agents

When an ACP agent connects to Crucible, it gets access to:

**MCP Tools (12 total):**

**Note Tools (6):**
- `create_note` - Create markdown notes with frontmatter
- `read_note` - Read note content
- `read_metadata` - Get metadata without full content
- `update_note` - Update notes and frontmatter
- `delete_note` - Remove notes
- `list_notes` - List directory contents

**Search Tools (3):**
- `semantic_search` - Find semantically similar content via embeddings
- `text_search` - Full-text search
- `property_search` - Search by frontmatter properties/tags

**Kiln Tools (3):**
- `get_kiln_info` - Vault path and statistics
- `get_kiln_roots` - Root directory information
- `get_kiln_stats` - Detailed statistics

### Context Flow

```
User Query
    ↓
ACP Agent (OpenCode/Claude Code)
    ↓
[Uses GitHub Copilot for LLM]
    ↓
Calls Crucible MCP Tools
    ↓
Crucible searches knowledge base
    ↓
Returns context to agent
    ↓
Agent synthesizes with Copilot LLM
    ↓
Response to user
```

---

## 🔒 IP Security Considerations

### Why This Maintains IP Security

**With OpenCode + Copilot:**
1. ✅ Your knowledge base files never leave your machine
2. ✅ LLM calls go through GitHub Copilot (your approved provider)
3. ✅ No third-party LLM services involved
4. ✅ OpenCode runs locally, communicates via ACP
5. ✅ Crucible stays on your infrastructure

**With Claude Code + Proxy:**
1. ✅ Proxy runs locally (localhost only)
2. ✅ Translates Claude API format to Copilot API
3. ✅ Knowledge base stays local
4. ⚠️ Some metadata/prompts sent to Copilot API
5. ✅ No Anthropic API calls (uses Copilot instead)

### Data Flow Diagram

```
┌─────────────────────────────────────────────────┐
│ Your Machine (Local)                            │
│                                                 │
│  ┌──────────────┐      ┌──────────────────┐   │
│  │ Crucible     │◄────►│ ACP Agent        │   │
│  │ Knowledge    │      │ (OpenCode/       │   │
│  │ Base         │      │  Claude Code)    │   │
│  └──────────────┘      └──────────┬───────┘   │
│                                    │            │
│                         ┌──────────▼───────┐   │
│                         │ Proxy (optional) │   │
│                         │ copilot-api /    │   │
│                         │ LiteLLM          │   │
│                         └──────────┬───────┘   │
└─────────────────────────────────────┼──────────┘
                                      │
                                      │ HTTPS
                                      ▼
                          ┌────────────────────┐
                          │ GitHub Copilot API │
                          │ (Your Subscription)│
                          └────────────────────┘
```

**Key Security Points:**
- 🔒 Knowledge base **never uploaded** to GitHub
- 🔒 Only LLM prompts/responses go through Copilot API
- 🔒 All processing happens locally
- 🔒 No third-party LLM providers involved

---

## 🚨 Known Limitations

### OpenCode Limitations
- Some models require GitHub Copilot Pro+ subscription
- Models must be manually enabled in GitHub settings
- Rate limits apply based on your Copilot plan

### Claude Code + Proxy Limitations
- **Tool calling doesn't work properly** (Claude Code tries web search, fails)
- System messages can cause Internal Server Errors
- More complex setup (requires proxy server running)
- Proxy must stay running for Claude Code to work

### General Limitations
- You're still subject to GitHub Copilot's rate limits
- Enterprise plans may have different model access
- Some advanced Copilot features may not work through proxies

---

## 🔍 Troubleshooting

### OpenCode: "No compatible agent found"

**Problem:** Crucible can't find OpenCode

**Solutions:**
1. Verify installation: `opencode --version`
2. Check PATH: `which opencode`
3. Reinstall: `npm install -g @opencode-ai/opencode`
4. Check Crucible agent discovery logs

### OpenCode: "Model not available"

**Problem:** Specific Copilot model isn't accessible

**Solutions:**
1. Check your Copilot subscription (Pro/Business/Enterprise)
2. Enable model at https://github.com/settings/copilot
3. Verify authentication: `opencode auth login`
4. Check `~/.local/share/opencode/auth.json` exists

### Claude Code: "Connection refused" or "API error"

**Problem:** Proxy isn't running or misconfigured

**Solutions:**
1. Verify proxy is running: `curl http://localhost:4141/health`
2. Check `~/.claude/settings.json` has correct URL
3. Restart proxy: `copilot-api start`
4. Check proxy logs for errors
5. Verify GitHub authentication

### Claude Code: "Tool calling failed"

**Problem:** Claude Code tries to use tools that don't work with Copilot

**Expected behavior:** This is a known limitation with Claude Code + Copilot

**Workaround:** Use OpenCode instead (has native tool support)

### Crucible: "Agent timeout" or "Connection lost"

**Problem:** ACP connection dropped

**Solutions:**
1. Check agent process is still running
2. Restart Crucible: `cru chat`
3. Clear agent cache (restart Crucible)
4. Check network/firewall (if using remote setup)

---

## 📊 Comparison Matrix

| Feature | OpenCode + Copilot | Claude Code + copilot-api | Claude Code + LiteLLM |
|---------|-------------------|---------------------------|----------------------|
| **Ease of Setup** | ⭐⭐⭐⭐⭐ Easy | ⭐⭐⭐ Moderate | ⭐⭐ Complex |
| **Tool Calling** | ✅ Works | ❌ Broken | ❌ Broken |
| **Proxy Required** | ❌ No | ✅ Yes | ✅ Yes |
| **IP Security** | ✅ Excellent | ✅ Good | ✅ Good |
| **Maintenance** | ⭐⭐⭐⭐⭐ Low | ⭐⭐⭐ Medium | ⭐⭐ High |
| **Rate Limits** | GitHub Copilot limits | GitHub Copilot limits | GitHub Copilot limits |
| **Model Access** | All Copilot models | All Copilot models | All Copilot models |
| **Crucible Integration** | ✅ Native ACP | ✅ Native ACP | ✅ Native ACP |

**Recommendation:** Use **OpenCode + Copilot** unless you specifically need Claude Code's interface.

---

## 🎯 Quick Start Checklist

### For OpenCode (Recommended):
- [ ] Install OpenCode: `npm install -g @opencode-ai/opencode`
- [ ] Authenticate: `opencode auth login` → Select "GitHub Copilot"
- [ ] Test: `opencode --version`
- [ ] Set Crucible path: `export CRUCIBLE_KILN_PATH=/path/to/vault`
- [ ] Start Crucible: `cru chat`
- [ ] Ask OpenCode to search your knowledge base

### For Claude Code + Proxy:
- [ ] Install copilot-api: `npm install -g copilot-api`
- [ ] Run wizard: `copilot-api start --claude-code`
- [ ] Install Claude Code: `npm install -g @zed-industries/claude-code-acp`
- [ ] Test proxy: `curl http://localhost:4141/health`
- [ ] Set Crucible path: `export CRUCIBLE_KILN_PATH=/path/to/vault`
- [ ] Start Crucible: `cru chat`
- [ ] Note: Tool calling won't work, but basic chat will

---

## 📚 Additional Resources

### Official Documentation
- [OpenCode Providers Documentation](https://opencode.ai/docs/providers/)
- [OpenCode Models Documentation](https://opencode.ai/docs/models/)
- [GitHub Copilot BYOK Documentation](https://docs.github.com/en/copilot/how-tos/administer-copilot/manage-for-enterprise/use-your-own-api-keys)

### Proxy Projects
- [copilot-api GitHub Repository](https://github.com/ericc-ch/copilot-api)
- [LiteLLM GitHub Copilot Tutorial](https://docs.litellm.ai/docs/tutorials/github_copilot_integration)

### Guides
- [How to use GitHub Copilot LLM on OpenCode](https://aiengineerguide.com/blog/github-copilot-llm-on-opencode/)
- [Using Claude Code with GitHub Copilot Subscription](https://dev.to/allentcm/using-claude-code-with-github-copilot-subscription-2obj)

### Crucible Code References
- `crates/crucible-cli/src/acp/agent.rs` - Agent discovery
- `crates/crucible-acp/` - ACP protocol implementation
- `crates/crucible-tools/src/mcp_server.rs` - MCP tools

---

## 🔄 Next Steps

1. **Choose your approach** (OpenCode recommended)
2. **Set up authentication** with GitHub Copilot
3. **Test basic connectivity** with the agent
4. **Configure Crucible** knowledge base path
5. **Start chatting** with your knowledge base through Copilot!

**Happy vibing with GitHub Copilot + Crucible! 🔥**
