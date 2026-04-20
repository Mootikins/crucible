//! Minimal vector-only LanceDB index.
//!
//! Implements [`crucible_core::storage::VectorStore`] with a two-column
//! Arrow schema: `id: utf8, embedding: fixed_size_list<float32>`. No
//! metadata, tags, properties, or text — those live in the SQLite note
//! store. This separation lets each backend do what it's good at:
//!
//! - LanceDB owns vector similarity at native speed.
//! - SQLite owns relational metadata, EAV properties, joins.
//!
//! The store appends-only and re-indexes on duplicate `id` (delete + insert)
//! since LanceDB doesn't have a native upsert.

use std::sync::Arc;

use arrow_array::{
    Array, FixedSizeListArray, Float32Array, RecordBatch, RecordBatchIterator, StringArray,
};
use arrow_schema::{DataType, Field, Schema};
use async_trait::async_trait;
use futures::TryStreamExt;
use lancedb::query::{ExecutableQuery, QueryBase};
use lancedb::{Connection, Table};
use tokio::sync::RwLock;
use tracing::{debug, warn};

use crucible_core::storage::{StorageError, StorageResult, StorageResultExt, VectorMatch, VectorStore};

const TABLE_NAME: &str = "vectors";
const DEFAULT_EMBEDDING_DIM: usize = 768;

fn vectors_schema(dim: usize) -> Schema {
    Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new(
            "embedding",
            DataType::FixedSizeList(
                Arc::new(Field::new("item", DataType::Float32, true)),
                dim as i32,
            ),
            false,
        ),
    ])
}

/// LanceDB-backed [`VectorStore`].
///
/// One instance per kiln. Persists to a directory on disk (LanceDB stores
/// each table as a sub-directory); the same directory may also hold other
/// Lance tables in the future without conflict.
pub struct LanceVectorIndex {
    connection: Arc<RwLock<Connection>>,
    table: Arc<RwLock<Option<Table>>>,
    schema: Schema,
    dimension: usize,
    db_path: String,
}

impl LanceVectorIndex {
    /// Open or create a vector index at `db_path` with the default
    /// embedding dimension (768, matching all-MiniLM and BGE-small).
    pub async fn open(db_path: &str) -> StorageResult<Self> {
        Self::open_with_dimension(db_path, DEFAULT_EMBEDDING_DIM).await
    }

    /// Open with a custom dimension. The dimension is fixed once the
    /// underlying table is created — mismatches on later opens will fail
    /// at insert time.
    pub async fn open_with_dimension(db_path: &str, dimension: usize) -> StorageResult<Self> {
        if let Some(parent) = std::path::Path::new(db_path).parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let connection = lancedb::connect(db_path)
            .execute()
            .await
            .storage_backend()?;

        let table = match connection.open_table(TABLE_NAME).execute().await {
            Ok(t) => {
                debug!(?db_path, "opened existing vector table");
                Some(t)
            }
            Err(_) => {
                debug!(?db_path, "vector table absent, will create on first upsert");
                None
            }
        };

        Ok(Self {
            connection: Arc::new(RwLock::new(connection)),
            table: Arc::new(RwLock::new(table)),
            schema: vectors_schema(dimension),
            dimension,
            db_path: db_path.to_string(),
        })
    }

    pub fn path(&self) -> &str {
        &self.db_path
    }

    async fn ensure_table(&self) -> StorageResult<Table> {
        {
            let g = self.table.read().await;
            if let Some(t) = g.as_ref() {
                return Ok(t.clone());
            }
        }
        let mut g = self.table.write().await;
        if let Some(t) = g.as_ref() {
            return Ok(t.clone());
        }
        let conn = self.connection.read().await;
        let table = match conn.open_table(TABLE_NAME).execute().await {
            Ok(t) => t,
            Err(_) => {
                debug!(table = TABLE_NAME, "creating vector table");
                conn.create_empty_table(TABLE_NAME, Arc::new(self.schema.clone()))
                    .execute()
                    .await
                    .storage_backend()?
            }
        };
        *g = Some(table.clone());
        Ok(table)
    }

    fn batch_for(&self, id: &str, embedding: &[f32]) -> StorageResult<RecordBatch> {
        if embedding.len() != self.dimension {
            return Err(StorageError::Backend(format!(
                "embedding dimension mismatch: store={}, got={}",
                self.dimension,
                embedding.len()
            )));
        }
        let id_arr = StringArray::from(vec![id]);
        let values = Float32Array::from(embedding.to_vec());
        let field = Arc::new(Field::new("item", DataType::Float32, true));
        let list = FixedSizeListArray::try_new(field, self.dimension as i32, Arc::new(values), None)
            .map_err(|e| StorageError::Backend(format!("failed to build embedding array: {e}")))?;
        RecordBatch::try_new(
            Arc::new(self.schema.clone()),
            vec![Arc::new(id_arr), Arc::new(list)],
        )
        .map_err(|e| StorageError::Backend(format!("failed to build record batch: {e}")))
    }
}

#[async_trait]
impl VectorStore for LanceVectorIndex {
    async fn upsert(&self, id: &str, embedding: Vec<f32>) -> StorageResult<()> {
        let table = self.ensure_table().await?;
        // LanceDB has no native upsert — delete first, then insert.
        // The delete is a no-op if the id doesn't exist.
        let escaped = id.replace('\'', "''");
        if let Err(e) = table.delete(&format!("id = '{escaped}'")).await {
            warn!(?e, id, "vector delete-before-insert failed (continuing)");
        }
        let batch = self.batch_for(id, &embedding)?;
        let schema = batch.schema();
        let reader = RecordBatchIterator::new(vec![Ok(batch)], schema);
        table
            .add(Box::new(reader) as Box<dyn arrow_array::RecordBatchReader + Send>)
            .execute()
            .await
            .storage_backend()?;
        Ok(())
    }

