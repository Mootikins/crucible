# Rust Inference Engines & Model Formats Research

## Summary: Best Options for AMD APU (Vulkan/ROCm)

**Highest Impact/Work Ratio:**
1. **FastEmbed (already integrated)** - ONNX Runtime, CPU-only but works now
2. **Candle** - SafeTensors/GGUF, CUDA/Metal/CPU, Vulkan experimental
3. **ort (ONNX Runtime)** - Direct ONNX, CUDA/ROCm/DirectML, but requires GPU build
4. **Burn (current work)** - Custom, Vulkan via wgpu, full control but needs implementation

---

## 1. FastEmbed (Already Integrated)

**Status:** ✅ Already in codebase, working

**Model Formats:**
- ONNX models (pre-converted, auto-downloaded from HuggingFace)
- Limited to ~18 pre-configured models (BGE, Nomic, MiniLM, E5, etc.)

**GPU Backends:**
- ❌ **CPU-only** - Uses ONNX Runtime CPU build
- ⚠️ Could use ONNX Runtime GPU build, but requires separate `ort` integration

**Pros:**
- ✅ Already working, zero integration work
- ✅ Production-ready, battle-tested
- ✅ Auto-downloads models, handles caching
- ✅ Good performance on CPU (5k-14k sentences/sec)
- ✅ Simple API, async-friendly

**Cons:**
- ❌ No GPU acceleration (CPU-only)
- ❌ Limited model selection (only pre-converted ONNX models)
- ❌ Can't use arbitrary SafeTensors/GGUF models
- ❌ ONNX Runtime GPU would require separate `ort` crate integration

**Work Required:** 0 (already done)

**Impact/Work:** ⭐⭐⭐⭐⭐ (infinite - already works)

---

## 2. Candle (HuggingFace)

**Status:** 🔄 Not integrated, actively maintained

**Model Formats:**
- ✅ **SafeTensors** (native, excellent support)
- ✅ **GGUF** (via `candle-core`, good support)
- ✅ **PyTorch .safetensors** (via HuggingFace Hub)
- ✅ **ONNX** (limited, via conversion)

**GPU Backends:**
- ✅ **CUDA** (stable, well-supported)
- ✅ **Metal** (Apple Silicon, stable)
- ✅ **CPU** (ndarray backend, stable)
- ❌ **Vulkan** (NOT implemented - only experimental fork by niklasha, stalled)
- ❌ **ROCm** (NOT officially supported - only POC by vberthet, has issues)

