# Refactor Clustering Plugin Architecture

## Status: Draft

## Problem Statement

The current clustering implementation has **architectural inconsistency**:

1. **Three Rune scripts exist** (`runes/events/clustering/{kmeans,hierarchical,graph_based}.rn` - 1387 total lines) that implement clustering algorithms
2. **Two Rust algorithms exist** (`algorithms/{heuristic,kmeans}.rs`) with the same purpose
3. **No bridge connects them** - Rune scripts are never loaded or executed
4. **Duplicated effort** - K-Means is implemented twice (Rune complete, Rust placeholder)

This creates confusion about which approach is canonical and wastes the effort put into the Rune implementations.

## Decision Required

**Option A: Delete Rune Scripts, Complete Rust Implementation**
- Delete `runes/events/clustering/*.rn` (1387 lines)
- Complete Rust K-Means implementation (placeholder → working)
- Add new algorithms in Rust as needed
- Simpler architecture, better performance, easier debugging

**Option B: Keep Rune Scripts, Build Integration Layer**
- Create `RuneClusteringAlgorithm` wrapper implementing `ClusteringAlgorithm` trait
- Build script discovery and loading from `runes/events/clustering/`
- Expose Rust math functions to Rune context
- More extensible for user-defined algorithms, but more complex

**Option C: Hybrid - Rust Core, Rune Extensions**
- Keep Rust implementations as the core (heuristic, kmeans)
- Allow Rune scripts for **experimental/user** algorithms only
- Rune scripts discovered but clearly secondary
- Best of both: performance + extensibility

## Recommendation: Option A (Delete Rune Scripts)

### Rationale

1. **Rune clustering was exploratory** - Written before the Rust architecture was finalized
2. **No users depend on it** - Feature branch, not released
3. **Performance matters** - Clustering large vaults needs Rust performance
4. **Simpler mental model** - One way to add algorithms (Rust trait)
5. **Existing Rune integration is for hooks/tools** - Not algorithm cores
6. **Maintenance burden** - Two implementations to keep in sync

### What We Preserve

The Rune scripts contain valuable **algorithm designs** we can reference:
- K-means++ initialization pattern
- Hierarchical clustering with linkage types (single, complete, average, ward)
- Graph-based clustering (Louvain communities, PageRank centrality)
- Silhouette score calculation
- Elbow method for optimal k

These can inform Rust implementations without keeping the Rune code.

## Scope

### In Scope
- Delete `runes/events/clustering/` directory (3 files, 1387 lines)
- Document algorithm designs from Rune scripts in `docs/`
- Update `add-rune-integration` openspec to clarify Rune is for hooks/tools, not algorithm cores
- Complete Rust K-Means (separate task, already planned)

### Out of Scope
- Adding new clustering algorithms (hierarchical, graph-based) - future work
- Changing the hook/tool Rune integration - that stays as-is
- Modifying the existing `plugin_api.rs` conversion functions - keep for potential future use

## Impact

### Files to Delete
```
runes/events/clustering/kmeans.rn        (467 lines)
runes/events/clustering/hierarchical.rn  (440 lines)
runes/events/clustering/graph_based.rn   (480 lines)
```

### Files to Update
```
openspec/changes/add-rune-integration/proposal.md  (clarify scope)
docs/PLUGIN_API.md                                  (remove Rune clustering examples)
```

### Files to Create
```
docs/clustering/ALGORITHM_DESIGNS.md  (preserve algorithm knowledge)
```

## Migration

No migration needed - this is unreleased feature branch code.

## Open Questions

1. Should we archive the Rune scripts in `docs/` for reference, or just delete them?
2. Do we want to add hierarchical/graph clustering to the Rust implementation now or later?

## References

- Research doc: `thoughts/shared/research/moc-clustering-research_2025-12-09-1245.md`
- K-Means plan: `thoughts/shared/plans/kmeans-algorithm-completion_2025-12-09.md`
- Rune integration: `openspec/changes/add-rune-integration/proposal.md`
