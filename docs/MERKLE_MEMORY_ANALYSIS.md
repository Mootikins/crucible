# Merkle Tree Memory Footprint Analysis

## Overview

This document analyzes the memory footprint of Merkle trees for typical knowledge management datasets in Crucible, focusing on in-memory and persistent storage scenarios with BLAKE3 hashing.

## 1. Memory per Node Analysis

### MerkleNode Structure Size

Based on the implementation in `/home/moot/crucible/crates/crucible-core/src/storage/merkle.rs`:

```rust
pub struct MerkleNode {
    pub hash: String,           // 24 bytes (String header) + hash data
    pub node_type: NodeType,    // Enum with variant data
    pub depth: usize,           // 8 bytes on 64-bit
    pub index: usize,           // 8 bytes on 64-bit
}

pub enum NodeType {
    Leaf {
        block_hash: String,     // 24 bytes + hash data
        block_index: usize,     // 8 bytes
    },
    Internal {
        left_hash: String,      // 24 bytes + hash data
        right_hash: String,     // 24 bytes + hash data
        left_index: usize,      // 8 bytes
        right_index: usize,     // 8 bytes
    },
}
```

### Memory Calculations per Node

**BLAKE3 Hash Storage:**
- Raw BLAKE3 hash: 32 bytes
- Hex string representation: 64 bytes (2 chars per byte)
- String header overhead: 24 bytes
- **Total per hash string: 88 bytes**

**Leaf Node Memory:**
- Main hash: 88 bytes
- NodeType enum discriminant: 8 bytes
- Depth field: 8 bytes
- Index field: 8 bytes
- Block hash (duplicate of main hash): 88 bytes
- Block index: 8 bytes
- **Total Leaf Node: 208 bytes**

**Internal Node Memory:**
- Main hash: 88 bytes
- NodeType enum discriminant: 8 bytes
- Depth field: 8 bytes
- Index field: 8 bytes
- Left child hash: 88 bytes
- Right child hash: 88 bytes
- Left index: 8 bytes
- Right index: 8 bytes
- **Total Internal Node: 224 bytes**

### HashMap Overhead for Node Indexing

The `MerkleTree` stores nodes in a `HashMap<String, MerkleNode>`:

**HashMap Entry Overhead:**
- Key (hash string): 88 bytes
- Value (MerkleNode): 208-224 bytes
- HashMap entry overhead: ~24 bytes
- **Total per HashMap entry: ~320-336 bytes**

## 2. Dataset Analysis

### Block Calculations by Document Size

| Document Size | 1KB Blocks | 4KB Blocks | 8KB Blocks |
|---------------|------------|------------|------------|
| 10 KB         | 10         | 3          | 2          |
| 25 KB         | 25         | 7          | 4          |
| 50 KB         | 50         | 13         | 7          |

### Merkle Tree Node Calculations

For a binary Merkle tree:
- Leaf nodes = number of blocks
- Internal nodes ≈ leaf nodes - 1 (for complete binary trees)
- Total nodes ≈ 2 × leaf nodes - 1

## 3. Memory Usage by Dataset Size

### Small Dataset: 100 Documents (avg 10KB each)

**Block Distribution:**
- 1KB blocks: 1,000 blocks → 1,000 leaf nodes
- 4KB blocks: 300 blocks → 300 leaf nodes
- 8KB blocks: 200 blocks → 200 leaf nodes

**Memory Usage (1KB blocks):**
- Leaf nodes: 1,000 × 208 bytes = 208 KB
- Internal nodes: ~999 × 224 bytes = 224 KB
- HashMap overhead: 1,999 × 24 bytes = 48 KB
- **Total: ~480 KB**
- Hash storage alone: 1,999 × 88 bytes = 176 KB

**Memory Usage (4KB blocks):**
- Total nodes: ~599
- **Total: ~144 KB**
- Hash storage: ~53 KB

**Memory Usage (8KB blocks):**
- Total nodes: ~399
- **Total: ~96 KB**
- Hash storage: ~35 KB

### Medium Dataset: 1,000 Documents (avg 25KB each)

**Block Distribution:**
- 1KB blocks: 25,000 blocks → 25,000 leaf nodes
- 4KB blocks: 7,000 blocks → 7,000 leaf nodes
- 8KB blocks: 4,000 blocks → 4,000 leaf nodes

