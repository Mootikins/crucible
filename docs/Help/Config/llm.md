---
description: Configure language model providers for chat and agents
tags:
  - reference
  - config
---

# LLM Configuration

Configure language model providers for the chat interface and agents.

## Configuration File

Add to `~/.config/crucible/config.toml`:

```toml
[llm]
provider = "ollama"
model = "llama3.2"
endpoint = "http://localhost:11434"
temperature = 0.7
```

## Providers

### Ollama (Local)

Run models locally with Ollama:

```toml
[llm]
provider = "ollama"
model = "llama3.2"
endpoint = "http://localhost:11434"
```

**Available models:**
- `llama3.2` - Llama 3.2 (8B)
- `qwen2.5` - Qwen 2.5
- `mistral` - Mistral 7B
- `codellama` - Code Llama

**Setup:**
```bash
# Install Ollama
curl -fsSL https://ollama.com/install.sh | sh

# Pull a model
ollama pull llama3.2

# Verify it's running
ollama list
```

### OpenAI

Use OpenAI's API:

```toml
[llm]
provider = "openai"
model = "gpt-4o"
```

**Environment variable:**
```bash
export OPENAI_API_KEY=your-api-key
```

**Available models:**
- `gpt-4o` - GPT-4o (recommended)
- `gpt-4o-mini` - Smaller, faster
- `gpt-4-turbo` - GPT-4 Turbo

### Anthropic

Use Anthropic's Claude models:

```toml
[llm]
provider = "anthropic"
model = "claude-sonnet-4-5-20250929"
```

**Environment variable:**
```bash
export ANTHROPIC_API_KEY=your-api-key
```

**Available models:**
- `claude-sonnet-4-5-20250929` - Claude Sonnet 4.5
- `claude-haiku-4-5-20251001` - Fast, efficient

## Parameters

### temperature

Controls randomness in responses (0.0 - 2.0):

```toml
[llm]
temperature = 0.7
```

- `0.0` - Deterministic, focused
- `0.7` - Balanced (default)
- `1.0+` - More creative, varied

### max_tokens

Maximum tokens in response:

```toml
[llm]
max_tokens = 4096
```

### endpoint

Custom API endpoint:

```toml
[llm]
endpoint = "http://localhost:11434"  # Ollama
# endpoint = "https://api.openai.com/v1"  # OpenAI
```

## Multiple Providers

Configure different providers for different uses:

```toml
# Default for chat
[llm]
provider = "ollama"
model = "llama3.2"

# Override via command line
# cru chat --provider openai --model gpt-4o
```

## CLI Override

Override configuration from command line:

```bash
# Use different provider
cru chat --internal --provider openai

# Use different model
cru chat --internal --provider ollama --model codellama

# Combine options
cru chat --internal --provider openai --model gpt-4o "Explain this code"
```

## Environment Variables

| Variable | Purpose |
|----------|---------|
| `OPENAI_API_KEY` | OpenAI API key |
| `ANTHROPIC_API_KEY` | Anthropic API key |
| `OLLAMA_HOST` | Ollama endpoint (default: localhost:11434) |

## Example Configurations

### Local Development

```toml
[llm]
provider = "ollama"
model = "llama3.2"
temperature = 0.7
```

### Production with OpenAI

```toml
[llm]
provider = "openai"
model = "gpt-4o"
max_tokens = 4096
```

### Cost-Conscious

```toml
[llm]
provider = "openai"
model = "gpt-4o-mini"  # Cheaper
temperature = 0.5       # More focused
max_tokens = 2048       # Limit response length
```

## Troubleshooting

### "Connection refused" with Ollama

Check Ollama is running:
```bash
ollama list
```

Start if needed:
```bash
ollama serve
```

### "Invalid API key" with OpenAI/Anthropic

Verify environment variable:
```bash
echo $OPENAI_API_KEY
```

### Model not found

For Ollama, pull the model first:
```bash
ollama pull llama3.2
```

## Implementation

**Source code:** `crates/crucible-llm/src/`

**Configuration parsing:** `crates/crucible-cli/src/config.rs`

## See Also

- `:h config.embedding` - Embedding configuration
- `:h chat` - Chat command reference
- [[Help/CLI/chat]] - Chat usage guide
