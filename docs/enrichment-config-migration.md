# Enrichment Configuration Migration Guide

## Overview

The embedding configuration has been refactored and moved from `crucible-config` to `crucible-core` with a new name: `EnrichmentConfig`. This change provides better type safety, clearer defaults, and improved separation of concerns.

## What Changed?

### Old Structure (`crucible-config`)

The old configuration used a generic structure with a type discriminator and options HashMap:

```rust
use crucible_config::{EmbeddingProviderConfig, EmbeddingProviderType, ApiConfig, ModelConfig};

let config = EmbeddingProviderConfig {
    provider_type: EmbeddingProviderType::OpenAI,
    api: ApiConfig {
        key: Some("sk-...".to_string()),
        base_url: Some("https://api.openai.com/v1".to_string()),
        timeout_seconds: Some(30),
        retry_attempts: Some(3),
        headers: HashMap::new(),
    },
    model: ModelConfig {
        name: "text-embedding-3-small".to_string(),
        dimensions: Some(1536),
        max_tokens: None,
    },
    options: HashMap::new(),
};
```

### New Structure (`crucible-core`)

The new configuration uses type-safe provider-specific enums:

```rust
use crucible_core::enrichment::{
    EnrichmentConfig,
    EmbeddingProviderConfig,
    OpenAIConfig,
    PipelineConfig,
};

let config = EnrichmentConfig {
    provider: EmbeddingProviderConfig::OpenAI(OpenAIConfig {
        api_key: "sk-...".to_string(),
        model: "text-embedding-3-small".to_string(),
        base_url: "https://api.openai.com/v1".to_string(),
        timeout_seconds: 30,
        retry_attempts: 3,
        dimensions: 1536,
        headers: HashMap::new(),
    }),
    pipeline: PipelineConfig::default(),
};
```

## Benefits

1. **Type Safety**: Each provider has its own struct with specific fields
2. **Clear Defaults**: No more guessing - defaults are explicit
3. **Better Documentation**: Each provider config is self-documenting
4. **Separation of Concerns**: Provider config separate from pipeline config
5. **Easier Validation**: Provider-specific validation rules

## Provider Examples

### OpenAI

```rust
use crucible_core::enrichment::{EnrichmentConfig, EmbeddingProviderConfig, OpenAIConfig};

let config = EnrichmentConfig {
    provider: EmbeddingProviderConfig::OpenAI(OpenAIConfig {
        api_key: env::var("OPENAI_API_KEY")?,
        ..Default::default()  // Uses sensible defaults
    }),
    pipeline: Default::default(),
};
```

### Ollama (Local)

```rust
use crucible_core::enrichment::{EnrichmentConfig, EmbeddingProviderConfig, OllamaConfig};

let config = EnrichmentConfig {
    provider: EmbeddingProviderConfig::Ollama(OllamaConfig {
        model: "nomic-embed-text".to_string(),
        base_url: "http://localhost:11434".to_string(),
        ..Default::default()
    }),
    pipeline: Default::default(),
};
```

### FastEmbed (Local, Privacy-Focused)

```rust
use crucible_core::enrichment::{EnrichmentConfig, EmbeddingProviderConfig, FastEmbedConfig};

let config = EnrichmentConfig {
    provider: EmbeddingProviderConfig::FastEmbed(FastEmbedConfig {
        model: "BAAI/bge-small-en-v1.5".to_string(),
        cache_dir: Some("/tmp/fastembed".to_string()),
        batch_size: 32,
        ..Default::default()
    }),
    pipeline: Default::default(),
};
```

### Cohere

```rust
use crucible_core::enrichment::{EnrichmentConfig, EmbeddingProviderConfig, CohereConfig};

let config = EnrichmentConfig {
    provider: EmbeddingProviderConfig::Cohere(CohereConfig {
        api_key: env::var("COHERE_API_KEY")?,
        input_type: "search_document".to_string(),
        ..Default::default()
    }),
    pipeline: Default::default(),
};
```

### Vertex AI

```rust
use crucible_core::enrichment::{EnrichmentConfig, EmbeddingProviderConfig, VertexAIConfig};

let config = EnrichmentConfig {
    provider: EmbeddingProviderConfig::VertexAI(VertexAIConfig {
        project_id: "my-project".to_string(),
        credentials_path: Some("/path/to/credentials.json".to_string()),
        ..Default::default()
    }),
    pipeline: Default::default(),
};
```

### Custom Provider

```rust
use crucible_core::enrichment::{EnrichmentConfig, EmbeddingProviderConfig, CustomConfig};

let config = EnrichmentConfig {
    provider: EmbeddingProviderConfig::Custom(CustomConfig {
        base_url: "https://my-embedding-api.com".to_string(),
        api_key: Some("my-key".to_string()),
        model: "custom-model".to_string(),
        dimensions: 1024,
        ..Default::default()
    }),
    pipeline: Default::default(),
};
```

## Pipeline Configuration

The new `EnrichmentConfig` includes pipeline settings for controlling processing behavior:

```rust
use crucible_core::enrichment::{EnrichmentConfig, PipelineConfig};

let config = EnrichmentConfig {
    provider: /* ... */,
    pipeline: PipelineConfig {
        worker_count: 4,           // Parallel workers
        batch_size: 32,            // Documents per batch
        max_queue_size: 1000,      // Max queued items
        timeout_ms: 30000,         // Operation timeout
        retry_attempts: 3,         // Retry on failure
        retry_delay_ms: 1000,      // Delay between retries
        circuit_breaker_threshold: 5,      // Failures before circuit opens
        circuit_breaker_timeout_ms: 60000, // Circuit breaker timeout
    },
};
```

