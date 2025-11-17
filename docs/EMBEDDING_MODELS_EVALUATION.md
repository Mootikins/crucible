# Advanced Embedding Models Evaluation for Crucible

**Date**: 2025-11-17
**Status**: Technical evaluation of multi-vector embedding models

---

## Executive Summary

**Recommendation**: Implement a **hybrid approach** using single-vector embeddings for first-stage retrieval (current) and ColBERT multi-vector embeddings for optional reranking.

**Rationale**: Multi-vector models like ColBERT offer significant accuracy improvements (13% on retrieval benchmarks) but require 3-10x more storage and are slower for first-stage search. A hybrid approach provides the best balance of performance, accuracy, and resource efficiency for Crucible's use case.

---

## Current Implementation

### Single-Vector Embeddings (Current)

**Architecture**:
- One embedding vector per block (paragraph, heading, code block, etc.)
- Default: Fastembed BGE-small (384 dimensions)
- Alternative providers: OpenAI, Ollama

**Advantages**:
- ✅ Fast first-stage retrieval (sub-second on 10K blocks)
- ✅ Compact storage (384-1536 dimensions per block)
- ✅ Simple architecture (vector similarity search)
- ✅ Well-supported by SurrealDB vector extensions
- ✅ Local inference with Fastembed (no API calls)

**Limitations**:
- ❌ Single vector must capture all semantic nuances of a block
- ❌ May miss fine-grained semantic matches within blocks
- ❌ Less effective for complex, multi-faceted queries

---

## Multi-Vector Embeddings: ColBERT

### What is ColBERT?

**ColBERT** (Contextual Late Interactions BERT) is a multi-vector embedding model that generates **one vector per token** rather than one vector per document/block.

**Architecture**:
```
Traditional:     "Machine learning concepts" → [single 384-dim vector]

ColBERT:         "Machine learning concepts" → [
                   [vec_machine],
                   [vec_learning],
                   [vec_concepts]
                 ]
```

**Retrieval Method**:
- **First stage**: Pre-compute document token embeddings
- **Query time**: Compute token embeddings for query
- **Late interaction**: MaxSim across all query-document token pairs
- **Score**: Sum of maximum similarities per query token

### Performance Gains

**Benchmark Results** (2025):
- **13% absolute improvement** over traditional bi-encoders on Natural Questions
- **82% top-5 retrieval accuracy** on complex queries
- **20 NDCG@10 improvement** on LongEmbed tasks (small models)

**Why It Works**:
- Captures nuanced semantics within blocks
- Better matching for complex, multi-faceted queries
- Token-level granularity preserves context

### Storage Requirements

**Traditional (BGE-small 384-dim)**:
- 10,000 blocks × 384 dimensions × 4 bytes = **15 MB**

**ColBERT (128-dim per token, avg 50 tokens/block)**:
- 10,000 blocks × 50 tokens × 128 dimensions × 4 bytes = **240 MB**
- With quantization (2 bytes): **120 MB**
- With aggressive quantization (1 byte): **60 MB**

**Storage Impact**: 4-16x larger depending on quantization

### Inference Speed

**First-Stage Retrieval**:
- Traditional: 10-50ms per query (vector similarity)
- ColBERT: 100-500ms per query (late interaction across all tokens)
- **10-50x slower** for first-stage retrieval on large collections

**Recommendation**: Use ColBERT for **reranking** top 100-500 results, not first-stage retrieval

---

## Rust/Fastembed Support

### Current Status (2025-11)

✅ **Fastembed Rust v0.3.0+** supports ColBERT via `LateInteractionTextEmbedding`

**Available in Crucible's tech stack**:
```rust
use fastembed::LateInteractionTextEmbedding;

// Initialize ColBERT model
let model = LateInteractionTextEmbedding::try_new(
    LateInteractionModelName::ColBERTv2,
    Default::default()
)?;

// Embed document (returns Vec<Vec<f32>>)
let doc_embeddings = model.embed(&["Machine learning concepts"], None)?;

// Late interaction scoring
let scores = model.query_embed(&["What is ML?"], &doc_embeddings)?;
```

**Models Available**:
- `colbert-ir/colbertv2.0` (BERT-base, 110M params)
- `mxbai-edge-colbert-v0` (17M params, 2025 release)

---

## Integration Strategy for Crucible

### Option 1: Hybrid Single-Vector + ColBERT Reranking (Recommended)

**Architecture**:
```
Query: "How do I implement semantic search?"

Stage 1 (Fast):
  - Single-vector semantic search (current implementation)
  - Retrieve top 100-500 candidates
  - Time: 10-50ms

Stage 2 (Accurate):
  - ColBERT reranking of top candidates
  - Late interaction scoring
  - Time: 50-100ms

Total: 60-150ms (still interactive)
```

**Implementation**:
1. Keep current single-vector embeddings for all blocks
2. Add optional ColBERT embeddings for blocks (configurable)
3. Add reranking stage in semantic search pipeline
4. Make ColBERT reranking opt-in via configuration

**Benefits**:
- ✅ Best of both worlds: speed + accuracy
- ✅ Backward compatible (existing vaults work as-is)
- ✅ Configurable (users can disable reranking for speed)
- ✅ Storage efficient (only store ColBERT for reranking pool)

**Storage Impact**:
- Single-vector: 15 MB (10K blocks)
- ColBERT for top 500 rerank pool: 12 MB (quantized)
- **Total: 27 MB** (1.8x increase)

### Option 2: ColBERT-Only (Not Recommended)

