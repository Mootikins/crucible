# Burn ML Framework Testing Implementation Plan

## Executive Summary

Create a new `crucible-burn` crate with multi-purpose binary (`burn-test`) for comprehensive testing of the Burn ML framework on AMD Strix Halo hardware. The implementation will provide CLI tools, HTTP inference server, and benchmark suite for both embedding models and LLMs, with primary Vulkan support and secondary ROCm container support.

## Recommended Architecture

### Core Design Principles
- **Isolation**: Separate crate initially, designed for future integration
- **Multi-Interface**: CLI + HTTP server + benchmarks in single binary
- **Hardware-Aware**: Vulkan primary (native), ROCm secondary (container)
- **Model-Agnostic**: Support for embeddings through LLMs from ~/models directory
- **Production-Ready**: Follow existing crucible patterns and configuration

### Crate Structure
```
crates/crucible-burn/
├── Cargo.toml
├── src/
│   ├── main.rs                    # Binary entry point (burn-test)
│   ├── lib.rs                     # Library interface for integration
│   ├── cli/
│   │   ├── mod.rs                 # CLI command definitions
│   │   ├── args.rs                # Command line argument parsing
│   │   └── commands/              # Specific command implementations
│   │       ├── embed.rs           # Embedding commands
│   │       ├── llm.rs             # LLM commands
│   │       ├── server.rs          # Server commands
│   │       ├── bench.rs           # Benchmark commands
│   │       └── detect.rs          # Hardware detection
│   ├── providers/
│   │   ├── mod.rs                 # Provider factory and traits
│   │   ├── embed.rs               # Burn embedding provider
│   │   ├── llm.rs                 # Burn LLM provider
│   │   └── base.rs                # Shared provider utilities
│   ├── hardware/
│   │   ├── mod.rs                 # Hardware detection and management
│   │   ├── vulkan.rs              # Vulkan backend detection
│   │   ├── rocm.rs                # ROCm backend detection
│   │   └── auto.rs                # Automatic backend selection
│   ├── models/
│   │   ├── mod.rs                 # Model discovery and loading
│   │   ├── loader.rs              # Model file loader utilities
│   │   └── registry.rs            # Model registry and metadata
│   ├── server/
│   │   ├── mod.rs                 # HTTP server implementation
│   │   ├── handlers.rs            # API endpoint handlers
│   │   ├── routes.rs              # API route definitions
│   │   └── middleware.rs          # HTTP middleware
│   ├── benchmarks/
│   │   ├── mod.rs                 # Benchmark suite orchestration
│   │   ├── embed_bench.rs         # Embedding benchmarks
│   │   ├── llm_bench.rs           # LLM benchmarks
│   │   └── reports.rs             # HTML report generation
│   └── config/
│       ├── mod.rs                 # Burn-specific configuration
│       ├── burn_config.rs         # Main configuration structures
│       └── validation.rs          # Configuration validation
```

## Implementation Phases

### Phase 1: Foundation (Weeks 1-2)

**Week 1: Basic Structure**
1. Create `crates/crucible-burn` in workspace
2. Set up Cargo.toml with core dependencies
3. Implement basic CLI structure following `crucible-cli` patterns
4. Add hardware detection for Vulkan/ROCm
5. Create basic configuration system

**Week 2: Core Providers**
1. Implement Burn embedding provider with `EmbeddingProvider` trait
2. Basic model loading from ~/models directory
3. Simple CLI commands for embedding testing
4. Error handling and logging integration

### Phase 2: Expanded Functionality (Weeks 3-4)

**Week 3: LLM Support**
1. Implement Burn LLM provider
2. Model registry for different model types
3. Streaming inference support
4. Advanced model architecture manipulation

**Week 4: HTTP Server**
1. Axum-based HTTP inference server
2. RESTful API endpoints for embeddings and LLM
3. Health checks and model information endpoints
4. Basic authentication and rate limiting

### Phase 3: Advanced Features (Weeks 5-6)

