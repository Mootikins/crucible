//! EmbeddingCache implementation backed by LanceDB
//!
//! Provides caching for embeddings using LanceDB's efficient vector storage.
//! This enables incremental embedding by checking if content has already been
//! embedded before calling the embedding provider.

use crate::store::LanceStore;
use anyhow::Result;
use arrow_array::{Float32Array, RecordBatch};
use async_trait::async_trait;
use crucible_core::enrichment::{CachedEmbedding, EmbeddingCache};
use futures::TryStreamExt;
use lancedb::query::{ExecutableQuery, QueryBase};

const TABLE_NAME: &str = "embeddings";

#[async_trait]
impl EmbeddingCache for LanceStore {
    async fn get_embedding(
        &self,
        content_hash: &str,
        model: &str,
        model_version: Option<&str>,
    ) -> Result<Option<CachedEmbedding>> {
        let conn = self.connection().await;

        // Try to open embeddings table - return None if it doesn't exist
        let table = match conn.open_table(TABLE_NAME).execute().await {
            Ok(t) => t,
            Err(_) => return Ok(None), // Table doesn't exist yet
        };

        // Build filter for query
        // Escape single quotes in values to prevent SQL injection
        let escaped_hash = content_hash.replace('\'', "''");
        let escaped_model = model.replace('\'', "''");

        let version_filter = match model_version {
            Some(v) => {
                let escaped_version = v.replace('\'', "''");
                format!(" AND model_version = '{}'", escaped_version)
            }
            None => " AND model_version IS NULL".to_string(),
        };

        let filter = format!(
            "content_hash = '{}' AND model = '{}'{}",
            escaped_hash, escaped_model, version_filter
        );

        // Query with filter - using only_if method for filtering
        let mut results = table.query().only_if(&filter).limit(1).execute().await?;

        // Collect batches from the stream
        let batch: RecordBatch = match results.try_next().await? {
            Some(b) => b,
            None => return Ok(None),
        };

        if batch.num_rows() == 0 {
            return Ok(None);
        }

        // Extract vector from the batch
        let vector = extract_vector_from_batch(&batch)?;

        Ok(Some(CachedEmbedding {
            vector,
            content_hash: content_hash.to_string(),
            model: model.to_string(),
            model_version: model_version.map(|s| s.to_string()),
        }))
    }
}

/// Extract the vector column from a RecordBatch
fn extract_vector_from_batch(batch: &RecordBatch) -> Result<Vec<f32>> {
    use arrow_array::FixedSizeListArray;

    let vector_col = batch
        .column_by_name("vector")
        .ok_or_else(|| anyhow::anyhow!("Vector column not found in batch"))?;

    // Try to downcast to FixedSizeListArray (typical for embeddings)
    if let Some(arr) = vector_col.as_any().downcast_ref::<FixedSizeListArray>() {
        let values = arr.value(0);
        let float_arr = values
            .as_any()
            .downcast_ref::<Float32Array>()
            .ok_or_else(|| anyhow::anyhow!("Vector values are not Float32Array"))?;
        return Ok(float_arr.values().to_vec());
    }

    // Fallback: try Float32Array directly (flat vector storage)
    if let Some(arr) = vector_col.as_any().downcast_ref::<Float32Array>() {
        return Ok(arr.values().to_vec());
    }

    Err(anyhow::anyhow!(
        "Vector column has unexpected type: {:?}",
        vector_col.data_type()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_get_embedding_returns_none_when_no_table() {
        let tmp = TempDir::new().unwrap();
        let store = LanceStore::open(tmp.path().join("lance")).await.unwrap();

        let result = store.get_embedding("hash123", "model", None).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_get_embedding_returns_none_when_not_found() {
        let tmp = TempDir::new().unwrap();
        let store = LanceStore::open(tmp.path().join("lance")).await.unwrap();

        // Even with a different hash, should return None
        let result = store
            .get_embedding("nonexistent_hash", "test-model", Some("v1"))
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_get_embedding_handles_special_characters() {
        let tmp = TempDir::new().unwrap();
        let store = LanceStore::open(tmp.path().join("lance")).await.unwrap();

        // Test with special characters that might break SQL
        let result = store
            .get_embedding("hash'with'quotes", "model's-name", Some("v1"))
            .await
            .unwrap();
        assert!(result.is_none());
    }
}
