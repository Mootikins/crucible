# Quantization Analysis: Burn, SafeTensors, and Model Formats

## Research Question
**Are SafeTensors available in variable quantizations? Can we avoid saving multiple quantized versions and quantizing ourselves?**

---

## SafeTensors Format & Quantization

### SafeTensors Native Support

**SafeTensors is a format, not a quantization method.** It can store tensors in various data types:

- ✅ **FP32** (float32) - Full precision
- ✅ **FP16** (float16) - Half precision
- ✅ **BF16** (bfloat16) - Brain float
- ✅ **INT8** - 8-bit integers
- ✅ **INT4** - 4-bit integers (via packing)
- ✅ **Other dtypes** - UINT8, INT16, etc.

**Key Point:** SafeTensors files can contain **any dtype**, but the quantization is determined by **what was saved**, not the format itself.

### HuggingFace Model Availability

**Most HuggingFace models are available in multiple formats:**

1. **FP32** - Full precision (default)
2. **FP16** - Half precision (common for GPU)
3. **INT8** - 8-bit quantized (via bitsandbytes)
4. **INT4** - 4-bit quantized (via bitsandbytes)
5. **GGUF quantized** - Various quantization levels (Q4_0, Q8_0, etc.)

**But:** These are **separate files** - you need to download the specific quantization you want.

**Example HuggingFace model files:**
```
model.safetensors          # FP32 (default)
model.fp16.safetensors     # FP16
model.int8.safetensors     # INT8 (if available)
model.safetensors.index.json  # Sharded models
```

---

## Burn Quantization Support

### Current Status (Updated!)

**Burn has QUANTIZATION SUPPORT (Beta) in 0.19+:**

- ✅ **FP32** - Full support (default)
- ✅ **FP16** - Supported (via `HalfPrecisionSettings`)
- ✅ **INT8** - **Supported (Beta)** - Per-tensor and per-block quantization
- ✅ **INT4** - **Supported (Beta)** - Per-tensor and per-block quantization
- ✅ **INT2** - **Supported (Beta)** - Per-tensor and per-block quantization
- ⚠️ **Status:** Beta (actively developed)

**From Burn 0.19.1 docs:**
> "Quantization support in Burn is currently in active development. It supports the following modes on some backends:
> - Per-tensor and per-block (linear) quantization to 8-bit, 4-bit and 2-bit representations"

### Burn Precision Settings

Burn uses "precision settings" for record loading/saving:

```rust
use burn::record::{FullPrecisionSettings, HalfPrecisionSettings, Recorder};

// FP32 (default)
let record = NamedMpkFileRecorder::<FullPrecisionSettings>::default()
    .load("model.mpk", &device)?;

// FP16
let record = NamedMpkFileRecorder::<HalfPrecisionSettings>::default()
    .load("model.mpk", &device)?;
```

**But:** This only affects **loading/saving**, not runtime quantization.

### Burn DType Support

From Burn's tensor API:
- ✅ `DType::F32` - Float32
- ✅ `DType::F16` - Float16
- ✅ `DType::BF16` - BFloat16
- ✅ `DType::I32` - Int32
- ✅ `DType::I64` - Int64
- ❌ **No INT8/INT4 support** for model weights

---

## Quantization Options

### Option 1: Use Pre-Quantized SafeTensors Models

**Status:** ✅ **Available but limited**

**Pros:**
- ✅ No quantization needed
- ✅ Smaller files (FP16, INT8, INT4)
- ✅ Faster inference (if backend supports)

**Cons:**
- ❌ **Not all models available in all quantizations**
- ❌ Need to download specific quantization
- ⚠️ **INT8/INT4 in SafeTensors format is rare** (usually GGUF or bitsandbytes)
- ❌ Quality loss (especially INT4)

**Where to find:**
- HuggingFace model pages (check "Files" tab)
- Some models have FP16 versions
- INT8/INT4 usually via bitsandbytes (not SafeTensors)

### Option 2: Quantize After Loading (Runtime) - **NOW POSSIBLE!**

**Status:** ✅ **Supported in Burn 0.19+ (Beta)**

**Approach:**
1. Load FP32 model from SafeTensors
2. Use Burn's quantization API to convert to INT8/INT4
3. Use quantized weights for inference

**Pros:**
- ✅ One model file (FP32)
- ✅ Can choose quantization at runtime
- ✅ Full control
- ✅ **Burn has built-in quantization (Beta)**

**Cons:**
- ⚠️ Quantization is Beta (may have issues)
- ⚠️ Quality loss (especially INT4)
- ⚠️ More complex setup
- ⚠️ May not work on all backends