**Week 5: Benchmarking Suite**
1. Comprehensive benchmarking with Criterion
2. HTML report generation
3. Comparative analysis (Burn vs FastEmbed)
4. Performance monitoring and metrics

**Week 6: Container Support**
1. ROCm Dockerfile with GPU support
2. Container detection and auto-switching
3. Hybrid deployment strategies
4. Production deployment configurations

### Phase 4: Integration & Polish (Weeks 7-8)

**Week 7: Integration Preparation**
1. Provider factory for crucible-llm integration
2. Configuration extension for crucible-config
3. Comprehensive testing and validation
4. Documentation and examples

**Week 8: Production Readiness**
1. Performance optimization
2. Error handling improvements
3. Monitoring and observability
4. Final testing and deployment preparation

## Critical Files & Dependencies

### Key Dependencies
```toml
[dependencies]
# Core workspace dependencies
crucible-core = { path = "../crucible-core" }
crucible-config = { path = "../crucible-config" }

# Burn ML framework
burn = { version = "0.13", features = ["train", "wgpu", "ndarray"] }
burn-wgpu = { version = "0.13" }
burn-tch = { version = "0.13", optional = true }

# Hardware detection
ash = "0.37"                    # Vulkan API
wgpu = "0.18"                   # WebGPU backend

# CLI and server
clap = { workspace = true, features = ["derive"] }
axum = "0.7"
tokio = { workspace = true }

# Benchmarking and testing
criterion = { workspace = true, optional = true }

# Async and error handling
anyhow = { workspace = true }
thiserror = { workspace = true }
tracing = { workspace = true }
```

### Critical Files to Create
1. `crates/crucible-burn/src/providers/embed.rs` - Main embedding provider
2. `crates/crucible-burn/src/hardware/mod.rs` - Hardware detection logic
3. `crates/crucible-burn/src/cli/args.rs` - CLI argument definitions
4. `crates/crucible-burn/src/config/burn_config.rs` - Configuration structures
5. `crates/crucible-burn/src/models/loader.rs` - Model loading utilities

### Files to Modify
1. `Cargo.toml` (workspace root) - Add new crate member
2. `crates/crucible-config/src/lib.rs` - Add Burn configuration integration

## Hardware Strategy

### AMD Strix Halo Optimization
**Primary Backend**: Vulkan (native execution)
- Use Burn's `wgpu` backend for cross-platform GPU acceleration
- Direct execution without container requirements
- Automatic detection of Vulkan capabilities

**Secondary Backend**: ROCm (containerized)
- Dockerfile with ROCm dependencies for maximum performance
- Container environment detection and auto-switching
- Fallback when Vulkan is unavailable or for performance comparison

### Detection Logic
```rust
pub enum BackendType {
    Vulkan { device_id: usize },
    Rocm { device_id: usize },
    Cpu { num_threads: usize },
}

impl BackendType {
    pub fn auto_detect() -> Self {
        // 1. Try Vulkan first (native AMD support)
        // 2. Fall back to ROCm if in container
        // 3. Final fallback to CPU
    }
}
```

## Model Management

### Directory Structure
```
~/models/
├── embeddings/
│   ├── nomic-embed-text/
│   │   ├── model.safetensors
│   │   ├── config.json
│   │   └── tokenizer.json
│   ├── bge-small-en-v1.5/
│   └── e5-large-multilingual/
├── llm/
│   ├── phi-2/
│   │   ├── model.safetensors
│   │   ├── config.json
│   │   └── tokenizer.json
│   ├── mistral-7b/
│   └── llama-7b/
└── registry.json               # Model metadata catalog
```

### Loading Strategy
1. Automatic model discovery via directory scanning
2. Model metadata registry for quick lookup
3. Lazy loading with memory management
4. Support for SafeTensors, PyTorch, and GGML formats

## Multi-Purpose Interface Design

