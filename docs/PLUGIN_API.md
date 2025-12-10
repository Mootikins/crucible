# Clustering Plugin API Documentation

**Date**: 2025-12-09
**Version**: 1.0
**Status**: ✅ COMPLETED

## Overview

The MoC clustering system supports extensible plugins through multiple interfaces:

1. **Rune Plugins** - Script-based algorithms in `runes/events/clustering/`
2. **Algorithm Trait** - Rust-based algorithm implementations
3. **MCP Tools** - HTTP/JSON-RPC exposed clustering functions
4. **CLI Commands** - Command-line interface for direct access

## 1. Rune Plugin API

### Location
Plugins are placed in: `runes/events/clustering/`

### Plugin Structure

Each Rune plugin should export a main function:

```rune
/// Main entry point for the clustering algorithm
pub fn cluster_documents(documents, parameters) -> Result
```

### Available Functions

#### `cluster_documents(documents, parameters)`

**Purpose**: Cluster documents based on similarity

**Parameters**:
- `documents`: Array of document objects with metadata
- `parameters`: Configuration object for the algorithm

**Document Object Structure**:
```rune
{
    "id": number,
    "title": string,
    "path": string,
    "embedding": [number],  // Optional: Pre-computed embedding
    "tags": [string],
    "links": [string],
    "inbound_links": [string],
    "content_length": number
}
```

**Returns**:
```rune
{
    "algorithm": "algorithm_name",
    "clusters": [
        {
            "cluster_id": number,
            "documents": [document_references],
            "size": number
        }
    ],
    "quality": number,  // Quality score (0.0-1.0)
    "metadata": {...}
}
```

### Example Implementation

#### K-Means Plugin (`kmeans.rn`)

```rune
/// Initialize centroids using k-means++ algorithm
pub fn initialize_centroids(data, k) { ... }

/// Assign each data point to nearest centroid
pub fn assign_clusters(data, centroids) { ... }

/// Main clustering function
pub fn cluster_documents(documents, suggested_k) {
    let features = documents_to_features(documents);

    if features.is_empty() {
        return {
            "error": "No documents to cluster",
            "clusters": []
        };
    }

    let k = suggested_k || optimize_k(features);
    let result = kmeans_cluster(features, k, 100, 0.001);

    {
        "algorithm": "kmeans",
        "k": k,
        "clusters": format_clusters(result.clusters, documents),
        "quality": silhouette_score(features, result.assignments)
    }
}
```

### Built-in Helper Functions

#### Vector Operations
```rune
/// Euclidean distance between two vectors
euclidean_distance(a, b)

/// Cosine similarity between two vectors
cosine_similarity(a, b)

/// Hash tags for feature representation
hash_tags(tags)
```

#### Document Processing
```rune
/// Convert documents to feature vectors
documents_to_features(documents)
```

#### Quality Metrics
```rune
/// Calculate silhouette score
silhouette_score(data, assignments)

/// Optimal K determination
optimize_k(data, max_k)
```

### Writing Custom Plugins

1. **Create a new file**: `runes/events/clustering/my_algorithm.rn`

2. **Implement the main function**:
```rune
/// My custom clustering algorithm
pub fn cluster_documents(documents, parameters) {
    // Your algorithm implementation here

    {
        "algorithm": "my_algorithm",
        "clusters": [...],
        "metadata": {
            "parameters_used": parameters
        }
    }
}
```

3. **Add configuration handling**:
```rune
let min_clusters = parameters["min_clusters"] || 2;
let max_iterations = parameters["max_iterations"] || 100;
```

4. **Handle edge cases**:
```rune
if documents.is_empty() {
    return {
        "error": "No documents provided",
        "clusters": []
    };
}
```

## 2. Rust Algorithm Trait API

### Implementing the ClusteringAlgorithm Trait

```rust
use async_trait::async_trait;
use crucible_surrealdb::clustering::*;

#[derive(Debug)]
pub struct MyClusteringAlgorithm {
    name: String,
    parameters: AlgorithmParameters,
}

#[async_trait]
impl ClusteringAlgorithm for MyClusteringAlgorithm {
    async fn cluster(
        &self,
        documents: &[DocumentInfo],
        config: &ClusteringConfig,
    ) -> Result<ClusteringResult, ClusteringError> {
        // Implementation
    }

    fn metadata(&self) -> &AlgorithmMetadata {
        &self.metadata
    }

    fn validate_parameters(
        &self,
        parameters: &HashMap<String, serde_json::Value>,
    ) -> Result<(), ClusteringError> {
        // Validation logic
    }
}
```

### Registering Your Algorithm

```rust
use crucible_surrealdb::clustering::registry::ClusteringRegistry;

// Create factory
pub struct MyAlgorithmFactory;

impl AlgorithmFactory for MyAlgorithmFactory {
    fn create(&self, parameters: &AlgorithmParameters) -> Box<dyn ClusteringAlgorithm> {
        Box::new(MyClusteringAlgorithm::new(parameters))
    }

    fn metadata(&self) -> &AlgorithmMetadata {
        &METADATA
    }
}

// Register with the registry
let registry = ClusteringRegistry::new();
registry.register("my_algorithm".to_string(), Box::new(MyAlgorithmFactory));
```

## 3. MCP Tool API

### Available Tools

#### `detect_mocs`

Detect Maps of Content using heuristic analysis.

**Parameters**:
```json
{
    "min_score": 0.5  // Optional: Minimum MoC score (0.0-1.0)
}
```

**Returns**:
```json
[
    {
        "path": "knowledge/index.md",
        "score": 0.85,
        "reasons": ["High outbound links", "Has index tag"],
        "outbound_links": 12,
        "inbound_links": 3
    }
]
```

