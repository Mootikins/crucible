# Provider Trait Unification - Tasks

## Phase 1: Adapter Layer

- [ ] Create `EmbeddingProviderAdapter` in unified/adapters.rs
- [ ] Create `TextGenerationProviderAdapter` in unified/adapters.rs
- [ ] Add tests for adapters
- [ ] Update unified factory to use adapters

## Phase 2: Native Implementations

### 2.1 FastEmbed
- [ ] Implement `Provider` for `FastEmbedProvider`
- [ ] Implement `CanEmbed` for `FastEmbedProvider`
- [ ] Add feature flag `fastembed` to Cargo.toml
- [ ] Conditional export in lib.rs
- [ ] Tests

### 2.2 LlamaCpp Embed
- [ ] Implement `Provider` for `LlamaCppBackend`
- [ ] Implement `CanEmbed` for `LlamaCppBackend`
- [ ] Tests

### 2.3 Burn
- [ ] Implement `Provider` for `BurnProvider`
- [ ] Implement `CanEmbed` for `BurnProvider`
- [ ] Tests

### 2.4 Ollama
- [ ] Implement `Provider` for `OllamaProvider`
- [ ] Implement `CanEmbed` for `OllamaProvider`
- [ ] Implement `CanChat` for `OllamaChatProvider`
- [ ] Tests

### 2.5 OpenAI
- [ ] Implement `Provider` for `OpenAIProvider`
- [ ] Implement `CanEmbed` for `OpenAIProvider`
- [ ] Implement `CanChat` for `OpenAIChatProvider`
- [ ] Implement `CanConstrainGeneration` for `OpenAIChatProvider` (JSON Schema)
- [ ] Tests

## Phase 3: Feature Gating

- [ ] Update Cargo.toml with all feature flags
- [ ] Update lib.rs with conditional exports
- [ ] Create feature bundles (local, cloud, full)
- [ ] Test all feature combinations compile

## Phase 4: Deprecation

- [ ] Mark `EmbeddingProvider` trait as deprecated
- [ ] Mark `TextGenerationProvider` trait as deprecated
- [ ] Update internal usage to new traits
- [ ] Update documentation

## Verification

- [ ] `cargo check -p crucible-llm --no-default-features --features fastembed`
- [ ] `cargo check -p crucible-llm --no-default-features --features llama-cpp`
- [ ] `cargo check -p crucible-llm --no-default-features --features ollama`
- [ ] `cargo check -p crucible-llm --all-features`
- [ ] `cargo test -p crucible-llm --all-features`
