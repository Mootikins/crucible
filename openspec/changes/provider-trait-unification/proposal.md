# Provider Trait Unification

**Status:** Draft
**Date:** 2025-12-14
**Scope:** crucible-core, crucible-llm, crucible-config

---

## Summary

Migrate all LLM/embedding providers to the unified trait system (`Provider` + capability extensions), enabling clean feature-gating of backends like fastembed, llama-cpp, burn, etc.

## Current State

### Trait Landscape

| Trait | Location | Purpose |
|-------|----------|---------|
| `EmbeddingProvider` | crucible-llm | Legacy embedding trait |
| `TextGenerationProvider` | crucible-llm | Legacy text gen trait |
| `Provider` | crucible-core | **New** unified base trait |
| `CanEmbed` | crucible-core | **New** embedding extension |
| `CanChat` | crucible-core | **New** chat extension |
| `CanConstrainGeneration` | crucible-core | **New** grammar/schema extension |

### Provider Implementation Status

| Provider | Old Traits | New Traits | Feature |
|----------|------------|------------|---------|
| FastEmbed | EmbeddingProvider ✓ | Provider ✗, CanEmbed ✗ | (none) |
| LlamaCppEmbed | EmbeddingProvider ✓ | Provider ✗, CanEmbed ✗ | llama-cpp |
| LlamaCppText | TextGenerationProvider ✓ | Provider ✓, CanConstrainGeneration ✓ | llama-cpp |
| BurnProvider | EmbeddingProvider ✓ | Provider ✗, CanEmbed ✗ | burn |
| OllamaEmbed | EmbeddingProvider ✓ | Provider ✗, CanEmbed ✗ | (none) |
| OllamaChat | TextGenerationProvider ✓ | Provider ✗, CanChat ✗ | (none) |
| OpenAIEmbed | EmbeddingProvider ✓ | Provider ✗, CanEmbed ✗ | (none) |
| OpenAIChat | TextGenerationProvider ✓ | Provider ✗, CanChat ✗ | (none) |

## Target State

```
┌─────────────────────────────────────────────────────────────┐
│                    Provider (base trait)                     │
│  name(), backend_type(), endpoint(), capabilities()         │
└─────────────────────────────────────────────────────────────┘
         │                    │                    │
         ▼                    ▼                    ▼
   ┌───────────┐       ┌───────────┐       ┌─────────────────┐
   │ CanEmbed  │       │ CanChat   │       │CanConstrain     │
   │           │       │           │       │Generation       │
   └───────────┘       └───────────┘       └─────────────────┘
         │                    │                    │
         ▼                    ▼                    ▼
   ┌───────────┐       ┌───────────┐       ┌─────────────────┐
   │ FastEmbed │       │ Ollama    │       │ LlamaCpp        │
   │ LlamaCpp  │       │ OpenAI    │       │ (future: OpenAI │
   │ Burn      │       │ Anthropic │       │  via JSON Schema│
   │ Ollama    │       │           │       │                 │
   │ OpenAI    │       │           │       │                 │
   └───────────┘       └───────────┘       └─────────────────┘
```

## Implementation Plan

### Phase 1: Adapter Layer (Non-Breaking)

Create adapters that wrap old trait implementations with new traits.

**File:** `crates/crucible-llm/src/unified/adapters.rs`

```rust
/// Wraps EmbeddingProvider to implement Provider + CanEmbed
pub struct EmbeddingProviderAdapter<P: EmbeddingProvider> {
    inner: P,
    backend_type: BackendType,
}

impl<P: EmbeddingProvider> Provider for EmbeddingProviderAdapter<P> { ... }
impl<P: EmbeddingProvider> CanEmbed for EmbeddingProviderAdapter<P> { ... }
```

This allows immediate use of new traits without modifying existing providers.

### Phase 2: Native Implementations

Migrate each provider to implement new traits directly.

#### 2.1 FastEmbed (crucible-llm/src/embeddings/fastembed.rs)

```rust
#[async_trait]
impl Provider for FastEmbedProvider {
    fn name(&self) -> &str { "fastembed" }
    fn backend_type(&self) -> BackendType { BackendType::FastEmbed }
    fn endpoint(&self) -> Option<&str> { None }
    fn capabilities(&self) -> ExtendedCapabilities {
        ExtendedCapabilities::embedding_only(self.dimensions())
    }
    async fn health_check(&self) -> LlmResult<bool> { Ok(true) }
}

#[async_trait]
impl CanEmbed for FastEmbedProvider {
    async fn embed(&self, text: &str) -> LlmResult<EmbeddingResponse> { ... }
    fn embedding_dimensions(&self) -> usize { self.dimensions }
    fn embedding_model(&self) -> &str { &self.model_name }
}
```