**Why Not**:
- ❌ 10-50x slower first-stage retrieval
- ❌ 4-16x storage increase
- ❌ Breaking change for existing vaults
- ❌ Overkill for simple queries

### Option 3: Status Quo (Current)

**When Acceptable**:
- ✅ Personal knowledge bases (<10K notes)
- ✅ Simple queries ("machine learning", "project notes")
- ✅ Storage/performance constraints

**When Insufficient**:
- ❌ Complex multi-faceted queries
- ❌ Research-heavy use cases requiring high precision
- ❌ Agent-driven retrieval needing best-possible context

---

## Recommendation: Phased Implementation

### Phase 1: Research & Prototyping (2-3 weeks)

**Tasks**:
1. Add `fastembed` ColBERT support to `crucible-llm` crate
2. Implement late interaction scoring in `EmbeddingProvider` trait
3. Build reranking pipeline in `crucible-core/enrichment`
4. Test on sample knowledge bases (measure accuracy vs speed)

**Success Criteria**:
- ColBERT reranking improves top-5 accuracy by >5%
- Reranking 100 candidates completes in <100ms
- Storage increase <2x for typical vaults

### Phase 2: Integration (2-3 weeks)

**Tasks**:
1. Add configuration option: `embedding.reranking.enabled`
2. Update semantic search to support optional reranking
3. Persist ColBERT embeddings in SurrealDB (new table: `late_interaction_embeddings`)
4. Add CLI flag: `cru semantic --rerank "complex query"`

**Configuration Example**:
```toml
[embedding]
provider = "fastembed"
model = "BGESmallENV15"

[embedding.reranking]
enabled = true
provider = "colbert"
model = "mxbai-edge-colbert-v0"  # 17M params, smaller
top_k = 100  # Rerank top 100 results
```

### Phase 3: Evaluation & Tuning (1-2 weeks)

**Tasks**:
1. Benchmark on diverse query types (simple, complex, multi-faceted)
2. A/B test with and without reranking
3. Optimize quantization (balance accuracy vs storage)
4. Document best practices for users

---

## Alternative Approaches

### Sparse + Dense Hybrid (Alternative to ColBERT)

**Approach**: Combine dense embeddings (current) with sparse embeddings (BM25, SPLADE)

**Advantages**:
- ✅ Captures both semantic and lexical matches
- ✅ Simpler than ColBERT (no late interaction)
- ✅ Lower storage overhead than ColBERT

**Disadvantages**:
- ❌ Less accurate than ColBERT on complex queries
- ❌ Requires separate sparse index

**Verdict**: Worth exploring, but ColBERT likely better for knowledge graphs

### Cross-Encoder Reranking (Alternative to ColBERT)

**Approach**: Use cross-encoder (full attention between query and document)

**Advantages**:
- ✅ Highest accuracy (no bottleneck layer)
- ✅ Well-supported (e.g., `ms-marco-MiniLM-L-6-v2`)

**Disadvantages**:
- ❌ Cannot pre-compute document embeddings
- ❌ Very slow (must encode query+doc pair for each candidate)

**Verdict**: Too slow for interactive search (use for offline tasks only)

---

## Implementation Estimate

**Effort**: 4-8 weeks (1 developer)

**Breakdown**:
- Phase 1 (Research): 2-3 weeks
- Phase 2 (Integration): 2-3 weeks
- Phase 3 (Evaluation): 1-2 weeks

**Risk**: Low (Fastembed already supports ColBERT, incremental addition)

**Value**: High for research-heavy users, Medium for casual users

---

## Decision Framework

**Implement ColBERT reranking if**:
- ✅ Users report low precision on complex queries
- ✅ Agent-driven retrieval requires best-possible context
- ✅ Willing to accept 1.5-2x storage increase
- ✅ Interactive latency <200ms is acceptable

**Defer ColBERT if**:
- ❌ Current single-vector search is sufficient
- ❌ Storage/performance constraints
- ❌ Higher priorities (ACP integration, desktop UI, etc.)

---

## References

### Research Papers
- [ColBERT: Efficient and Effective Passage Search via Contextualized Late Interaction over BERT](https://arxiv.org/abs/2004.12832) (Khattab & Zaharia, 2020)
- [Fantastic (small) Retrievers and How to Train Them: mxbai-edge-colbert-v0 Tech Report](https://arxiv.org/abs/2510.14880) (2025)
- [WARP: An Efficient Engine for Multi-Vector Retrieval](https://arxiv.org/abs/2501.17788) (2025)

### Technical Resources
- [Qdrant FastEmbed ColBERT Guide](https://qdrant.tech/documentation/fastembed/fastembed-colbert/)
- [Weaviate: Late Interaction Overview](https://weaviate.io/blog/late-interaction-overview)
- [Jina AI: What is ColBERT and Why It Matters](https://jina.ai/news/what-is-colbert-and-late-interaction-and-why-they-matter-in-search/)

### Implementation
- [fastembed-rs v0.3.0+](https://github.com/Anush008/fastembed-rs) - Rust library with ColBERT support
- [colbert-ir/colbertv2.0](https://huggingface.co/colbert-ir/colbertv2.0) - Official ColBERT model
- [mxbai-edge-colbert-v0](https://huggingface.co/mixedbread-ai/mxbai-edge-colbert-v0) - Smaller 17M param model

---

**Next Steps**: Discuss with stakeholders whether ColBERT reranking aligns with current roadmap priorities (ACP integration, CLI chat interface, etc.). If approved, begin Phase 1 prototyping.