    async fn search(&self, query: &[f32], limit: usize) -> StorageResult<Vec<VectorMatch>> {
        let table = match self.ensure_table().await {
            Ok(t) => t,
            Err(_) => return Ok(Vec::new()),
        };
        // Empty index: return empty regardless of query dimension. Lets
        // callers probe a fresh kiln without first knowing the embedding
        // model's dimension.
        if table.count_rows(None).await.storage_backend()? == 0 {
            return Ok(Vec::new());
        }
        if query.len() != self.dimension {
            return Err(StorageError::Backend(format!(
                "query dimension mismatch: store={}, got={}",
                self.dimension,
                query.len()
            )));
        }
        let stream = table
            .vector_search(query.to_vec())
            .map_err(|e| StorageError::Backend(format!("vector_search setup failed: {e}")))?
            .limit(limit)
            .execute()
            .await
            .storage_backend()?;
        let batches: Vec<RecordBatch> = stream.try_collect().await.storage_backend()?;
        let mut matches = Vec::new();
        for batch in batches {
            let ids = batch
                .column_by_name("id")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>())
                .ok_or_else(|| StorageError::Backend("missing id column".into()))?;
            let distances = batch
                .column_by_name("_distance")
                .and_then(|c| c.as_any().downcast_ref::<Float32Array>());
            for row in 0..batch.num_rows() {
                let id = ids.value(row).to_string();
                let distance = distances.map(|d| d.value(row)).unwrap_or(0.0);
                // LanceDB returns L2 distance by default; convert to a
                // bounded similarity for the trait contract. Higher = better.
                let similarity = 1.0 / (1.0 + distance);
                matches.push(VectorMatch { id, similarity });
            }
        }
        Ok(matches)
    }

    async fn delete(&self, id: &str) -> StorageResult<()> {
        let table = match self.ensure_table().await {
            Ok(t) => t,
            Err(_) => return Ok(()), // table doesn't exist, no-op
        };
        let escaped = id.replace('\'', "''");
        table
            .delete(&format!("id = '{escaped}'"))
            .await
            .storage_backend()?;
        Ok(())
    }

    async fn count(&self) -> StorageResult<usize> {
        let table = match self.ensure_table().await {
            Ok(t) => t,
            Err(_) => return Ok(0),
        };
        let count = table.count_rows(None).await.storage_backend()?;
        Ok(count)
    }

    fn dimension(&self) -> usize {
        self.dimension
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn open_test_index(dim: usize) -> (TempDir, LanceVectorIndex) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("vectors.lance");
        let idx = LanceVectorIndex::open_with_dimension(path.to_str().unwrap(), dim)
            .await
            .unwrap();
        (dir, idx)
    }

    #[tokio::test]
    async fn upsert_then_search_returns_match() {
        let (_dir, idx) = open_test_index(4).await;
        idx.upsert("note-a", vec![1.0, 0.0, 0.0, 0.0]).await.unwrap();
        idx.upsert("note-b", vec![0.0, 1.0, 0.0, 0.0]).await.unwrap();

        let results = idx.search(&[1.0, 0.0, 0.0, 0.0], 2).await.unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].id, "note-a");
        assert!(results[0].similarity > results[1].similarity);
    }

    #[tokio::test]
    async fn upsert_overwrites_existing_id() {
        let (_dir, idx) = open_test_index(4).await;
        idx.upsert("note", vec![1.0, 0.0, 0.0, 0.0]).await.unwrap();
        idx.upsert("note", vec![0.0, 1.0, 0.0, 0.0]).await.unwrap();
        assert_eq!(idx.count().await.unwrap(), 1);

        let results = idx.search(&[0.0, 1.0, 0.0, 0.0], 1).await.unwrap();
        assert_eq!(results[0].id, "note");
    }

    #[tokio::test]
    async fn delete_removes_id() {
        let (_dir, idx) = open_test_index(4).await;
        idx.upsert("a", vec![1.0, 0.0, 0.0, 0.0]).await.unwrap();
        idx.upsert("b", vec![0.0, 1.0, 0.0, 0.0]).await.unwrap();
        idx.delete("a").await.unwrap();
        assert_eq!(idx.count().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn search_on_empty_index_returns_empty() {
        let (_dir, idx) = open_test_index(4).await;
        let results = idx.search(&[1.0, 0.0, 0.0, 0.0], 5).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn dimension_mismatch_errors() {
        let (_dir, idx) = open_test_index(4).await;
        let err = idx.upsert("a", vec![1.0, 2.0]).await.unwrap_err();
        assert!(err.to_string().contains("dimension mismatch"));
    }

    #[tokio::test]
    async fn reopen_preserves_data() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("vectors.lance");
        let path_str = path.to_str().unwrap();

        {
            let idx = LanceVectorIndex::open_with_dimension(path_str, 4).await.unwrap();
            idx.upsert("a", vec![1.0, 0.0, 0.0, 0.0]).await.unwrap();
            idx.upsert("b", vec![0.0, 1.0, 0.0, 0.0]).await.unwrap();
        }

        let idx = LanceVectorIndex::open_with_dimension(path_str, 4).await.unwrap();
        assert_eq!(idx.count().await.unwrap(), 2);
    }
}