#### 2.2 Ollama (crucible-llm/src/embeddings/ollama.rs + chat/ollama.rs)

Ollama supports both embeddings and chat, so it implements multiple traits:

```rust
impl Provider for OllamaProvider { ... }
impl CanEmbed for OllamaProvider { ... }  // If embedding model configured
impl CanChat for OllamaProvider { ... }   // If chat model configured
```

#### 2.3 OpenAI (crucible-llm/src/embeddings/openai.rs + chat/openai.rs)

Similar to Ollama - supports embeddings, chat, and structured output.

### Phase 3: Feature Gating

Once providers implement new traits natively:

**File:** `crates/crucible-llm/Cargo.toml`

```toml
[features]
default = ["ollama"]  # Ollama is lightweight (just HTTP)

# Embedding backends
fastembed = ["dep:fastembed"]
burn = ["dep:burn", "dep:burn-tensor", ...]
llama-cpp-embed = ["llama-cpp"]

# Text generation backends
llama-cpp = ["dep:llama-cpp-2"]
ollama = []   # No extra deps, just reqwest
openai = []   # No extra deps, just reqwest

# Bundles
local-inference = ["fastembed", "llama-cpp"]
cloud-inference = ["ollama", "openai"]
full = ["local-inference", "cloud-inference"]
```

**Conditional exports in lib.rs:**

```rust
#[cfg(feature = "fastembed")]
pub mod fastembed;

#[cfg(feature = "fastembed")]
pub use fastembed::FastEmbedProvider;
```

### Phase 4: Deprecation

1. Mark old traits with `#[deprecated]`
2. Update all internal usage to new traits
3. Remove old traits in next major version

## Migration Order

1. **FastEmbed** - Simple, CPU-only, good test case
2. **LlamaCpp Embed** - Already has llama-cpp text done
3. **Burn** - Similar to FastEmbed
4. **Ollama** - Multi-capability provider
5. **OpenAI** - Multi-capability + CanConstrainGeneration (JSON Schema)

## Testing Strategy

Each migration includes:
1. Unit tests for trait implementations
2. Integration test with MockConstrainedProvider pattern
3. Feature-gate compilation tests (ensure crate compiles with each feature combo)

```rust
#[test]
fn test_fastembed_implements_provider() {
    fn assert_provider<T: Provider>() {}
    assert_provider::<FastEmbedProvider>();
}

#[test]
fn test_fastembed_implements_can_embed() {
    fn assert_can_embed<T: CanEmbed>() {}
    assert_can_embed::<FastEmbedProvider>();
}
```

## Verification

```bash
# Each feature compiles independently
cargo check -p crucible-llm --no-default-features --features fastembed
cargo check -p crucible-llm --no-default-features --features llama-cpp
cargo check -p crucible-llm --no-default-features --features ollama

# Full build
cargo check -p crucible-llm --all-features

# Tests pass
cargo test -p crucible-llm --all-features
```

## Files to Modify

| File | Changes |
|------|---------|
| `crucible-llm/Cargo.toml` | Add feature flags |
| `crucible-llm/src/lib.rs` | Conditional exports |
| `crucible-llm/src/embeddings/fastembed.rs` | Implement Provider + CanEmbed |
| `crucible-llm/src/embeddings/ollama.rs` | Implement Provider + CanEmbed |
| `crucible-llm/src/embeddings/openai.rs` | Implement Provider + CanEmbed |
| `crucible-llm/src/embeddings/burn.rs` | Implement Provider + CanEmbed |
| `crucible-llm/src/embeddings/llama_cpp_backend.rs` | Implement Provider + CanEmbed |
| `crucible-llm/src/chat/ollama.rs` | Implement Provider + CanChat |
| `crucible-llm/src/chat/openai.rs` | Implement Provider + CanChat + CanConstrainGeneration |
| `crucible-llm/src/unified/adapters.rs` | Adapter implementations |

## Risks

1. **Breaking change for external users** - Mitigated by adapter layer
2. **Feature flag complexity** - Keep bundles simple (local, cloud, full)
3. **Compile time increase** - Feature flags actually reduce it for most users
