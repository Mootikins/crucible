# Internal Agent System - Design

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                        ChatSession                          │
│                    (existing orchestrator)                  │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
              ┌───────────────────────────────┐
              │      AgentHandle trait        │
              │   (existing in crucible-core) │
              └───────────────────────────────┘
                     │                │
         ┌───────────┘                └───────────┐
         ▼                                        ▼
┌─────────────────────┐                ┌─────────────────────┐
│ CrucibleAcpClient   │                │ CrucibleAgentHandle │
│ (existing - ACP)    │                │ (NEW - internal)    │
└─────────────────────┘                └─────────────────────┘
                                                  │
                              ┌───────────────────┼───────────────────┐
                              ▼                   ▼                   ▼
                       ┌────────────┐    ┌──────────────┐    ┌────────────────┐
                       │ AgentCard  │    │LlmProvider   │    │ContextStrategy │
                       │(system     │    │(API calls)   │    │(token mgmt)    │
                       │ prompt)    │    └──────────────┘    └────────────────┘
                       └────────────┘           │
                                    ┌───────────┴───────────┐
                                    ▼                       ▼
                           ┌──────────────┐        ┌──────────────────┐
                           │OllamaProvider│        │OpenAiCompatible  │
                           └──────────────┘        │Provider          │
                                                   └──────────────────┘
```

## Component Details

### 1. LlmProvider Extensions

The existing `LlmProvider` trait in `crucible-core/src/traits/llm.rs` needs one addition:

```rust
#[async_trait]
pub trait LlmProvider: Send + Sync {
    // ... existing methods ...

    /// Get the context window size for this model (in tokens)
    /// Used by ContextStrategy to decide when to compact
    fn context_window(&self) -> usize;
}
```

### 2. Provider Implementations

**Location:** `crucible-llm/src/text_generation/`

```rust
pub struct OllamaProvider {
    client: reqwest::Client,
    base_url: String,
    model: String,
}

pub struct OpenAiCompatibleProvider {
    client: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
    model: String,
}
```

Both implement `LlmProvider` trait. OpenAI-compatible works with:
- OpenAI API
- Azure OpenAI
- LiteLLM proxy
- vLLM
- Any OpenAI-compatible endpoint

### 3. Context Management

**Trait location:** `crucible-core/src/traits/context.rs`

```rust
#[async_trait]
pub trait ContextStrategy: Send + Sync {
    /// Prepare messages to fit within token budget
    fn prepare(&self, messages: &[LlmMessage], budget: ContextBudget) -> Vec<LlmMessage>;

    /// Estimate tokens for a message (heuristic)
    fn estimate_tokens(&self, message: &LlmMessage) -> usize;

    /// Check if compaction is recommended
    fn should_compact(&self, messages: &[LlmMessage], budget: ContextBudget) -> bool;
}

pub struct ContextBudget {
    pub max_tokens: usize,           // From model's context_window()
    pub reserve_for_response: usize, // Default: 4096
}
```

**Default implementation:** `SlidingWindowStrategy`
- Always keeps system prompt (pinned)
- Keeps most recent messages that fit
- Triggers at 80% of budget
- Uses 4 chars/token heuristic for estimation

**Token tracking:**
- Estimation (heuristic) for "should we compact?" decisions
- Actual usage from API responses for reporting

### 4. CrucibleAgentHandle

**Location:** `crucible-cli/src/agents/handle.rs`

```rust
pub struct CrucibleAgentHandle {
    provider: Arc<dyn LlmProvider>,
    context: Box<dyn ContextStrategy>,
    agent_card: AgentCard,
    messages: Vec<LlmMessage>,
    mode: ChatMode,
    token_usage: TokenUsage,
}

