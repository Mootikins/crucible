# Burn Version Analysis

## Current Status

### Our Codebase
- **Using:** Burn 0.14
- **Status:** ⚠️ **OUTDATED** (6 versions behind stable)

### Burn Project Status
- **Latest Stable:** v0.19.1 (Nov 6, 2025)
- **Latest Pre-release:** v0.20.0-pre.5 (Dec 8, 2025)
- **GitHub Activity:** 
  - ⭐ **13,598 stars** (very popular!)
  - 🍴 **744 forks**
  - 📅 **Last updated:** Dec 10, 2025 (yesterday!)
  - 🔄 **Recent commits:** Multiple commits per day
  - 🔀 **5 open PRs** (active development)

**Verdict:** ✅ **VERY ACTIVELY MAINTAINED** - Burn is a thriving project with daily commits and regular releases.

### sentence-transformers-burn Status
- **Using:** Burn 0.8.0 (from 2023!)
- **Last updated:** Sept 2, 2023 (over 2 years ago!)
- **Status:** ❌ **ABANDONED/STALE**

**Verdict:** ⚠️ **OUTDATED** - The reference implementation uses Burn 0.8.0, which is 11+ versions behind current.

---

## Version Comparison

| Version | Release Date | Status | Notes |
|---------|--------------|--------|-------|
| **0.8.0** | 2023 | Old | Used by sentence-transformers-burn |
| **0.14** | ? | Old | **What we're using** |
| **0.19.1** | Nov 2025 | **Latest Stable** | Should upgrade to this |
| **0.20.0-pre.5** | Dec 2025 | Pre-release | Cutting edge |

---

## Implications

### 1. We Should Upgrade Burn
**From:** 0.14 → **To:** 0.19.1 (stable) or 0.20.0-pre.5 (latest)

**Why:**
- ✅ Bug fixes and improvements
- ✅ Better performance
- ✅ More stable API
- ✅ Better wgpu/Vulkan support (likely improved)

**Work:** Medium (1-2 days)
- Update Cargo.toml
- Fix breaking API changes
- Test thoroughly

### 2. sentence-transformers-burn Code Needs Major Updates
**Problem:** Their code uses Burn 0.8.0, which is incompatible with 0.14+ (let alone 0.19+)

**What we can still use:**
- ✅ **Architecture patterns** (BERT structure, layer organization)
- ✅ **Weight mapping logic** (HuggingFace → Burn name mapping)
- ✅ **Forward pass logic** (attention, embeddings, encoder flow)

**What needs updating:**
- ❌ **API calls** (Burn API changed significantly)
- ❌ **Tensor operations** (new API)
- ❌ **Module initialization** (different patterns)
- ❌ **Backend usage** (wgpu backend API changed)

**Work:** High (3-5 days to adapt their patterns to Burn 0.19+)

### 3. Alternative: Use Burn's Official Examples
**Better approach:** Check Burn's official examples for:
- Model loading patterns
- SafeTensors integration
- Transformer implementations

**Location:** `examples/` in Burn repo

---

## Recommendations

### Option 1: Upgrade First, Then Build (RECOMMENDED)
1. **Upgrade Burn to 0.19.1** (1-2 days)
   - Update dependencies
   - Fix breaking changes
   - Test existing code

2. **Study Burn 0.19+ examples** (1 day)
   - Check official examples
   - Look for transformer/BERT patterns
   - Understand new API

3. **Build BERT implementation** (3-5 days)
   - Use sentence-transformers-burn as **architecture reference only**
   - Use Burn 0.19+ API
   - Use our existing SafeTensors loading

**Total:** 5-8 days

### Option 2: Build on 0.14, Upgrade Later
1. **Build BERT on Burn 0.14** (4-6 days)
   - Adapt sentence-transformers-burn patterns
   - Use existing SafeTensors loading

2. **Upgrade to 0.19.1** (1-2 days)
   - Fix breaking changes
   - Test

**Total:** 5-8 days (same, but more risk of rework)

### Option 3: Check for Newer BERT Examples
1. **Search for Burn 0.19+ BERT/transformer examples** (1 day)
   - Maybe someone already ported it?
   - Check Burn's official examples
   - Check other repos

2. **Use if found, otherwise build** (3-5 days)

**Total:** 4-6 days (if found) or 5-8 days (if not)

---

## Action Items

1. ✅ **Upgrade Burn to 0.19.1** (do this first!)
2. ✅ **Search Burn 0.19+ examples for BERT/transformer patterns**
3. ✅ **Use sentence-transformers-burn as architecture reference only**
4. ✅ **Build implementation using Burn 0.19+ API**

---

## Conclusion

**Burn is VERY actively maintained** - we should definitely use it, but:
- ⚠️ We're on an old version (0.14)
- ⚠️ sentence-transformers-burn is very outdated (0.8.0)
- ✅ But Burn's active maintenance means good long-term support
- ✅ Upgrade path is clear (0.14 → 0.19.1)

**Best path:** Upgrade to 0.19.1 first, then build BERT implementation using sentence-transformers-burn as an architecture reference (not code reference).
