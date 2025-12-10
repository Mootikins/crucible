# Burn Model Formats: SafeTensors vs Native (.mpk)

## ✅ Confirmed: Burn Supports Vulkan & ROCm

From Burn's README:
- **Vulkan:** ✅ Supported (via wgpu backend)
- **ROCm:** ✅ Supported (for AMD GPUs)
- **CUDA:** ✅ Supported (for NVIDIA GPUs)

**You're right - the gap is just model conversion!**

---

## Model Format Options

### 1. SafeTensors (Direct Loading) ✅ **RECOMMENDED**

**Status:** ✅ **You can load SafeTensors directly!**

Burn has built-in support for loading SafeTensors files directly using `burn-import`:

```rust
use burn_import::safetensors::SafetensorsFileRecorder;
use burn::record::{FullPrecisionSettings, Recorder};

let record: ModelRecord<B> = SafetensorsFileRecorder::<FullPrecisionSettings>::default()
    .load("model.safetensors", &Default::default())
    .expect("Failed to load SafeTensors");
```

**Benefits:**
- ✅ **No conversion needed** - Use models directly from HuggingFace
- ✅ **Standard format** - Most models available in SafeTensors
- ✅ **Works immediately** - No preprocessing step
- ✅ **Smaller files** - SafeTensors is already optimized

**Drawbacks:**
- ⚠️ Slightly slower loading (needs to parse SafeTensors format)
- ⚠️ Name mapping required (HuggingFace names → Burn structure)

---

### 2. Burn Native Format (.mpk - MessagePack)

**Status:** Optional optimization format

Burn's native format is **MessagePack** (`.mpk` files) using `NamedMpkFileRecorder`:

```rust
use burn::record::{FullPrecisionSettings, NamedMpkFileRecorder, Recorder};

let record: ModelRecord<B> = NamedMpkFileRecorder::<FullPrecisionSettings>::default()
    .load("model.mpk", &Default::default())
    .expect("Failed to load model");
```

**Benefits:**
- ✅ **Faster loading** - Pre-parsed, optimized format
- ✅ **Smaller files** - MessagePack compression
- ✅ **Type-safe** - Native Burn format, no name mapping needed
- ✅ **Better for production** - Optimized for repeated loading

**Drawbacks:**
- ❌ **Requires conversion** - Need to convert from SafeTensors/PyTorch first
- ❌ **Extra step** - One-time conversion process
- ❌ **Less portable** - Burn-specific format

---

## Conversion Workflow

### Option 1: Use SafeTensors Directly (EASIEST) ✅

```rust
// Just load SafeTensors directly - no conversion!
let record = SafetensorsFileRecorder::<FullPrecisionSettings>::default()
    .load("model.safetensors", &Default::default())?;
```

**Work:** None - just use it!

### Option 2: Convert to .mpk (OPTIMIZATION)

```rust
// Step 1: Load from SafeTensors
let record = SafetensorsFileRecorder::<FullPrecisionSettings>::default()
    .load("model.safetensors", &Default::default())?;

// Step 2: Save as .mpk
NamedMpkFileRecorder::<FullPrecisionSettings>::default()
    .save(record, "model.mpk".into())?;

// Step 3: Load .mpk (faster in future)
let record = NamedMpkFileRecorder::<FullPrecisionSettings>::default()
    .load("model.mpk", &Default::default())?;
```

**Work:** One-time conversion (can be done offline)

---

## Pre-Converted Models

### HuggingFace Models

**Status:** ⚠️ **Very few pre-converted models available**

- HuggingFace has a "burn" library tag, but only **1 model** found
- Most models are in SafeTensors format (which Burn can load directly!)
- No need for pre-conversion - just use SafeTensors

### Burn Community Models

**Found repositories:**
- `tracel-ai/models` - Official Burn models repository
- Various community ports (whisper-burn, llama2-burn, etc.)

**But:** Most use SafeTensors anyway, so you can load them directly!

---

## Recommendation

### For Your Use Case: **Use SafeTensors Directly** ✅

**Why:**
1. ✅ **No conversion needed** - You already have SafeTensors
2. ✅ **Works immediately** - Burn loads SafeTensors natively
3. ✅ **Standard format** - All HuggingFace models available
4. ✅ **Simpler workflow** - One less step

**When to convert to .mpk:**
- If you're loading the same model repeatedly (production)
- If loading speed is critical
- If you want to optimize for deployment

**For development/testing:** SafeTensors is perfect!

---

## Implementation Strategy

### Step 1: Load SafeTensors (What you have)
```rust
use burn_import::safetensors::SafetensorsFileRecorder;
use burn::record::{FullPrecisionSettings, Recorder};

// Load your existing SafeTensors model
let record = SafetensorsFileRecorder::<FullPrecisionSettings>::default()
    .load("path/to/model.safetensors", &device)?;
```

### Step 2: Map to BERT Structure
- Use `sentence-transformers-burn` as reference for weight name mapping
- Map HuggingFace names → Burn BERT structure
- This is the main work (but patterns exist)

### Step 3: Run Inference
- Use Burn's BERT implementation
- Run on Vulkan/ROCm backend
- Generate embeddings

---

## Summary

| Format | Loading Speed | Conversion | Availability | Recommendation |
|--------|---------------|------------|--------------|----------------|
| **SafeTensors** | Fast | None needed | ✅ All models | ✅ **Use this!** |
| **.mpk (MessagePack)** | Faster | One-time | ⚠️ Few pre-converted | Optional optimization |

**Bottom line:** 
- ✅ **Burn supports Vulkan & ROCm** - confirmed!
- ✅ **You can use SafeTensors directly** - no conversion needed!
- ✅ **The gap is just implementing BERT forward pass** - not format conversion
- ⚠️ **Pre-converted models are rare** - but you don't need them (use SafeTensors)

**Your workflow:**
1. Use your existing SafeTensors model ✅
2. Load with `SafetensorsFileRecorder` ✅
3. Map weights to BERT structure (main work)
4. Run inference on Vulkan/ROCm ✅