#[async_trait]
impl AgentHandle for CrucibleAgentHandle {
    async fn send_message(&mut self, message: &str) -> ChatResult<ChatResponse> {
        // 1. Add user message to history
        self.messages.push(LlmMessage::user(message));

        // 2. Check if compaction needed
        let budget = ContextBudget {
            max_tokens: self.provider.context_window(),
            reserve_for_response: 4096,
        };
        if self.context.should_compact(&self.messages, budget) {
            self.messages = self.context.prepare(&self.messages, budget);
        }

        // 3. Build request with system prompt first
        let mut request_messages = vec![
            LlmMessage::system(&self.agent_card.system_prompt)
        ];
        request_messages.extend(self.messages.clone());

        // 4. Call provider
        let response = self.provider.complete(LlmRequest::new(request_messages)).await?;

        // 5. Track actual token usage
        self.token_usage = response.usage;

        // 6. Add assistant response to history
        self.messages.push(response.message.clone());

        // 7. Convert to ChatResponse
        Ok(ChatResponse::from(response))
    }

    // ... other methods
}
```

### 5. AgentFactory

**Location:** `crucible-cli/src/agents/factory.rs`

```rust
pub struct AgentFactory {
    providers: HashMap<String, Arc<dyn LlmProvider>>,
}

impl AgentFactory {
    pub fn spawn(
        &self,
        card: &AgentCard,
        config: &AgentConfig,
    ) -> Result<CrucibleAgentHandle> {
        let provider = self.get_or_create_provider(&config.provider)?;
        let context = SlidingWindowStrategy::new(config.context_threshold);

        Ok(CrucibleAgentHandle::new(
            provider,
            card.clone(),
            Box::new(context),
        ))
    }
}
```

### 6. API Key Resolution

Priority order (first found wins):
1. Environment variable (e.g., `OPENAI_API_KEY`)
2. File reference from config (`api_key_file = "~/.config/crucible/keys/openai.key"`)
3. Plaintext in config (with warning)

```rust
fn resolve_api_key(config: &ProviderConfig) -> Result<Option<String>> {
    // 1. Env var
    if let Ok(key) = std::env::var(config.env_var_name()) {
        return Ok(Some(key));
    }
    // 2. File
    if let Some(ref path) = config.api_key_file {
        return Ok(Some(std::fs::read_to_string(path)?.trim().to_string()));
    }
    // 3. Plain (with warning)
    if let Some(ref key) = config.api_key {
        warn!("Using plaintext API key - consider env var or file");
        return Ok(Some(key.clone()));
    }
    Ok(None)
}
```

**Future TODO:** System keyring integration via `keyring` crate.

### 7. Backend Selection

At startup only (not mid-session):

```bash
cru chat                           # Config default
cru chat --agent general           # Internal + agent card
cru chat --acp claude-code         # ACP backend
cru chat --acp claude-code --agent # Error: incompatible
```

**Constraint:** ACP agents are self-contained; cannot inject agent cards into them.

### 8. Configuration

```toml
[llm]
default_provider = "ollama"

[llm.ollama]
url = "http://localhost:11434"
model = "llama3.2"

[llm.openai]
url = "https://api.openai.com/v1"
model = "gpt-4o"
# api_key from OPENAI_API_KEY env var
# or: api_key_file = "~/.config/crucible/keys/openai.key"

[chat]
default_backend = "internal"  # or "acp"
default_agent = "general"
default_acp_agent = "claude-code"
```

### 9. New Slash Commands

| Command | Description |
|---------|-------------|
| `/compact` | Force context compaction now |
| `/context` | Show token usage (estimated + actual) |
| `/agent <name>` | Switch agent card (same backend) |
| `/agent` | Show current agent info |

## Design Decisions

1. **Flat handle hierarchy** - No sub-agents in v1. Add orchestrator layer later if needed.

2. **Heuristic + actual token tracking** - Use 4 chars/token for planning, API response for reporting. Avoids tokenizer dependencies.

3. **Backend selection at startup** - Swapping ACP/internal mid-session is complex; defer to future.

4. **Snake_case config** - Matches existing config. Kebab-case migration later.

5. **No keyring in v1** - Adds platform complexity. Env/file/plain sufficient for now.
