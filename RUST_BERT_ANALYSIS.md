# rust-bert Analysis

## Overview

**rust-bert** is a **completely separate project from Burn**. It's a Rust port of HuggingFace Transformers using **LibTorch (PyTorch C++ API)** via `tch-rs` bindings.

## Project Status

- **Stars:** 2,998 (popular but smaller than Burn's 13.6k)
- **Forks:** 238
- **Last Updated:** Dec 8, 2025 (recently updated!)
- **Latest Version:** 0.23.0 (on crates.io)
- **Last Commit:** May 26, 2025 (6+ months ago - less active than Burn)

**Verdict:** ✅ **Maintained** but less active than Burn

## Backend Support

### Primary Backend: LibTorch (via tch-rs)
- ✅ **CUDA** (via LibTorch CUDA builds)
- ✅ **CPU** (via LibTorch CPU builds)
- ❌ **Vulkan** (NOT supported - LibTorch doesn't support Vulkan)
- ❌ **ROCm** (NOT supported - LibTorch doesn't support ROCm)
- ⚠️ **ONNX Runtime** (optional backend via `ort` crate)

### GPU Support
- ✅ **CUDA** - Full support via LibTorch
- ❌ **Vulkan** - Not available
- ❌ **ROCm** - Not available
- ⚠️ **ONNX Runtime** - Can use ONNX Runtime GPU backends (CUDA/ROCm/DirectML) if using ONNX backend

## Model Support

### Embedding Models
✅ **Sentence Embeddings** - Supported!
- BERT
- DistilBERT
- RoBERTa
- ALBERT
- T5

### Other Models
- BERT, DistilBERT, RoBERTa, ALBERT, DeBERTa, MobileBERT, FNet
- GPT, GPT2, GPT-Neo, GPT-J
- BART, T5, LongT5
- Marian, MBart, M2M100, NLLB
- XLNet, Reformer, Longformer, Pegasus
- And more...

## Key Differences from Burn

| Feature | rust-bert | Burn |
|---------|-----------|------|
| **Backend** | LibTorch (PyTorch C++) | Native Rust + wgpu/ndarray |
| **CUDA** | ✅ Yes (via LibTorch) | ✅ Yes |
| **Vulkan** | ❌ No | ✅ Yes (via wgpu) |
| **ROCm** | ❌ No | ⚠️ Experimental |
| **CPU** | ✅ Yes | ✅ Yes |
| **ONNX** | ✅ Optional | ❌ No |
| **Dependencies** | Heavy (LibTorch ~GBs) | Light (pure Rust) |
| **Embedding Support** | ✅ Yes | ⚠️ Need to build |
| **Maintenance** | Moderate | Very Active |

## Pros

✅ **Ready-to-use BERT/embedding models** - No implementation needed!
✅ **Sentence embeddings** - Built-in support
✅ **Many model architectures** - BERT, RoBERTa, ALBERT, T5, etc.
✅ **Production-ready** - Used in real projects
✅ **CUDA support** - Works on NVIDIA GPUs
✅ **ONNX backend option** - Can use ONNX Runtime (which has ROCm support)

## Cons

❌ **No Vulkan support** - LibTorch doesn't support Vulkan
❌ **No ROCm support** - LibTorch doesn't support ROCm (unless using ONNX backend)
❌ **Heavy dependencies** - LibTorch is several GBs
❌ **C++ dependency** - Not pure Rust (LibTorch is C++)
❌ **Not suitable for AMD APU** - No Vulkan/ROCm support (unless using ONNX backend with ROCm)

## For AMD APU / Strix Halo

### Option 1: rust-bert with ONNX backend + ROCm
**Status:** ⚠️ **POSSIBLE but complex**
- Use rust-bert's ONNX backend
- Use ONNX Runtime with ROCm build
- ROCm support is experimental
- May not work on APU

**Work:** Medium-High (3-5 days)
- Set up ONNX backend
- Build ONNX Runtime with ROCm
- Test on APU
- May not work

### Option 2: rust-bert CPU-only
**Status:** ✅ **WORKS but slow**
- Use rust-bert with CPU backend
- No GPU acceleration
- Same as FastEmbed (CPU-only)

**Work:** Low (1 day)
- Just use rust-bert
- CPU-only inference

## Comparison with Other Options

| Option | Vulkan | ROCm | Work | Status |
|--------|--------|------|------|--------|
| **rust-bert (CPU)** | ❌ | ❌ | Low | ✅ Works |
| **rust-bert (ONNX+ROCm)** | ❌ | ⚠️ | Medium-High | ⚠️ Experimental |
| **Burn + wgpu** | ✅ | ❌ | High | ✅ Should work |
| **llama.cpp + embellama** | ⚠️ | ⚠️ | Medium | ⚠️ Experimental |
| **FastEmbed** | ❌ | ❌ | None | ✅ Works (CPU) |

## Recommendation

### For AMD APU / Strix Halo:

**rust-bert is NOT a good fit** because:
- ❌ No Vulkan support
- ❌ No ROCm support (unless using experimental ONNX+ROCm)
- ❌ Heavy LibTorch dependency
- ❌ C++ dependency

**Better options:**
1. **Burn + wgpu** - Vulkan support, pure Rust
2. **llama.cpp + embellama** - Vulkan/ROCm experimental
3. **FastEmbed** - CPU-only but works now

### If you had NVIDIA GPU:

**rust-bert would be excellent!**
- ✅ Ready-to-use embeddings
- ✅ CUDA support
- ✅ Production-ready
- ✅ No implementation needed

## Conclusion

**rust-bert is a great library**, but it's **not suitable for AMD APU/Strix Halo** because:
- Uses LibTorch (no Vulkan/ROCm support)
- Would be CPU-only on AMD APU
- Same limitation as FastEmbed

**For AMD APU, stick with:**
1. **Burn + wgpu** (Vulkan) - Best long-term option
2. **llama.cpp + embellama** (Vulkan/ROCm experimental) - Quick test option
3. **FastEmbed** (CPU) - Works now, no GPU