**Real-World Status (from GitHub Issues):**
- **Vulkan (Issue #1810)**: Open since March 2024, no official support. One contributor (niklasha) started work in private fork for OpenBSD/Radeon but stalled. No shader caching, no production-ready implementation.
- **ROCm (Issue #346)**: Open issue, POC exists but:
  - Only works on specific GPU architectures (gfx1030/RDNA2 tested)
  - Has issues with APUs (HIP selects embedded GPU incorrectly)
  - Examples hang on some GPUs (gfx1102/RX7600 reported)
  - Requires hardcoded GPU arch in build.rs
  - Not production-ready

**Pros:**
- ✅ Native SafeTensors support (perfect for HuggingFace models)
- ✅ GGUF support built-in
- ✅ Actively maintained by HuggingFace
- ✅ Good documentation and examples
- ✅ Embedding models work well (BGE, Nomic, etc.)
- ✅ Can load arbitrary models from HuggingFace Hub
- ✅ CUDA and Metal work great

**Cons:**
- ❌ **No Vulkan support** (only stalled experimental fork)
- ❌ **No ROCm support** (only broken POC, especially problematic for APUs)
- ⚠️ Requires implementing transformer forward pass (but examples exist)
- ⚠️ More complex than FastEmbed (but more flexible)
- ❌ **Not viable for AMD APU/Strix Halo** - no Vulkan or ROCm support

**Work Required:** Medium (2-3 days) - but **only for CUDA/Metal/CPU**
- Add `candle-core`, `candle-nn`, `candle-transformers`
- Implement embedding provider wrapper
- Load SafeTensors models, run forward pass
- Test with actual models

**Impact/Work:** ⭐⭐ (low impact for AMD APU - no GPU support)

**AMD APU Compatibility:** ❌ **Not viable** - No Vulkan or ROCm support. CPU-only on AMD APU.

---

## 3. ort (ONNX Runtime Rust Bindings)

**Status:** 🔄 Not integrated, actively maintained

**Model Formats:**
- ✅ **ONNX** (native, excellent support)
- ⚠️ **SafeTensors** (via conversion to ONNX)
- ❌ **GGUF** (not supported)

**GPU Backends:**
- ✅ **CUDA** (stable, requires CUDA build)
- ✅ **ROCm** (experimental, requires ROCm build)
- ✅ **DirectML** (Windows, stable)
- ✅ **TensorRT** (NVIDIA, optional)
- ✅ **CPU** (default, stable)

**Pros:**
- ✅ Production-grade ONNX Runtime (Microsoft-backed)
- ✅ ROCm support exists (experimental but available)
- ✅ Excellent performance optimizations
- ✅ Can use any ONNX model (not limited to pre-converted)
- ✅ Direct GPU acceleration

**Cons:**
- ⚠️ Requires building ONNX Runtime with GPU support (complex)
- ⚠️ ROCm build is experimental (may have issues)
- ❌ No GGUF support
- ⚠️ Need to convert SafeTensors → ONNX (extra step)
- ⚠️ More complex setup than FastEmbed

**Work Required:** Medium-High (3-5 days)
- Add `ort` crate
- Build ONNX Runtime with ROCm support (or use pre-built)
- Implement embedding provider
- Handle model conversion if needed

**Impact/Work:** ⭐⭐⭐ (good impact, higher work)

**AMD APU Compatibility:** ⚠️ ROCm experimental, may not work on APU

---

## 4. Tract (ONNX/TensorFlow Runtime)

**Status:** 🔄 Not integrated, actively maintained

**Model Formats:**
- ✅ **ONNX** (native, excellent support)
- ✅ **TensorFlow** (native)
- ✅ **TensorFlow Lite** (native)
- ❌ **SafeTensors** (not directly, need conversion)
- ❌ **GGUF** (not supported)

**GPU Backends:**
- ❌ **CPU-only** (no GPU acceleration)
- ❌ No CUDA, ROCm, Vulkan support

**Pros:**
- ✅ Pure Rust implementation (no C++ dependencies)
- ✅ Small binary size
- ✅ Good ONNX support
- ✅ Simple API

**Cons:**
- ❌ **No GPU acceleration** (CPU-only)
- ❌ No GGUF support
- ❌ Less optimized than ONNX Runtime
- ❌ Not ideal for GPU workloads

**Work Required:** Low (1-2 days)
- Add `tract` crate
- Implement embedding provider
- Load ONNX models

**Impact/Work:** ⭐⭐ (low impact - CPU-only, no GPU benefit)

**AMD APU Compatibility:** ❌ CPU-only, no GPU acceleration

---

## 5. llama.cpp + Rust Bindings (embellama, llama_cpp, etc.)

**Status:** 🔄 Not integrated, community-maintained

**Model Formats:**
- ✅ **GGUF** (native, excellent support)
- ❌ **SafeTensors** (not supported - would need conversion)
- ❌ **ONNX** (not supported)

**GPU Backends:**
- ✅ **CUDA** (via llama.cpp, stable)
- ⚠️ **Vulkan** (via llama.cpp, **EXPERIMENTAL** - merged in 2024)
- ⚠️ **ROCm** (via llama.cpp, **EXPERIMENTAL** - exists but limited)
- ✅ **CPU** (via llama.cpp, stable)

**Rust Bindings Available:**
- `embellama` (0.8.0) - **Specifically for embeddings** using llama-cpp
- `llama_cpp` (0.3.2) - High-level bindings
- `llama-cpp-4` (0.1.94) - Lower-level bindings
- `rs-llama-cpp` (0.1.67) - Automated bindings

**Pros:**
- ✅ **Vulkan support exists** (experimental but merged into llama.cpp)
- ✅ **ROCm support exists** (experimental)
- ✅ Excellent GGUF support (native format)
- ✅ Can use any GGUF embedding model
- ✅ `embellama` crate specifically designed for embeddings
- ✅ Mature C++ codebase (llama.cpp)
- ✅ Good performance on CPU, CUDA works well

**Cons:**
- ⚠️ **Vulkan/ROCm are EXPERIMENTAL** (may have issues)
- ❌ No SafeTensors support (need to convert models)
- ⚠️ Primarily designed for LLMs, embedding support is secondary
- ⚠️ llama.cpp C++ dependency (FFI complexity)
- ⚠️ Experimental GPU backends may not work on all hardware
- ⚠️ Need to verify embedding model compatibility with Vulkan/ROCm

**Work Required:** Medium (2-3 days)
- Add `embellama` or `llama_cpp` crate
- Implement embedding provider wrapper
- Test with GGUF embedding models
- Verify Vulkan/ROCm actually works for embeddings (not just LLMs)

**Impact/Work:** ⭐⭐⭐⭐ (potentially high impact if Vulkan/ROCm work)

**AMD APU Compatibility:** ⚠️ **POTENTIALLY VIABLE** - Vulkan/ROCm support exists but experimental. Need to test.

---

## 6. Burn (Current Work)

**Status:** 🔄 Partially integrated, in progress

**Model Formats:**
- ✅ **SafeTensors** (via `safetensors` crate, manual loading)
- ⚠️ **GGUF** (basic parsing, full inference not implemented)
- ❌ **ONNX** (not supported)

**GPU Backends:**
- ✅ **Vulkan** (via wgpu, stable)
- ✅ **CUDA** (experimental)
- ⚠️ **ROCm** (experimental)
- ✅ **CPU** (ndarray backend, stable)

**Pros:**
- ✅ **Vulkan support** (perfect for AMD APU via wgpu)
- ✅ Full control over inference pipeline
- ✅ Pure Rust, no C++ dependencies
- ✅ Can implement custom optimizations
- ✅ SafeTensors loading already implemented

**Cons:**
- ❌ **High implementation work** (need to build transformer forward pass)
- ❌ GGUF inference not implemented (only discovery)
- ⚠️ ROCm support experimental
- ⚠️ Less mature than other options
- ⚠️ Need to implement attention, layer norm, etc.

**Work Required:** High (1-2 weeks)
- Implement full BERT/transformer forward pass
- Implement GGUF tensor reading and inference
- Test with actual models
- Optimize for performance

**Impact/Work:** ⭐⭐⭐ (high impact, but very high work)

**AMD APU Compatibility:** ✅ Vulkan via wgpu should work

---

## 7. Direct llama.cpp Bindings

**Status:** 🔄 Not integrated, community-maintained

**Model Formats:**
- ✅ **GGUF** (native, excellent support)
- ❌ **SafeTensors** (not supported)

**GPU Backends:**
- ✅ **CUDA** (via llama.cpp)
- ⚠️ **ROCm** (via llama.cpp HIP backend, experimental)
- ⚠️ **Vulkan** (via llama.cpp, experimental)
- ✅ **CPU** (stable)

**Pros:**
- ✅ Excellent GGUF support
- ✅ ROCm support exists (experimental)
- ✅ Vulkan support exists (experimental)
- ✅ Very mature C++ codebase

**Cons:**
- ❌ C++ dependency (not pure Rust)
- ⚠️ ROCm/Vulkan backends are experimental
- ❌ No SafeTensors support
- ⚠️ Primarily for LLMs, embedding models less common
- ⚠️ FFI complexity

**Work Required:** Medium-High (3-4 days)
- Add llama.cpp bindings crate
- Build llama.cpp with ROCm/Vulkan
- Implement embedding provider
- Handle FFI complexity

**Impact/Work:** ⭐⭐⭐ (good for GGUF, but experimental GPU backends)

**AMD APU Compatibility:** ⚠️ ROCm/Vulkan experimental, may not work

---

## Recommendations for AMD APU (128GB unified RAM)

### Short-term (Immediate):
1. **Keep FastEmbed** - Already works, CPU-only but functional
2. ~~**Add Candle with Vulkan**~~ - **NOT VIABLE** - No Vulkan support exists (only stalled experimental fork)

### Medium-term (If Candle Vulkan works):
1. **Complete Burn implementation** - Full control, Vulkan native
   - Implement transformer forward pass
   - Add GGUF inference
   - High work, but best long-term solution

### Long-term (If needed):
1. **ort with ROCm** - If ROCm support improves for APUs
2. **llama.cpp with Vulkan** - If GGUF becomes primary format

---

## Model Format Support Matrix

| Engine | SafeTensors | GGUF | ONNX | Notes |
|--------|-------------|------|------|-------|
| **FastEmbed** | ❌ | ❌ | ✅ | Pre-converted models only |
| **Candle** | ✅ | ✅ | ⚠️ | Native SafeTensors, good GGUF |
| **ort** | ⚠️ | ❌ | ✅ | Need conversion for SafeTensors |
| **Tract** | ❌ | ❌ | ✅ | CPU-only |
| **llama-rs** | ❌ | ✅ | ❌ | GGUF native |
| **Burn** | ✅ | ⚠️ | ❌ | SafeTensors native, GGUF partial |
| **llama.cpp** | ❌ | ✅ | ❌ | GGUF native |

---

## GPU Backend Support Matrix (AMD APU)

| Engine | Vulkan | ROCm | CUDA | CPU | Notes |
|--------|--------|------|------|-----|-------|
| **FastEmbed** | ❌ | ❌ | ❌ | ✅ | CPU-only |
| **Candle** | ❌ | ❌ | ✅ | ✅ | No Vulkan/ROCm support |
| **ort** | ❌ | ⚠️ | ✅ | ✅ | ROCm experimental |
| **Tract** | ❌ | ❌ | ❌ | ✅ | CPU-only |
| **llama.cpp** | ⚠️ | ⚠️ | ✅ | ✅ | Vulkan/ROCm experimental |
| **Burn** | ✅ | ⚠️ | ⚠️ | ✅ | Vulkan stable via wgpu |
| **llama.cpp** | ⚠️ | ⚠️ | ✅ | ✅ | Vulkan/ROCm experimental |

---

## Final Recommendation

**For AMD APU with Vulkan/ROCm requirements:**

1. **Immediate:** Keep FastEmbed (CPU-only, but works now)
2. **Next step (TEST FIRST):** Try **llama.cpp + embellama** with Vulkan/ROCm
   - Vulkan/ROCm support exists (experimental but merged)
   - `embellama` crate specifically for embeddings
   - GGUF format (need to convert SafeTensors if needed)
   - Medium work, potentially high impact if it works
   - **TEST THIS FIRST** - might actually work!

3. **If llama.cpp doesn't work:** Complete **Burn implementation**
   - Full control, Vulkan native via wgpu
   - Should work on AMD APU (wgpu supports Vulkan)
   - More work, but guaranteed to work
   - SafeTensors support already implemented

**Avoid:**
- ❌ **Candle** - No Vulkan/ROCm support (only stalled experimental forks)
- ❌ ort with ROCm (experimental, may not work on APU)
- ❌ Tract (CPU-only)
