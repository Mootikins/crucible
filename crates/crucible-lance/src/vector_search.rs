//! Vector similarity search using LanceDB

use crate::store::LanceStore;
use anyhow::Result;
use arrow_array::{Float32Array, StringArray};
use futures::TryStreamExt;
use lancedb::query::{ExecutableQuery, QueryBase};

const TABLE_NAME: &str = "embeddings";

/// Result of a vector similarity search
#[derive(Debug, Clone)]
pub struct VectorSearchResult {
    /// Content hash of the matched embedding
    pub content_hash: String,
    /// Distance score (lower is more similar for L2, higher for cosine)
    pub distance: f32,
}

impl LanceStore {
    /// Search for similar vectors
    ///
    /// Returns content hashes of the most similar embeddings.
    /// Results are ordered by distance (most similar first).
    pub async fn search_vectors(
        &self,
        query_vector: &[f32],
        limit: usize,
    ) -> Result<Vec<VectorSearchResult>> {
        let conn = self.connection().await;

        // Open embeddings table
        let table = match conn.open_table(TABLE_NAME).execute().await {
            Ok(t) => t,
            Err(_) => return Ok(vec![]), // No embeddings yet
        };

        // Perform vector search
        let results = table
            .vector_search(query_vector)?
            .limit(limit)
            .execute()
            .await?
            .try_collect::<Vec<_>>()
            .await?;

        let mut search_results = Vec::new();

        for batch in results {
            // Extract content_hash column
            let content_hash_col = batch
                .column_by_name("content_hash")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>());

            // Extract _distance column (added by LanceDB vector search)
            let distance_col = batch
                .column_by_name("_distance")
                .and_then(|c| c.as_any().downcast_ref::<Float32Array>());

            if let Some(hashes) = content_hash_col {
                for i in 0..batch.num_rows() {
                    let content_hash = hashes.value(i).to_string();
                    let distance = distance_col.map(|d| d.value(i)).unwrap_or(0.0);

                    search_results.push(VectorSearchResult {
                        content_hash,
                        distance,
                    });
                }
            }
        }

        Ok(search_results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_search_vectors_empty_db() {
        let tmp = TempDir::new().unwrap();
        let store = LanceStore::open(tmp.path().join("lance")).await.unwrap();

        // Search with random vector - should return empty since no table exists
        let results = store.search_vectors(&[0.1; 384], 10).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_vector_search_result_fields() {
        // Test VectorSearchResult struct creation and field access
        let result = VectorSearchResult {
            content_hash: "abc123".to_string(),
            distance: 0.5,
        };

        assert_eq!(result.content_hash, "abc123");
        assert!((result.distance - 0.5).abs() < f32::EPSILON);
    }

    #[tokio::test]
    async fn test_vector_search_result_clone() {
        let result = VectorSearchResult {
            content_hash: "hash456".to_string(),
            distance: 0.25,
        };

        let cloned = result.clone();
        assert_eq!(cloned.content_hash, result.content_hash);
        assert_eq!(cloned.distance, result.distance);
    }
}