### CLI Commands
```bash
# Hardware detection
burn-test detect hardware

# Embedding testing
burn-test embed test --model nomic-embed-text --text "Hello world"
burn-test embed batch --model bge-small --file input.txt

# LLM inference
burn-test llm inference --model phi-2 --prompt "Write a story"
burn-test llm stream --model mistral-7b --prompt "Continue: "

# HTTP server
burn-test server start --port 8080 --backend vulkan

# Benchmarking
burn-test bench embed --all --iterations 100
burn-test bench llm --models phi-2,mistral-7b --compare fastembed
```

### HTTP API Endpoints
```http
POST /api/v1/embeddings
POST /api/v1/llm/inference
POST /api/v1/llm/stream
GET  /api/v1/models
GET  /api/v1/health
GET  /api/v1/hardware
```

## Configuration System

### Burn Configuration Structure
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BurnConfig {
    pub default_backend: BackendConfig,
    pub model_dir: PathBuf,
    pub cache_dir: Option<PathBuf>,
    pub server: ServerConfig,
    pub benchmarks: BenchmarkConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BackendConfig {
    Auto,                           // Auto-detect best backend
    Vulkan { device_id: usize },    // Force Vulkan
    Rocm { device_id: usize },      // Force ROCm
    Cpu { num_threads: usize },     // Force CPU
}
```

### Integration with Existing Config
- Extend `crucible-config` patterns
- Add `[burn]` section to existing config files
- Support environment variable overrides
- Hot-reload capability for server mode

## Container Strategy

### ROCm Dockerfile
```dockerfile
FROM rocm/pytorch:rocm5.7.1_ubuntu22.04_py3.10

# Install Rust and Burn dependencies
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
RUN cargo install burn-cli

# Copy application
COPY . /app
WORKDIR /app
RUN cargo build --release

EXPOSE 8080
CMD ["./target/release/burn-test", "server", "start", "--backend", "rocm"]
```

### Auto-Detection Logic
```rust
pub fn detect_container_environment() -> bool {
    std::env::var("DOCKER_CONTAINER").is_ok() ||
    std::path::Path::new("/.dockerenv").exists()
}
```

## Success Metrics

### Performance Targets
- **Embedding Inference**: <50ms (Vulkan), <30ms (ROCm)
- **LLM Generation**: <100ms/token (Vulkan), <60ms/token (ROCm)
- **Memory Efficiency**: <80% of 128GB for 7B models
- **Throughput**: >100 embeddings/sec, >10 tokens/sec for LLMs

### Integration Metrics
- **API Compatibility**: 100% compatibility with existing EmbeddingProvider trait
- **Configuration Migration**: Seamless integration with crucible-config
- **Model Coverage**: Support for all models in ~/models directory
- **Hardware Utilization**: >80% GPU utilization on AMD Strix Halo

## Risk Mitigation

### Technical Risks
1. **Vulkan Compatibility**: Provide ROCm container fallback
2. **Model Format Issues**: Support multiple formats (SafeTensors, PyTorch, GGML)
3. **Memory Management**: Implement streaming and batching for large models
4. **Backend Detection**: Robust hardware detection with fallbacks

### Integration Risks
1. **Breaking Changes**: Maintain backward compatibility with existing providers
2. **Configuration Conflicts**: Separate config sections and validation
3. **Performance Regression**: Comprehensive benchmarking against FastEmbed
4. **Deployment Complexity**: Container support and documentation

## Long-term Integration Path

### Phase 1: Standalone Testing (Current Plan)
- Independent binary for Burn framework testing
- Separate model management and configuration
- Comprehensive validation of Burn capabilities

### Phase 2: Provider Integration
- Add Burn provider to crucible-llm provider factory
- Unified configuration through crucible-config
- Model sharing with existing FastEmbed system

### Phase 3: Full Integration
- Backend selection in existing EmbeddingProvider trait
- Seamless switching between FastEmbed and Burn backends
- Unified model management and caching

This plan provides a comprehensive, production-ready approach to Burn ML framework testing while maintaining the flexibility for future integration with the broader crucible ecosystem.