**Memory Usage (1KB blocks):**
- Leaf nodes: 25,000 × 208 bytes = 5.2 MB
- Internal nodes: ~24,999 × 224 bytes = 5.6 MB
- HashMap overhead: 49,999 × 24 bytes = 1.2 MB
- **Total: ~12.0 MB**
- Hash storage: 49,999 × 88 bytes = 4.4 MB

**Memory Usage (4KB blocks):**
- Total nodes: ~13,999
- **Total: ~3.4 MB**
- Hash storage: ~1.2 MB

**Memory Usage (8KB blocks):**
- Total nodes: ~7,999
- **Total: ~1.9 MB**
- Hash storage: ~0.7 MB

### Large Dataset: 10,000 Documents (avg 50KB each)

**Block Distribution:**
- 1KB blocks: 500,000 blocks → 500,000 leaf nodes
- 4KB blocks: 130,000 blocks → 130,000 leaf nodes
- 8KB blocks: 70,000 blocks → 70,000 leaf nodes

**Memory Usage (1KB blocks):**
- Leaf nodes: 500,000 × 208 bytes = 104 MB
- Internal nodes: ~499,999 × 224 bytes = 112 MB
- HashMap overhead: 999,999 × 24 bytes = 24 MB
- **Total: ~240 MB**
- Hash storage: 999,999 × 88 bytes = 88 MB

**Memory Usage (4KB blocks):**
- Total nodes: ~259,999
- **Total: ~62 MB**
- Hash storage: ~23 MB

**Memory Usage (8KB blocks):**
- Total nodes: ~139,999
- **Total: ~34 MB**
- Hash storage: ~12 MB

### Very Large Dataset: 100,000 Documents (avg 50KB each)

**Block Distribution:**
- 1KB blocks: 5,000,000 blocks → 5,000,000 leaf nodes
- 4KB blocks: 1,300,000 blocks → 1,300,000 leaf nodes
- 8KB blocks: 700,000 blocks → 700,000 leaf nodes

**Memory Usage (1KB blocks):**
- Total nodes: ~9,999,999
- **Total: ~2.4 GB**
- Hash storage: ~880 MB

**Memory Usage (4KB blocks):**
- Total nodes: ~2,599,999
- **Total: ~620 MB**
- Hash storage: ~230 MB

**Memory Usage (8KB blocks):**
- Total nodes: ~1,399,999
- **Total: ~340 MB**
- Hash storage: ~120 MB

## 4. Hash Storage Optimization Analysis

### Current Hash Storage
- **Format:** 64-character hex strings
- **Storage per hash:** 88 bytes (24 header + 64 data)
- **Efficiency:** 27.3% overhead (64/88 = useful data)

### Alternative Hash Storage Options

**Option 1: Raw Bytes Storage**
```rust
pub struct MerkleNode {
    pub hash: [u8; 32],  // 32 bytes directly
    // ... rest of structure
}
```
- **Memory per hash:** 32 bytes
- **Savings:** 56 bytes per hash (63.6% reduction)
- **Trade-offs:** Less human-readable, requires serialization conversion

**Option 2: String Interning**
```rust
use std::collections::HashSet;
pub struct MerkleTree {
    pub hash_interner: HashSet<String>,  // Shared string storage
    pub nodes: HashMap<usize, MerkleNode>,  // Index-based access
    // ... rest
}
```
- **Estimated savings:** 30-50% for duplicate hashes
- **Trade-offs:** Added complexity, lookup overhead

## 5. Memory Scaling Characteristics

### Scaling Ratios (Tree Memory vs Original Content)

| Dataset Size | Content Size | Tree Memory (1KB) | Ratio | Tree Memory (4KB) | Ratio | Tree Memory (8KB) | Ratio |
|--------------|-------------|-------------------|-------|-------------------|-------|-------------------|-------|
| Small        | 1 MB        | 0.5 MB            | 50%   | 0.1 MB            | 10%   | 0.1 MB            | 10%   |
| Medium       | 25 MB       | 12 MB             | 48%   | 3.4 MB            | 14%   | 1.9 MB            | 8%    |
| Large        | 500 MB      | 240 MB            | 48%   | 62 MB             | 12%   | 34 MB             | 7%    |
| Very Large   | 5 GB        | 2.4 GB            | 48%   | 620 MB            | 12%   | 340 MB            | 7%    |

