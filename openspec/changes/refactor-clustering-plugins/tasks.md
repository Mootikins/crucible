# Tasks: Refactor Clustering Plugin Architecture

## Status: Pending Approval

**Recommendation**: Option A - Delete Rune scripts, complete Rust implementation

---

## Phase 1: Document Algorithm Designs (Before Deletion)

### Extract Knowledge from Rune Scripts
- [ ] 1.1 Read `runes/events/clustering/kmeans.rn` and document:
  - K-means++ initialization algorithm
  - Elbow method for optimal k
  - Silhouette score calculation
  - Feature vector construction from documents

- [ ] 1.2 Read `runes/events/clustering/hierarchical.rn` and document:
  - Linkage types (single, complete, average, ward)
  - Dendrogram construction
  - Cophenetic correlation metric
  - ASCII dendrogram visualization

- [ ] 1.3 Read `runes/events/clustering/graph_based.rn` and document:
  - Graph construction from document links
  - Louvain community detection algorithm
  - PageRank-based clustering
  - Modularity scoring

- [ ] 1.4 Create `docs/clustering/ALGORITHM_DESIGNS.md` with extracted knowledge
  - Pseudocode for each algorithm
  - Parameter recommendations
  - Quality metrics and their interpretation
  - Performance characteristics

## Phase 2: Delete Rune Clustering Scripts

- [ ] 2.1 Delete `runes/events/clustering/kmeans.rn` (467 lines)
- [ ] 2.2 Delete `runes/events/clustering/hierarchical.rn` (440 lines)
- [ ] 2.3 Delete `runes/events/clustering/graph_based.rn` (480 lines)
- [ ] 2.4 Remove `runes/events/clustering/` directory if empty

## Phase 3: Update Documentation

- [ ] 3.1 Update `docs/PLUGIN_API.md`:
  - Remove Rune clustering plugin examples (Section 1)
  - Keep Rust Algorithm Trait API section
  - Keep MCP Tool API section
  - Keep CLI Command API section
  - Add note: "Clustering algorithms are implemented in Rust for performance"

- [ ] 3.2 Update `openspec/changes/add-rune-integration/proposal.md`:
  - Add section clarifying Rune scope
  - Rune is for: hooks, event handlers, tool enrichment, workflow scripts
  - Rune is NOT for: core algorithm implementations

- [ ] 3.3 Update research doc if needed:
  - `thoughts/shared/research/moc-clustering-research_2025-12-09-1245.md`
  - Remove "Connect Rune Plugins" thread
  - Update architecture overview

## Phase 4: Verify Clean State

- [ ] 4.1 Run `cargo check -p crucible-surrealdb` - should still compile
- [ ] 4.2 Run `cargo test -p crucible-surrealdb clustering` - tests should pass
- [ ] 4.3 Verify no dangling references to deleted files
- [ ] 4.4 Commit changes with clear message

---

## Commit Message Template

```
refactor(clustering): remove Rune clustering scripts in favor of Rust

The Rune clustering scripts (kmeans.rn, hierarchical.rn, graph_based.rn)
were never integrated with the Rust clustering infrastructure. Rather than
building a complex integration layer, we're standardizing on Rust-only
algorithm implementations for:
- Better performance on large knowledge bases
- Simpler debugging and maintenance
- Consistent architecture with existing heuristic algorithm

Algorithm designs from Rune scripts preserved in docs/clustering/ALGORITHM_DESIGNS.md
for reference when implementing additional Rust algorithms.

Deleted:
- runes/events/clustering/kmeans.rn (467 lines)
- runes/events/clustering/hierarchical.rn (440 lines)
- runes/events/clustering/graph_based.rn (480 lines)

Total: 1387 lines removed
```

---

## Future Work (Not This Change)

After this refactor, these become standalone tasks:

1. **Complete K-Means in Rust** - Use `plans/kmeans-algorithm-completion_2025-12-09.md`
2. **Add Hierarchical Clustering** - New Rust implementation using extracted design
3. **Add Graph-Based Clustering** - New Rust implementation using extracted design
4. **Integrate with Embeddings** - Auto-generate embeddings when clustering needs them