### Pipeline Optimization Presets

```rust
use crucible_core::enrichment::PipelineConfig;

// High throughput (batch processing)
let throughput = PipelineConfig::optimize_for_throughput();

// Low latency (real-time)
let latency = PipelineConfig::optimize_for_latency();

// Low resource usage
let resources = PipelineConfig::optimize_for_resources();
```

## Migration Strategies

### Strategy 1: Automatic Conversion

Use the conversion utilities in `crucible-llm`:

```rust
use crucible_config::EmbeddingProviderConfig as OldConfig;
use crucible_llm::convert_to_enrichment_config;

// Load old config from file
let old_config: OldConfig = load_from_file()?;

// Convert to new format
let new_config = convert_to_enrichment_config(&old_config);

// Use new config
use_enrichment_config(&new_config)?;
```

### Strategy 2: Manual Migration

Gradually migrate by creating new configs alongside old ones:

```rust
// Step 1: Keep old config working
let old = EmbeddingProviderConfig::openai(api_key, None);

// Step 2: Create equivalent new config
let new = EnrichmentConfig {
    provider: EmbeddingProviderConfig::OpenAI(OpenAIConfig {
        api_key,
        ..Default::default()
    }),
    pipeline: Default::default(),
};

// Step 3: Test new config
// Step 4: Remove old config references
```

### Strategy 3: Feature Flags

Use conditional compilation during migration:

```rust
#[cfg(feature = "legacy-config")]
use crucible_config::EmbeddingProviderConfig;

#[cfg(not(feature = "legacy-config"))]
use crucible_core::enrichment::EnrichmentConfig;
```

## Configuration File Format

### Old Format (TOML)

```toml
[embedding]
type = "openai"

[embedding.api]
key = "sk-..."
base_url = "https://api.openai.com/v1"
timeout_seconds = 30
retry_attempts = 3

[embedding.model]
name = "text-embedding-3-small"
dimensions = 1536
```

### New Format (TOML)

```toml
[enrichment.provider]
type = "openai"
api_key = "sk-..."
model = "text-embedding-3-small"
base_url = "https://api.openai.com/v1"
timeout_seconds = 30
retry_attempts = 3
dimensions = 1536

[enrichment.pipeline]
worker_count = 4
batch_size = 16
max_queue_size = 1000
timeout_ms = 30000
retry_attempts = 3
```

### New Format (JSON)

```json
{
  "enrichment": {
    "provider": {
      "type": "openai",
      "api_key": "sk-...",
      "model": "text-embedding-3-small",
      "base_url": "https://api.openai.com/v1",
      "timeout_seconds": 30,
      "retry_attempts": 3,
      "dimensions": 1536
    },
    "pipeline": {
      "worker_count": 4,
      "batch_size": 16,
      "max_queue_size": 1000,
      "timeout_ms": 30000,
      "retry_attempts": 3
    }
  }
}
```

## Validation

The new config has built-in validation:

```rust
use crucible_core::enrichment::EmbeddingProviderConfig;

let config = /* ... */;

// Validate provider config
if let Err(e) = config.provider.validate() {
    eprintln!("Invalid configuration: {}", e);
}

// Helper methods
let timeout = config.provider.timeout();      // Returns Duration
let model = config.provider.model();          // Returns &str
let dims = config.provider.dimensions();      // Returns Option<u32>
let retries = config.provider.retry_attempts(); // Returns u32
```

## Backwards Compatibility

The old `crucible-config::EmbeddingProviderConfig` is marked as deprecated but still functional. It will be maintained for compatibility during the transition period.

### Timeline

- **v0.2.0**: New config introduced, old config deprecated
- **v0.3.0**: Migration complete, old config warnings
- **v0.4.0**: Old config removed

## Common Issues

### Issue: "Cannot find type EnrichmentConfig"

**Solution**: Update imports from `crucible-config` to `crucible-core::enrichment`

### Issue: "Field not found on provider config"

**Solution**: Provider-specific fields are now in the enum variant structs:

```rust
// Old: config.api.key
// New: match config.provider {
//    EmbeddingProviderConfig::OpenAI(cfg) => cfg.api_key,
//    ...
// }
```

### Issue: "Options HashMap not found"

**Solution**: Provider-specific options are now dedicated fields:

```rust
// Old: config.options.get("cache_dir")
// New: match config.provider {
//    EmbeddingProviderConfig::FastEmbed(cfg) => cfg.cache_dir,
//    ...
// }
```

## Getting Help

- Check module documentation: `crucible_core::enrichment::config`
- See examples in `crucible-llm/src/config_conversion.rs`
- File issues on GitHub with the `config-migration` label

## Summary

The new `EnrichmentConfig` provides a modern, type-safe configuration system that's easier to use and understand. The migration path is straightforward, with automatic conversion utilities available for legacy configs.

**Key Takeaways:**
- Use `crucible_core::enrichment::EnrichmentConfig` for new code
- Provider-specific configs are now type-safe enum variants
- Pipeline configuration is separate and explicit
- Conversion utilities available in `crucible-llm`
- Old config will be maintained temporarily for compatibility