### Option 3: Use FP16 SafeTensors (RECOMMENDED)

**Status:** ✅ **Best balance**

**Approach:**
1. Download FP16 SafeTensors model (if available)
2. Load with `HalfPrecisionSettings`
3. Use FP16 for inference

**Pros:**
- ✅ **2x smaller files** (FP16 vs FP32)
- ✅ **Faster inference** (GPU optimized)
- ✅ **Burn supports FP16** natively
- ✅ **Minimal quality loss** (usually <1%)
- ✅ **Widely available** on HuggingFace

**Cons:**
- ⚠️ Not all models have FP16 versions
- ⚠️ Slightly less precision than FP32

---

## SafeTensors Quantization Availability

### What's Actually Available

**On HuggingFace, most embedding models are available as:**

1. **FP32 SafeTensors** - ✅ Always available
2. **FP16 SafeTensors** - ✅ Often available (check model files)
3. **INT8/INT4** - ❌ Rarely in SafeTensors format (usually GGUF or bitsandbytes)

### Example: nomic-embed-text-v1.5

**Available formats:**
- `model.safetensors` - FP32 (default)
- May have FP16 version (check HuggingFace files)

**Not typically available:**
- INT8 SafeTensors
- INT4 SafeTensors

---

## Recommendations

### For Your Use Case (Embedding Models)

**Best Approach: Use FP16 SafeTensors**

1. ✅ **Download FP16 version** if available (2x smaller)
2. ✅ **Load with `HalfPrecisionSettings`** in Burn
3. ✅ **Use FP16 for inference** (GPU optimized)
4. ✅ **Minimal quality loss** (negligible for embeddings)

**If FP16 not available:**
- Use FP32 SafeTensors (larger but full quality)
- Can convert to FP16 in Rust if needed (simple cast)

### Why Not INT8/INT4?

- ⚠️ **Burn supports INT8/INT4 (Beta)** - but may have limitations
- ❌ **Quality loss** can be significant for embeddings
- ❌ **Not typically available** in SafeTensors format (would need runtime quantization)
- ⚠️ **Beta status** - may have bugs or incomplete backend support

### Storage Strategy

**Don't save multiple quantizations:**
- ✅ **Save FP32 or FP16** (one version)
- ✅ **Convert at runtime** if needed (FP32 → FP16 is trivial)
- ✅ **Use FP16 for inference** (best balance)

---

## Implementation

### Loading FP16 SafeTensors in Burn

```rust
use burn_import::safetensors::SafetensorsFileRecorder;
use burn::record::{HalfPrecisionSettings, Recorder};

// Load FP16 SafeTensors model
let record = SafetensorsFileRecorder::<HalfPrecisionSettings>::default()
    .load("model.fp16.safetensors", &device)?;
```

### Converting FP32 → FP16 at Runtime

```rust
use burn::tensor::{Tensor, DType};

// Load FP32
let record_fp32 = SafetensorsFileRecorder::<FullPrecisionSettings>::default()
    .load("model.safetensors", &device)?;

// Convert to FP16 (if needed)
// This would be done during weight mapping
let tensor_fp16 = tensor_fp32.to_dtype(DType::F16);
```

---

## Summary

| Format | File Size | Quality | Burn Support | Availability |
|--------|-----------|---------|--------------|--------------|
| **FP32 SafeTensors** | 1x (largest) | Best | ✅ Yes | ✅ Always |
| **FP16 SafeTensors** | 0.5x | Excellent | ✅ Yes | ✅ Often |
| **INT8 SafeTensors** | 0.25x | Good | ✅ Yes (Beta) | ❌ Rare (can quantize runtime) |
| **INT4 SafeTensors** | 0.125x | Fair | ✅ Yes (Beta) | ❌ Very Rare (can quantize runtime) |

**Answer to your question:**
- ✅ **Yes, SafeTensors can store different quantizations** (FP32, FP16, etc.)
- ❌ **But you need to download the specific quantization** you want
- ✅ **Recommendation: Use FP16 SafeTensors** (if available)
- ✅ **Don't save multiple versions** - just use FP16 for inference
- ✅ **Burn supports FP16 natively** - no quantization needed

**Bottom line:** 
- ✅ Download **FP16 SafeTensors** if available (best balance)
- ✅ Use **FP32 SafeTensors** if FP16 not available
- ⚠️ **INT8/INT4** - Burn supports it (Beta), but:
  - Not typically in SafeTensors format (would need runtime quantization)
  - Quality loss may be significant for embeddings
  - Beta status - may have limitations
  - **Recommendation: Stick with FP16 for embeddings** (quality/speed balance)