#### `cluster_documents`

Cluster documents using heuristic similarity.

**Parameters**:
```json
{
    "min_similarity": 0.2,      // Optional: Minimum similarity
    "min_cluster_size": 2,      // Optional: Min documents per cluster
    "link_weight": 0.6,         // Optional: Link importance
    "tag_weight": 0.3,          // Optional: Tag importance
    "title_weight": 0.1        // Optional: Title importance
}
```

**Returns**:
```json
[
    {
        "id": "cluster_1",
        "documents": ["doc1.md", "doc2.md"],
        "confidence": 0.78
    }
]
```

#### `get_document_stats`

Get statistics about the knowledge base.

**Parameters**: None

**Returns**:
```json
{
    "total_documents": 150,
    "total_links": 342,
    "total_tags": 89,
    "unique_tags": 67,
    "average_links_per_doc": 2.28,
    "average_tags_per_doc": 0.59,
    "average_content_length": 1250.5
}
```

### Using via MCP Server

1. Start the MCP server:
```bash
cru mcp
```

2. Call tools via JSON-RPC:
```json
{
    "jsonrpc": "2.0",
    "method": "tools/call",
    "params": {
        "name": "cluster_documents",
        "arguments": {
            "min_similarity": 0.3,
            "min_cluster_size": 3
        }
    }
}
```

## 4. CLI Command API

### Basic Usage

```bash
# Detect MoCs
cru cluster mocs

# Cluster documents
cru cluster documents --min-similarity 0.3 --min-cluster-size 3

# Get statistics
cru cluster stats

# Run all operations
cru cluster
```

### Advanced Options

```bash
# Use specific algorithm (when implemented)
cru cluster --algorithm kmeans

# Save results to file
cru cluster all --format json --output results.json

# Custom thresholds
cru cluster documents \
    --min-similarity 0.15 \
    --min-cluster-size 5 \
    --min-moc-score 0.4

# Verbose output
cru cluster all --format table
```

### Output Formats

#### Summary (Default)
```
## Maps of Content

1. Knowledge Index (score: 0.85)
2. Project Overview (score: 0.72)
3. Research Hub (score: 0.68)

Found 3 MoCs
```

#### Table
```
1. Knowledge Index (score: 0.85)
2. Project Overview (score: 0.72)
3. Research Hub (score: 0.68)
```

#### JSON
```json
[
    {
        "path": "index.md",
        "score": 0.85,
        "reasons": ["High outbound links"],
        "outbound_links": 12,
        "inbound_links": 3
    }
]
```

## 5. Plugin Development Guidelines

### Best Practices

1. **Error Handling**
   - Always handle edge cases (empty input, invalid parameters)
   - Return structured error messages
   - Log warnings for recoverable issues

2. **Performance**
   - Use efficient data structures
   - Implement early termination for large datasets
   - Consider memory usage for large knowledge bases

3. **Testing**
   - Include unit tests for your algorithm
   - Test with various document types
   - Validate quality metrics

4. **Documentation**
   - Document algorithm parameters
   - Provide usage examples
   - Explain complexity and limitations

### Algorithm Requirements

1. **Deterministic**: Same input should produce same output
2. **Idempotent**: Running multiple times with same data is safe
3. **Scalable**: Handle 100-10,000 documents efficiently
4. **Robust**: Gracefully handle invalid or malformed data

### Quality Metrics

Algorithms should provide at least one of:

- **Silhouette Score**: For clustering quality
- **Modularity Score**: For community detection
- **Convergence**: For iterative algorithms
- **Processing Time**: Performance metrics

## 6. Integration Examples

### Example 1: Custom Document Type

```rune
pub fn cluster_blog_posts(documents, parameters) {
    // Filter for blog posts only
    let blog_posts = documents.filter(|doc|
        doc["tags"].includes("blog")
    );

    // Use tag similarity primarily
    cluster_documents(blog_posts, {
        "tag_weight": 0.8,
        "link_weight": 0.1,
        "title_weight": 0.1
    });
}
```

### Example 2: Time-Based Clustering

```rune
pub fn cluster_by_recency(documents, parameters) {
    // Add recency score to similarity
    let now = timestamp();

    for doc in documents {
        let age_days = (now - doc["created_date"]) / 86400;
        let recency_score = 1.0 / (1.0 + age_days / 30.0);  // Decay over 30 days

        doc["recency_boost"] = recency_score;
    }

    // Weight recency in clustering
    cluster_with_recency(documents, parameters);
}
```

### Example 3: Hierarchical Organization

```rune
pub fn cluster_hierarchical(documents, parameters) {
    // First level: Broad categories
    let primary_clusters = cluster_documents(documents, {
        "min_similarity": 0.3,
        "min_cluster_size": 5
    });

    // Second level: Sub-clusters within each primary
    let all_clusters = [];

    for cluster in primary_clusters {
        let sub_clusters = cluster_documents(cluster.documents, {
            "min_similarity": 0.5,
            "min_cluster_size": 3
        });

        all_clusters.push({
            "primary": cluster,
            "sub_clusters": sub_clusters
        });
    }

    all_clusters
}
```

## Conclusion

The clustering plugin API provides multiple levels of extensibility:

1. **Quick Prototyping**: Use Rune plugins for fast iteration
2. **Performance**: Use Rust traits for optimized algorithms
3. **Integration**: Use MCP for tool integration
4. **Accessibility**: Use CLI for direct access

Choose the approach that best fits your needs, and don't hesitate to combine them for maximum flexibility!