### Key Observations

1. **Consistent Scaling:** Memory ratios stabilize at ~48% for 1KB blocks
2. **Block Size Impact:** Larger blocks significantly reduce overhead
3. **Hash Dominance:** Hash storage represents 35-40% of total memory
4. **HashMap Overhead:** ~10% of total memory usage

## 6. Persistent Storage Considerations

### Serialization Overhead

When persisting Merkle trees to disk (e.g., in SurrealDB):

**JSON Serialization:**
- Additional whitespace and formatting: ~20% overhead
- Field names and structure: ~15% overhead
- **Total persistent storage: ~1.35 × in-memory size**

**Binary Serialization (MessagePack/CBOR):**
- Compact binary format: ~10% overhead
- **Total persistent storage: ~1.10 × in-memory size**

### Storage Compression

**Hash Compression:**
- Base64 encoding: 33% reduction vs hex
- Binary storage: 50% reduction vs hex
- **Recommended:** Store hashes as binary, convert to hex for display

## 7. Memory Management Recommendations

### Immediate Optimizations

1. **Use Larger Block Sizes**
   - 4KB blocks provide good balance between granularity and memory
   - 8KB blocks for large datasets where fine-grained diffing is less critical

2. **Implement Hash Compression**
   ```rust
   pub struct CompressedHash {
       data: [u8; 32],  // Raw bytes
   }
   impl std::fmt::Display for CompressedHash {
       fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
           write!(f, "{}", hex::encode(self.data))
       }
   }
   ```

3. **Lazy Tree Loading**
   - Load only leaf hashes for initial comparison
   - Load internal nodes on-demand for detailed analysis

### Structural Optimizations

1. **Incremental Tree Building**
   - Build trees incrementally as content is processed
   - Share common subtrees between similar documents

2. **Tree Caching Strategy**
   - Cache frequently accessed subtrees
   - Implement LRU eviction for large datasets

3. **Memory-Mapped Storage**
   - Use memory-mapped files for very large datasets
   - Load tree structures on-demand from disk

### Long-term Architectural Changes

1. **Hybrid Approach**
   - Store leaf nodes in memory for fast access
   - Keep internal nodes on disk, loaded as needed

2. **Content-Defined Chunking**
   - Use content-defined chunking instead of fixed-size blocks
   - Better deduplication across similar documents

3. **Compressed Node Representation**
   ```rust
   pub struct CompactMerkleNode {
       hash: [u8; 32],
       // Bit-packed flags and small indices
       metadata: u64,
   }
   ```

## 8. Performance vs Memory Trade-offs

### Memory Usage by Use Case

**Real-time Collaboration (Small/Medium datasets):**
- Prioritize speed over memory efficiency
- Keep full trees in memory
- Use 4KB blocks for good balance

**Archive Storage (Large datasets):**
- Prioritize memory efficiency
- Use 8KB blocks or larger
- Implement lazy loading

**Backup/Sync (Very Large datasets):**
- Minimize memory footprint
- Use content-defined chunking
- Store only root hashes initially

### Monitoring Recommendations

1. **Memory Metrics to Track:**
   - Tree size as percentage of content size
   - Hash storage as percentage of total memory
   - HashMap load factor and collision rate

2. **Alert Thresholds:**
   - Tree memory > 50% of content size: Consider larger blocks
   - Hash storage > 40% of tree memory: Consider compression
   - HashMap load factor < 0.25: Consider resizing strategy

## 9. Implementation Priority

### High Priority (Immediate Impact)
1. Increase default block size to 4KB
2. Implement binary hash storage
3. Add memory usage monitoring

### Medium Priority (Structural Improvements)
1. Implement lazy tree loading
2. Add tree caching with LRU eviction
3. Optimize HashMap usage

### Low Priority (Advanced Features)
1. Content-defined chunking
2. Memory-mapped storage
3. Compressed node representation

This analysis provides a foundation for optimizing Merkle tree memory usage in Crucible while maintaining the benefits of content integrity verification and efficient change detection.